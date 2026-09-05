//! Unix process phases — authoritative state machine.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The Unix-authentic phase a Process is in.
///
/// Canonical transitions:
/// ```text
/// Pending → Forking → Execing → Running → Attested
///                                       ↘ Failed
/// Attested → Reconverging → Execing                       (SIGHUP, no zombie)
/// Attested → Releasing  → Exiting → Zombie → Reaped       (export-then-SIGTERM)
/// Attested → Exiting    → Zombie → Reaped                 (no-exports SIGTERM)
/// Failed   → Releasing  → Zombie → Reaped                 (post-mortem exports)
/// Failed   → Zombie     → Reaped                          (no-exports failed)
/// Running  → Exiting    → Zombie → Reaped                 (early SIGTERM, no exports)
/// Running  → Failed                                       (non-zero exit)
/// ```
///
/// `Releasing` is the export window — the reconciler runs declared
/// `ExportSpec`s (via tatara-export-worker Jobs) between the
/// terminal phase reached (`Attested` or `Failed`) and `Exiting` /
/// `Zombie`. A Process with no `lifetime.ephemeral.exports`, or
/// where no export's trigger matches the phase reached, skips
/// `Releasing` entirely. See [`crate::export`] + [`crate::lifetime`].
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    tatara_closed_set::DeriveClosedSet,
)]
#[closed_set(
    via = "as_str",
    unknown = "UnknownPhase",
    display,
    generate_unknown = "process phase"
)]
pub enum ProcessPhase {
    /// Admitted; PID not assigned yet.
    Pending,
    /// PID assigned in ProcessTable; parent linked; content hash computed.
    Forking,
    /// RENDER phase — evaluating Nix / expanding Lisp / rendering Helm;
    /// emitting Kustomization + HelmRelease CRs.
    Execing,
    /// Flux resources applied; boundary preconditions being checked.
    Running,
    /// All postconditions hold; three-pillar attestation written.
    Attested,
    /// SIGHUP received or drift detected; returning to Execing.
    Reconverging,
    /// Export window — running declared `ExportSpec`s before SIGTERM.
    /// Each export becomes a typed Job; the Process advances only
    /// when every Job has reached a terminal state. Failures here
    /// short-circuit straight to `Zombie` (the export attempt itself
    /// is attested; partial-success is fine for best-effort channels).
    Releasing,
    /// SIGTERM received; graceful shutdown; children draining.
    Exiting,
    /// Exited non-zero; awaiting reap.
    Failed,
    /// Exited; children gone; finalizer not yet released.
    Zombie,
    /// Finalizer released; K8s GC will remove.
    Reaped,
}

impl Default for ProcessPhase {
    fn default() -> Self {
        Self::Pending
    }
}

impl ProcessPhase {
    /// The closed set of phases — single source of truth that drives
    /// `as_str` / Display / `FromStr` so adding a variant updates every
    /// projection at once (and the `display_matches_as_str` +
    /// `all_phases_roundtrip_via_as_str` tests pin the bridge). Also
    /// used by the test sites that need to sweep every-other-variant
    /// (`reaped_is_sink`, `releasing_can_only_be_entered_from_terminal_gates`,
    /// `terminal_reached_gates_are_attested_and_failed`), so a new
    /// variant lands in ALL once and reaches every test by iteration
    /// rather than by per-test array maintenance.
    pub const ALL: [Self; 11] = [
        Self::Pending,
        Self::Forking,
        Self::Execing,
        Self::Running,
        Self::Attested,
        Self::Reconverging,
        Self::Releasing,
        Self::Exiting,
        Self::Failed,
        Self::Zombie,
        Self::Reaped,
    ];

