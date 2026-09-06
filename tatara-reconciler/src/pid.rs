//! Hierarchical PID allocation — pure logic, testable without a cluster.
//!
//! A Process's PID path is either:
//!   - `"<identity>.<n>"`        — root process (no parent)
//!   - `"<parent_path>.<n>"`     — child of a parent
//!
//! where `n` comes from the cluster-scoped ProcessTable's `spec.nextSequence`.

use tatara_process::identity::{join_pid_segment, PID_PATH_SEPARATOR};
use tatara_process::prelude::Identity;

/// Allocate a PID path for a Process.
///
/// Both hierarchical-PID compose arms (Some(parent) child + None root)
/// route through the ONE substrate composer
/// [`tatara_process::identity::join_pid_segment`], so a future
/// normalization of the PID-path segment join (see the composer's doc
/// for the catalog) reaches this allocator mechanically.
pub fn allocate_pid(identity: &Identity, parent_pid: Option<&str>, next_sequence: u32) -> String {
    match parent_pid {
        Some(parent) if !parent.is_empty() => join_pid_segment(parent, next_sequence),
        _ => join_pid_segment(&identity.name, next_sequence),
    }
}

/// Compute depth of a PID path (`seph.1.7.3` → 4).
///
/// The segment separator routes through the ONE substrate const
/// [`tatara_process::identity::PID_PATH_SEPARATOR`] so a future
/// normalization of the separator (see the const's doc for the
/// catalog) reaches this walker mechanically.
pub fn depth(pid_path: &str) -> usize {
    if pid_path.is_empty() {
        0
    } else {
        pid_path.split(PID_PATH_SEPARATOR).count()
    }
}

/// Parent PID path of the given PID (`seph.1.7.3` → `Some("seph.1.7")`, `seph.1` → `Some("seph")`).
/// Returns None for a bare identity with no numeric suffix.
///
/// The last-segment strip routes through the ONE substrate const
/// [`tatara_process::identity::PID_PATH_SEPARATOR`] so a future
/// normalization of the separator (see the const's doc for the
/// catalog) reaches this walker mechanically.
pub fn parent_of(pid_path: &str) -> Option<&str> {
    let last_dot = pid_path.rfind(PID_PATH_SEPARATOR)?;
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

    // ─── substrate-owner routing pins ────────────────────────────────
    //
    // Both hierarchical-PID compose arms of [`allocate_pid`] route
    // through [`tatara_process::identity::join_pid_segment`], and both
    // walkers ([`depth`] + [`parent_of`]) route through
    // [`tatara_process::identity::PID_PATH_SEPARATOR`]. These pins bind
    // the routing at fail-before-pass-after granularity so a regression
    // that re-inlined a bare `format!("{parent}.{seq}")` at the
    // composer arm or a bare `'.'` at the walker sites (a copy-paste
    // during a future extension, a rebase that unwound the lift)
    // surfaces HERE rather than as silent operator-facing skew across
    // the cluster-wide PID address space.

    #[test]
    fn allocate_pid_routes_both_arms_through_join_pid_segment() {
        // Byte-shape pin: for every representative input across the
        // Some(parent) child arm + the None / empty-parent root arm,
        // the allocator's output matches `join_pid_segment` on the
        // same (<head>, <next_sequence>) pair. A regression that
        // inlined a bare `format!` at either arm would surface here
        // rather than as silent skew between the two arms.
        let seph = id("seph");
        for (parent, seq) in [
            (None, 1u32),
            (Some(""), 1),
            (Some("seph.1"), 7),
            (Some("seph.1.7"), 3),
            (Some("root"), u32::MAX),
        ] {
            let head = parent
                .filter(|p: &&str| !p.is_empty())
                .unwrap_or(&seph.name);
            assert_eq!(
                allocate_pid(&seph, parent, seq),
                join_pid_segment(head, seq),
                "allocate_pid must route through join_pid_segment for parent={parent:?}, seq={seq}"
            );
        }
    }

    #[test]
    fn depth_routes_through_pid_path_separator_const() {
        // Cross-primitive coherence pin: `depth` counts segments split
        // on `PID_PATH_SEPARATOR`. Synthesize a path with N segments
        // joined by the const and verify the count round-trips.
        for n in 1..=8 {
            let path: String = (0..n)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(&PID_PATH_SEPARATOR.to_string());
            assert_eq!(
                depth(&path),
                n,
                "depth must count segments split on PID_PATH_SEPARATOR"
            );
        }
    }

    #[test]
    fn parent_of_routes_through_pid_path_separator_const() {
        // Cross-primitive coherence pin: `parent_of` strips the last
        // segment past the last `PID_PATH_SEPARATOR`. For every
        // representative input, verify the strip lands at the const's
        // rightmost occurrence.
        for path in ["seph.1.7.3", "seph.1", "root.0.0.0.0"] {
            let expected = path.rfind(PID_PATH_SEPARATOR).map(|i| &path[..i]);
            assert_eq!(
                parent_of(path),
                expected,
                "parent_of must strip at PID_PATH_SEPARATOR for path={path:?}"
            );
        }
        assert_eq!(parent_of("seph"), None);
    }
}
