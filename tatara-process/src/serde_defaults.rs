//! Workspace-canonical serde-default owners — the ONE substrate site
//! every `#[serde(default = "…")]` slot on a plain scalar constant
//! (`true`, `false`, `0u32`, …) reaches for its serde-default fn.
//!
//! Pre-lift each shape lived as a per-file `fn default_<X>() -> <T>
//! { <constant> }` private shim; because serde's `default = "path"`
//! contract dispatches on a NAMED function rather than an inline
//! literal, every consumer file grew its own local shim body — and the
//! FIVE consumer files that all needed the `-> bool { true }` shape
//! each spelled the SAME three-line body byte-identically. Post-lift
//! every consumer routes through the ONE substrate owner here so a
//! future normalization (a debug-build assertion, a per-fleet override
//! injected via env var, a rename that folds `default_true` into a
//! `serde_defaults::TRUE` typed handle) lands at THIS module and every
//! downstream serde-default consumer inherits the upgrade mechanically.
//!
//! Peer to [`crate::lifetime::default_ephemeral_ttl`] +
//! [`crate::lifetime::default_ephemeral_max_concurrent`] on the
//! "workspace-canonical serde-default owners" axis. Where those two
//! primitives own the *ephemeral-authoring-surface-specific* defaults
//! (`"1h"` / `1u32`), this module owns the *plain-scalar-constant*
//! defaults that no CRD-specific axis binds to a single crate — the
//! `-> bool { true }` shape recurs anywhere a boolean field's serde
//! default is "on" for backward-compat, safe-default, or feature-flag
//! reasons, and each variant of that shape wants ONE substrate owner
//! rather than a private per-file shim.

/// Workspace-canonical `-> bool { true }` serde-default owner — the
/// ONE substrate site every `#[serde(default = "…")]` slot on a `bool`
/// field whose "on by default" invariant matches the pre-lift shape
/// routes through.
///
/// Pre-lift the SAME 3-line `fn default_true() -> bool { true }` shim
/// was hand-authored at FIVE workspace-wide sites past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold, each serving as the
/// `#[serde(default = "default_true")]` seed on a different `bool`
/// field:
///
/// * [`crate::matrix::BreatheSpec::dry_run`] — the env-matrix
///   observability breathe-band spec's "start observe-only" flag; the
///   safe-by-default posture for a fresh breathe sweep.
/// * [`crate::spec::SignalPolicy::sigkill_force`] — the process-spec
///   signal policy's "permit force-reap via SIGKILL" allowance; the
///   permissive default matching Unix's own SIGKILL semantics.
/// * [`crate::intent::FluxIntent::decrypt_sops`] — the flux-intent
///   spec's SOPS-decryption toggle; the pleme-io-convention default of
///   "SOPS envelopes decrypt on apply".
/// * [`crate::table::ProcessTableSpec::orphan_reaping_enabled`] — the
///   process-table spec's PID-1-adopts-orphans switch; the Unix-
///   process-model default of "PID 1 reaps orphans".
/// * `tatara-reconciler::ephemeral_defaults::EphemeralDefaults::emit_oci_repository`
///   — the reconciler's operator-facing "auto-emit OCIRepository peer
///   for `oci://` chart refs" toggle; the convenience default matching
///   the sibling render path.
///
/// All FIVE sites walked the SAME 3-line body — `fn default_true()
/// -> bool { true }` — and served the SAME "on-by-default" invariant
/// through the SAME serde `default = "…"` contract. Post-lift every
/// consumer's serde slot reads `default = "…serde_defaults::default_true"`
/// (in-crate via `crate::serde_defaults::default_true`; cross-crate
/// via `tatara_process::serde_defaults::default_true`) and the local
/// per-file `fn default_true` shim disappears at each site.
///
/// Return-form axis: `bool` — matches the serde `default = "…"` slot
/// contract exactly (serde invokes the named function and stamps its
/// returned owned value into the field). The paired
/// [`DEFAULT_TRUE`] const exposes the underlying `bool` for callers
/// that want a compile-time handle (a `const fn`-visible pin, a
/// `matches!(x, DEFAULT_TRUE)` peer check, a const-context comparison).
///
/// A future normalization on the workspace-canonical "on-by-default"
/// invariant (a debug-build assertion that the caller has permission to
/// stamp `true` at all, a per-fleet override injected via a
/// `TATARA_DEFAULT_TRUE_<slot>` env var, a shift to a typed
/// `SerdeDefault<bool>` newtype that carries the semantic label) lands
/// at THIS ONE substrate primitive and every downstream serde-default
/// consumer inherits the upgrade mechanically — no per-site edit at
/// any of the FIVE listed callers or at future consumers (a new
/// bool-slot serde default, a fleet-wide dashboard reading the
/// canonical "on" wire-form, a new tatara-eval fixture).
///
/// Peer to [`crate::lifetime::default_ephemeral_ttl`] +
/// [`crate::lifetime::default_ephemeral_max_concurrent`] on the
/// workspace-canonical serde-default-owner axis — those primitives own
/// the ephemeral-authoring-surface-specific defaults (`"1h"` / `1u32`),
/// while this primitive owns the axis-agnostic `-> bool { true }`
/// scalar-constant default.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// `fn default_true() -> bool { true }` 3-line body recurred at FIVE
/// hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger, spanning two workspace crates + four `tatara-process`
/// modules, and is lifted onto ONE workspace-wide substrate owner
/// here). THEORY.md §II.1 invariant 5 (composition preserves proofs —
/// the pins bind the wire-form at fail-before-pass-after granularity
/// so a regression that drifted the returned bool surfaces at
/// [`tests::default_true_returns_true_bytewise`] rather than as silent
/// operator-facing skew across the five downstream consumers).
#[must_use]
pub fn default_true() -> bool {
    DEFAULT_TRUE
}