    /// Canonical PascalCase wire-format projection. Used by Display
    /// (single source of truth) and by `FromStr` to identify the
    /// variant from its annotation / status-field representation.
    /// The serde rename derives produce the same form on the JSON
    /// boundary; this method exposes it to Rust callers (logs,
    /// annotation values, error messages) without re-serializing.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Forking => "Forking",
            Self::Execing => "Execing",
            Self::Running => "Running",
            Self::Attested => "Attested",
            Self::Reconverging => "Reconverging",
            Self::Releasing => "Releasing",
            Self::Exiting => "Exiting",
            Self::Failed => "Failed",
            Self::Zombie => "Zombie",
            Self::Reaped => "Reaped",
        }
    }

    /// True if the phase is a terminal sink with no further transitions.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Reaped)
    }

    /// True if the process has reached a running state (Running or Attested).
    pub const fn is_running(self) -> bool {
        matches!(self, Self::Running | Self::Attested)
    }

    /// True if the process is still eligible to receive SIGHUP/SIGUSR* signals.
    /// `Releasing` is alive — the Process hasn't been SIGTERM'd yet; its
    /// children (export Jobs) are running.
    pub const fn is_alive(self) -> bool {
        !matches!(self, Self::Zombie | Self::Reaped | Self::Failed)
    }

    /// True if the process has left the alive set — the closed-set
    /// complement of [`Self::is_alive`]. Sinks to `Failed | Zombie |
    /// Reaped`: the three phases where a Process is no longer
    /// converging and its supervisor (pool reconciler, allocation
    /// controller, cascade-delete GC) treats it as a terminated
    /// member for reap / replace / status-count decisions. Named on
    /// the "positive" pole so caller sites read as
    /// `phase.has_exited()` instead of `!phase.is_alive()` — the
    /// closed-set predicate family gains a symmetric member for the
    /// same reason [`Self::is_running`] sits next to [`Self::is_alive`]
    /// (both express live-set membership positively).
    ///
    /// Pre-lift the `Failed | Zombie | Reaped` set was hand-restated
    /// as an inline `matches!(phase, Failed | Zombie | Reaped)` on
    /// [`crate::pool::PoolMemberSnapshot::is_failed`] (peer to that
    /// snapshot's `is_healthy` which restated the `Running | Attested`
    /// set of [`Self::is_running`]). Both duplications now route
    /// through their respective substrate closed-set predicate, so a
    /// future variant added to the alive/dead partition (a new
    /// `Draining` phase, a rename of `Zombie` → `Terminated`) lands
    /// at the ONE closed-set surface here rather than as silent skew
    /// between the substrate's `is_alive` and the pool reconciler's
    /// downstream `is_failed` restatement.
    pub const fn has_exited(self) -> bool {
        !self.is_alive()
    }

    /// True if the phase is the export window — declared `ExportSpec`s
    /// run here before SIGTERM. Reserved for the reconciler's
    /// `handle_releasing` step + tatara-export-worker Job emission.
    pub const fn is_releasing(self) -> bool {
        matches!(self, Self::Releasing)
    }

    /// True if the phase is a terminal-reached gate (`Attested` or
    /// `Failed`) — the points where the reconciler decides whether
    /// to enter `Releasing`, jump straight to `Exiting`/`Zombie`, or
    /// stay (for inspection per `TeardownPolicy`).
    pub const fn is_terminal_reached(self) -> bool {
        matches!(self, Self::Attested | Self::Failed)
    }

    /// Canonical wire label stamped into the
    /// [`crate::annotations::RELEASED_FROM`] annotation when a
    /// Process transitions Attested/Failed → Releasing. Encodes the
    /// terminal-reached-gate axis into the two labels
    /// `handle_releasing`'s `advance_out_of_releasing` dispatch reads
    /// back through [`Self::parse_released_from`]. Non-gate phases
    /// (Running, Reconverging, etc.) collapse to `"Attested"` per the
    /// forward-compat invariant `p_current_phase_str` promised
    /// pre-lift, so an unexpected observed-phase never leaks into the
    /// annotation as a Zombie-routing "Failed" label.
    ///
    /// Peer to [`Self::parse_released_from`] on the encoder/decoder
    /// pair — a wire-format drift on `Failed`'s `as_str` reaches both
    /// sites through the ONE substrate owner.
    pub const fn released_from_label(self) -> &'static str {
        match self {
            Self::Failed => Self::Failed.as_str(),
            _ => Self::Attested.as_str(),
        }
    }

    /// Decode a [`crate::annotations::RELEASED_FROM`] annotation
    /// value back to the terminal-reached gate the Process came
    /// through. Mirror of [`Self::released_from_label`]: only the
    /// exact string [`Self::Failed`]`.as_str()` decodes to `Failed`;
    /// every other value (including `None`, an empty string, a
    /// case-drifted "failed", a value from a future phase variant)
    /// collapses to `Attested`, preserving `released_from_annotation`'s
    /// pre-lift forward-compat semantics byte-for-byte.
    ///
    /// The two together form a total closed-set projection over
    /// `{Attested, Failed}` — every input on both sides maps into
    /// the gate set, and the composition
    /// `parse_released_from(Some(p.released_from_label()))` is the
    /// identity on `{Attested, Failed}` (pinned by
    /// `released_from_label_and_parse_are_inverse_on_terminal_gates`).
    pub fn parse_released_from(s: Option<&str>) -> Self {
        match s {
            Some(v) if v == Self::Failed.as_str() => Self::Failed,
            _ => Self::Attested,
        }
    }

    /// True if the phase transition `self → next` is legal.
    pub const fn can_transition_to(self, next: Self) -> bool {
        use ProcessPhase::*;
        matches!(
            (self, next),
            (Pending, Forking)
                | (Forking, Execing)
                | (Execing, Running)
                | (Execing, Failed)
                | (Running, Attested)
                | (Running, Exiting)
                | (Running, Failed)
                | (Running, Reconverging)
                | (Attested, Reconverging)
                | (Attested, Releasing)
                | (Attested, Exiting)
                | (Failed, Releasing)
                | (Failed, Zombie)
                | (Releasing, Exiting)
                | (Releasing, Zombie)
                | (Reconverging, Execing)
                | (Exiting, Zombie)
                | (Zombie, Reaped)
        )
    }
}

