use serde::Deserialize;

const GITHUB_API_BASE_URL: &str = "https://api.github.com";

/// Credentials for authenticating as a GitHub App.
#[derive(Clone, Debug)]
pub struct GitHubAppCredentials {
    pub app_id: String,
    pub private_key_pem: String,
}

/// Parse `owner` and `repo` from a GitHub HTTPS URL.
///
/// Accepts URLs like:
/// - `https://github.com/owner/repo.git`
/// - `https://github.com/owner/repo`
/// - `https://github.com/owner/repo/`
pub fn parse_github_owner_repo(url: &str) -> Result<(String, String), String> {
    let path = url
        .strip_prefix("https://github.com/")
        .ok_or_else(|| format!("Not a GitHub HTTPS URL: {url}"))?;

    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);

    let mut parts = path.splitn(3, '/');
    let owner = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing owner in GitHub URL: {url}"))?;
    let repo = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing repo in GitHub URL: {url}"))?;

    Ok((owner.to_string(), repo.to_string()))
}

/// Create a signed JWT for GitHub App authentication (RS256).
///
/// The JWT is valid for 10 minutes with a 60-second clock skew allowance.
pub fn sign_app_jwt(app_id: &str, private_key_pem: &str) -> Result<String, String> {
    use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
    use serde::Serialize;

    #[derive(Serialize)]
    struct Claims {
        iss: String,
        iat: i64,
        exp: i64,
    }

    let now = chrono::Utc::now().timestamp();
    let claims = Claims {
        iss: app_id.to_string(),
        iat: now - 60,
        exp: now + 600,
    };

    let key = EncodingKey::from_rsa_pem(private_key_pem.as_bytes())
        .map_err(|e| format!("Invalid RSA private key: {e}"))?;

    let jwt = encode(&Header::new(Algorithm::RS256), &claims, &key)
        .map_err(|e| format!("Failed to sign JWT: {e}"))?;
    Ok(jwt)
}

/// Request a scoped Installation Access Token for a specific repository.
///
/// Uses the App JWT to find the installation for `owner/repo`, then requests
/// a token scoped to the given `permissions` on that single repository.
async fn create_installation_access_token_with_permissions(
    client: &reqwest::Client,
    jwt: &str,
    owner: &str,
    repo: &str,
    base_url: &str,
    permissions: serde_json::Value,
) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Installation {
        id: u64,
    }

    #[derive(Deserialize)]
    struct AccessToken {
        token: String,
    }

    // Step 1: Find the installation for this repo
    let install_url = format!("{base_url}/repos/{owner}/{repo}/installation");
    let install_resp = client
        .get(&install_url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "arc")
        .send()
        .await
        .map_err(|e| format!("Failed to look up GitHub App installation: {e}"))?;

    let status = install_resp.status();
    match status.as_u16() {
        200 => {}
        404 => {
            return Err(format!(
                "GitHub App is not installed for {owner}. \
                 Install it at https://github.com/organizations/{owner}/settings/installations"
            ));
        }
        403 => {
            return Err("GitHub App installation is suspended. \
                 Re-enable it in your organization's GitHub App settings."
                .to_string());
        }
        401 => {
            return Err("GitHub App authentication failed. \
                 Check that app_id and GITHUB_APP_PRIVATE_KEY are correct."
                .to_string());
        }
        _ => {
            return Err(format!(
                "Unexpected status {status} looking up GitHub App installation"
            ));
        }
    }

    let installation: Installation = install_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse installation response: {e}"))?;

    // Step 2: Create a scoped access token
    let token_url = format!(
        "{base_url}/app/installations/{}/access_tokens",
        installation.id
    );
    let body = serde_json::json!({
        "repositories": [repo],
        "permissions": permissions,
    });

    let token_resp = client
        .post(&token_url)
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "arc")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to create installation access token: {e}"))?;

    let token_status = token_resp.status();
    match token_status.as_u16() {
        201 => {}
        422 => {
            return Err(format!(
                "GitHub App does not have access to repository {repo}. \
                 Update the installation's repository permissions to include it."
            ));
        }
        401 => {
            return Err("GitHub App authentication failed. \
                 Check that app_id and GITHUB_APP_PRIVATE_KEY are correct."
                .to_string());
        }
        _ => {
            return Err(format!(
                "Unexpected status {token_status} creating installation access token"
            ));
        }
    }

    let access_token: AccessToken = token_resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse access token response: {e}"))?;

    Ok(access_token.token)
}

