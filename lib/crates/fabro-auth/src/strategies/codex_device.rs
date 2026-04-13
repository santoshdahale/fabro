use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use fabro_http::HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::time::sleep;

use crate::context::{AuthContextRequest, AuthContextResponse};
use crate::credential::{AuthCredential, AuthDetails, OAuthConfig, OAuthTokens};
use crate::strategy::AuthStrategy;

const DEVICE_AUTH_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const DEVICE_AUTH_POLL_INTERVAL: Duration = Duration::from_secs(2);

fn http_client() -> anyhow::Result<HttpClient> {
    #[cfg(test)]
    {
        fabro_http::test_http_client().map_err(anyhow::Error::from)
    }
    #[cfg(not(test))]
    {
        fabro_http::http_client().map_err(anyhow::Error::from)
    }
}

fn join_url(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn expires_at_from_now(expires_in: Option<u64>) -> chrono::DateTime<chrono::Utc> {
    let seconds = i64::try_from(expires_in.unwrap_or(3600)).unwrap_or(i64::MAX);
    chrono::Utc::now() + chrono::Duration::seconds(seconds)
}

#[derive(Debug, Deserialize)]
struct JwtPayload {
    #[serde(default)]
    chatgpt_account_id: Option<String>,
    #[serde(default, rename = "https://api.openai.com/auth")]
    auth_claim:         Option<AuthClaim>,
    #[serde(default)]
    organizations:      Option<Vec<Organization>>,
}

#[derive(Debug, Deserialize)]
struct AuthClaim {
    #[serde(default)]
    chatgpt_account_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Organization {
    #[serde(default)]
    id: Option<String>,
}

fn parse_jwt_payload(token: &str) -> Option<JwtPayload> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1]).ok()?;
    serde_json::from_slice(&payload_bytes).ok()
}

pub fn extract_chatgpt_account_id(id_token: &str) -> Option<String> {
    let payload = parse_jwt_payload(id_token)?;
    payload
        .chatgpt_account_id
        .or_else(|| {
            payload
                .auth_claim
                .and_then(|claim| claim.chatgpt_account_id)
        })
        .or_else(|| {
            payload
                .organizations
                .and_then(|orgs| orgs.into_iter().next())
                .and_then(|org| org.id)
        })
}

#[derive(Debug, Deserialize)]
struct DeviceCodeInitResponse {
    device_auth_id:   String,
    user_code:        String,
    #[serde(alias = "verificationUrl", alias = "verification_uri")]
    verification_uri: String,
    #[serde(default)]
    expires_in:       Option<u64>,
}

#[derive(Debug, Deserialize)]
struct DeviceCodePollResponse {
    #[serde(default)]
    status:             Option<String>,
    #[serde(default)]
    authorization_code: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeviceCodeInitRequest<'a> {
    client_id:      &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    code_challenge: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scope:          Option<String>,
}

pub struct CodexDeviceStrategy {
    config:         OAuthConfig,
    device_auth_id: Option<String>,
    code_verifier:  Option<String>,
}

impl CodexDeviceStrategy {
    #[must_use]
    pub fn new(config: OAuthConfig) -> Self {
        Self {
            config,
            device_auth_id: None,
            code_verifier: None,
        }
    }

    async fn poll_codex_device(&self, device_auth_id: &str) -> anyhow::Result<String> {
        let client = http_client()?;
        let deadline = Instant::now() + DEVICE_AUTH_TIMEOUT;
        let url = join_url(&self.config.auth_url, "/api/accounts/deviceauth/token");

        loop {
            if Instant::now() >= deadline {
                return Err(anyhow::anyhow!("device auth timed out after 15 minutes"));
            }

            let response = client
                .post(&url)
                .json(&json!({ "device_auth_id": device_auth_id }))
                .send()
                .await?;
            let status = response.status();
            if !status.is_success() {
                let body = response.text().await.unwrap_or_default();
                return Err(anyhow::anyhow!(
                    "device auth failed with status {status}: {body}"
                ));
            }

            let payload: DeviceCodePollResponse = response.json().await?;
            if let Some(code) = payload.authorization_code {
                return Ok(code);
            }

            match payload.status.as_deref() {
                Some("pending") | Some("running") | None => {
                    sleep(DEVICE_AUTH_POLL_INTERVAL).await;
                }
                Some(other) => {
                    return Err(anyhow::anyhow!("device code exchange failed: {other}"));
                }
            }
        }
    }
}