// `impl FromStr for ProcessPhase` +
// `impl tatara_lisp::ClosedSet for ProcessPhase` +
// `impl std::fmt::Display for ProcessPhase` +
// `pub struct UnknownPhase(pub String)` are all generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", unknown = "UnknownPhase", display,
// generate_unknown = "process phase")]` on the enum declaration
// above. `label` delegates to the inherent `ProcessPhase::as_str`
// — the inherent name (PascalCase `as_str`) stays the load-bearing
// wire-vocabulary projection that matches the serde rename + the
// CRD `enum:` enumeration verbatim, while generic `T: ClosedSet`
// consumers reach the STABLE workspace-wide name (`label`). The
// `display` flag emits the `f.write_str(self.as_str())` delegation
// block at the same proc-macro site. The carrier is named
// `UnknownPhase` (not the auto-derived `UnknownProcessPhase`)
// because the short name is the published public-API surface every
// downstream caller imports — `#[closed_set(unknown =
// "UnknownPhase")]` pins it. The explicit `generate_unknown =
// "process phase"` label overrides the auto-derived "process phase"
// (which happens to match byte-for-byte — pinning it here keeps the
// pre-lift wording stable against any future change to the
// `pascal_to_spaced_lowercase` helper's behavior). Symmetric to
// every other `#[derive(DeriveClosedSet)]` implementor across the
// crate (`WorkloadKind`, `VerificationPhase`, `MustReachPhase`,
// `SighupStrategy`, `TeardownPolicy`, `ConditionKind`, every
// classification axis, every pool/export/allocation closed-set).

#[cfg(test)]
mod tests {
    use super::ProcessPhase::*;

    #[test]
    fn canonical_path_is_legal() {
        assert!(Pending.can_transition_to(Forking));
        assert!(Forking.can_transition_to(Execing));
        assert!(Execing.can_transition_to(Running));
        assert!(Running.can_transition_to(Attested));
        assert!(Attested.can_transition_to(Reconverging));
        assert!(Reconverging.can_transition_to(Execing));
        assert!(Attested.can_transition_to(Exiting));
        assert!(Exiting.can_transition_to(Zombie));
        assert!(Zombie.can_transition_to(Reaped));
    }

    /// Releasing path — Attested or Failed may detour through the
    /// export window before terminating. Releasing is itself a
    /// legal source for Exiting (happy path) or Zombie (export-
    /// worker terminal-failure shortcut).
    #[test]
    fn releasing_path_is_legal() {
        assert!(Attested.can_transition_to(Releasing));
        assert!(Failed.can_transition_to(Releasing));
        assert!(Releasing.can_transition_to(Exiting));
        assert!(Releasing.can_transition_to(Zombie));
        // Releasing is alive — children (export Jobs) still running.
        assert!(Releasing.is_alive());
        // Releasing is not a terminal-reached gate.
        assert!(!Releasing.is_terminal_reached());
    }

