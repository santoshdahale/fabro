use anyhow::Result;
use fabro_api::types;
use fabro_auth::credential_id_for;
use fabro_config::legacy_env;
use fabro_util::printer::Printer;
use fabro_util::terminal::Styles;

use crate::args::{GlobalArgs, ProviderLoginArgs};
use crate::command_context::CommandContext;
use crate::shared::provider_auth;

pub(super) async fn login_command(
    args: ProviderLoginArgs,
    globals: &GlobalArgs,
    printer: Printer,
) -> Result<()> {
    globals.require_no_json()?;
    let s = Styles::detect_stderr();
    let ctx = CommandContext::for_target(&args.target, printer)?;
    let server = ctx.server().await?;
    let credential = provider_auth::authenticate_provider(args.provider, &s, printer).await?;
    let credential_id = credential_id_for(&credential).map_err(anyhow::Error::msg)?;
    let value = serde_json::to_string(&credential)?;

    {
        let path = legacy_env::legacy_env_file_path();
        if path.exists() {
            fabro_util::printerr!(
                printer,
                "  Warning: {} is no longer read by fabro server. Re-enter credentials with `fabro provider login`.",
                path.display()
            );
        }
    }

    server
        .api()
        .create_secret()
        .body(types::CreateSecretRequest {
            name: credential_id.clone(),
            value,
            type_: types::SecretType::Credential,
            description: None,
        })
        .send()
        .await?;
    fabro_util::printerr!(
        printer,
        "  {} Saved {}",
        s.green.apply_to("✔"),
        credential_id
    );
    Ok(())
}
