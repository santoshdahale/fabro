/// Compute a hashed repository identifier from the git origin remote URL.
///
/// Returns an MD5 hex digest of the normalized remote URL, or a sentinel
/// string if the repository, remote, or URL cannot be determined.
pub fn repository_identifier() -> String {
    match try_repository_identifier() {
        Ok(hash) => hash,
        Err(sentinel) => sentinel,
    }
}

fn try_repository_identifier() -> Result<String, String> {
    let repo = git2::Repository::discover(".").map_err(|_| "no_repo".to_string())?;
    let remote = repo
        .find_remote("origin")
        .map_err(|_| "no_remote".to_string())?;
    let url = remote.url().ok_or_else(|| "no_url".to_string())?;

    let normalized = normalize_remote_url(url);
    let digest = md5::compute(normalized.as_bytes());
    Ok(format!("{digest:x}"))
}

/// Normalize a git remote URL to a canonical form so that HTTPS and SSH
/// variants of the same repository produce the same hash.
///
/// Strips protocol prefixes (`https://`, `ssh://`, `git://`), user prefixes
/// (`user@`), and `.git` suffixes, then replaces `:` with `/`.
fn normalize_remote_url(url: &str) -> String {
    let mut s = url.to_string();

    // Strip protocol
    for prefix in &["https://", "ssh://", "git://", "http://"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.to_string();
            break;
        }
    }

    // Strip user@ prefix
    if let Some(at_pos) = s.find('@') {
        // Only strip if '@' comes before the first '/' or ':'
        let slash_pos = s.find('/').unwrap_or(usize::MAX);
        let colon_pos = s.find(':').unwrap_or(usize::MAX);
        if at_pos < slash_pos && at_pos < colon_pos {
            s = s[at_pos + 1..].to_string();
        }
    }

    // Replace ':' with '/' (for git@host:user/repo style)
    s = s.replace(':', "/");

    // Strip .git suffix
    if let Some(stripped) = s.strip_suffix(".git") {
        s = stripped.to_string();
    }

    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn https_and_ssh_normalize_to_same() {
        let https = normalize_remote_url("https://github.com/fabro-sh/fabro.git");
        let ssh = normalize_remote_url("git@github.com:fabro-sh/fabro.git");
        assert_eq!(https, ssh);
        insta::assert_snapshot!(https, @"github.com/fabro-sh/fabro");
    }

    #[test]
    fn ssh_protocol_prefix() {
        let result = normalize_remote_url("ssh://git@github.com/fabro-sh/fabro.git");
        insta::assert_snapshot!(result, @"github.com/fabro-sh/fabro");
    }

    #[test]
    fn url_without_git_suffix() {
        let result = normalize_remote_url("https://github.com/fabro-sh/fabro");
        insta::assert_snapshot!(result, @"github.com/fabro-sh/fabro");
    }

    #[test]
    fn repository_identifier_returns_hash_in_arc_repo() {
        // We're running inside the fabro repo, so this should return a real hash
        let id = repository_identifier();
        assert_ne!(id, "no_repo");
        assert_ne!(id, "no_remote");
        assert_ne!(id, "no_url");
        // MD5 hex is 32 chars
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