    #[test]
    fn terminal_reached_gates_are_attested_and_failed() {
        assert!(Attested.is_terminal_reached());
        assert!(Failed.is_terminal_reached());
        // Sweep every other variant via ALL so a future variant is
        // covered automatically (was a hand-maintained 9-entry array).
        for p in super::ProcessPhase::ALL {
            if matches!(p, Attested | Failed) {
                continue;
            }
            assert!(!p.is_terminal_reached(), "{p:?} is not a terminal gate");
        }
    }

    #[test]
    fn releasing_can_only_be_entered_from_terminal_gates() {
        // Releasing has exactly two legal entries — the terminal-
        // reached gates. Anything else is a state-machine bug.
        // ALL is the source of truth for the candidate set.
        let entries: Vec<_> = super::ProcessPhase::ALL
            .into_iter()
            .filter(|p| p.can_transition_to(Releasing))
            .collect();
        assert_eq!(entries, vec![Attested, Failed]);
    }

    #[test]
    fn reaped_is_sink() {
        assert!(Reaped.is_terminal());
        // Sweep every non-Reaped variant via ALL so a new phase
        // pins the sink-ness invariant automatically.
        for next in super::ProcessPhase::ALL {
            if next == Reaped {
                continue;
            }
            assert!(
                !Reaped.can_transition_to(next),
                "Reaped → {next:?} should be illegal"
            );
        }
    }

    #[test]
    fn cannot_skip_forking() {
        assert!(!Pending.can_transition_to(Execing));
        assert!(!Pending.can_transition_to(Running));
    }

    #[test]
    fn running_is_alive() {
        assert!(Running.is_alive());
        assert!(Attested.is_alive());
        assert!(!Zombie.is_alive());
        assert!(!Reaped.is_alive());
    }

    /// [`ProcessPhase::has_exited`] sinks to `{Failed, Zombie, Reaped}`
    /// verbatim — pins the closed set the substrate's
    /// [`crate::pool::PoolMemberSnapshot::is_failed`] alias delegates
    /// to post-lift, so a variant added inside the exited partition
    /// (a new terminal-error variant) is caught here rather than as
    /// silent drift at the pool reconciler's health-count seed.
    #[test]
    fn has_exited_sinks_to_failed_zombie_reaped() {
        for p in super::ProcessPhase::ALL {
            let expected = matches!(p, Failed | Zombie | Reaped);
            assert_eq!(
                p.has_exited(),
                expected,
                "{p:?}.has_exited() should be {expected}"
            );
        }
    }

    /// [`ProcessPhase::has_exited`] IS the boolean complement of
    /// [`ProcessPhase::is_alive`] across every variant — pinning the
    /// closed-set complement invariant so a future rename of either
    /// primitive that drifted one edge (e.g. a variant classified as
    /// both alive AND exited, or as neither) surfaces here rather
    /// than as an operator-facing pool-count skew where the same
    /// Process is counted both toward the alive pool AND toward the
    /// failed-reap queue.
    #[test]
    fn has_exited_is_complement_of_is_alive() {
        for p in super::ProcessPhase::ALL {
            assert_eq!(
                p.has_exited(),
                !p.is_alive(),
                "{p:?}: has_exited should equal !is_alive"
            );
        }
    }

    /// The exited set and the [`ProcessPhase::is_running`] set are
    /// disjoint — no Process is both "healthy" (Running or Attested)
    /// and "exited" (Failed/Zombie/Reaped) at the same phase. Pins
    /// the substrate invariant the pool reconciler's `is_healthy` +
    /// `is_failed` snapshot predicates rely on to partition members
    /// without double-counting.
    #[test]
    fn is_running_and_has_exited_are_disjoint() {
        for p in super::ProcessPhase::ALL {
            assert!(
                !(p.is_running() && p.has_exited()),
                "{p:?}: cannot be both is_running() and has_exited()"
            );
        }
    }

    // ── closed-set algebra contracts (ALL × as_str × FromStr) ────────

