use crate::error::{error_from_status_code, SdkError};
use crate::types::{Message, Role};

#[derive(serde::Serialize)]
pub struct ApiMessage {
    pub role: String,
    pub content: String,
}

/// Parse an error response body, extracting the message and error code.
/// `error_code_field` is the JSON field name for the error code (e.g. "type" or "status").
#[must_use]
pub fn parse_error_body(
    body: &str,
    error_code_field: &str,
) -> (String, Option<String>, Option<serde_json::Value>) {
    serde_json::from_str::<serde_json::Value>(body).map_or_else(
        |_| (body.to_string(), None, None),
        |v| {
            let message = v
                .get("error")
                .and_then(|e| e.get("message"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Unknown error")
                .to_string();
            let error_code = v
                .get("error")
                .and_then(|e| e.get(error_code_field))
                .and_then(serde_json::Value::as_str)
                .map(String::from);
            (message, error_code, Some(v))
        },
    )
}

/// Send an HTTP request and read the response body, returning an error on non-success status.
///
/// # Errors
///
/// Returns `SdkError::Network` on connection failure or `SdkError::Provider` on non-success status.
pub async fn send_and_read_body(
    request: reqwest::RequestBuilder,
    provider: &str,
    error_code_field: &str,
) -> Result<String, SdkError> {
    let http_resp = request
        .send()
        .await
        .map_err(|e| SdkError::Network {
            message: e.to_string(),
        })?;

    let status = http_resp.status();
    let body = http_resp
        .text()
        .await
        .map_err(|e| SdkError::Network {
            message: e.to_string(),
        })?;

    if !status.is_success() {
        let (msg, code, raw) = parse_error_body(&body, error_code_field);
        return Err(error_from_status_code(
            status.as_u16(),
            msg,
            provider.to_string(),
            code,
            raw,
            None,
        ));
    }

    Ok(body)
}

/// Extract system messages from a message list, returning the joined system prompt
/// and the remaining non-system messages.
#[must_use]
pub fn extract_system_prompt(messages: &[Message]) -> (Option<String>, Vec<&Message>) {
    let mut system_parts = Vec::new();
    let mut other = Vec::new();
    for msg in messages {
        if msg.role == Role::System {
            system_parts.push(msg.text());
        } else {
            other.push(msg);
        }
    }
    let system = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n"))
    };
    (system, other)
}
