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

/// Workspace-canonical `-> u32 { 480 }` SIGTERM→SIGKILL grace-window
/// serde-default owner — the ONE substrate site every
/// `#[serde(default = "…")]` slot on a `u32` field whose semantic is
/// "seconds to wait after delivering SIGTERM before escalating to
/// SIGKILL" routes through.
///
/// Pre-lift the SAME 3-line `fn default_<slot>() -> u32 { 480 }` shim
/// was hand-authored at TWO workspace-wide sites past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold, each serving as the
/// `#[serde(default = "default_<slot>")]` seed on a different `u32`
/// field carrying the SAME semantic wire-form (8-minute SIGTERM→
/// SIGKILL grace window):
///
/// * [`crate::spec::SignalPolicy::sigterm_grace_seconds`] — the
///   per-Process signal-policy's SIGTERM→SIGKILL grace window; the
///   escalation timer the Unix-process-model reconciler honors during
///   Exiting → Zombie.
/// * [`crate::table::ProcessTableSpec::sigterm_timeout_seconds`] — the
///   per-ProcessTable default SIGTERM→SIGKILL grace window; the
///   fallback the reconciler injects when a child Process omits its
///   own [`SignalPolicy::sigterm_grace_seconds`] slot.
///
/// Both sites walked the SAME 3-line body — `-> u32 { 480 }` — and
/// served the SAME "8-minute SIGTERM→SIGKILL escalation" invariant
/// through the SAME serde `default = "…"` contract. Post-lift every
/// consumer's serde slot reads
/// `default = "…serde_defaults::default_sigterm_grace_seconds"`
/// (in-crate via `crate::serde_defaults::default_sigterm_grace_seconds`;
/// cross-crate via
/// `tatara_process::serde_defaults::default_sigterm_grace_seconds`)
/// and the local per-file `fn default_sigterm_<slot>` shim disappears
/// at each site.
///
/// Return-form axis: `u32` — matches the serde `default = "…"` slot
/// contract exactly (serde invokes the named function and stamps its
/// returned owned value into the field). The paired
/// [`DEFAULT_SIGTERM_GRACE_SECONDS`] const exposes the underlying
/// `u32` for callers that want a compile-time handle (a `const fn`-
/// visible pin, a `matches!(x, DEFAULT_SIGTERM_GRACE_SECONDS)` peer
/// check, a const-context comparison against a k8s pod-eviction
/// grace annotation).
///
/// A future normalization on the workspace-canonical "SIGTERM→
/// SIGKILL grace" invariant (a shift to the k8s-recommended 30s
/// default, a per-fleet override injected via a `TATARA_SIGTERM_
/// GRACE_SECONDS` env var, a shift to a typed `GracePeriod`
/// newtype that carries the "seconds" unit) lands at THIS ONE
/// substrate primitive and every downstream serde-default consumer
/// inherits the upgrade mechanically — no per-site edit at either
/// listed caller or at future consumers (a new Process-plane
/// termination window, a per-container override in `ContainerIntent`,
/// a `PoolSpec::sigterm_grace_seconds` slot).
///
/// Peer to [`default_true`] on the workspace-canonical serde-default-
/// owner axis. Where [`default_true`] owns the `-> bool { true }`
/// on-by-default scalar shape, this primitive owns the `-> u32
/// { 480 }` SIGTERM-grace scalar shape.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// `fn default_<slot>() -> u32 { 480 }` 3-line body recurred at TWO
/// hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger, and is lifted onto ONE workspace-wide substrate owner
/// here). THEORY.md §II.1 invariant 5 (composition preserves proofs —
/// the pins bind the wire-form at fail-before-pass-after granularity
/// so a regression that drifted the returned u32 surfaces at
/// [`tests::default_sigterm_grace_seconds_returns_480_bytewise`]
/// rather than as silent operator-facing skew across the two
/// downstream consumers).
#[must_use]
pub fn default_sigterm_grace_seconds() -> u32 {
    DEFAULT_SIGTERM_GRACE_SECONDS
}

/// Workspace-canonical `u32` handle over the same `480` value
/// [`default_sigterm_grace_seconds`] returns. Use this const for
/// compile-time comparisons and const-context readers; use
/// [`default_sigterm_grace_seconds`] for the serde `default = "…"`
/// slot contract.
pub const DEFAULT_SIGTERM_GRACE_SECONDS: u32 = 480;

