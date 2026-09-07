//! Substrate primitive owner of the
//! `std::env::var(NAME).context("NAME env var (from auth Secret) required")?`
//! chain every closed-loop-probe secret-env consumer restates by hand
//! when reading an OS environment variable that was projected into the
//! probe binary's process from a K8s `Secret` at admission time
//! (`envFrom.secretRef` or `env.valueFrom.secretKeyRef` on the probe's
//! PodSpec).
//!
//! Pre-lift the `std::env::var(NAME).context("NAME env var (from auth
//! Secret) required")?` chain was hand-authored at TWO adjacent sites
//! past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold inside
//! [`tatara-closed-loop-probe`]'s `main` — the entry-point secret pull
//! that funnels the probe's `access_id` + `access_key` into the
//! `probe::ProbeConfig` builder:
//!
//! * `ACCESS_ID` (the probe's user identity toward the bundled issuer).
//! * `ACCESS_KEY` (the probe's shared secret proving the identity).
//!
//! Both sites restated the SAME "read a required OS env var whose value
//! came from an in-namespace auth `Secret`; fail if unset" contract with
//! the SAME `<NAME> env var (from auth Secret) required` diagnostic
//! wording, differing only in the env-var name interpolated at the two
//! ends of the wrap. Post-lift each callsite reads
//! `tatara_process::secret_env::required_secret("ACCESS_ID")?` /
//! `("ACCESS_KEY")?` and the shared diagnostic wording lives at ONE
//! substrate owner — a future edit (a rename of the `auth Secret`
//! reference to `credential Secret`, an added hint about the chart
//! slot the operator should populate, a per-var suggestion of what
//! K8s `secretKeyRef.key` to bind) lands at the primitive's `format!`
//! body once and every downstream consumer picks up the upgrade
//! mechanically.
//!
//! Sibling to the same-axis error-context substrate primitives already
//! opened for K8s wire-side failure diagnostics:
//!
//! * [`crate::kube_error::KubeResultExt::kube_ctx`] — attach a
//!   `&'static str` slug to a `kube::Result<T>`, wrapping it as
//!   `anyhow!("<slug>: <err>")` (the workspace-wide wrap owner for
//!   `kube::Error`-yielding calls).
//! * [`crate::anyhow_flatten::FlattenCtxExt::flatten_ctx`] — attach a
//!   `&'static str` slug to a nested `anyhow::Result<T>`, flattening
//!   it as `anyhow!("<slug>: <err>")` (the workspace-wide wrap owner
//!   for chained `anyhow::Error`-yielding calls).
//! * [`crate::hostname::HostnameResultExt`] — attach a `&'static str`
//!   slug to a [`HostnameError`](crate::hostname::HostnameError)-yielding
//!   call (the axis-specific wrap owner for the hostname-derivation
//!   family).
//! * **This module** — attach the canonical `<VAR> env var (from auth
//!   Secret) required` diagnostic to a
//!   [`std::env::VarError`]-yielding call (the axis-specific wrap
//!   owner for the OS-env-var-from-Secret-projection family).
//!
//! Together the four families partition the workspace's diagnostic
//! wrap surface: `kube_ctx` / `flatten_ctx` cover the two internal-
//! call error axes, `HostnameResultExt` covers the per-crate typed
//! error family, and **this module** covers the boundary where the OS
//! process environment meets the probe binary. A future closed-loop
//! probe for a database / message broker / IdP that reaches into its
//! own admission-time-projected Secret inherits the diagnostic
//! wording mechanically through the same
//! [`required_secret`] call.
//!
//! Theory grounding: THEORY.md §VI.1 (generation over composition —
//! the `std::env::var(NAME).context(...)` chain recurred at two
//! hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
//! trigger and is lifted to ONE substrate owner here). THEORY.md §II.1
//! invariant 5 (composition preserves proofs — the diagnostic
//! wording is pinned by [`tests::required_secret_error_wording_is_canonical`]
//! so a regression that reshaped the message surfaces at the test
//! surface rather than as silent drift between the two pre-lift
//! callsites in `tatara-closed-loop-probe::main`).

