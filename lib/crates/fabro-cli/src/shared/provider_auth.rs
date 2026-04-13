use std::sync::Arc;

use anyhow::Result;
use dialoguer::console::Term;
use dialoguer::theme::ColorfulTheme;
use dialoguer::{Confirm, Password};
use fabro_auth::{
    ApiCredential, ApiKeyHeader, AuthContextRequest, AuthContextResponse, AuthCredential,
    AuthMethod, codex_oauth_config, strategy_for,
};
use fabro_llm::client::Client as LlmClient;
use fabro_llm::generate::{GenerateParams, generate};
use fabro_model::{Catalog, Provider};
use fabro_util::printer::Printer;
use fabro_util::terminal::Styles;
use tokio::task::spawn_blocking;
use tokio::time::timeout;

// ---------------------------------------------------------------------------
// Provider key URLs
// ---------------------------------------------------------------------------

pub(crate) fn provider_key_url(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "https://console.anthropic.com/settings/keys",
        Provider::OpenAi => "https://platform.openai.com/api-keys",
        Provider::Gemini => "https://aistudio.google.com/apikey",
        Provider::Kimi => "https://platform.moonshot.cn/console/api-keys",
        Provider::Zai => "https://open.bigmodel.cn/usercenter/apikeys",
        Provider::Minimax => {
            "https://platform.minimaxi.com/user-center/basic-information/interface-key"
        }
        Provider::Inception => "https://console.inceptionlabs.ai/api-keys",
        Provider::OpenAiCompatible => "",
    }
}

pub(crate) fn provider_display_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Anthropic => "Anthropic",
        Provider::OpenAi => "OpenAI",
        Provider::Gemini => "Gemini",
        Provider::Kimi => "Kimi",
        Provider::Zai => "Zai",
        Provider::Minimax => "Minimax",
        Provider::Inception => "Inception",
        Provider::OpenAiCompatible => "OpenAI Compatible",
    }
}

// ---------------------------------------------------------------------------
// Interactive prompts
// ---------------------------------------------------------------------------

pub(crate) fn prompt_confirm(prompt: &str, default: bool) -> Result<bool> {
    Ok(Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(default)
        .interact_on(&Term::stderr())?)
}

pub(crate) fn prompt_password(prompt: &str) -> Result<String> {
    Ok(Password::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .interact_on(&Term::stderr())?)
}

// ---------------------------------------------------------------------------
// API key validation
// ---------------------------------------------------------------------------

