//! Typed GitHub event shapes — only the fields the watcher reads.
//! Everything else is intentionally dropped (forward-compatible — any
//! new GitHub field doesn't break our deserialization).

use serde::{Deserialize, Serialize};

/// Discriminator over the event types the watcher handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    PullRequest,
    Push,
    Other,
}

impl EventKind {
    /// Decode from the `X-GitHub-Event` header value.
    pub fn from_header(s: &str) -> Self {
        match s {
            "pull_request" => Self::PullRequest,
            "push" => Self::Push,
            _ => Self::Other,
        }
    }
}

/// PR event subaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrAction {
    Opened,
    Reopened,
    Synchronize,
    Closed,
    /// Anything else — typed-honest fallthrough.
    #[serde(other)]
    Other,
}

/// Pull-request event payload.
#[derive(Debug, Clone, Deserialize)]
pub struct PullRequestEvent {
    pub action: PrAction,
    pub number: u64,
    pub repository: Repository,
    pub pull_request: PullRequest,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullRequest {
    pub head: Branch,
    pub base: Branch,
    pub draft: Option<bool>,
    pub merged: Option<bool>,
    #[serde(default)]
    pub labels: Vec<Label>,
    pub user: User,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Branch {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub sha: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Label {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct User {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Repository {
    pub full_name: String, // "org/repo"
    #[serde(rename = "default_branch")]
    pub default_branch: Option<String>,
}

/// Push event payload.
#[derive(Debug, Clone, Deserialize)]
pub struct PushEvent {
    #[serde(rename = "ref")]
    pub ref_name: String,
    pub after: String,
    pub repository: Repository,
    pub pusher: Pusher,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Pusher {
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_kind_decodes_known_headers() {
        assert_eq!(EventKind::from_header("pull_request"), EventKind::PullRequest);
        assert_eq!(EventKind::from_header("push"), EventKind::Push);
        assert_eq!(EventKind::from_header("ping"), EventKind::Other);
    }

    #[test]
    fn pr_action_deserializes_known() {
        for s in ["opened", "reopened", "synchronize", "closed"] {
            let q = format!("\"{s}\"");
            let action: PrAction = serde_json::from_str(&q).unwrap();
            assert!(matches!(
                action,
                PrAction::Opened | PrAction::Reopened | PrAction::Synchronize | PrAction::Closed
            ));
        }
    }

    #[test]
    fn pr_action_unknown_falls_through_to_other() {
        let action: PrAction = serde_json::from_str("\"edited\"").unwrap();
        assert_eq!(action, PrAction::Other);
    }

    #[test]
    fn pull_request_event_parses_minimal_payload() {
        let json = r#"{
            "action": "opened",
            "number": 123,
            "repository": {
                "full_name": "pleme-io/akeyless-deployment",
                "default_branch": "main"
            },
            "pull_request": {
                "head": { "ref": "fix-something", "sha": "abc123" },
                "base": { "ref": "main", "sha": "def456" },
                "draft": false,
                "merged": false,
                "labels": [ { "name": "needs-akeyless" } ],
                "user": { "login": "drzln" }
            }
        }"#;
        let evt: PullRequestEvent = serde_json::from_str(json).unwrap();
        assert_eq!(evt.action, PrAction::Opened);
        assert_eq!(evt.number, 123);
        assert_eq!(evt.repository.full_name, "pleme-io/akeyless-deployment");
        assert_eq!(evt.pull_request.head.ref_name, "fix-something");
        assert_eq!(evt.pull_request.labels.len(), 1);
        assert_eq!(evt.pull_request.labels[0].name, "needs-akeyless");
        assert_eq!(evt.pull_request.user.login, "drzln");
    }

    #[test]
    fn push_event_parses_minimal_payload() {
        let json = r#"{
            "ref": "refs/heads/main",
            "after": "deadbeef",
            "repository": { "full_name": "pleme-io/tatara", "default_branch": "main" },
            "pusher": { "name": "drzln" }
        }"#;
        let evt: PushEvent = serde_json::from_str(json).unwrap();
        assert_eq!(evt.ref_name, "refs/heads/main");
        assert_eq!(evt.after, "deadbeef");
        assert_eq!(evt.repository.full_name, "pleme-io/tatara");
    }

    #[test]
    fn extra_fields_are_ignored() {
        // GitHub's actual payloads have ~hundreds of fields; we drop them.
        let json = r#"{
            "action": "opened",
            "number": 1,
            "repository": {
                "full_name": "x/y",
                "default_branch": "main",
                "forks_count": 42,
                "open_issues": 7
            },
            "pull_request": {
                "head": { "ref": "x", "sha": "y", "extra": "ignored" },
                "base": { "ref": "x", "sha": "y" },
                "user": { "login": "x", "site_admin": false }
            },
            "sender": { "anything": "here" }
        }"#;
        let evt: PullRequestEvent = serde_json::from_str(json).unwrap();
        assert_eq!(evt.number, 1);
    }
}