#[async_trait]
impl AuthStrategy for CodexDeviceStrategy {
    async fn init(&mut self) -> anyhow::Result<AuthContextRequest> {
        let pkce = self.config.use_pkce.then(fabro_oauth::generate_pkce);
        self.code_verifier = pkce.as_ref().map(|codes| codes.verifier.clone());

        let client = http_client()?;
        let url = join_url(&self.config.auth_url, "/api/accounts/deviceauth/usercode");
        let response = client
            .post(&url)
            .json(&DeviceCodeInitRequest {
                client_id:      &self.config.client_id,
                code_challenge: pkce.as_ref().map(|codes| codes.challenge.as_str()),
                scope:          (!self.config.scopes.is_empty())
                    .then(|| self.config.scopes.join(" ")),
            })
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "device code request failed with status {status}: {body}"
            ));
        }

        let payload: DeviceCodeInitResponse = response.json().await?;
        self.device_auth_id = Some(payload.device_auth_id);

        Ok(AuthContextRequest::DeviceCode {
            user_code:        payload.user_code,
            verification_uri: payload.verification_uri,
            expires_in:       payload.expires_in.unwrap_or(900),
        })
    }

    async fn complete(&mut self, response: AuthContextResponse) -> anyhow::Result<AuthCredential> {
        match response {
            AuthContextResponse::ApiKey { .. } => Err(anyhow::anyhow!(
                "expected device code confirmation response"
            )),
            AuthContextResponse::DeviceCodeConfirmed => {
                let device_auth_id = self
                    .device_auth_id
                    .take()
                    .ok_or_else(|| anyhow::anyhow!("device auth flow was not initialized"))?;
                let authorization_code = self.poll_codex_device(&device_auth_id).await?;
                let token_response = fabro_oauth::exchange_code(
                    fabro_oauth::OAuthEndpoint {
                        token_url: &self.config.token_url,
                        client_id: &self.config.client_id,
                    },
                    &authorization_code,
                    self.config.redirect_uri.as_deref(),
                    self.code_verifier.as_deref(),
                )
                .await
                .map_err(anyhow::Error::msg)?;

                Ok(AuthCredential {
                    provider: fabro_model::Provider::OpenAi,
                    details:  AuthDetails::CodexOAuth {
                        tokens:     OAuthTokens {
                            access_token:  token_response.access_token,
                            refresh_token: token_response.refresh_token,
                            expires_at:    expires_at_from_now(token_response.expires_in),
                        },
                        config:     self.config.clone(),
                        account_id: token_response
                            .id_token
                            .as_deref()
                            .and_then(extract_chatgpt_account_id),
                    },
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_jwt(claims: &serde_json::Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_string(claims).unwrap());
        format!("{header}.{payload}.signature")
    }

    #[test]
    fn extract_chatgpt_account_id_prefers_top_level_claim() {
        let jwt = make_test_jwt(&json!({
            "chatgpt_account_id": "top_level",
            "https://api.openai.com/auth": { "chatgpt_account_id": "nested" },
            "organizations": [{ "id": "org_123" }]
        }));
        assert_eq!(
            extract_chatgpt_account_id(&jwt).as_deref(),
            Some("top_level")
        );
    }

    #[test]
    fn extract_chatgpt_account_id_falls_back_to_nested_claim() {
        let jwt = make_test_jwt(&json!({
            "https://api.openai.com/auth": { "chatgpt_account_id": "nested" }
        }));
        assert_eq!(extract_chatgpt_account_id(&jwt).as_deref(), Some("nested"));
    }

    #[test]
    fn extract_chatgpt_account_id_falls_back_to_organization() {
        let jwt = make_test_jwt(&json!({
            "organizations": [{ "id": "org_123" }]
        }));
        assert_eq!(extract_chatgpt_account_id(&jwt).as_deref(), Some("org_123"));
    }
}
