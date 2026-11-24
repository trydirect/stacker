//! Pure GitHub App webhook logic for TryDirect Stacker (WS-G).
//!
//! Plan: config/PLAN-github-webhooks.md. Contracts:
//! config/shared-fixtures/api-contracts/github-webhooks.json.
//!
//! This crate owns three pure concerns: `X-Hub-Signature-256` verification,
//! webhook payload normalization, and watch-path matching. It does no
//! database, Vault or HTTP work.

use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Verify a `X-Hub-Signature-256` header (`sha256=<hex>`) against the raw
/// body. Comparison is constant-time (`verify_slice`).
pub fn verify_signature(
    secret: &str,
    body: &[u8],
    signature_header: Option<&str>,
) -> Result<(), String> {
    let header = signature_header.ok_or_else(|| "Missing X-Hub-Signature-256 header".to_string())?;
    let hex_signature = header
        .strip_prefix("sha256=")
        .ok_or_else(|| "X-Hub-Signature-256 must start with 'sha256='".to_string())?;
    let expected = hex_decode(hex_signature)?;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| format!("invalid webhook secret: {}", e))?;
    mac.update(body);
    mac.verify_slice(&expected)
        .map_err(|_| "Invalid webhook signature".to_string())
}

fn hex_decode(input: &str) -> Result<Vec<u8>, String> {
    if input.len() % 2 != 0 {
        return Err("hex signature has odd length".to_string());
    }
    (0..input.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&input[i..i + 2], 16)
                .map_err(|_| "hex signature contains non-hex characters".to_string())
        })
        .collect()
}