    /// Structural well-formedness of [`ProcessPhase`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants
    /// (`ALL` is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the
    /// hand-derived `all_phases_roundtrip_via_as_str` +
    /// `all_is_unique_and_complete` + the empty-input arm of the
    /// per-implementor unknown-error test — those three sites
    /// re-derived byte-for-byte across 36+ closed-set implementors
    /// pre-lift; this helper lifts them all onto the trait so any
    /// future closed-set implementor inherits the contract by
    /// implementing the trait + calling this one helper, with no
    /// HashSet sweep or `FromStr` round-trip loop to copy.
    ///
    /// `FromStr` delegates to `<Self as tatara_closed_set::ClosedSet>::parse_label`,
    /// so this helper exercises the exact code path the operator hits
    /// when parsing an annotation / status-field value back to the
    /// typed phase.
    #[test]
    fn process_phase_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<super::ProcessPhase>();
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift. If a
    /// reviewer accidentally re-introduces an inline match in
    /// Display, this test would fail the moment a variant rename
    /// touches one site but not the other. NOT lifted into the
    /// `ClosedSet` testkit because `Display` is a per-implementor
    /// concern (the trait can't provide a default `Display` impl in
    /// stable Rust) and the projection's choice (`as_str` vs.
    /// inherent label vs. tagged-Debug) is domain-specific.
    #[test]
    fn display_matches_as_str() {
        for phase in super::ProcessPhase::ALL {
            assert_eq!(phase.to_string(), phase.as_str());
        }
    }

