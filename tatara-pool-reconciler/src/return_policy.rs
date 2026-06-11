//! Translate `ReturnPolicy` into concrete return actions.

use tatara_process::pool::ReturnPolicy;

/// What the reconciler does to a member after release.
///
/// Closed-set image of `ReturnPolicy` under `plan_return`. The
/// `(keeps_process, runs_reset_job)` predicate pair on `ReturnPolicy`
/// is injective onto these three variants — see
/// `tatara_process::pool::ReturnPolicy::ALL` and the
/// `return_policy_predicate_pair_is_injective` test pinned alongside
/// it. Adding a fourth `ReturnPlan` variant requires extending the
/// predicate pair AND the `plan_return` arm together; the
/// `plan_return_covers_all_return_policies` sweep test below pins
/// that contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnPlan {
    /// Delete the backing Process. The pool's regular Spawn loop
    /// will create a fresh slot to replace it.
    DeleteAndRespawn,
    /// Keep the Process running; run a typed Reset Job that wipes
    /// state, then flip the member back to `Free`.
    ResetThenFree { reset_job_image: String },
    /// Keep the Process running indefinitely; flip the member to
    /// a special "Kept" state surfaced for operator inspection.
    KeepForInspection,
}

/// Default image stamped into the Reset Job when the pool didn't
/// override `reset_image` at install time. Pinned as a `const` so the
/// canonical default is one site (not duplicated between code + tests
/// + docs) and a future rename lands at one location.
pub const DEFAULT_RESET_JOB_IMAGE: &str = "ghcr.io/pleme-io/pool-reset:latest";

/// Compute the return plan from a pool's ReturnPolicy.
///
/// `reset_image` is the image the reconciler stamps into the Reset Job
/// for `Reset` policy. It's pool-configured at install time.
///
/// Exhaustiveness contract: this match is the consumer dispatch site
/// referenced from `ReturnPolicy::keeps_process` /
/// `runs_reset_job`. Adding a fourth `ReturnPolicy` variant fails the
/// closed-set match here AND the
/// `plan_return_covers_all_return_policies` sweep test below.
#[must_use]
pub fn plan_return(policy: ReturnPolicy, reset_image: Option<&str>) -> ReturnPlan {
    match policy {
        ReturnPolicy::Replace => ReturnPlan::DeleteAndRespawn,
        ReturnPolicy::Reset => ReturnPlan::ResetThenFree {
            reset_job_image: reset_image.unwrap_or(DEFAULT_RESET_JOB_IMAGE).to_string(),
        },
        ReturnPolicy::Keep => ReturnPlan::KeepForInspection,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_default_yields_delete_and_respawn() {
        assert_eq!(
            plan_return(ReturnPolicy::Replace, None),
            ReturnPlan::DeleteAndRespawn
        );
    }

    #[test]
    fn reset_with_default_image() {
        match plan_return(ReturnPolicy::Reset, None) {
            ReturnPlan::ResetThenFree { reset_job_image } => {
                assert_eq!(reset_job_image, "ghcr.io/pleme-io/pool-reset:latest");
            }
            other => panic!("expected ResetThenFree, got {other:?}"),
        }
    }

    #[test]
    fn reset_with_custom_image() {
        match plan_return(ReturnPolicy::Reset, Some("ghcr.io/example/reset:1.2.3")) {
            ReturnPlan::ResetThenFree { reset_job_image } => {
                assert_eq!(reset_job_image, "ghcr.io/example/reset:1.2.3");
            }
            other => panic!("expected ResetThenFree, got {other:?}"),
        }
    }

    #[test]
    fn keep_yields_keep_for_inspection() {
        assert_eq!(
            plan_return(ReturnPolicy::Keep, None),
            ReturnPlan::KeepForInspection
        );
    }

    /// EXHAUSTIVENESS CONTRACT: `plan_return` covers every variant in
    /// `ReturnPolicy::ALL`. A fourth `ReturnPolicy` variant added
    /// without a matching `plan_return` arm fails the inner closed-set
    /// `match` at compile time; this sweep additionally pins the
    /// runtime that every `ALL` entry projects to a `ReturnPlan` of
    /// the expected shape (no panics, no unintended `KeepForInspection`
    /// fallback if the inner match ever loosens). Mirrors the
    /// `replacement_policy_predicate_pair_is_injective` sweep test
    /// pinned alongside `ReplacementPolicy::ALL` on the same axis.
    #[test]
    fn plan_return_covers_all_return_policies() {
        for policy in ReturnPolicy::ALL {
            let plan = plan_return(policy, None);
            // `(keeps_process, runs_reset_job)` is injective onto
            // `ReturnPlan` — pin the per-bucket mapping so a future
            // predicate flip lands on a wrong-shape plan here.
            match (policy.keeps_process(), policy.runs_reset_job()) {
                (false, false) => assert_eq!(
                    plan,
                    ReturnPlan::DeleteAndRespawn,
                    "{policy:?} maps to wrong plan",
                ),
                (true, true) => assert!(
                    matches!(plan, ReturnPlan::ResetThenFree { .. }),
                    "{policy:?} maps to wrong plan: {plan:?}",
                ),
                (true, false) => assert_eq!(
                    plan,
                    ReturnPlan::KeepForInspection,
                    "{policy:?} maps to wrong plan",
                ),
                (false, true) => panic!(
                    "{policy:?} hit the impossible (!keeps, runs_reset) bucket; \
                     the predicate-pair implication contract pinned in \
                     `tatara_process::pool::ReturnPolicy` should have caught this"
                ),
            }
        }
    }

    /// The `DEFAULT_RESET_JOB_IMAGE` const is the canonical default
    /// — pin it so a rename lands at one site and so a future call
    /// site composing the default through `unwrap_or` rather than the
    /// `const` is caught.
    #[test]
    fn default_reset_image_const_matches_plan_return_fallback() {
        match plan_return(ReturnPolicy::Reset, None) {
            ReturnPlan::ResetThenFree { reset_job_image } => {
                assert_eq!(reset_job_image, DEFAULT_RESET_JOB_IMAGE);
            }
            other => panic!("expected ResetThenFree, got {other:?}"),
        }
    }
}
