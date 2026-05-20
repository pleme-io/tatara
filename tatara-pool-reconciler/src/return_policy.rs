//! Translate `ReturnPolicy` into concrete return actions.

use tatara_process::pool::ReturnPolicy;

/// What the reconciler does to a member after release.
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

/// Compute the return plan from a pool's ReturnPolicy.
///
/// `reset_image` is the image the reconciler stamps into the Reset Job
/// for `Reset` policy. It's pool-configured at install time.
#[must_use]
pub fn plan_return(policy: ReturnPolicy, reset_image: Option<&str>) -> ReturnPlan {
    match policy {
        ReturnPolicy::Replace => ReturnPlan::DeleteAndRespawn,
        ReturnPolicy::Reset => ReturnPlan::ResetThenFree {
            reset_job_image: reset_image
                .unwrap_or("ghcr.io/pleme-io/pool-reset:latest")
                .to_string(),
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
}
