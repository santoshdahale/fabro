use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use futures::stream;

use crate::error::{error_from_status_code, ProviderErrorDetail, ProviderErrorKind, SdkError};
use crate::provider::{ProviderAdapter, StreamEventStream};
use crate::providers::common::{
    extract_system_prompt, parse_error_body, parse_retry_after, send_and_read_body,
};
use crate::types::{
    ContentPart, FinishReason, Message, Request, Response, ResponseFormat, ResponseFormatType,
    Role, StreamEvent, ToolCall, ToolChoice, ToolDefinition, Usage,
};

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";

/// Provider adapter for the Google Gemini `generateContent` API.
pub struct Adapter {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
    request_timeout: std::time::Duration,
}

impl Adapter {
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        let timeout = crate::types::AdapterTimeout::default();
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs_f64(timeout.connect))
            .build()
            .unwrap_or_default();
        Self {
            api_key: api_key.into(),
            base_url: DEFAULT_BASE_URL.to_string(),
            client,
            request_timeout: std::time::Duration::from_secs_f64(timeout.request),
        }
    }

    #[must_use]
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

// --- Request types ---

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ApiRequest {
    contents: Vec<Content>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<SystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiToolGroup>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_config: Option<serde_json::Value>,
}

#[derive(serde::Serialize)]
struct Content {
    role: String,
    parts: Vec<serde_json::Value>,
}

#[derive(serde::Serialize)]
struct SystemInstruction {
    parts: Vec<serde_json::Value>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_schema: Option<serde_json::Value>,
}

/// Gemini groups function declarations under a `tools` array.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiToolGroup {
    function_declarations: Vec<GeminiFunctionDecl>,
}

#[derive(serde::Serialize)]
struct GeminiFunctionDecl {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

// --- Response types ---

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiResponse {
    candidates: Option<Vec<Candidate>>,
    usage_metadata: Option<UsageMetadata>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    content: Option<CandidateContent>,
    finish_reason: Option<String>,
}

#[derive(serde::Deserialize)]
struct CandidateContent {
    parts: Option<Vec<serde_json::Value>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_field_names)]
struct UsageMetadata {
    prompt_token_count: Option<i64>,
    candidates_token_count: Option<i64>,
    total_token_count: Option<i64>,
    thoughts_token_count: Option<i64>,
    cached_content_token_count: Option<i64>,
}

/// Map Gemini's finish reason, inferring `ToolCalls` from content when needed.
fn map_finish_reason(reason: Option<&str>, has_function_calls: bool) -> FinishReason {
    if has_function_calls {
        return FinishReason::ToolCalls;
    }
    match reason {
        Some("STOP") | None => FinishReason::Stop,
        Some("MAX_TOKENS") => FinishReason::Length,
        Some("SAFETY" | "RECITATION") => FinishReason::ContentFilter,
        Some(other) => FinishReason::Other(other.to_string()),
    }
}

fn parse_part(part: &serde_json::Value) -> Option<ContentPart> {
    if let Some(text) = part.get("text").and_then(serde_json::Value::as_str) {
        return Some(ContentPart::text(text));
    }
    if let Some(fc) = part.get("functionCall") {
        let name = fc.get("name")?.as_str()?.to_string();
        let args = fc
            .get("args")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
        return Some(ContentPart::ToolCall(ToolCall::new(
            uuid::Uuid::new_v4().to_string(),
            name,
            args,
        )));
    }
    None
}

/// Check if any parts contain function calls.
fn parts_have_function_calls(parts: &[serde_json::Value]) -> bool {
    parts.iter().any(|p| p.get("functionCall").is_some())
}

/// Build a mapping from tool call ID to function name by scanning assistant messages.
///
/// Gemini uses function names (not call IDs) in `functionResponse`. Since the adapter
/// generates synthetic UUIDs as tool call IDs, we need this mapping to recover the
/// original function name when sending tool results back.
fn build_tool_call_id_to_name(messages: &[&Message]) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for msg in messages {
        if msg.role == Role::Assistant {
            for part in &msg.content {
                if let ContentPart::ToolCall(tc) = part {
                    map.insert(tc.id.clone(), tc.name.clone());
                }
            }
        }
    }
    map
}

