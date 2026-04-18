//! Hierarchical PID allocation — pure logic, testable without a cluster.
//!
//! A Process's PID path is either:
//!   - `"<identity>.<n>"`        — root process (no parent)
//!   - `"<parent_path>.<n>"`     — child of a parent
//!
//! where `n` comes from the cluster-scoped ProcessTable's `spec.nextSequence`.

use tatara_process::prelude::Identity;

/// Allocate a PID path for a Process.
pub fn allocate_pid(identity: &Identity, parent_pid: Option<&str>, next_sequence: u32) -> String {
    match parent_pid {
        Some(parent) if !parent.is_empty() => format!("{parent}.{next_sequence}"),
        _ => format!("{}.{}", identity.name, next_sequence),
    }
}

/// Compute depth of a PID path (`seph.1.7.3` → 4).
pub fn depth(pid_path: &str) -> usize {
    if pid_path.is_empty() {
        0
    } else {
        pid_path.split('.').count()
    }
}

/// Parent PID path of the given PID (`seph.1.7.3` → `Some("seph.1.7")`, `seph.1` → `Some("seph")`).
/// Returns None for a bare identity with no numeric suffix.
pub fn parent_of(pid_path: &str) -> Option<&str> {
    let last_dot = pid_path.rfind('.')?;
    Some(&pid_path[..last_dot])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> Identity {
        Identity {
            name: name.into(),
            content_hash: "a".repeat(26),
            name_override: true,
        }
    }

    #[test]
    fn root_uses_identity_prefix() {
        assert_eq!(allocate_pid(&id("seph"), None, 1), "seph.1");
    }

    #[test]
    fn empty_parent_treated_as_none() {
        assert_eq!(allocate_pid(&id("seph"), Some(""), 1), "seph.1");
    }

    #[test]
    fn child_extends_parent() {
        assert_eq!(
            allocate_pid(&id("observability"), Some("seph.1"), 7),
            "seph.1.7"
        );
    }

    #[test]
    fn deeper_chain() {
        assert_eq!(
            allocate_pid(&id("irrelevant"), Some("seph.1.7"), 3),
            "seph.1.7.3"
        );
    }

    #[test]
    fn depth_counts_segments() {
        assert_eq!(depth(""), 0);
        assert_eq!(depth("seph"), 1);
        assert_eq!(depth("seph.1"), 2);
        assert_eq!(depth("seph.1.7.3"), 4);
    }

    #[test]
    fn parent_strips_last_segment() {
        assert_eq!(parent_of("seph.1.7.3"), Some("seph.1.7"));
        assert_eq!(parent_of("seph.1"), Some("seph"));
        assert_eq!(parent_of("seph"), None);
    }
}
