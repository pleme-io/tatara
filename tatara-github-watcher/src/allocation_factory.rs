//! Translate a GitHub event into an `EphemeralAllocation` spec.
//!
//! Pure function: GitHub event in, typed Allocation out. The handler
//! applies the resulting Allocation via kube-rs.

use kube::Resource;

use tatara_process::allocation::{AllocationSpec, EphemeralAllocation, Requestor};
use tatara_process::pool::AllocationRef;

use crate::event::{PrAction, PullRequestEvent};

/// Errors building an Allocation from an event.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FactoryError {
    /// The PR action doesn't warrant an allocation (e.g., draft PRs,
    /// labeling-only edits).
    #[error("PR action {0:?} does not warrant allocation")]
    NotAllocatable(PrAction),
    /// PR is a draft and the watcher's policy excludes drafts.
    #[error("PR is draft and drafts are excluded")]
    DraftExcluded,
}

/// Deterministic Allocation name from a PR event — re-running the same
/// event yields the same name (idempotent create).
#[must_use]
pub fn allocation_name(repo: &str, pr_number: u64) -> String {
    // Replace `/` so the result is a valid K8s name.
    let safe_repo = repo.replace('/', "-");
    let safe_repo: String = safe_repo
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    let trimmed = if safe_repo.len() > 50 {
        &safe_repo[..50]
    } else {
        &safe_repo
    };
    format!("pr-{pr_number}-{trimmed}").to_lowercase()
}

/// Build a typed EphemeralAllocation from a PR event.
///
/// Routing knobs:
/// * `namespace` — where to create the Allocation. Same namespace as the
///   pool (typically `ephemeral-pools`).
/// * `pool_name` — when `Some`, pin the allocation to a named pool (skips
///   selector routing). When `None`, the reconciler matches via PoolSelector.
/// * `include_drafts` — when `false`, draft PRs return `DraftExcluded`.
pub fn build_allocation(
    evt: &PullRequestEvent,
    namespace: &str,
    pool_name: Option<&str>,
    include_drafts: bool,
) -> Result<EphemeralAllocation, FactoryError> {
    // Action filter — only opening states allocate; "closed" is handled
    // by the deletion path elsewhere.
    match evt.action {
        PrAction::Opened | PrAction::Reopened | PrAction::Synchronize => {}
        other => return Err(FactoryError::NotAllocatable(other)),
    }

    if !include_drafts && evt.pull_request.draft.unwrap_or(false) {
        return Err(FactoryError::DraftExcluded);
    }

    let name = allocation_name(&evt.repository.full_name, evt.number);
    let labels: Vec<String> = evt
        .pull_request
        .labels
        .iter()
        .map(|l| l.name.clone())
        .collect();

    let pool_ref = pool_name.map(|n| AllocationRef {
        name: n.to_string(),
        namespace: namespace.to_string(),
    });

    let spec = AllocationSpec {
        pool_ref,
        requestor: Requestor {
            kind: "github-pr".into(),
            repo: Some(evt.repository.full_name.clone()),
            branch: Some(evt.pull_request.head.ref_name.clone()),
            pr_number: Some(evt.number),
            sha: Some(evt.pull_request.head.sha.clone()),
            pr_labels: labels,
            actor: Some(evt.pull_request.user.login.clone()),
        },
        ttl: None,
        note: Some(format!(
            "github webhook: PR #{} on {} ({})",
            evt.number,
            evt.repository.full_name,
            format_action(evt.action)
        )),
    };

    let mut alloc = EphemeralAllocation::new(&name, spec);
    alloc.meta_mut().namespace = Some(namespace.to_string());
    Ok(alloc)
}

