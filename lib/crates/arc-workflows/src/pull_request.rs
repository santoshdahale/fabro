use std::path::Path;

use serde::Serialize;
use tracing::{debug, info};

use arc_github::{self as github_app, ssh_url_to_https, GitHubAppCredentials};

/// Record of a pull request created for a workflow run.
#[derive(Debug, Clone, Serialize)]
pub struct PullRequestRecord {
    pub html_url: String,
    pub number: u64,
    pub owner: String,
    pub repo: String,
    pub base_branch: String,
    pub head_branch: String,
    pub title: String,
}

impl PullRequestRecord {
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize pull_request.json: {e}"))?;
        std::fs::write(path, json).map_err(|e| format!("Failed to write pull_request.json: {e}"))
    }
}

/// Derive a PR title from the workflow goal.
///
/// Uses the first line, truncated to 120 characters for readability.
fn pr_title_from_goal(goal: &str) -> String {
    let first_line = goal.lines().next().unwrap_or(goal);
    let first_line = first_line
        .strip_prefix("## ")
        .or_else(|| first_line.strip_prefix("# "))
        .unwrap_or(first_line);
    if first_line.chars().count() > 120 {
        let truncated: String = first_line.chars().take(119).collect();
        format!("{truncated}…")
    } else {
        first_line.to_string()
    }
}

/// Truncate a PR body to fit GitHub's 65,536 character limit.
fn truncate_pr_body(body: &str) -> String {
    const MAX_BODY: usize = 65_536;
    const SUFFIX: &str = "\n\n_(truncated)_";
    if body.len() <= MAX_BODY {
        return body.to_string();
    }
    let cutoff = MAX_BODY - SUFFIX.len();
    format!("{}{SUFFIX}", &body[..cutoff])
}

/// Generate a PR body from the diff and goal using an LLM.
pub async fn generate_pr_body(diff: &str, goal: &str, model: &str) -> Result<String, String> {
    let system = "Write a concise PR description summarizing the changes.".to_string();

    // Truncate diff to fit context windows (~50k chars)
    let max_diff_len = 50_000;
    let truncated_diff = if diff.len() > max_diff_len {
        &diff[..max_diff_len]
    } else {
        diff
    };

    let prompt = format!("Goal: {goal}\n\nDiff:\n```\n{truncated_diff}\n```");

    let params = arc_llm::generate::GenerateParams::new(model)
        .system(system)
        .prompt(prompt);

    let result = arc_llm::generate::generate(params)
        .await
        .map_err(|e| format!("LLM generation failed: {e}"))?;

    Ok(result.response.text())
}

/// Optionally open a pull request after a successful workflow run.
///
/// Returns `Ok(Some(PullRequestRecord))` if a PR was created, `Ok(None)` if
/// the diff was empty, or `Err` on failure.
#[allow(clippy::too_many_arguments)]
pub async fn maybe_open_pull_request(
    creds: &GitHubAppCredentials,
    origin_url: &str,
    base_branch: &str,
    head_branch: &str,
    goal: &str,
    diff: &str,
    model: &str,
    draft: bool,
) -> Result<Option<PullRequestRecord>, String> {
    if diff.is_empty() {
        debug!("Empty diff, skipping pull request creation");
        return Ok(None);
    }

    let https_url = ssh_url_to_https(origin_url);
    let (owner, repo) = github_app::parse_github_owner_repo(&https_url)?;

    let body = generate_pr_body(diff, goal, model).await?;
    let body = truncate_pr_body(&body);

    let title = pr_title_from_goal(goal);

    let (html_url, number) = github_app::create_pull_request(
        creds,
        &owner,
        &repo,
        base_branch,
        head_branch,
        &title,
        &body,
        draft,
    )
    .await?;

    info!(pr_url = %html_url, number, "Pull request created");

    Ok(Some(PullRequestRecord {
        html_url,
        number,
        owner,
        repo,
        base_branch: base_branch.to_string(),
        head_branch: head_branch.to_string(),
        title,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pr_title_uses_first_line() {
        let goal = "Add Draft PR Mode\n\nMore details here...";
        assert_eq!(pr_title_from_goal(goal), "Add Draft PR Mode");
    }

    #[test]
    fn pr_title_strips_h1_prefix() {
        assert_eq!(
            pr_title_from_goal("# Add Draft PR Mode"),
            "Add Draft PR Mode"
        );
    }

    #[test]
    fn pr_title_strips_h2_prefix() {
        assert_eq!(
            pr_title_from_goal("## Add Draft PR Mode"),
            "Add Draft PR Mode"
        );
    }

    #[test]
    fn pr_title_does_not_strip_h3_prefix() {
        assert_eq!(
            pr_title_from_goal("### Add Draft PR Mode"),
            "### Add Draft PR Mode"
        );
    }

    #[test]
    fn pr_title_truncates_long_line() {
        let long = "x".repeat(300);
        let title = pr_title_from_goal(&long);
        assert_eq!(title.chars().count(), 120);
        assert!(title.ends_with('…'));
    }

    #[test]
    fn pr_body_truncates_long_body() {
        let long = "x".repeat(70_000);
        let body = truncate_pr_body(&long);
        assert!(body.len() <= 65_536);
        assert!(body.ends_with("\n\n_(truncated)_"));
    }

    #[test]
    fn pr_body_short_body_unchanged() {
        let short = "Some PR description";
        assert_eq!(truncate_pr_body(short), short);
    }

    #[test]
    fn pr_title_short_goal_unchanged() {
        assert_eq!(pr_title_from_goal("Fix bug"), "Fix bug");
    }

    #[test]
    fn pull_request_record_save_writes_json() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("pull_request.json");
        let record = PullRequestRecord {
            html_url: "https://github.com/owner/repo/pull/42".to_string(),
            number: 42,
            owner: "owner".to_string(),
            repo: "repo".to_string(),
            base_branch: "main".to_string(),
            head_branch: "arc/run/abc".to_string(),
            title: "Fix the thing".to_string(),
        };
        record.save(&path).unwrap();

        let content: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(content["html_url"], "https://github.com/owner/repo/pull/42");
        assert_eq!(content["number"], 42);
        assert_eq!(content["owner"], "owner");
        assert_eq!(content["repo"], "repo");
        assert_eq!(content["base_branch"], "main");
        assert_eq!(content["head_branch"], "arc/run/abc");
        assert_eq!(content["title"], "Fix the thing");
    }

    #[tokio::test]
    async fn empty_diff_returns_none() {
        let creds = GitHubAppCredentials {
            app_id: "123".to_string(),
            private_key_pem: "unused".to_string(),
        };
        let result = maybe_open_pull_request(
            &creds,
            "https://github.com/owner/repo.git",
            "main",
            "arc/run/123",
            "Fix bug",
            "",
            "claude-sonnet-4-20250514",
            false,
        )
        .await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}