pub(crate) async fn validate_api_key(provider: Provider, api_key: &str) -> Result<(), String> {
    let auth_header = if provider == Provider::Anthropic {
        ApiKeyHeader::Custom {
            name:  "x-api-key".to_string(),
            value: api_key.to_string(),
        }
    } else {
        ApiKeyHeader::Bearer(api_key.to_string())
    };
    let client = LlmClient::from_credentials(vec![ApiCredential {
        provider,
        auth_header,
        extra_headers: std::collections::HashMap::new(),
        base_url: None,
        codex_mode: false,
        org_id: None,
        project_id: None,
    }])
    .await
    .map_err(|e| e.to_string())?;

    let probe_model = Catalog::builtin().probe_for_provider(provider).map_or_else(
        || format!("unknown-{}", provider.as_str()),
        |model| model.id.clone(),
    );

    let params = GenerateParams::new(probe_model)
        .provider(provider.as_str())
        .prompt("Say OK")
        .max_tokens(16)
        .client(Arc::new(client));

    timeout(std::time::Duration::from_secs(30), generate(params))
        .await
        .map_err(|_| "timeout (30s)".to_string())?
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub(crate) async fn prompt_and_validate_key(
    provider: Provider,
    s: &Styles,
    printer: Printer,
) -> Result<(String, String)> {
    let env_var = provider.api_key_env_vars()[0];
    let url = provider_key_url(provider);
    fabro_util::printerr!(
        printer,
        "  {}",
        s.dim.apply_to(format!("Get your API key at: {url}"))
    );

    loop {
        let prompt = env_var.to_string();
        let key: String = spawn_blocking(move || prompt_password(&prompt)).await??;

        fabro_util::printerr!(printer, "  {}", s.dim.apply_to("Validating API key..."));
        match validate_api_key(provider, &key).await {
            Ok(()) => {
                fabro_util::printerr!(printer, "  {} API key is valid", s.green.apply_to("✔"));
                return Ok((env_var.to_string(), key));
            }
            Err(e) => {
                fabro_util::printerr!(printer, "  [error] API key validation failed: {e}");
                let retry =
                    spawn_blocking(|| prompt_confirm("Try again with a different key?", true))
                        .await??;
                if !retry {
                    return Ok((env_var.to_string(), key));
                }
            }
        }
    }
}

pub(crate) async fn pick_auth_method(provider: Provider) -> Result<AuthMethod> {
    if provider != Provider::OpenAi {
        return Ok(AuthMethod::ApiKey);
    }

    let use_device_auth =
        spawn_blocking(|| prompt_confirm("Log in with OpenAI account (device code)?", true))
            .await??;
    if use_device_auth {
        Ok(AuthMethod::CodexDevice(codex_oauth_config()))
    } else {
        Ok(AuthMethod::ApiKey)
    }
}

pub(crate) async fn authenticate_provider(
    provider: Provider,
    s: &Styles,
    printer: Printer,
) -> Result<AuthCredential> {
    let method = pick_auth_method(provider).await?;
    authenticate_provider_with_method(provider, method, s, printer).await
}

pub(crate) async fn authenticate_provider_with_method(
    provider: Provider,
    method: AuthMethod,
    s: &Styles,
    printer: Printer,
) -> Result<AuthCredential> {
    let mut strategy = strategy_for(provider, method);
    let request = strategy.init().await?;
    present_to_user(&request, s, printer)?;
    let response = await_user_response(&request, s, printer).await?;
    strategy.complete(response).await
}

pub(crate) fn present_to_user(
    request: &AuthContextRequest,
    s: &Styles,
    printer: Printer,
) -> Result<()> {
    match request {
        AuthContextRequest::ApiKey {
            provider,
            env_var_names,
        } => {
            let env_var = env_var_names
                .first()
                .map(String::as_str)
                .unwrap_or("API_KEY");
            let url = provider_key_url(*provider);
            fabro_util::printerr!(
                printer,
                "  {}",
                s.dim.apply_to(format!("Get your API key at: {url}"))
            );
            fabro_util::printerr!(
                printer,
                "  {}",
                s.dim.apply_to(format!("Expected variable name: {env_var}"))
            );
        }
        AuthContextRequest::DeviceCode {
            user_code,
            verification_uri,
            expires_in,
        } => {
            fabro_util::printerr!(printer, "  Open this URL in your browser:");
            fabro_util::printerr!(printer, "    {verification_uri}");
            fabro_util::printerr!(printer, "  Enter this one-time code:");
            fabro_util::printerr!(printer, "    {}", s.bold.apply_to(user_code));
            fabro_util::printerr!(
                printer,
                "  {}",
                s.dim
                    .apply_to(format!("Code expires in {} minutes", expires_in / 60))
            );
        }
    }
    Ok(())
}

pub(crate) async fn await_user_response(
    request: &AuthContextRequest,
    s: &Styles,
    printer: Printer,
) -> Result<AuthContextResponse> {
    match request {
        AuthContextRequest::ApiKey { provider, .. } => {
            let (_, key) = prompt_and_validate_key(*provider, s, printer).await?;
            Ok(AuthContextResponse::ApiKey { key })
        }
        AuthContextRequest::DeviceCode { .. } => {
            let ready = spawn_blocking(|| {
                prompt_confirm("Continue after completing sign-in in the browser?", true)
            })
            .await??;
            if !ready {
                return Err(anyhow::anyhow!("device code login cancelled"));
            }
            Ok(AuthContextResponse::DeviceCodeConfirmed)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Provider key URLs --

    #[test]
    fn every_provider_has_key_url() {
        for provider in Provider::ALL {
            let url = provider_key_url(*provider);
            assert!(!url.is_empty(), "{provider:?} has empty URL");
            assert!(url.starts_with("https://"), "{provider:?} URL: {url}");
        }
    }

    // -- API key validation --

    #[fabro_macros::e2e_test(live("ANTHROPIC_API_KEY"))]
    async fn validate_api_key_rejects_invalid_key() {
        let result = validate_api_key(Provider::Anthropic, "sk-invalid-key-12345").await;
        assert!(result.is_err(), "expected invalid key to be rejected");
    }
}