/// Workspace-canonical `bool` handle over the same `true` value
/// [`default_true`] returns. Use this const for compile-time
/// comparisons and const-context readers; use [`default_true`] for
/// the serde `default = "…"` slot contract.
pub const DEFAULT_TRUE: bool = true;

#[cfg(test)]
mod tests {
    use super::*;

    // ─── default_true substrate pins ─────────────────────────────────
    //
    // Bind [`default_true`] + the paired [`DEFAULT_TRUE`] const at
    // fail-before-pass-after granularity so a regression that drifted
    // the wire-form default (a shift from `true` to `false` at ONLY
    // the const, a decoupling of the const from the fn's returned
    // value, a rename that broke the serde `default = "…"` path
    // resolution) surfaces HERE rather than as silent operator-visible
    // skew across the FIVE serde-default consumers that ride through
    // this ONE substrate owner.

    #[test]
    fn default_true_returns_true_bytewise() {
        // Byte-shape parity with the FIVE hand-authored pre-lift shims
        // that each returned `true`. A regression that flipped the
        // returned bool (an accidental `false` seed, a `!true` typo
        // that survives parse, a shift to a typed newtype without a
        // Deref) fails HERE rather than at the five downstream
        // consumers whose "on-by-default" invariant would silently
        // become "off by default".
        assert!(
            default_true(),
            "default_true must return `true` bytewise; a regression \
             surfaces here rather than as five-way skew across \
             BreatheSpec::dry_run + SignalPolicy::sigkill_force + \
             FluxIntent::decrypt_sops + ProcessTableSpec::orphan_reaping_enabled + \
             EphemeralDefaults::emit_oci_repository",
        );
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    // The `assert!(DEFAULT_TRUE, …)` shape IS the pin: we're binding
    // the const's wire-form value at test time. Clippy sees a compile-
    // time-constant assertion and flags it; the flag is exactly the
    // corner we WANT to trip if a future edit drifts the const to
    // `false` — the test would then fail with a comprehensible message
    // rather than the const change slipping past.
    fn default_true_wire_form_const_matches_fn_return_bytewise() {
        // Cross-form coherence pin: the `pub const DEFAULT_TRUE: bool`
        // handle and the `pub fn default_true() -> bool` owner MUST
        // project onto the SAME wire-form value. A regression that
        // updated one but not the other (e.g. lifted the const to
        // `false` for a fleet-wide safe-default shift but forgot the
        // fn body, or vice versa) would silently produce two divergent
        // workspace-canonical defaults — the const for compile-time
        // consumers, the fn for serde-default consumers. Pin the two
        // projections at equality.
        assert!(
            DEFAULT_TRUE,
            "DEFAULT_TRUE const must byte-match the pre-lift wire-form \
             default `true`",
        );
        assert_eq!(
            default_true(),
            DEFAULT_TRUE,
            "default_true() must byte-match the paired DEFAULT_TRUE \
             const — a divergence would silently skew serde-default \
             consumers vs compile-time const readers",
        );
    }

    #[test]
    fn default_true_composes_at_breathe_envelope_serde_default() {
        // End-to-end: the [`crate::matrix::BreatheEnvelope::dry_run`]
        // serde default MUST route through the substrate owner. A
        // YAML fragment that omits the `dryRun:` field parses into a
        // BreatheEnvelope whose `dry_run` reads bytewise-identical to
        // [`default_true`]. A regression that reintroduced a local
        // `fn default_true` shim in `matrix.rs` (bypassing the
        // substrate) would produce a silent skew between the breathe
        // envelope's "start observe-only" invariant and the four peer
        // consumers.
        let yaml = "\
dimensions: []
";
        let spec: crate::matrix::BreatheEnvelope =
            serde_yaml::from_str(yaml).expect("BreatheEnvelope YAML parses");
        assert_eq!(
            spec.dry_run,
            default_true(),
            "BreatheEnvelope serde-default for the omitted `dryRun:` slot \
             must route through crate::serde_defaults::default_true \
             — a private-shim reintroduction skews the breathe-envelope \
             observability posture silently",
        );
    }

    #[test]
    fn default_true_composes_at_signal_policy_serde_default() {
        // Peer to the BreatheSpec pin above — pin the
        // [`crate::spec::SignalPolicy::sigkill_force`] serde default
        // at the substrate owner. A YAML fragment that omits
        // `sigkillForce:` parses into a SignalPolicy whose
        // `sigkill_force` reads bytewise-identical to
        // [`default_true`]. A regression that decoupled the
        // signal-policy default from the substrate would silently
        // flip the "permit force-reap" posture without any consumer
        // tripping.
        let yaml = "";
        let sp: crate::spec::SignalPolicy =
            serde_yaml::from_str(yaml).expect("SignalPolicy YAML parses");
        assert_eq!(
            sp.sigkill_force,
            default_true(),
            "SignalPolicy serde-default for the omitted `sigkillForce:` \
             slot must route through crate::serde_defaults::default_true",
        );
    }

    #[test]
    fn default_true_composes_at_process_table_spec_serde_default() {
        // Peer to the SignalPolicy pin above — pin the
        // [`crate::table::ProcessTableSpec::orphan_reaping_enabled`]
        // serde default at the substrate owner. A YAML fragment that
        // omits `orphanReapingEnabled:` parses into a ProcessTableSpec
        // whose `orphan_reaping_enabled` reads bytewise-identical to
        // [`default_true`]. A regression that decoupled the
        // process-table default from the substrate would silently
        // flip the "PID 1 reaps orphans" posture without any consumer
        // tripping.
        let yaml = "";
        let ts: crate::table::ProcessTableSpec =
            serde_yaml::from_str(yaml).expect("ProcessTableSpec YAML parses");
        assert_eq!(
            ts.orphan_reaping_enabled,
            default_true(),
            "ProcessTableSpec serde-default for the omitted \
             `orphanReapingEnabled:` slot must route through \
             crate::serde_defaults::default_true",
        );
    }
}