fn format_action(a: PrAction) -> &'static str {
    match a {
        PrAction::Opened => "opened",
        PrAction::Reopened => "reopened",
        PrAction::Synchronize => "synchronize",
        PrAction::Closed => "closed",
        PrAction::Other => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Branch, Label, PullRequest, Repository, User};

    fn sample_event(action: PrAction, draft: bool) -> PullRequestEvent {
        PullRequestEvent {
            action,
            number: 42,
            repository: Repository {
                full_name: "pleme-io/akeyless-deployment".into(),
                default_branch: Some("main".into()),
            },
            pull_request: PullRequest {
                head: Branch {
                    ref_name: "fix-something".into(),
                    sha: "abc123def".into(),
                },
                base: Branch {
                    ref_name: "main".into(),
                    sha: "def456abc".into(),
                },
                draft: Some(draft),
                merged: Some(false),
                labels: vec![
                    Label {
                        name: "needs-akeyless".into(),
                    },
                    Label {
                        name: "integration".into(),
                    },
                ],
                user: User {
                    login: "drzln".into(),
                },
            },
        }
    }

    #[test]
    fn opened_pr_builds_typed_allocation() {
        let evt = sample_event(PrAction::Opened, false);
        let alloc = build_allocation(&evt, "ephemeral-pools", None, false).unwrap();
        assert_eq!(
            alloc.metadata.name.as_deref(),
            Some("pr-42-pleme-io-akeyless-deployment")
        );
        assert_eq!(alloc.metadata.namespace.as_deref(), Some("ephemeral-pools"));
        assert_eq!(alloc.spec.requestor.kind, "github-pr");
        assert_eq!(
            alloc.spec.requestor.repo.as_deref(),
            Some("pleme-io/akeyless-deployment")
        );
        assert_eq!(alloc.spec.requestor.pr_number, Some(42));
        assert_eq!(alloc.spec.requestor.sha.as_deref(), Some("abc123def"));
        assert_eq!(alloc.spec.requestor.pr_labels, vec!["needs-akeyless", "integration"]);
        assert_eq!(alloc.spec.requestor.actor.as_deref(), Some("drzln"));
        // Selector-routed (no pool pinned).
        assert!(alloc.spec.pool_ref.is_none());
    }

    #[test]
    fn pool_ref_pins_to_named_pool() {
        let evt = sample_event(PrAction::Opened, false);
        let alloc = build_allocation(&evt, "pools", Some("akeyless-pool"), false).unwrap();
        let pr = alloc.spec.pool_ref.unwrap();
        assert_eq!(pr.name, "akeyless-pool");
        assert_eq!(pr.namespace, "pools");
    }

    #[test]
    fn draft_pr_excluded_by_default() {
        let evt = sample_event(PrAction::Opened, true);
        let err = build_allocation(&evt, "pools", None, false).unwrap_err();
        assert_eq!(err, FactoryError::DraftExcluded);
    }

    #[test]
    fn draft_pr_included_when_allowed() {
        let evt = sample_event(PrAction::Opened, true);
        let alloc = build_allocation(&evt, "pools", None, true).unwrap();
        assert_eq!(alloc.spec.requestor.pr_number, Some(42));
    }

    #[test]
    fn closed_pr_does_not_allocate() {
        let evt = sample_event(PrAction::Closed, false);
        let err = build_allocation(&evt, "pools", None, false).unwrap_err();
        assert_eq!(err, FactoryError::NotAllocatable(PrAction::Closed));
    }

    #[test]
    fn allocation_name_is_deterministic() {
        let a = allocation_name("pleme-io/akeyless-deployment", 42);
        let b = allocation_name("pleme-io/akeyless-deployment", 42);
        assert_eq!(a, b);
    }

    #[test]
    fn allocation_name_is_dns_safe() {
        let n = allocation_name("Some/Org/Weird@Name", 7);
        assert!(n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'));
        assert!(n.starts_with("pr-7-"));
    }

    #[test]
    fn synchronize_action_allocates() {
        // PR push events come as `synchronize` — same allocation is
        // refreshed (idempotent name).
        let evt = sample_event(PrAction::Synchronize, false);
        let alloc = build_allocation(&evt, "pools", None, false).unwrap();
        assert_eq!(alloc.spec.requestor.sha.as_deref(), Some("abc123def"));
    }

    #[test]
    fn note_records_event_action_for_audit() {
        let evt = sample_event(PrAction::Reopened, false);
        let alloc = build_allocation(&evt, "pools", None, false).unwrap();
        let note = alloc.spec.note.unwrap();
        assert!(note.contains("PR #42"));
        assert!(note.contains("reopened"));
        assert!(note.contains("pleme-io/akeyless-deployment"));
    }
}
