mod context;
mod credential;
mod refresh;
mod resolve;
mod strategy;
mod vault_ext;

pub mod strategies;

pub use context::{AuthContextRequest, AuthContextResponse};
pub use credential::{
    ApiKeyHeader, AuthCredential, AuthDetails, OAuthConfig, OAuthTokens, credential_id_for,
    parse_credential_secret,
};
pub use refresh::refresh_oauth_credential;
pub use resolve::{
    ApiCredential, CliAgentKind, CliCredential, CredentialResolver, CredentialUsage, EnvLookup,
    ResolveError, ResolvedCredential,
};
pub use strategy::{
    AuthMethod, AuthStrategy, CODEX_AUTH_URL, CODEX_CLIENT_ID, CODEX_TOKEN_URL, codex_oauth_config,
    strategy_for,
};
pub use vault_ext::{vault_credentials_for_provider, vault_get_credential, vault_set_credential};