/// The normalized events WS-G acts on. `Other` carries the event name for
/// anything else (ignored by the dispatch logic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitEvent {
    Push(PushEvent),
    PullRequest(PullRequestEvent),
    Installation(InstallationEvent),
    InstallationRepositories(InstallationRepositoriesEvent),
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushEvent {
    /// `owner/name`
    pub repo: String,
    /// Branch name without the `refs/heads/` prefix.
    pub branch: String,
    pub head_sha: String,
    /// Union of added/modified/removed/renamed files across all commits.
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestEvent {
    pub action: String,
    pub repo: String,
    pub number: i64,
    pub head_ref: String,
    pub base_ref: String,
    pub head_sha: String,
    /// A PR whose head repository differs from the base repository.
    pub from_fork: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationEvent {
    pub action: String,
    pub account_login: String,
    pub installation_id: i64,
    pub repos: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationRepositoriesEvent {
    pub action: String,
    pub installation_id: i64,
    pub repos_added: Vec<String>,
    pub repos_removed: Vec<String>,
}

fn str_field(value: &serde_json::Value, path: &[&str]) -> String {
    let mut current = value;
    for key in path {
        current = &current[*key];
    }
    current.as_str().unwrap_or_default().to_string()
}

fn repo_names(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("full_name").and_then(|v| v.as_str()))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Normalize a webhook payload by its `X-GitHub-Event` name.
pub fn parse_event(event_name: &str, body: &serde_json::Value) -> Result<GitEvent, String> {
    match event_name {
        "push" => {
            let branch = str_field(body, &["ref"])
                .strip_prefix("refs/heads/")
                .unwrap_or_default()
                .to_string();
            if branch.is_empty() {
                return Err("push event has no refs/heads branch".to_string());
            }
            let mut changed = Vec::new();
            if let Some(commits) = body.get("commits").and_then(|v| v.as_array()) {
                for commit in commits {
                    for key in ["added", "modified", "removed"] {
                        if let Some(files) = commit.get(key).and_then(|v| v.as_array()) {
                            changed.extend(
                                files.iter().filter_map(|f| f.as_str()).map(str::to_string),
                            );
                        }
                    }
                }
            }
            changed.sort();
            changed.dedup();
            Ok(GitEvent::Push(PushEvent {
                repo: str_field(body, &["repository", "full_name"]),
                branch,
                head_sha: str_field(body, &["after"]),
                changed_files: changed,
            }))
        }
        "pull_request" => {
            let head_repo = str_field(body, &["pull_request", "head", "repo", "full_name"]);
            let base_repo = str_field(body, &["pull_request", "base", "repo", "full_name"]);
            Ok(GitEvent::PullRequest(PullRequestEvent {
                action: str_field(body, &["action"]),
                repo: base_repo.clone(),
                number: body
                    .get("pull_request")
                    .and_then(|v| v.get("number"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or_default(),
                head_ref: str_field(body, &["pull_request", "head", "ref"]),
                base_ref: str_field(body, &["pull_request", "base", "ref"]),
                head_sha: str_field(body, &["pull_request", "head", "sha"]),
                from_fork: !head_repo.is_empty() && head_repo != base_repo,
            }))
        }
        "installation" => Ok(GitEvent::Installation(InstallationEvent {
            action: str_field(body, &["action"]),
            account_login: str_field(body, &["installation", "account", "login"]),
            installation_id: body
                .get("installation")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_i64())
                .unwrap_or_default(),
            repos: repo_names(body.get("repositories").unwrap_or(&serde_json::Value::Null)),
        })),
        "installation_repositories" => {
            Ok(GitEvent::InstallationRepositories(InstallationRepositoriesEvent {
                action: str_field(body, &["action"]),
                installation_id: body
                    .get("installation")
                    .and_then(|v| v.get("id"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or_default(),
                repos_added: repo_names(
                    body.get("repositories_added").unwrap_or(&serde_json::Value::Null),
                ),
                repos_removed: repo_names(
                    body.get("repositories_removed")
                        .unwrap_or(&serde_json::Value::Null),
                ),
            }))
        }
        other => Ok(GitEvent::Other(other.to_string())),
    }
}

/// Watch-path matching (G3): a changed file matches when it matches at least
/// one include pattern (or there are none) and no `!`-prefixed exclusion
/// pattern. Patterns use globset syntax (`src/*`, `**/*.rs`, `!docs/**`).
/// Invalid patterns are rejected.
pub fn matches_watch_paths(patterns: &[String], changed_files: &[String]) -> Result<bool, String> {
    let mut includes: Vec<String> = Vec::new();
    let mut excludes: Vec<String> = Vec::new();
    for pattern in patterns {
        match pattern.strip_prefix('!') {
            Some(exclude) => excludes.push(exclude.to_string()),
            None => includes.push(pattern.clone()),
        }
    }

    let include_set = build_globset(&includes)?;
    let exclude_set = build_globset(&excludes)?;

    Ok(changed_files.iter().any(|file| {
        let included = includes.is_empty()
            || include_set
                .as_ref()
                .map(|set| set.is_match(file))
                .unwrap_or(false);
        let excluded = exclude_set
            .as_ref()
            .map(|set| set.is_match(file))
            .unwrap_or(false);
        included && !excluded
    }))
}

fn build_globset(patterns: &[String]) -> Result<Option<globset::GlobSet>, String> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = globset::GlobSetBuilder::new();
    for pattern in patterns {
        let glob = globset::Glob::new(pattern)
            .map_err(|e| format!("invalid watch path pattern '{}': {}", pattern, e))?;
        builder.add(glob);
    }
    builder
        .build()
        .map(Some)
        .map_err(|e| format!("failed to build watch path set: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(secret: &str, body: &[u8]) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body);
        let hex: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        format!("sha256={}", hex)
    }

    #[test]
    fn test_verify_signature_accepts_valid_and_rejects_bad() {
        let body = br#"{"zen":"Design for failure."}"#;
        let header = sign("top-secret", body);
        assert!(verify_signature("top-secret", body, Some(&header)).is_ok());
        assert!(verify_signature("wrong-secret", body, Some(&header)).is_err());
        assert!(verify_signature("top-secret", b"tampered", Some(&header)).is_err());
        assert!(verify_signature("top-secret", body, None).is_err());
        assert!(verify_signature("top-secret", body, Some("deadbeef")).is_err());
    }

    #[test]
    fn test_parse_push_event() {
        let body: serde_json::Value = serde_json::json!({
            "ref": "refs/heads/main",
            "after": "abc123",
            "repository": { "full_name": "alice/shop" },
            "commits": [
                { "added": ["src/a.rs"], "modified": ["src/b.rs"], "removed": [] },
                { "modified": ["src/b.rs"], "removed": ["old.txt"] }
            ]
        });
        match parse_event("push", &body).unwrap() {
            GitEvent::Push(push) => {
                assert_eq!(push.repo, "alice/shop");
                assert_eq!(push.branch, "main");
                assert_eq!(push.head_sha, "abc123");
                assert_eq!(
                    push.changed_files,
                    vec!["old.txt", "src/a.rs", "src/b.rs"]
                );
            }
            other => panic!("expected push, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_pull_request_detects_fork() {
        let body: serde_json::Value = serde_json::json!({
            "action": "opened",
            "pull_request": {
                "number": 12,
                "head": { "ref": "feature", "sha": "def456", "repo": { "full_name": "mallory/shop" } },
                "base": { "ref": "main", "repo": { "full_name": "alice/shop" } }
            }
        });
        match parse_event("pull_request", &body).unwrap() {
            GitEvent::PullRequest(pr) => {
                assert_eq!(pr.number, 12);
                assert!(pr.from_fork);
            }
            other => panic!("expected pull_request, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_installation_events() {
        let body: serde_json::Value = serde_json::json!({
            "action": "created",
            "installation": { "id": 42, "account": { "login": "alice" } },
            "repositories": [ { "full_name": "alice/shop" } ]
        });
        match parse_event("installation", &body).unwrap() {
            GitEvent::Installation(installation) => {
                assert_eq!(installation.installation_id, 42);
                assert_eq!(installation.repos, vec!["alice/shop"]);
            }
            other => panic!("expected installation, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_unknown_event_is_other() {
        assert_eq!(
            parse_event("issues", &serde_json::json!({})).unwrap(),
            GitEvent::Other("issues".to_string())
        );
    }

    #[test]
    fn test_watch_paths_include_exclude_and_empty() {
        let files = vec!["src/main.rs".to_string(), "docs/readme.md".to_string()];
        // No patterns = everything matches.
        assert!(matches_watch_paths(&[], &files).unwrap());
        // Include only.
        let only_src = vec!["src/*".to_string()];
        assert!(matches_watch_paths(&only_src, &files).unwrap());
        let only_ci = vec![".github/*".to_string()];
        assert!(!matches_watch_paths(&only_ci, &files).unwrap());
        // Exclusion wins.
        let src_without_docs = vec!["**/*".to_string(), "!docs/*".to_string()];
        assert!(matches_watch_paths(&src_without_docs, &files).unwrap());
        let docs_only_excluded = vec!["docs/*".to_string(), "!docs/*".to_string()];
        assert!(!matches_watch_paths(&docs_only_excluded, &files).unwrap());
        // No changed files never matches.
        assert!(!matches_watch_paths(&[], &[]).unwrap());
    }

    #[test]
    fn test_watch_paths_rejects_invalid_pattern() {
        let patterns = vec!["[".to_string()];
        assert!(matches_watch_paths(&patterns, &["a".to_string()]).is_err());
    }
}