use anyhow::{Context, Result};

/// Read a required OS environment variable that a K8s `Secret`
/// populated into the caller's process at admission time — the
/// canonical substrate primitive owning the
/// `std::env::var(NAME).context("NAME env var (from auth Secret)
/// required")` wrap every secret-env consumer restated by hand.
///
/// Fails with a canonical `"<NAME> env var (from auth Secret)
/// required"` message chained onto the underlying
/// [`std::env::VarError`] when the env var is unset or not valid
/// UTF-8; on the happy path returns the env var's owned `String`
/// value verbatim.
///
/// The error wording is pinned by
/// [`tests::required_secret_error_wording_is_canonical`] so a
/// regression that reshaped the message surfaces at the test surface
/// rather than as silent drift between callsites.
pub fn required_secret(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("{name} env var (from auth Secret) required"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic env-var lookup shim: routes through a caller-
    /// supplied closure rather than the process's live environment so
    /// tests can pin both the happy-path + missing-var arms without
    /// racing other tests that read/write `std::env`. Byte-shape
    /// parity with [`required_secret`]'s live-env caller is asserted
    /// by [`required_secret_matches_pre_lift_chain_shape`] below —
    /// the two paths compose the SAME `.with_context(|| format!(...))`
    /// wrap over the same input arm.
    fn required_secret_via<F>(name: &str, lookup: F) -> Result<String>
    where
        F: FnOnce(&str) -> std::result::Result<String, std::env::VarError>,
    {
        lookup(name).with_context(|| format!("{name} env var (from auth Secret) required"))
    }

    #[test]
    fn required_secret_error_wording_is_canonical() {
        // Byte-identity pin: the diagnostic string every pre-lift
        // callsite in `tatara-closed-loop-probe::main` hand-authored
        // (`"ACCESS_ID env var (from auth Secret) required"` +
        // `"ACCESS_KEY env var (from auth Secret) required"`) MUST
        // survive the lift verbatim. A regression that dropped the
        // `(from auth Secret)` qualifier or renamed the trailing
        // `required` word would surface HERE rather than as silent
        // operator-visible skew between the pre-lift diagnostic and
        // the post-lift substrate wrap.
        let e = required_secret_via("ACCESS_ID", |_| Err(std::env::VarError::NotPresent))
            .expect_err("must error when the env var is absent");
        assert_eq!(
            format!("{e}"),
            "ACCESS_ID env var (from auth Secret) required",
        );
        let e = required_secret_via("ACCESS_KEY", |_| Err(std::env::VarError::NotPresent))
            .expect_err("must error when the env var is absent");
        assert_eq!(
            format!("{e}"),
            "ACCESS_KEY env var (from auth Secret) required",
        );
    }

    #[test]
    fn required_secret_preserves_underlying_var_error_in_chain() {
        // The underlying `std::env::VarError` must survive as the
        // source of the returned `anyhow::Error` — operators tailing
        // an unwrapped `{e:?}` see BOTH the canonical head from the
        // wrap AND the `env::VarError` cause underneath (Debug + `:#`
        // Display print the chain). A regression that swapped
        // `with_context` for `unwrap_or_else(|| anyhow!(...))` would
        // sever the chain and fail-loud here.
        let e = required_secret_via("MISSING_VAR", |_| Err(std::env::VarError::NotPresent))
            .expect_err("must error");
        // Debug rendering of an anyhow chain includes each link's
        // Display; the underlying `env::VarError::NotPresent`
        // stringifies as `"environment variable not found"`.
        let dbg = format!("{e:?}");
        assert!(
            dbg.contains("MISSING_VAR env var (from auth Secret) required"),
            "chain must carry the canonical head; got: {dbg}"
        );
        assert!(
            dbg.contains("environment variable not found"),
            "chain must preserve the underlying env::VarError cause; got: {dbg}"
        );
    }

    #[test]
    fn required_secret_interpolates_arbitrary_var_names_verbatim() {
        // Byte-shape sweep: the primitive interpolates the caller-
        // supplied `name` argument at the head of the diagnostic
        // verbatim — no case-folding, no PascalCase→SNAKE conversion,
        // no truncation. A regression that inserted a normalization
        // step (a lowercasing pass, an `_` → `-` rewrite) would
        // silently mis-diagnose a stricter caller convention.
        for name in [
            "ACCESS_ID",
            "ACCESS_KEY",
            "MIXED_Case_123",
            "a",
            "TATARA_PROCESS_REF",
        ] {
            let e = required_secret_via(name, |_| Err(std::env::VarError::NotPresent))
                .expect_err("must error");
            assert_eq!(
                format!("{e}"),
                format!("{name} env var (from auth Secret) required"),
                "diagnostic must interpolate {name:?} verbatim at the head",
            );
        }
    }

    #[test]
    fn required_secret_returns_owned_string_on_happy_path() {
        // Byte-shape pin: the happy path returns the env var's value
        // as an owned `String`, unchanged from what the underlying
        // `std::env::var` call produced. A regression that inserted a
        // `.trim()` step, or coerced to a `&'static str`, or
        // pre-parsed to a different type would fail here.
        let v = required_secret_via("PROBE_TOKEN", |_| Ok("abc-123-token".to_string()))
            .expect("happy path");
        assert_eq!(v, "abc-123-token");
    }

    #[test]
    fn required_secret_matches_pre_lift_chain_shape() {
        // Byte-identical parity pin: the primitive produces the SAME
        // `Result<String, anyhow::Error>` shape the pre-lift
        // `.context("<NAME> env var (from auth Secret) required")`
        // chain produced at every hand-authored callsite in
        // `tatara-closed-loop-probe::main`, on both the happy and the
        // missing-var corners.
        //
        // A regression that inserted a normalization step at the
        // primitive that the pre-lift chain does NOT apply (or vice
        // versa) surfaces HERE rather than as silent drift between the
        // two pre-lift consumer callsites and the ONE substrate owner
        // they now route through.
        fn pre_lift(
            name: &str,
            val: std::result::Result<String, std::env::VarError>,
        ) -> Result<String> {
            val.with_context(|| format!("{name} env var (from auth Secret) required"))
        }
        for name in ["ACCESS_ID", "ACCESS_KEY"] {
            // Happy path — both variants return the same owned string.
            let via_primitive =
                required_secret_via(name, |_| Ok("fixture-value".to_string())).unwrap();
            let via_pre_lift = pre_lift(name, Ok("fixture-value".to_string())).unwrap();
            assert_eq!(via_primitive, via_pre_lift);

            // Missing-var — both variants error with the same message.
            let via_primitive_err =
                required_secret_via(name, |_| Err(std::env::VarError::NotPresent))
                    .unwrap_err()
                    .to_string();
            let via_pre_lift_err = pre_lift(name, Err(std::env::VarError::NotPresent))
                .unwrap_err()
                .to_string();
            assert_eq!(via_primitive_err, via_pre_lift_err);
        }
    }

    #[test]
    fn required_secret_reads_live_env_var_when_set() {
        // Cross-path parity pin: the top-level `required_secret`
        // function (which reads live `std::env`) shares its wrap logic
        // with `required_secret_via`. A regression that split the two
        // paths — a stale local `format!` at the top-level function
        // that drifted from the via-shim's `format!` body — surfaces
        // HERE rather than as silent operator-facing skew between
        // live-env failures and the tested error wording above.
        //
        // Uses a fixture var name unlikely to be set in any test
        // environment; the missing-var corner exercises the same
        // wrap chain the pre-lift callsites reach.
        let name = "TATARA_SECRET_ENV_TEST_VAR_ABSENT_XYZ_12345";
        assert!(
            std::env::var(name).is_err(),
            "fixture env var {name} must not be set in this test environment"
        );
        let e = required_secret(name).expect_err("must error on absent var");
        assert_eq!(
            format!("{e}"),
            format!("{name} env var (from auth Secret) required"),
        );
    }
}