/// Request a scoped Installation Access Token with `contents: write`.
pub async fn create_installation_access_token(
    client: &reqwest::Client,
    jwt: &str,
    owner: &str,
    repo: &str,
    base_url: &str,
) -> Result<String, String> {
    create_installation_access_token_with_permissions(
        client,
        jwt,
        owner,
        repo,
        base_url,
        serde_json::json!({ "contents": "write" }),
    )
    .await
}

/// Request a scoped Installation Access Token with `contents: write`
/// and `pull_requests: write`. Used for creating pull requests.
pub async fn create_installation_access_token_for_pr(
    client: &reqwest::Client,
    jwt: &str,
    owner: &str,
    repo: &str,
    base_url: &str,
) -> Result<String, String> {
    create_installation_access_token_with_permissions(
        client,
        jwt,
        owner,
        repo,
        base_url,
        serde_json::json!({ "contents": "write", "pull_requests": "write" }),
    )
    .await
}

/// Create a pull request on GitHub.
///
/// Signs a JWT, obtains a PR-scoped installation token, and POSTs to the
/// GitHub pulls API. Returns `(html_url, pr_number)` on success.
#[allow(clippy::too_many_arguments)]
pub async fn create_pull_request(
    creds: &GitHubAppCredentials,
    owner: &str,
    repo: &str,
    base: &str,
    head: &str,
    title: &str,
    body: &str,
    draft: bool,
) -> Result<(String, u64), String> {
    let jwt = sign_app_jwt(&creds.app_id, &creds.private_key_pem)?;
    let client = reqwest::Client::new();

    let token =
        create_installation_access_token_for_pr(&client, &jwt, owner, repo, GITHUB_API_BASE_URL)
            .await?;

    tracing::debug!(title = %title, head = %head, base = %base, draft, "Creating pull request");

    let pr_body = serde_json::json!({
        "title": title,
        "head": head,
        "base": base,
        "body": body,
        "draft": draft,
    });

    let url = format!("{GITHUB_API_BASE_URL}/repos/{owner}/{repo}/pulls");
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "arc")
        .json(&pr_body)
        .send()
        .await
        .map_err(|e| format!("Failed to create pull request: {e}"))?;

    let status = resp.status();
    match status.as_u16() {
        201 => {}
        422 => {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(format!(
                "Pull request could not be created (422): {body_text}"
            ));
        }
        401 | 403 => {
            return Err(format!(
                "Authentication failed creating pull request ({})",
                status
            ));
        }
        _ => {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(format!(
                "Unexpected status {status} creating pull request: {body_text}"
            ));
        }
    }

    #[derive(Deserialize)]
    struct PullRequestResponse {
        html_url: String,
        number: u64,
    }

    let pr: PullRequestResponse = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse pull request response: {e}"))?;

    Ok((pr.html_url, pr.number))
}

/// Convert a Git SSH URL to HTTPS format for token-based authentication.
///
/// SSH URLs like `git@github.com:owner/repo.git` become
/// `https://github.com/owner/repo.git`. URLs that are already HTTPS
/// (or any other non-SSH format) are returned unchanged.
pub fn ssh_url_to_https(url: &str) -> String {
    // Match `git@<host>:<path>` (standard SSH URL format)
    if let Some(rest) = url.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return format!("https://{host}/{path}");
        }
    }
    // Match `ssh://git@<host>/<path>`
    if let Some(rest) = url.strip_prefix("ssh://git@") {
        return format!("https://{rest}");
    }
    url.to_string()
}