/// Translate unified messages to Gemini content format.
fn translate_messages(messages: &[&Message]) -> Vec<Content> {
    let id_to_name = build_tool_call_id_to_name(messages);
    let mut contents: Vec<Content> = Vec::new();

    for msg in messages {
        let role = match msg.role {
            Role::Assistant => "model",
            Role::User | Role::Tool => "user",
            Role::System | Role::Developer => continue,
        };

        let parts: Vec<serde_json::Value> = msg
            .content
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text(text) => Some(serde_json::json!({"text": text})),
                ContentPart::ToolCall(tc) => Some(serde_json::json!({
                    "functionCall": {
                        "name": tc.name,
                        "args": tc.arguments,
                    }
                })),
                ContentPart::Image(img) => {
                    img.url.as_ref().map_or_else(
                        || {
                            img.data.as_ref().map(|data| {
                                let mime = img.media_type.as_deref().unwrap_or("image/png");
                                let b64 = BASE64_STANDARD.encode(data);
                                serde_json::json!({"inlineData": {"mimeType": mime, "data": b64}})
                            })
                        },
                        |url| {
                            if crate::providers::common::is_file_path(url) {
                                match crate::providers::common::load_file_as_base64(url) {
                                    Ok((b64, mime)) => Some(serde_json::json!({"inlineData": {"mimeType": mime, "data": b64}})),
                                    Err(_) => None,
                                }
                            } else {
                                let mime = img.media_type.as_deref().unwrap_or("image/png");
                                Some(serde_json::json!({"fileData": {"mimeType": mime, "fileUri": url}}))
                            }
                        },
                    )
                }
                ContentPart::ToolResult(tr) => {
                    // Gemini's functionResponse uses the function *name*, not the call ID.
                    // Look up the original function name from the tool call mapping.
                    let function_name = id_to_name
                        .get(&tr.tool_call_id)
                        .cloned()
                        .unwrap_or_else(|| tr.tool_call_id.clone());
                    let response = tr.content.as_str().map_or_else(
                        || {
                            if tr.content.is_object() {
                                tr.content.clone()
                            } else {
                                serde_json::json!({"result": tr.content.to_string()})
                            }
                        },
                        |s| serde_json::json!({"result": s}),
                    );
                    Some(serde_json::json!({
                        "functionResponse": {
                            "name": function_name,
                            "response": response,
                        }
                    }))
                }
                _ => None,
            })
            .collect();

        if parts.is_empty() {
            continue;
        }

        contents.push(Content {
            role: role.to_string(),
            parts,
        });
    }

    contents
}

