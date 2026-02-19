use crate::error::SdkError;
use crate::provider::{ProviderAdapter, StreamEventStream};
use crate::providers::common::{extract_system_prompt, send_and_read_body, ApiMessage};
use crate::types::{
    ContentPart, FinishReason, Message, Request, Response, Role, ToolCall, Usage,
};

/// Provider adapter for the Anthropic Messages API.
pub struct Adapter {
    api_key: String,
    client: reqwest::Client,
}

impl Adapter {
    #[must_use]
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            client: reqwest::Client::new(),
        }
    }
}

// --- Request types ---

#[derive(serde::Serialize)]
struct ApiRequest {
    model: String,
    messages: Vec<ApiMessage>,
    max_tokens: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_sequences: Option<Vec<String>>,
}

// --- Response types ---

#[derive(serde::Deserialize)]
struct ApiResponse {
    id: String,
    model: String,
    content: Vec<serde_json::Value>,
    stop_reason: Option<String>,
    usage: ApiUsage,
}

#[derive(serde::Deserialize)]
struct ApiUsage {
    input_tokens: i64,
    output_tokens: i64,
}

fn map_finish_reason(stop_reason: Option<&str>) -> FinishReason {
    match stop_reason {
        Some("end_turn") | None => FinishReason::Stop,
        Some("max_tokens") => FinishReason::Length,
        Some("tool_use") => FinishReason::ToolCalls,
        Some(other) => FinishReason::Other(other.to_string()),
    }
}

fn parse_content_block(block: &serde_json::Value) -> Option<ContentPart> {
    match block.get("type")?.as_str()? {
        "text" => Some(ContentPart::text(block.get("text")?.as_str()?)),
        "tool_use" => Some(ContentPart::ToolCall(ToolCall::new(
            block.get("id")?.as_str()?,
            block.get("name")?.as_str()?,
            block.get("input")?.clone(),
        ))),
        _ => None,
    }
}

#[allow(clippy::unnecessary_literal_bound)]
#[async_trait::async_trait]
impl ProviderAdapter for Adapter {
    fn name(&self) -> &str {
        "anthropic"
    }

    async fn complete(&self, request: &Request) -> Result<Response, SdkError> {
        let (system, other_messages) = extract_system_prompt(&request.messages);

        let api_messages: Vec<ApiMessage> = other_messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::Assistant => "assistant",
                    Role::System | Role::User | Role::Tool | Role::Developer => "user",
                };
                ApiMessage {
                    role: role.to_string(),
                    content: msg.text(),
                }
            })
            .collect();

        let api_request = ApiRequest {
            model: request.model.clone(),
            messages: api_messages,
            max_tokens: request.max_tokens.unwrap_or(1024),
            system,
            temperature: request.temperature,
            top_p: request.top_p,
            stop_sequences: request.stop_sequences.clone(),
        };

        let body = send_and_read_body(
            self.client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&api_request),
            "anthropic",
            "type",
        )
        .await?;

        let api_resp: ApiResponse =
            serde_json::from_str(&body).map_err(|e| SdkError::Network {
                message: format!("failed to parse Anthropic response: {e}"),
            })?;

        let content_parts: Vec<ContentPart> = api_resp
            .content
            .iter()
            .filter_map(parse_content_block)
            .collect();

        let finish_reason = map_finish_reason(api_resp.stop_reason.as_deref());
        let total = api_resp.usage.input_tokens + api_resp.usage.output_tokens;

        Ok(Response {
            id: api_resp.id,
            model: api_resp.model,
            provider: "anthropic".to_string(),
            message: Message {
                role: Role::Assistant,
                content: content_parts,
                name: None,
                tool_call_id: None,
            },
            finish_reason,
            usage: Usage {
                input_tokens: api_resp.usage.input_tokens,
                output_tokens: api_resp.usage.output_tokens,
                total_tokens: total,
                ..Usage::default()
            },
            raw: serde_json::from_str(&body).ok(),
            warnings: vec![],
            rate_limit: None,
        })
    }

    async fn stream(&self, _request: &Request) -> Result<StreamEventStream, SdkError> {
        Err(SdkError::Configuration {
            message: "streaming not yet implemented".to_string(),
        })
    }
}