    /// `FromStr` rejects domain-specific bad inputs — case-drifted /
    /// typo / extinct-variant — and the error echoes the input
    /// VERBATIM so the operator-facing diagnostic carries the
    /// offending value, not a normalized form. Kept per-implementor
    /// because the verbatim-payload contract is a property of the
    /// per-enum `Unknown<X>(pub String)` newtype, not of the trait's
    /// structural surface — the trait's `make_unknown(s: &str)`
    /// hook lets a future implementor swap the carrier for a
    /// structured diagnostic without changing the trait contract, so
    /// the payload-echo invariant lives with the implementor that
    /// chose the newtype shape. (The empty-input arm is now lifted
    /// into `process_phase_is_well_formed_closed_set`; the
    /// case-drifted / typo / extinct-variant arms stay here as
    /// they're representative non-canonical inputs the operator
    /// might supply.)
    #[test]
    fn unknown_phase_errors() {
        use std::str::FromStr;
        for bad in ["attested", "FAILED", "Cancelled", "Reapped"] {
            let err = super::ProcessPhase::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    // ─── released_from encoder/decoder substrate pins ────────────────
    //
    // Bind [`ProcessPhase::released_from_label`] +
    // [`ProcessPhase::parse_released_from`] at fail-before-pass-after
    // granularity so a regression that reshapes either side of the
    // wire-format bijection over the terminal-reached-gate set
    // {Attested, Failed} surfaces HERE rather than as silent skew
    // between the phase machine's `p_current_phase_str` writer and
    // its `released_from_annotation` reader — the two callsites the
    // encoder/decoder pair collapses onto ONE substrate owner.
    //
    // Pre-lift both sites hand-authored a `match phase { Failed =>
    // "Failed", _ => "Attested" }` / `match anno { Some("Failed") =>
    // Failed, _ => Attested }` block with hardcoded string literals
    // that did not go through `ProcessPhase::as_str`. A wire-format
    // rename of the `Failed` or `Attested` variant that touched
    // `as_str` alone would silently break the annotation contract at
    // both callsites; post-lift the primitives route through
    // `ProcessPhase::as_str` so the rename lands mechanically.

    #[test]
    fn released_from_label_maps_failed_to_failed_string() {
        assert_eq!(super::ProcessPhase::Failed.released_from_label(), "Failed");
    }

    #[test]
    fn released_from_label_maps_attested_to_attested_string() {
        assert_eq!(
            super::ProcessPhase::Attested.released_from_label(),
            "Attested"
        );
    }

    #[test]
    fn released_from_label_collapses_non_gate_phases_to_attested() {
        // Forward-compat pin: every non-{Attested,Failed} variant
        // collapses to "Attested" — an unexpected observed-phase
        // (Running, Reconverging, Zombie, …) leaking through to
        // `p_current_phase_str`'s writer never stamps a Zombie-
        // routing "Failed" label into the annotation. Sweep via ALL
        // so a future variant is covered automatically.
        for p in super::ProcessPhase::ALL {
            if matches!(p, super::ProcessPhase::Failed) {
                continue;
            }
            assert_eq!(
                p.released_from_label(),
                "Attested",
                "{p:?} must collapse to \"Attested\" under released_from_label",
            );
        }
    }

    #[test]
    fn parse_released_from_matches_hardcoded_pre_lift_reader() {
        // Byte-for-byte parity witness against the pre-lift
        // `phase_machine::released_from_annotation` match block:
        //     match p.annotation(RELEASED_FROM) {
        //         Some("Failed") => ProcessPhase::Failed,
        //         _              => ProcessPhase::Attested,
        //     }
        // A regression that widened the "Failed" arm (a case-fold to
        // Some("failed"), an alias like Some("FailedRun")), narrowed
        // it (Some("Failed") + a trailing-newline gate), or drifted
        // the default arm (a None-vs-Some("Attested") split) surfaces
        // HERE.
        assert_eq!(
            super::ProcessPhase::parse_released_from(Some("Failed")),
            super::ProcessPhase::Failed,
        );
        assert_eq!(
            super::ProcessPhase::parse_released_from(Some("Attested")),
            super::ProcessPhase::Attested,
        );
        assert_eq!(
            super::ProcessPhase::parse_released_from(None),
            super::ProcessPhase::Attested,
        );
        // Non-canonical inputs collapse to Attested — same forward-
        // compat semantics the pre-lift `_` arm gave.
        for bad in [
            "",
            "failed",
            "FAILED",
            "attested",
            "Running",
            "Reaped",
            "Some(Failed)",
        ] {
            assert_eq!(
                super::ProcessPhase::parse_released_from(Some(bad)),
                super::ProcessPhase::Attested,
                "non-canonical input {bad:?} must collapse to Attested",
            );
        }
    }

    #[test]
    fn released_from_label_and_parse_are_inverse_on_terminal_gates() {
        // The encoder/decoder pair round-trips exactly over
        // {Attested, Failed} — the closed set the annotation is
        // designed to carry. Sweep both gates; a regression that
        // desynced the two projections (e.g. the encoder started
        // stamping "attested" lowercase while the decoder still
        // keyed on "Attested") would surface HERE.
        for gate in [super::ProcessPhase::Attested, super::ProcessPhase::Failed] {
            let round_trip =
                super::ProcessPhase::parse_released_from(Some(gate.released_from_label()));
            assert_eq!(
                round_trip, gate,
                "{gate:?} must round-trip through label→parse",
            );
        }
    }

    #[test]
    fn released_from_label_routes_through_as_str_not_a_hardcoded_literal() {
        // The primitive dispatches through `ProcessPhase::as_str` for
        // both canonical labels — a future rename of either variant's
        // wire form (an operator-facing normalization pass, a serde-
        // rename attribute) that touched `as_str` alone would silently
        // break the annotation contract if the encoder used hardcoded
        // string literals. Witness the routing by matching the
        // encoder's output against the corresponding variant's
        // `as_str` projection.
        assert_eq!(
            super::ProcessPhase::Failed.released_from_label(),
            super::ProcessPhase::Failed.as_str(),
        );
        assert_eq!(
            super::ProcessPhase::Attested.released_from_label(),
            super::ProcessPhase::Attested.as_str(),
        );
    }

    #[test]
    fn released_from_label_output_is_always_a_terminal_gate() {
        // The encoder's codomain is exactly {Attested, Failed} — the
        // two labels `advance_out_of_releasing`'s reader dispatches
        // on. A regression that leaked a third label (e.g. the
        // default arm returning "Unknown", or a new `Draining` gate
        // added to the terminal-reached set producing its own label
        // without a matching decoder arm) surfaces HERE.
        for p in super::ProcessPhase::ALL {
            let label = p.released_from_label();
            let decoded = super::ProcessPhase::parse_released_from(Some(label));
            assert!(
                matches!(decoded, super::ProcessPhase::Attested | super::ProcessPhase::Failed),
                "{p:?}'s label {label:?} decoded to {decoded:?} — must land in the {{Attested,Failed}} gate set",
            );
        }
    }
}