/// Check whether a branch exists in a GitHub repository.
///
/// Uses a GitHub App installation token to query the branches API.
/// Returns `true` if the branch exists, `false` if it doesn't (404).
pub async fn branch_exists(
    creds: &GitHubAppCredentials,
    owner: &str,
    repo: &str,
    branch: &str,
    base_url: &str,
) -> Result<bool, String> {
    let jwt = sign_app_jwt(&creds.app_id, &creds.private_key_pem)?;
    let client = reqwest::Client::new();

    let token = create_installation_access_token(&client, &jwt, owner, repo, base_url).await?;

    let url = format!("{base_url}/repos/{owner}/{repo}/branches/{branch}");
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "arc")
        .send()
        .await
        .map_err(|e| format!("Failed to check branch existence: {e}"))?;

    match resp.status().as_u16() {
        200 => Ok(true),
        404 => Ok(false),
        status => Err(format!(
            "Unexpected status {status} checking branch '{branch}'"
        )),
    }
}

/// Resolve git clone credentials for a GitHub repository.
///
/// Returns `(username, password)` for authenticated cloning.
/// Always generates a token regardless of repo visibility, since the token
/// is needed for pushing from the sandbox.
pub async fn resolve_clone_credentials(
    creds: &GitHubAppCredentials,
    owner: &str,
    repo: &str,
) -> Result<(Option<String>, Option<String>), String> {
    let jwt = sign_app_jwt(&creds.app_id, &creds.private_key_pem)?;
    let client = reqwest::Client::new();

    let token =
        create_installation_access_token(&client, &jwt, owner, repo, GITHUB_API_BASE_URL).await?;
    Ok((Some("x-access-token".to_string()), Some(token)))
}

/// Embed a token into an HTTPS URL for authenticated git operations.
///
/// Converts `https://github.com/owner/repo` to
/// `https://x-access-token:<token>@github.com/owner/repo`.
pub fn embed_token_in_url(url: &str, token: &str) -> String {
    url.replacen("https://", &format!("https://x-access-token:{token}@"), 1)
}