/// Translate unified tool definitions to Gemini's format.
fn translate_tools(tools: &[ToolDefinition]) -> Vec<GeminiToolGroup> {
    vec![GeminiToolGroup {
        function_declarations: tools
            .iter()
            .map(|t| GeminiFunctionDecl {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect(),
    }]
}

/// Translate unified `ToolChoice` to Gemini's `toolConfig`.
fn translate_tool_choice(choice: &ToolChoice) -> serde_json::Value {
    match choice {
        ToolChoice::Auto => serde_json::json!({
            "functionCallingConfig": {"mode": "AUTO"}
        }),
        ToolChoice::None => serde_json::json!({
            "functionCallingConfig": {"mode": "NONE"}
        }),
        ToolChoice::Required => serde_json::json!({
            "functionCallingConfig": {"mode": "ANY"}
        }),
        ToolChoice::Named { tool_name } => serde_json::json!({
            "functionCallingConfig": {
                "mode": "ANY",
                "allowedFunctionNames": [tool_name],
            }
        }),
    }
}

/// Translate unified `ResponseFormat` to Gemini generation config fields.
///
/// Returns `(response_mime_type, response_schema)`.
fn translate_response_format(
    format: &ResponseFormat,
) -> (Option<String>, Option<serde_json::Value>) {
    match format.kind {
        ResponseFormatType::Text => (None, None),
        ResponseFormatType::JsonObject => (Some("application/json".to_string()), None),
        ResponseFormatType::JsonSchema => (
            Some("application/json".to_string()),
            format.json_schema.clone(),
        ),
    }
}

/// Build the Gemini API request body from a unified `Request`.
fn build_api_request(request: &Request) -> ApiRequest {
    let (system_text, other_messages) = extract_system_prompt(&request.messages);

    let system_instruction = system_text.map(|text| SystemInstruction {
        parts: vec![serde_json::json!({"text": text})],
    });

    let contents = translate_messages(&other_messages);

    let (response_mime_type, response_schema) = request
        .response_format
        .as_ref()
        .map_or((None, None), translate_response_format);

    let generation_config = GenerationConfig {
        temperature: request.temperature,
        max_output_tokens: request.max_tokens,
        top_p: request.top_p,
        stop_sequences: request.stop_sequences.clone(),
        response_mime_type,
        response_schema,
    };

    let api_tools = request.tools.as_ref().map(|t| translate_tools(t));
    let tool_config = request.tool_choice.as_ref().map(translate_tool_choice);

    ApiRequest {
        contents,
        system_instruction,
        generation_config: Some(generation_config),
        tools: api_tools,
        tool_config,
    }
}

/// Convert `UsageMetadata` from the Gemini API into a unified `Usage`.
fn parse_usage(metadata: Option<&UsageMetadata>) -> Usage {
    metadata.map_or_else(Usage::default, |u| {
        let input = u.prompt_token_count.unwrap_or(0);
        let output = u.candidates_token_count.unwrap_or(0);
        let total = u.total_token_count.unwrap_or(input + output);
        Usage {
            input_tokens: input,
            output_tokens: output,
            total_tokens: total,
            reasoning_tokens: u.thoughts_token_count,
            cache_read_tokens: u.cached_content_token_count,
            ..Usage::default()
        }
    })
}

/// Send an HTTP request for streaming and return the `reqwest::Response`.
///
/// Checks for HTTP errors before returning. On error, reads the body and
/// maps it to `SdkError` using the same logic as `send_and_read_body`.
async fn send_streaming_request(
    request: reqwest::RequestBuilder,
) -> Result<reqwest::Response, SdkError> {
    let http_resp = request.send().await.map_err(|e| SdkError::Network {
        message: e.to_string(),
    })?;

    let status = http_resp.status();
    if !status.is_success() {
        let retry_after = parse_retry_after(http_resp.headers());
        let body = http_resp.text().await.map_err(|e| SdkError::Network {
            message: e.to_string(),
        })?;
        let (msg, code, raw) = parse_error_body(&body, "status");
        return Err(error_from_status_code(
            status.as_u16(),
            msg,
            "gemini".to_string(),
            code,
            raw,
            retry_after,
        ));
    }

    Ok(http_resp)
}

/// Process a stream of SSE chunks from the Gemini `streamGenerateContent` endpoint
/// and yield `StreamEvent` values.
fn process_sse_stream(http_resp: reqwest::Response, model: String) -> StreamEventStream {
    Box::pin(stream::unfold(
        SseStreamState::new(http_resp, model),
        |mut state| async move {
            // If we have buffered events, yield them first.
            if let Some(event) = state.pending_events.pop_front() {
                return Some((Ok(event), state));
            }

            // Read SSE lines until we get a data payload or the stream ends.
            loop {
                let line = match state.read_line().await {
                    Ok(Some(line)) => line,
                    Ok(None) => {
                        // Stream ended. Emit Finish if we haven't yet.
                        if !state.finished {
                            state.finished = true;
                            let event = state.build_finish_event();
                            return Some((Ok(event), state));
                        }
                        return None;
                    }
                    Err(e) => return Some((Err(e), state)),
                };

                // SSE format: lines starting with "data:" carry the payload.
                let data = if let Some(stripped) = line.strip_prefix("data:") {
                    stripped.trim()
                } else {
                    // Ignore non-data lines (empty lines, comments, event: lines).
                    continue;
                };

                // Skip empty data lines.
                if data.is_empty() {
                    continue;
                }

                // Parse the JSON chunk.
                let chunk: ApiResponse = match serde_json::from_str(data) {
                    Ok(v) => v,
                    Err(e) => {
                        return Some((
                            Err(SdkError::Stream {
                                message: format!("failed to parse Gemini SSE chunk: {e}"),
                            }),
                            state,
                        ));
                    }
                };

                // Extract events from this chunk.
                state.process_chunk(&chunk);

                // Track usage from every chunk; the final one will have the totals.
                if let Some(ref usage_meta) = chunk.usage_metadata {
                    state.usage = parse_usage(Some(usage_meta));
                }

                // Extract finish reason from the candidate if present.
                let candidate_finish = chunk
                    .candidates
                    .as_ref()
                    .and_then(|c| c.first())
                    .and_then(|c| c.finish_reason.clone());
                if let Some(reason) = candidate_finish {
                    state.finish_reason_str = Some(reason);
                }

                // Yield the first buffered event if any were produced.
                if let Some(event) = state.pending_events.pop_front() {
                    return Some((Ok(event), state));
                }
                // If no events were produced from this chunk, continue reading.
            }
        },
    ))
}

/// Internal state for the SSE stream processor.
struct SseStreamState {
    http_resp: reqwest::Response,
    model: String,
    /// Buffered SSE text not yet split into complete lines.
    line_buffer: String,
    /// Events extracted from a chunk but not yet yielded.
    pending_events: std::collections::VecDeque<StreamEvent>,
    /// Whether we have emitted a `StreamStart` event.
    stream_started: bool,
    /// Whether we have emitted a `TextStart` event.
    text_started: bool,
    /// Accumulated text across all chunks.
    accumulated_text: String,
    /// Accumulated tool calls across all chunks.
    accumulated_tool_calls: Vec<ToolCall>,
    /// The `text_id` used for `TextStart`/`TextDelta`/`TextEnd`.
    text_id: String,
    /// Latest usage metadata (updated per chunk; final chunk has totals).
    usage: Usage,
    /// The finish reason string from the candidate, if received.
    finish_reason_str: Option<String>,
    /// Whether we have emitted the `Finish` event.
    finished: bool,
}

impl SseStreamState {
    fn new(http_resp: reqwest::Response, model: String) -> Self {
        Self {
            http_resp,
            model,
            line_buffer: String::new(),
            pending_events: std::collections::VecDeque::new(),
            stream_started: false,
            text_started: false,
            accumulated_text: String::new(),
            accumulated_tool_calls: Vec::new(),
            text_id: uuid::Uuid::new_v4().to_string(),
            usage: Usage::default(),
            finish_reason_str: None,
            finished: false,
        }
    }

    /// Read the next complete line from the HTTP byte stream.
    ///
    /// Returns `Ok(None)` when the stream is exhausted.
    async fn read_line(&mut self) -> Result<Option<String>, SdkError> {
        loop {
            // Check if we already have a complete line in the buffer.
            if let Some(newline_pos) = self.line_buffer.find('\n') {
                let line = self.line_buffer[..newline_pos]
                    .trim_end_matches('\r')
                    .to_string();
                self.line_buffer = self.line_buffer[newline_pos + 1..].to_string();
                return Ok(Some(line));
            }

            // Read more bytes from the HTTP response.
            match self.http_resp.chunk().await {
                Ok(Some(bytes)) => {
                    let text = String::from_utf8_lossy(&bytes);
                    self.line_buffer.push_str(&text);
                }
                Ok(None) => {
                    // Stream ended. Return any remaining buffered content.
                    if self.line_buffer.is_empty() {
                        return Ok(None);
                    }
                    let remaining = std::mem::take(&mut self.line_buffer);
                    let line = remaining.trim_end_matches('\r').to_string();
                    if line.is_empty() {
                        return Ok(None);
                    }
                    return Ok(Some(line));
                }
                Err(e) => {
                    return Err(SdkError::Stream {
                        message: format!("error reading Gemini stream: {e}"),
                    });
                }
            }
        }
    }

    /// Extract stream events from a parsed SSE chunk and buffer them.
    fn process_chunk(&mut self, chunk: &ApiResponse) {
        if !self.stream_started {
            self.stream_started = true;
            self.pending_events.push_back(StreamEvent::StreamStart);
        }

        let parts = chunk
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .and_then(|c| c.content.as_ref())
            .and_then(|c| c.parts.as_ref());

        let Some(parts) = parts else {
            return;
        };

        for part in parts {
            if let Some(text) = part.get("text").and_then(serde_json::Value::as_str) {
                if !self.text_started {
                    self.text_started = true;
                    self.pending_events.push_back(StreamEvent::TextStart {
                        text_id: Some(self.text_id.clone()),
                    });
                }
                self.accumulated_text.push_str(text);
                self.pending_events
                    .push_back(StreamEvent::text_delta(text, Some(self.text_id.clone())));
            } else if let Some(fc) = part.get("functionCall") {
                let name = fc
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let args = fc
                    .get("args")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
                let tool_call = ToolCall::new(uuid::Uuid::new_v4().to_string(), name, args);

                // Gemini delivers function calls as complete objects in a single chunk.
                self.pending_events
                    .push_back(StreamEvent::ToolCallStart {
                        tool_call: tool_call.clone(),
                    });
                self.pending_events.push_back(StreamEvent::ToolCallEnd {
                    tool_call: tool_call.clone(),
                });
                self.accumulated_tool_calls.push(tool_call);
            }
        }

        // If a finish reason is present on this chunk's candidate, emit TextEnd.
        let has_finish_reason = chunk
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .and_then(|c| c.finish_reason.as_ref())
            .is_some();

        if has_finish_reason && self.text_started {
            self.pending_events.push_back(StreamEvent::TextEnd {
                text_id: Some(self.text_id.clone()),
            });
        }
    }

    /// Build the final `Finish` event from accumulated state.
    fn build_finish_event(&self) -> StreamEvent {
        let has_tool_calls = !self.accumulated_tool_calls.is_empty();
        let finish_reason =
            map_finish_reason(self.finish_reason_str.as_deref(), has_tool_calls);

        let mut content_parts: Vec<ContentPart> = Vec::new();
        if !self.accumulated_text.is_empty() {
            content_parts.push(ContentPart::text(&self.accumulated_text));
        }
        for tc in &self.accumulated_tool_calls {
            content_parts.push(ContentPart::ToolCall(tc.clone()));
        }

        let response = Response {
            id: uuid::Uuid::new_v4().to_string(),
            model: self.model.clone(),
            provider: "gemini".to_string(),
            message: Message {
                role: Role::Assistant,
                content: content_parts,
                name: None,
                tool_call_id: None,
            },
            finish_reason: finish_reason.clone(),
            usage: self.usage.clone(),
            raw: None,
            warnings: vec![],
            rate_limit: None,
        };

        StreamEvent::finish(finish_reason, self.usage.clone(), response)
    }
}

#[allow(clippy::unnecessary_literal_bound)]
#[async_trait::async_trait]
impl ProviderAdapter for Adapter {
    fn name(&self) -> &str {
        "gemini"
    }

    async fn complete(&self, request: &Request) -> Result<Response, SdkError> {
        let api_request = build_api_request(request);

        let url = format!(
            "{}/models/{}:generateContent?key={}",
            self.base_url, request.model, self.api_key
        );

        let body = send_and_read_body(
            self.client.post(&url).json(&api_request).timeout(self.request_timeout),
            "gemini",
            "status",
        )
        .await?;

        let api_resp: ApiResponse =
            serde_json::from_str(&body).map_err(|e| SdkError::Network {
                message: format!("failed to parse Gemini response: {e}"),
            })?;

        let candidate = api_resp
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .ok_or_else(|| SdkError::Provider {
                kind: ProviderErrorKind::Server,
                detail: Box::new(ProviderErrorDetail::new(
                    "no candidates in Gemini response",
                    "gemini",
                )),
            })?;

        let raw_parts = candidate.content.as_ref().and_then(|c| c.parts.as_ref());

        let content_parts: Vec<ContentPart> = raw_parts
            .map(|parts| parts.iter().filter_map(parse_part).collect())
            .unwrap_or_default();

        // Gemini has no dedicated tool_calls finish reason; infer from parts
        let has_tool_calls = raw_parts.is_some_and(|p| parts_have_function_calls(p));
        let finish_reason =
            map_finish_reason(candidate.finish_reason.as_deref(), has_tool_calls);

        let usage = parse_usage(api_resp.usage_metadata.as_ref());

        Ok(Response {
            id: uuid::Uuid::new_v4().to_string(),
            model: request.model.clone(),
            provider: "gemini".to_string(),
            message: Message {
                role: Role::Assistant,
                content: content_parts,
                name: None,
                tool_call_id: None,
            },
            finish_reason,
            usage,
            raw: serde_json::from_str(&body).ok(),
            warnings: vec![],
            rate_limit: None,
        })
    }

    async fn stream(&self, request: &Request) -> Result<StreamEventStream, SdkError> {
        let api_request = build_api_request(request);

        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse&key={}",
            self.base_url, request.model, self.api_key
        );

        let http_resp =
            send_streaming_request(self.client.post(&url).json(&api_request)).await?;

        Ok(process_sse_stream(http_resp, request.model.clone()))
    }
}
