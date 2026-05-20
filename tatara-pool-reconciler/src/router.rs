//! PoolSelector matching across a candidate-pool set.
//!
//! Pure function: given a list of candidate pools + an allocation's
//! MatchKey, return the single best-match pool (by specificity score).
//! Ties broken by pool name (deterministic across reconciler restarts).

use tatara_process::pool::{EphemeralPool, MatchKey};

/// The matched pool the reconciler will bind the allocation to.
#[derive(Debug, Clone)]
pub struct MatchedPool<'a> {
    pub pool: &'a EphemeralPool,
    pub specificity: u32,
}

/// Best-match across `candidates` for `key`. Returns None when no
/// candidate matches.
#[must_use]
pub fn best_match<'a>(
    candidates: &'a [EphemeralPool],
    key: &MatchKey<'_>,
) -> Option<MatchedPool<'a>> {
    let mut best: Option<MatchedPool<'a>> = None;
    for pool in candidates {
        if !pool.spec.selector.matches(key) {
            continue;
        }
        let score = pool.spec.selector.specificity();
        match &best {
            None => {
                best = Some(MatchedPool { pool, specificity: score });
            }
            Some(b) => {
                let beat_score = score > b.specificity;
                let tie_break = score == b.specificity
                    && pool_name(pool).cmp(pool_name(b.pool)) == std::cmp::Ordering::Less;
                if beat_score || tie_break {
                    best = Some(MatchedPool { pool, specificity: score });
                }
            }
        }
    }
    best
}

fn pool_name(p: &EphemeralPool) -> &str {
    p.metadata.name.as_deref().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::Resource;
    use tatara_process::ephemeral::EphemeralSpec;
    use tatara_process::intent::AplicacaoIntent;
    use tatara_process::lifetime::TeardownPolicy;
    use tatara_process::pool::{PoolSelector, PoolSpec, ReturnPolicy};

    fn empty_template() -> EphemeralSpec {
        EphemeralSpec {
            aplicacao: AplicacaoIntent {
                chart_ref: "oci://x".into(),
                version: "1".into(),
                profile: String::new(),
                values_overlay: serde_json::Value::Null,
                release_name: None,
                target_namespace: None,
                install_timeout: None,
            },
            ttl: "1h".into(),
            teardown: TeardownPolicy::Always,
            max_concurrent: 0,
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: None,
            exports: vec![],
        }
    }

    fn pool(name: &str, selector: PoolSelector) -> EphemeralPool {
        let spec = PoolSpec {
            desired_size: 1,
            min_size: 0,
            max_size: 0,
            return_policy: ReturnPolicy::Replace,
            selector,
            template: empty_template(),
            free_ttl: "24h".into(),
            max_allocation_ttl: "4h".into(),
        };
        let mut p = EphemeralPool::new(name, spec);
        p.meta_mut().namespace = Some("ephemeral-pools".into());
        p
    }

    #[test]
    fn no_candidates_returns_none() {
        let key = MatchKey {
            repo: "x",
            branch: "y",
            pr_labels: &[],
            kind: "manual",
        };
        assert!(best_match(&[], &key).is_none());
    }

    #[test]
    fn unmatched_selector_skipped() {
        let p = pool(
            "akeyless",
            PoolSelector {
                repos: vec!["pleme-io/akeyless-*".into()],
                ..Default::default()
            },
        );
        let key = MatchKey {
            repo: "drzln/dotfiles",
            branch: "main",
            pr_labels: &[],
            kind: "manual",
        };
        assert!(best_match(&[p], &key).is_none());
    }

    #[test]
    fn higher_specificity_wins() {
        let general = pool("general", PoolSelector::default());
        let specific = pool(
            "specific",
            PoolSelector {
                repos: vec!["pleme-io/akeyless-*".into()],
                branches: vec!["main".into()],
                pr_labels: vec!["needs-akeyless".into()],
                ..Default::default()
            },
        );
        let key = MatchKey {
            repo: "pleme-io/akeyless-deployment",
            branch: "main",
            pr_labels: &["needs-akeyless".into()],
            kind: "github-pr",
        };
        let pools = vec![general, specific];
        let m = best_match(&pools, &key).unwrap();
        assert_eq!(m.pool.metadata.name.as_deref(), Some("specific"));
    }

    #[test]
    fn tie_broken_lexicographically_by_name() {
        let a = pool(
            "a-pool",
            PoolSelector {
                repos: vec!["pleme-io/akeyless-*".into()],
                ..Default::default()
            },
        );
        let z = pool(
            "z-pool",
            PoolSelector {
                repos: vec!["pleme-io/akeyless-*".into()],
                ..Default::default()
            },
        );
        let key = MatchKey {
            repo: "pleme-io/akeyless-deployment",
            branch: "x",
            pr_labels: &[],
            kind: "manual",
        };
        let pools = vec![z, a];
        let m = best_match(&pools, &key).unwrap();
        assert_eq!(m.pool.metadata.name.as_deref(), Some("a-pool"));
    }

    #[test]
    fn default_selector_matches_general_traffic() {
        let p = pool("general", PoolSelector::default());
        let key = MatchKey {
            repo: "any/repo",
            branch: "any-branch",
            pr_labels: &[],
            kind: "manual",
        };
        assert!(best_match(&[p], &key).is_some());
    }
}