/// Resolve an authenticated HTTPS URL for a GitHub repository.
///
/// Parses owner/repo from the URL, obtains a fresh installation access token,
/// and returns the URL with embedded credentials. Returns the original URL
/// unchanged if it's not a GitHub URL.
pub async fn resolve_authenticated_url(
    creds: &GitHubAppCredentials,
    url: &str,
) -> Result<String, String> {
    let (owner, repo) = parse_github_owner_repo(url)?;
    let (_username, password) = resolve_clone_credentials(creds, &owner, &repo).await?;
    match password {
        Some(token) => Ok(embed_token_in_url(url, &token)),
        None => Ok(url.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // parse_github_owner_repo
    // -----------------------------------------------------------------------

    #[test]
    fn parse_https_with_git_suffix() {
        let (owner, repo) = parse_github_owner_repo("https://github.com/owner/repo.git").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parse_https_without_git_suffix() {
        let (owner, repo) = parse_github_owner_repo("https://github.com/owner/repo").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parse_https_with_trailing_slash() {
        let (owner, repo) = parse_github_owner_repo("https://github.com/owner/repo/").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    // -----------------------------------------------------------------------
    // ssh_url_to_https
    // -----------------------------------------------------------------------

    #[test]
    fn ssh_url_to_https_converts_git_at_syntax() {
        assert_eq!(
            ssh_url_to_https("git@github.com:brynary/arc.git"),
            "https://github.com/brynary/arc.git"
        );
    }

    #[test]
    fn ssh_url_to_https_converts_ssh_protocol() {
        assert_eq!(
            ssh_url_to_https("ssh://git@github.com/brynary/arc.git"),
            "https://github.com/brynary/arc.git"
        );
    }

    #[test]
    fn ssh_url_to_https_passes_through_https() {
        assert_eq!(
            ssh_url_to_https("https://github.com/brynary/arc.git"),
            "https://github.com/brynary/arc.git"
        );
    }

    #[test]
    fn parse_non_github_url_errors() {
        let result = parse_github_owner_repo("https://gitlab.com/owner/repo");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Not a GitHub HTTPS URL"));
    }

    #[test]
    fn parse_missing_repo_errors() {
        let result = parse_github_owner_repo("https://github.com/owner");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing repo"));
    }

    #[test]
    fn parse_empty_string_errors() {
        let result = parse_github_owner_repo("");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // sign_app_jwt
    // -----------------------------------------------------------------------

    fn test_rsa_key() -> String {
        use std::process::Command;
        let output = Command::new("openssl")
            .args([
                "genpkey",
                "-algorithm",
                "RSA",
                "-pkeyopt",
                "rsa_keygen_bits:2048",
            ])
            .output()
            .expect("openssl should be available");
        assert!(output.status.success(), "openssl keygen failed");
        String::from_utf8(output.stdout).unwrap()
    }

    #[test]
    fn jwt_is_three_part_string() {
        let pem = test_rsa_key();
        let jwt = sign_app_jwt("12345", &pem).unwrap();
        assert_eq!(jwt.split('.').count(), 3);
    }

    #[test]
    fn jwt_has_rs256_header() {
        let pem = test_rsa_key();
        let jwt = sign_app_jwt("12345", &pem).unwrap();
        let header_b64 = jwt.split('.').next().unwrap();
        let header_json = base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            header_b64,
        )
        .unwrap();
        let header: serde_json::Value = serde_json::from_slice(&header_json).unwrap();
        assert_eq!(header["alg"], "RS256");
    }

    #[test]
    fn jwt_has_correct_claims() {
        let pem = test_rsa_key();
        let jwt = sign_app_jwt("99999", &pem).unwrap();
        let payload_b64 = jwt.split('.').nth(1).unwrap();
        let payload_json = base64::Engine::decode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            payload_b64,
        )
        .unwrap();
        let claims: serde_json::Value = serde_json::from_slice(&payload_json).unwrap();
        assert_eq!(claims["iss"], "99999");

        let now = chrono::Utc::now().timestamp();
        let iat = claims["iat"].as_i64().unwrap();
        let exp = claims["exp"].as_i64().unwrap();
        // iat should be ~60s before now
        assert!((now - 60 - iat).abs() < 5);
        // exp should be ~10min after now
        assert!((now + 600 - exp).abs() < 5);
    }

    #[test]
    fn jwt_invalid_pem_errors() {
        let result = sign_app_jwt("12345", "not-a-pem");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid RSA private key"));
    }

    // -----------------------------------------------------------------------
    // create_installation_access_token — success
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn create_iat_success() {
        let mut server = mockito::Server::new_async().await;

        let install_mock = server
            .mock("GET", "/repos/owner/repo/installation")
            .match_header("Authorization", "Bearer test-jwt")
            .with_status(200)
            .with_body(r#"{"id": 123}"#)
            .create_async()
            .await;

        let token_mock = server
            .mock("POST", "/app/installations/123/access_tokens")
            .match_header("Authorization", "Bearer test-jwt")
            .match_body(mockito::Matcher::JsonString(
                r#"{"repositories":["repo"],"permissions":{"contents":"write"}}"#.to_string(),
            ))
            .with_status(201)
            .with_body(r#"{"token": "ghs_xxx"}"#)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let token =
            create_installation_access_token(&client, "test-jwt", "owner", "repo", &server.url())
                .await
                .unwrap();
        assert_eq!(token, "ghs_xxx");

        install_mock.assert_async().await;
        token_mock.assert_async().await;
    }

    // -----------------------------------------------------------------------
    // create_installation_access_token — failure modes
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn create_iat_not_installed() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/repos/owner/repo/installation")
            .with_status(404)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let err = create_installation_access_token(&client, "jwt", "owner", "repo", &server.url())
            .await
            .unwrap_err();
        assert!(err.contains("not installed"), "got: {err}");
        assert!(err.contains("owner"), "got: {err}");
    }

    #[tokio::test]
    async fn create_iat_suspended() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/repos/owner/repo/installation")
            .with_status(403)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let err = create_installation_access_token(&client, "jwt", "owner", "repo", &server.url())
            .await
            .unwrap_err();
        assert!(err.contains("suspended"), "got: {err}");
    }

    #[tokio::test]
    async fn create_iat_no_repo_access() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/repos/owner/repo/installation")
            .with_status(200)
            .with_body(r#"{"id": 123}"#)
            .create_async()
            .await;
        server
            .mock("POST", "/app/installations/123/access_tokens")
            .with_status(422)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let err = create_installation_access_token(&client, "jwt", "owner", "repo", &server.url())
            .await
            .unwrap_err();
        assert!(err.contains("does not have access"), "got: {err}");
        assert!(err.contains("repo"), "got: {err}");
    }

    #[tokio::test]
    async fn create_iat_auth_failed() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/repos/owner/repo/installation")
            .with_status(401)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let err = create_installation_access_token(&client, "jwt", "owner", "repo", &server.url())
            .await
            .unwrap_err();
        assert!(err.contains("authentication failed"), "got: {err}");
    }

    // -----------------------------------------------------------------------
    // create_installation_access_token_for_pr
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn create_iat_for_pr_requests_pr_permissions() {
        let mut server = mockito::Server::new_async().await;

        server
            .mock("GET", "/repos/owner/repo/installation")
            .match_header("Authorization", "Bearer test-jwt")
            .with_status(200)
            .with_body(r#"{"id": 456}"#)
            .create_async()
            .await;

        let token_mock = server
            .mock("POST", "/app/installations/456/access_tokens")
            .match_header("Authorization", "Bearer test-jwt")
            .match_body(mockito::Matcher::JsonString(
                r#"{"repositories":["repo"],"permissions":{"contents":"write","pull_requests":"write"}}"#.to_string(),
            ))
            .with_status(201)
            .with_body(r#"{"token": "ghs_pr_token"}"#)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let token = create_installation_access_token_for_pr(
            &client,
            "test-jwt",
            "owner",
            "repo",
            &server.url(),
        )
        .await
        .unwrap();
        assert_eq!(token, "ghs_pr_token");

        token_mock.assert_async().await;
    }

    // -----------------------------------------------------------------------
    // branch_exists
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn branch_exists_returns_true_on_200() {
        let mut server = mockito::Server::new_async().await;

        server
            .mock("GET", "/repos/owner/repo/installation")
            .with_status(200)
            .with_body(r#"{"id": 1}"#)
            .create_async()
            .await;
        server
            .mock("POST", "/app/installations/1/access_tokens")
            .with_status(201)
            .with_body(r#"{"token": "ghs_test"}"#)
            .create_async()
            .await;
        server
            .mock("GET", "/repos/owner/repo/branches/my-branch")
            .with_status(200)
            .with_body(r#"{"name": "my-branch"}"#)
            .create_async()
            .await;

        let pem = test_rsa_key();
        let creds = GitHubAppCredentials {
            app_id: "test".to_string(),
            private_key_pem: pem,
        };
        let result = branch_exists(&creds, "owner", "repo", "my-branch", &server.url()).await;
        assert_eq!(result.unwrap(), true);
    }

    #[tokio::test]
    async fn branch_exists_returns_false_on_404() {
        let mut server = mockito::Server::new_async().await;

        server
            .mock("GET", "/repos/owner/repo/installation")
            .with_status(200)
            .with_body(r#"{"id": 1}"#)
            .create_async()
            .await;
        server
            .mock("POST", "/app/installations/1/access_tokens")
            .with_status(201)
            .with_body(r#"{"token": "ghs_test"}"#)
            .create_async()
            .await;
        server
            .mock("GET", "/repos/owner/repo/branches/no-such-branch")
            .with_status(404)
            .create_async()
            .await;

        let pem = test_rsa_key();
        let creds = GitHubAppCredentials {
            app_id: "test".to_string(),
            private_key_pem: pem,
        };
        let result = branch_exists(&creds, "owner", "repo", "no-such-branch", &server.url()).await;
        assert_eq!(result.unwrap(), false);
    }

    #[tokio::test]
    async fn branch_exists_returns_error_on_500() {
        let mut server = mockito::Server::new_async().await;

        server
            .mock("GET", "/repos/owner/repo/installation")
            .with_status(200)
            .with_body(r#"{"id": 1}"#)
            .create_async()
            .await;
        server
            .mock("POST", "/app/installations/1/access_tokens")
            .with_status(201)
            .with_body(r#"{"token": "ghs_test"}"#)
            .create_async()
            .await;
        server
            .mock("GET", "/repos/owner/repo/branches/broken")
            .with_status(500)
            .create_async()
            .await;

        let pem = test_rsa_key();
        let creds = GitHubAppCredentials {
            app_id: "test".to_string(),
            private_key_pem: pem,
        };
        let result = branch_exists(&creds, "owner", "repo", "broken", &server.url()).await;
        assert!(result.is_err());
    }
}