/// Workspace-canonical `-> u32 { 600 }` Zombie-phase force-reap
/// timeout serde-default owner — the ONE substrate site every
/// `#[serde(default = "…")]` slot on a `u32` field whose semantic is
/// "seconds a Process is permitted to sit in `Zombie` before PID 1
/// force-reaps it" routes through, PLUS the ONE substrate anchor
/// every hand-authored `ProcessTableSpec` composer that pre-populates
/// the `zombie_timeout_seconds` slot with the workspace-canonical
/// wire-form routes through.
///
/// Pre-lift the SAME `600` u32 constant lived at TWO workspace-wide
/// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold, each
/// encoding the SAME semantic wire-form (10-minute Zombie force-reap
/// window):
///
/// * [`crate::table::ProcessTableSpec::zombie_timeout_seconds`] — the
///   per-ProcessTable Zombie force-reap window; the `#[serde(default
///   = "…")]` seed the wire-parser stamps when a serialized
///   `ProcessTable` YAML omits its own `zombieTimeoutSeconds:` slot.
/// * `tatara_reconciler::patch::ensure_process_table` — the
///   ProcessTable-singleton bootstrap composer; the explicit
///   `zombie_timeout_seconds: 600` slot in the hand-authored
///   [`crate::table::ProcessTableSpec`] struct literal used to
///   materialize a fresh singleton on first observation.
///
/// Both sites walked the SAME bare `600u32` constant and served the
/// SAME "10-minute Zombie force-reap" invariant — the serde path
/// through the `#[serde(default = "…")]` machinery, the composer path
/// through the explicit struct-literal slot. Post-lift the serde slot
/// reads `default = "…serde_defaults::default_zombie_timeout_seconds"`
/// (in-crate via `crate::serde_defaults::default_zombie_timeout_seconds`;
/// cross-crate via
/// `tatara_process::serde_defaults::default_zombie_timeout_seconds`)
/// and the reconciler composer feeds the substrate fn directly at the
/// `zombie_timeout_seconds:` slot in place of the bare `600` literal.
///
/// Return-form axis: `u32` — matches the serde `default = "…"` slot
/// contract exactly (serde invokes the named function and stamps its
/// returned owned value into the field). The paired
/// [`DEFAULT_ZOMBIE_TIMEOUT_SECONDS`] const exposes the underlying
/// `u32` for callers that want a compile-time handle (a `const fn`-
/// visible pin, a `matches!(x, DEFAULT_ZOMBIE_TIMEOUT_SECONDS)` peer
/// check, a const-context comparison against a k8s pod-termination
/// grace annotation).
///
/// A future normalization on the workspace-canonical "Zombie force-
/// reap" invariant (a shift to the k8s pod-eviction-recommended 300s
/// default, a per-fleet override injected via a
/// `TATARA_ZOMBIE_TIMEOUT_SECONDS` env var, a shift to a typed
/// `ReapDeadline` newtype that carries the "seconds since Zombie
/// entry" phase-anchored semantics) lands at THIS ONE substrate
/// primitive and every downstream serde-default consumer PLUS every
/// hand-authored `ensure_process_table`-shaped composer inherits the
/// upgrade mechanically — no per-site edit at either listed caller or
/// at future consumers (a per-Process `zombieTimeoutSecondsOverride`
/// slot, a `PoolSpec`-scoped Zombie window, a new debug-build sub-10s
/// override for local development).
///
/// Peer to [`default_sigterm_grace_seconds`] on the workspace-
/// canonical "termination-phase timing" scalar axis. Where
/// [`default_sigterm_grace_seconds`] owns the `-> u32 { 480 }`
/// SIGTERM→SIGKILL escalation window (the Exiting → Zombie phase
/// transition's grace clock), this primitive owns the `-> u32 { 600 }`
/// Zombie → Reaped force-reap window (the phase transition that
/// follows). The two primitives compose sequentially at the
/// reconciler: a `Process` with the default policy spends up to
/// [`default_sigterm_grace_seconds`] seconds in Exiting, then up to
/// [`default_zombie_timeout_seconds`] seconds in Zombie, then is
/// force-reaped and cascaded via ownerRefs.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// bare `600u32` constant recurred at TWO hand-authored sites past
/// the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, spanning two
/// workspace crates, and is lifted onto ONE workspace-wide substrate
/// owner here). THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — the pins bind the wire-form at fail-before-pass-after
/// granularity so a regression that drifted the returned u32 surfaces
/// at [`tests::default_zombie_timeout_seconds_returns_600_bytewise`]
/// rather than as silent operator-facing skew across the two
/// downstream consumers).
#[must_use]
pub fn default_zombie_timeout_seconds() -> u32 {
    DEFAULT_ZOMBIE_TIMEOUT_SECONDS
}

