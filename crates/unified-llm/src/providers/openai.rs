use crate::error::SdkError;
use crate::provider::{ProviderAdapter, StreamEventStream};
use crate::error::{ProviderErrorDetail, ProviderErrorKind};
use crate::providers::common::{send_and_read_body, ApiMessage};
use crate::types::{
    ContentPart, FinishReason, Message, Request, Response, Role, ToolCall, Usage,
};

/// Provider adapter for the `OpenAI` Chat Completions API.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop: Option<Vec<String>>,
}

// --- Response types ---

#[derive(serde::Deserialize)]
struct ApiResponse {
    id: String,
    model: String,
    choices: Vec<ApiChoice>,
    usage: Option<ApiUsage>,
}

#[derive(serde::Deserialize)]
struct ApiChoice {
    message: ApiChoiceMessage,
    finish_reason: Option<String>,
}

#[derive(serde::Deserialize)]
struct ApiChoiceMessage {
    content: Option<String>,
    tool_calls: Option<Vec<ApiToolCall>>,
}

#[derive(serde::Deserialize)]
struct ApiToolCall {
    id: String,
    function: ApiFunction,
}

#[derive(serde::Deserialize)]
struct ApiFunction {
    name: String,
    arguments: String,
}

#[derive(serde::Deserialize)]
#[allow(clippy::struct_field_names)]
struct ApiUsage {
    prompt_tokens: i64,
    completion_tokens: i64,
    total_tokens: i64,
}

fn map_finish_reason(reason: Option<&str>) -> FinishReason {
    match reason {
        Some("stop") | None => FinishReason::Stop,
        Some("length") => FinishReason::Length,
        Some("tool_calls") => FinishReason::ToolCalls,
        Some("content_filter") => FinishReason::ContentFilter,
        Some(other) => FinishReason::Other(other.to_string()),
    }
}

#[allow(clippy::unnecessary_literal_bound)]
#[async_trait::async_trait]
impl ProviderAdapter for Adapter {
    fn name(&self) -> &str {
        "openai"
    }

    async fn complete(&self, request: &Request) -> Result<Response, SdkError> {
        let api_messages: Vec<ApiMessage> = request
            .messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System | Role::Developer => "system",
                    Role::User | Role::Tool => "user",
                    Role::Assistant => "assistant",
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
            temperature: request.temperature,
            max_tokens: request.max_tokens,
            top_p: request.top_p,
            stop: request.stop_sequences.clone(),
        };

        let body = send_and_read_body(
            self.client
                .post("https://api.openai.com/v1/chat/completions")
                .bearer_auth(&self.api_key)
                .json(&api_request),
            "openai",
            "type",
        )
        .await?;

        let api_resp: ApiResponse =
            serde_json::from_str(&body).map_err(|e| SdkError::Network {
                message: format!("failed to parse OpenAI response: {e}"),
            })?;

        let choice = api_resp.choices.first().ok_or_else(|| SdkError::Provider {
            kind: ProviderErrorKind::Server,
            detail: Box::new(ProviderErrorDetail::new("no choices in OpenAI response", "openai")),
        })?;

        let mut content_parts = Vec::new();
        if let Some(text) = &choice.message.content {
            if !text.is_empty() {
                content_parts.push(ContentPart::text(text));
            }
        }
        if let Some(tool_calls) = &choice.message.tool_calls {
            for tc in tool_calls {
                let arguments = serde_json::from_str(&tc.function.arguments)
                    .unwrap_or_else(|_| serde_json::json!({}));
                content_parts.push(ContentPart::ToolCall(ToolCall::new(
                    &tc.id,
                    &tc.function.name,
                    arguments,
                )));
            }
        }

        let finish_reason = map_finish_reason(choice.finish_reason.as_deref());

        let usage = api_resp.usage.as_ref().map_or_else(Usage::default, |u| Usage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            ..Usage::default()
        });

        Ok(Response {
            id: api_resp.id,
            model: api_resp.model,
            provider: "openai".to_string(),
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

    async fn stream(&self, _request: &Request) -> Result<StreamEventStream, SdkError> {
        Err(SdkError::Configuration {
            message: "streaming not yet implemented".to_string(),
        })
    }
}