/// Workspace-canonical `u32` handle over the same `600` value
/// [`default_zombie_timeout_seconds`] returns. Use this const for
/// compile-time comparisons and const-context readers; use
/// [`default_zombie_timeout_seconds`] for the serde `default = "…"`
/// slot contract.
pub const DEFAULT_ZOMBIE_TIMEOUT_SECONDS: u32 = 600;

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

    // ─── default_sigterm_grace_seconds substrate pins ────────────────
    //
    // Bind [`default_sigterm_grace_seconds`] + the paired
    // [`DEFAULT_SIGTERM_GRACE_SECONDS`] const at fail-before-pass-after
    // granularity so a regression that drifted the wire-form default
    // (a shift from `480` to `30` at ONLY the const, a decoupling of
    // the const from the fn's returned value, a rename that broke the
    // serde `default = "…"` path resolution) surfaces HERE rather than
    // as silent operator-visible skew across the TWO serde-default
    // consumers that ride through this ONE substrate owner.

    #[test]
    fn default_sigterm_grace_seconds_returns_480_bytewise() {
        // Byte-shape parity with the TWO hand-authored pre-lift shims
        // that each returned `480`. A regression that flipped the
        // returned u32 (an accidental `48` or `4800` seed, a shift to
        // millis without a unit rename, a shift to a typed newtype
        // without a Deref) fails HERE rather than at the two
        // downstream consumers whose "8-minute SIGTERM→SIGKILL grace"
        // invariant would silently become sub-minute or 80-minute.
        assert_eq!(
            default_sigterm_grace_seconds(),
            480,
            "default_sigterm_grace_seconds must return `480` bytewise; \
             a regression surfaces here rather than as two-way skew \
             across SignalPolicy::sigterm_grace_seconds + \
             ProcessTableSpec::sigterm_timeout_seconds",
        );
    }

    #[test]
    fn default_sigterm_grace_seconds_wire_form_const_matches_fn_return_bytewise() {
        // Cross-form coherence pin: the `pub const
        // DEFAULT_SIGTERM_GRACE_SECONDS: u32` handle and the `pub fn
        // default_sigterm_grace_seconds() -> u32` owner MUST project
        // onto the SAME wire-form value. A regression that updated
        // one but not the other (e.g. lifted the const to `30` for a
        // fleet-wide k8s-alignment shift but forgot the fn body, or
        // vice versa) would silently produce two divergent workspace-
        // canonical defaults — the const for compile-time consumers,
        // the fn for serde-default consumers. Pin the two projections
        // at equality.
        assert_eq!(
            DEFAULT_SIGTERM_GRACE_SECONDS, 480,
            "DEFAULT_SIGTERM_GRACE_SECONDS const must byte-match the \
             pre-lift wire-form default `480`",
        );
        assert_eq!(
            default_sigterm_grace_seconds(),
            DEFAULT_SIGTERM_GRACE_SECONDS,
            "default_sigterm_grace_seconds() must byte-match the \
             paired DEFAULT_SIGTERM_GRACE_SECONDS const — a divergence \
             would silently skew serde-default consumers vs compile-\
             time const readers",
        );
    }

    #[test]
    fn default_sigterm_grace_seconds_composes_at_signal_policy_serde_default() {
        // End-to-end: the
        // [`crate::spec::SignalPolicy::sigterm_grace_seconds`] serde
        // default MUST route through the substrate owner. A YAML
        // fragment that omits `sigtermGraceSeconds:` parses into a
        // SignalPolicy whose `sigterm_grace_seconds` reads bytewise-
        // identical to [`default_sigterm_grace_seconds`]. A regression
        // that reintroduced a local `fn default_sigterm_grace` shim in
        // `spec.rs` (bypassing the substrate) would produce a silent
        // skew between the per-Process signal-policy escalation window
        // and the ProcessTable-plane default that peers with it.
        let yaml = "";
        let sp: crate::spec::SignalPolicy =
            serde_yaml::from_str(yaml).expect("SignalPolicy YAML parses");
        assert_eq!(
            sp.sigterm_grace_seconds,
            default_sigterm_grace_seconds(),
            "SignalPolicy serde-default for the omitted \
             `sigtermGraceSeconds:` slot must route through \
             crate::serde_defaults::default_sigterm_grace_seconds \
             — a private-shim reintroduction skews the per-Process \
             SIGTERM→SIGKILL escalation window silently",
        );
    }

    #[test]
    fn default_sigterm_grace_seconds_composes_at_process_table_spec_serde_default() {
        // Peer to the SignalPolicy pin above — pin the
        // [`crate::table::ProcessTableSpec::sigterm_timeout_seconds`]
        // serde default at the substrate owner. A YAML fragment that
        // omits `sigtermTimeoutSeconds:` parses into a
        // ProcessTableSpec whose `sigterm_timeout_seconds` reads
        // bytewise-identical to [`default_sigterm_grace_seconds`]. A
        // regression that decoupled the process-table default from the
        // substrate would silently flip the "8-minute grace" invariant
        // at the fallback layer while the per-Process layer stayed
        // put, producing table-scoped children with a mismatched
        // escalation window.
        let yaml = "";
        let ts: crate::table::ProcessTableSpec =
            serde_yaml::from_str(yaml).expect("ProcessTableSpec YAML parses");
        assert_eq!(
            ts.sigterm_timeout_seconds,
            default_sigterm_grace_seconds(),
            "ProcessTableSpec serde-default for the omitted \
             `sigtermTimeoutSeconds:` slot must route through \
             crate::serde_defaults::default_sigterm_grace_seconds",
        );
    }

    #[test]
    fn default_sigterm_grace_seconds_composes_at_signal_policy_impl_default() {
        // `SignalPolicy::default()` reconstructs the same wire-form
        // 480 through the substrate — pin it. A regression that
        // decoupled the Default-impl path from the serde-default path
        // (e.g. hard-coded `480` at the Default impl, then drifted the
        // substrate owner to `30`) would produce two different
        // "default" SignalPolicies depending on construction path.
        let sp = crate::spec::SignalPolicy::default();
        assert_eq!(
            sp.sigterm_grace_seconds,
            default_sigterm_grace_seconds(),
            "SignalPolicy::default() must reconstruct \
             `sigterm_grace_seconds` through the substrate owner — a \
             hand-authored literal at the Default impl would decouple \
             the two construction paths",
        );
    }

    // ─── default_zombie_timeout_seconds substrate pins ───────────────
    //
    // Bind [`default_zombie_timeout_seconds`] + the paired
    // [`DEFAULT_ZOMBIE_TIMEOUT_SECONDS`] const at fail-before-pass-
    // after granularity so a regression that drifted the wire-form
    // default (a shift from `600` to `300` at ONLY the const, a
    // decoupling of the const from the fn's returned value, a rename
    // that broke the serde `default = "…"` path resolution) surfaces
    // HERE rather than as silent operator-visible skew across the TWO
    // consumers that ride through this ONE substrate owner (the
    // `ProcessTableSpec::zombie_timeout_seconds` serde default AND
    // the `tatara_reconciler::patch::ensure_process_table` composer's
    // explicit `zombie_timeout_seconds:` slot).

    #[test]
    fn default_zombie_timeout_seconds_returns_600_bytewise() {
        // Byte-shape parity with the TWO hand-authored pre-lift sites
        // that each encoded `600` (the serde-default fn body in
        // `crate::table` + the composer's explicit struct-literal slot
        // in `tatara_reconciler::patch::ensure_process_table`). A
        // regression that flipped the returned u32 (an accidental `60`
        // or `6000` seed, a shift to millis without a unit rename, a
        // shift to a typed newtype without a Deref) fails HERE rather
        // than at the two downstream consumers whose "10-minute Zombie
        // force-reap" invariant would silently become sub-minute or
        // 100-minute.
        assert_eq!(
            default_zombie_timeout_seconds(),
            600,
            "default_zombie_timeout_seconds must return `600` bytewise; \
             a regression surfaces here rather than as two-way skew \
             across ProcessTableSpec::zombie_timeout_seconds + \
             tatara_reconciler::patch::ensure_process_table",
        );
    }

    #[test]
    fn default_zombie_timeout_seconds_wire_form_const_matches_fn_return_bytewise() {
        // Cross-form coherence pin: the `pub const
        // DEFAULT_ZOMBIE_TIMEOUT_SECONDS: u32` handle and the `pub fn
        // default_zombie_timeout_seconds() -> u32` owner MUST project
        // onto the SAME wire-form value. A regression that updated
        // one but not the other (e.g. lifted the const to `300` for a
        // fleet-wide k8s-alignment shift but forgot the fn body, or
        // vice versa) would silently produce two divergent workspace-
        // canonical defaults — the const for compile-time consumers,
        // the fn for serde-default + composer consumers. Pin the two
        // projections at equality.
        assert_eq!(
            DEFAULT_ZOMBIE_TIMEOUT_SECONDS, 600,
            "DEFAULT_ZOMBIE_TIMEOUT_SECONDS const must byte-match the \
             pre-lift wire-form default `600`",
        );
        assert_eq!(
            default_zombie_timeout_seconds(),
            DEFAULT_ZOMBIE_TIMEOUT_SECONDS,
            "default_zombie_timeout_seconds() must byte-match the \
             paired DEFAULT_ZOMBIE_TIMEOUT_SECONDS const — a divergence \
             would silently skew serde-default + composer consumers \
             vs compile-time const readers",
        );
    }

    #[test]
    fn default_zombie_timeout_seconds_composes_at_process_table_spec_serde_default() {
        // End-to-end: the
        // [`crate::table::ProcessTableSpec::zombie_timeout_seconds`]
        // serde default MUST route through the substrate owner. A
        // YAML fragment that omits `zombieTimeoutSeconds:` parses
        // into a ProcessTableSpec whose `zombie_timeout_seconds`
        // reads bytewise-identical to
        // [`default_zombie_timeout_seconds`]. A regression that
        // reintroduced a local `fn default_zombie_timeout` shim in
        // `table.rs` (bypassing the substrate) would produce a silent
        // skew between the wire-parser Zombie force-reap window and
        // the reconciler-composer Zombie force-reap window.
        let yaml = "";
        let ts: crate::table::ProcessTableSpec =
            serde_yaml::from_str(yaml).expect("ProcessTableSpec YAML parses");
        assert_eq!(
            ts.zombie_timeout_seconds,
            default_zombie_timeout_seconds(),
            "ProcessTableSpec serde-default for the omitted \
             `zombieTimeoutSeconds:` slot must route through \
             crate::serde_defaults::default_zombie_timeout_seconds",
        );
    }

    #[test]
    fn default_zombie_timeout_seconds_pairs_with_sigterm_grace_on_termination_axis() {
        // Composition pin: the two termination-phase timing primitives
        // ([`default_sigterm_grace_seconds`] +
        // [`default_zombie_timeout_seconds`]) fire sequentially at the
        // reconciler — Exiting-phase grace first, then Zombie-phase
        // force-reap. Pin both against their canonical wire-forms in
        // a single assertion so a regression that lifted one primitive
        // through a rename that shadowed the peer (or that drifted
        // both together in a coordinated typo) surfaces HERE. The
        // Zombie window MUST also be strictly greater than the SIGTERM
        // grace window (a process only enters Zombie once its
        // SIGTERM grace has already elapsed, so the reap deadline
        // must be at least the escalation deadline).
        assert_eq!(default_sigterm_grace_seconds(), 480);
        assert_eq!(default_zombie_timeout_seconds(), 600);
        assert!(
            default_zombie_timeout_seconds() > default_sigterm_grace_seconds(),
            "Zombie force-reap window must exceed the SIGTERM grace \
             window — the Zombie phase only begins after the SIGTERM \
             grace has elapsed, so a Zombie deadline shorter than the \
             SIGTERM deadline would mean force-reap fires before the \
             process ever entered Zombie",
        );
    }
}
