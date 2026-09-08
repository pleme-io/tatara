//! Substrate primitive over the K8s `metav1.Condition.type` wire-
//! form axis every writer + reader in this workspace hand-authored
//! as a bare `&'static str` literal on the `ProcessStatus.conditions[]`
//! wire. Sibling on the same wire-form axis-family to
//! [`crate::k8s_condition::K8sConditionStatus`] — that primitive
//! owns the closed set of the `status` slot (`"True"` / `"False"`
//! / `"Unknown"`); THIS primitive owns the closed set of the
//! `type` slot (`"Ready"` / `"Attested"`) every
//! [`crate::status::ProcessCondition`] carries.
//!
//! ## Why the substrate lives here
//!
//! Pre-lift the two `type` wire-form literals were hand-authored at
//! FOUR production sites across two crates past the ★★ PRIME-
//! DIRECTIVE ≥ 2 duplication threshold — each writer + reader pair
//! silently coupled by exact-case ASCII agreement on the `type` slot:
//!
//! * [`crate::status::ProcessCondition::ready`] — writer, `type_:
//!   "Ready".into()` on the `Ready` row emitted whenever the
//!   reconciler observes a Process running.
//! * [`crate::status::ProcessCondition::not_ready`] — writer, `type_:
//!   "Ready".into()` on the same row emitted with `status: "False"`
//!   whenever the reconciler observes a Process failing.
//! * [`crate::status::ProcessCondition::attested`] — writer, `type_:
//!   "Attested".into()` on the `Attested` row emitted whenever a
//!   three-pillar attestation lands.
//! * `tatara-reconciler::ssapply::ready_condition_value` — reader,
//!   `if typ != "Ready" { continue; }` filter that isolates the
//!   `Ready`-typed condition out of every FluxCD / Deployment /
//!   HelmRelease / Kustomization / StatefulSet resource's
//!   `status.conditions[]` list before classifying its `status` slot
//!   through [`crate::k8s_condition::K8sConditionStatus::from_wire_str`].
//!
//! Every site restated the SAME `&'static str` byte-literal
//! (`"Ready"` × 3 or `"Attested"` × 1). A copy-paste that lower-cased
//! one letter (`"ready"` — silently invalid; the K8s API server does
//! NOT case-normalize condition types, and every observer that
//! selects by exact case would silently miss the drifted row),
//! swapped the semantic slot (a writer that emitted `"Attested"` on
//! the `Ready` row would silently drift the reconciler's
//! `ready_condition_value` observer to always-Unknown on that
//! resource), or introduced an alternate spelling drifts the wire-
//! form at ONE end and leaves the other end unable to classify the
//! condition — the reader falls through to `ReadyState::Unknown` and
//! every Flux / Deployment readiness gate silently reports "not
//! observed" for the remainder of the resource's life. Post-lift
//! each writer composes with `ProcessConditionType::<V>.as_wire_str()`
//! and the reader binds through `ProcessConditionType::from_wire_str
//! (...)`; the wire-form literal lives at ONE substrate owner and a
//! drift at either end becomes unrepresentable at the closed-set
//! level.
//!
//! ## Closed-set completeness
//!
//! `ProcessCondition` is the crate-owned condition type — the K8s
//! API does NOT dictate the `type` slot's closed set (unlike the
//! `status` slot's three-value set at
//! [`crate::k8s_condition::K8sConditionStatus`]); each CRD owns its
//! own type alphabet. Every `ProcessCondition` constructor in
//! [`crate::status`] pre-lift wrote exactly one of the two literals
//! `"Ready"` or `"Attested"`; the enum below IS that closed set.
//! A future ProcessCondition constructor that adds a third condition
//! type (e.g. `"Reconverging"` for the SIGHUP re-convergence path,
//! `"Terminated"` for the Zombie/Reaped gate) would land as ONE new
//! variant at this ONE substrate owner AND ONE new
//! `ProcessCondition::<constructor>` at [`crate::status`] AND (if
//! the reader wants to classify it) ONE new arm at
//! `ssapply::ready_condition_value` — exhaustively checked by the
//! compiler at every downstream consumer that pattern-matches on
//! the closed set.
//!
//! ## Byte-shape parity
//!
//! `as_wire_str` returns the EXACT-CASE ASCII byte-shape every pre-
//! lift site restated inline — pinned bytewise at
//! [`tests::as_wire_str_matches_pre_lift_literals_bytewise`] against
//! a hand-authored fixture table of the pre-lift strings. A
//! regression that lower-cased a variant, added a whitespace prefix,
//! or reshaped the byte-form under a future `#[derive(Serialize)]`
//! `serde(rename = "…")` drift surfaces at the pin rather than as
//! silent operator-facing wire-form skew across every
//! `ProcessCondition` writer + `ready_condition_value` reader in the
//! workspace.
//!
//! `from_wire_str` is the invertible partner — a round-trip through
//! `from_wire_str(v.as_wire_str())` yields `Some(v)` for every
//! variant, pinned at
//! [`tests::wire_form_round_trip_holds_for_every_variant`]. Any
//! input outside the closed set (case-drift, whitespace, empty
//! string, unrelated K8s condition-type literals) returns `None`;
//! the closed-set nature is pinned at
//! [`tests::from_wire_str_rejects_case_drift_and_unknown`].
//!
//! ## Naming — `as_wire_str`, not `as_str`
//!
//! Same discipline as the [`crate::k8s_condition::K8sConditionStatus`]
//! sibling — the method signals that the returned `&'static str` is
//! the K8s WIRE FORM (the exact byte-shape the API server accepts on
//! the `type` slot of a `metav1.Condition`), not a debug-print or
//! `Display` projection. A caller that reads `.as_wire_str()`
//! immediately understands the return value is safe to write into a
//! JSON payload without any further normalization; a call spelled
//! `.as_str()` reads as a generic string projection and invites
//! callers to reach for `.to_lowercase()` / `.trim()` normalizations
//! that would break the wire form.
//!
//! ## `#[must_use]` on `as_wire_str`
//!
//! Every consumer feeds the returned `&'static str` into either a
//! `String::from(...)` composition (writer side, going into
//! `ProcessCondition.type_`) or a pattern-match / equality arm
//! (reader side, filtering `status.conditions[]`). Dropping the
//! return means the wire-form projection was computed for no
//! observable reason — the attribute surfaces that as a warning at
//! every consumer site.
//!
//! Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
//! proofs — the wire-form literal at ONE substrate owner means the
//! writer + reader sides of the K8s `status.conditions[].type` wire
//! agree bytewise by construction; a drift at either end becomes
//! unrepresentable at the closed-set level, not "detected at
//! runtime by a mismatched observer log line"). THEORY.md §III
//! (typescape — the ProcessCondition `type` slot's closed set is a
//! first-class Rust enum, not a stringly-typed wire-form).
//! THEORY.md §VI.1 (generation over composition — the two-literal
//! closed set recurred at FOUR hand-authored sites past the ★★
//! PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to ONE
//! substrate owner here on the K8s-Condition `type` wire-form axis).

use std::fmt;

/// The closed set of `metav1.Condition.type` values every
/// [`crate::status::ProcessCondition`] constructor emits and every
/// downstream `status.conditions[]` classifier filters against.
/// Wire-form is the exact-case ASCII literal (`"Ready"`,
/// `"Attested"`); a case-drifted variant would silently miss the
/// reader's byte-exact filter.
///
/// Substrate primitive over the `type`-slot wire-form literal every
/// writer + reader hand-authors on opposite sides of
/// `status.conditions[]`. Sibling on the same K8s-Condition wire-form
/// axis-family to [`crate::k8s_condition::K8sConditionStatus`] —
/// that primitive owns the `status`-slot closed set, this primitive
/// owns the `type`-slot closed set. See the module docs for the
/// pre-lift lift audit + closed-set completeness argument.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum ProcessConditionType {
    /// The `Ready`-typed condition — the reconciler's per-observation
    /// readiness row emitted by [`crate::status::ProcessCondition::
    /// ready`] (status = `True`) + [`crate::status::ProcessCondition::
    /// not_ready`] (status = `False`), and filtered by
    /// `tatara-reconciler::ssapply::ready_condition_value` on the
    /// reader side.
    Ready,
    /// The `Attested`-typed condition — the reconciler's post-
    /// three-pillar-attestation row emitted by
    /// [`crate::status::ProcessCondition::attested`] (status = `True`)
    /// once a `ProcessAttestation`'s `composed_root` is written into
    /// the `message` slot.
    Attested,
}

impl ProcessConditionType {
    /// The K8s wire-form literal for this variant — exact-case ASCII,
    /// safe to write directly into a `metav1.Condition.type` slot
    /// without further normalization. Byte-identical to the pre-lift
    /// hand-authored `"Ready"` / `"Attested"` literals every writer
    /// and reader restated inline.
    #[must_use = "a ProcessCondition type wire-form projection that isn't bound swallows the composition"]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Attested => "Attested",
        }
    }

    /// Parse a K8s wire-form ProcessCondition type literal into its
    /// typed variant. Returns `None` for any input outside the
    /// closed set — case-drift (`"ready"`), whitespace-wrapped
    /// variants (`" Ready"`), unrelated literals (`""`, `"True"`,
    /// `"KustomizationHealthy"`), all reject silently and the caller
    /// falls through to its own `_ => ...` arm.
    ///
    /// Invertible with [`Self::as_wire_str`]: a round-trip through
    /// `from_wire_str(v.as_wire_str())` yields `Some(v)` for every
    /// variant. Pinned at
    /// [`tests::wire_form_round_trip_holds_for_every_variant`].
    #[must_use = "a ProcessCondition type parse result that isn't bound swallows the classification"]
    pub fn from_wire_str(s: &str) -> Option<Self> {
        match s {
            "Ready" => Some(Self::Ready),
            "Attested" => Some(Self::Attested),
            _ => None,
        }
    }
}

impl fmt::Display for ProcessConditionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

#[cfg(test)]
mod tests {
    use super::ProcessConditionType;

    /// Fail-before-pass-after: the substrate's `as_wire_str`
    /// projection produces byte-identical output to the pre-lift
    /// hand-authored `"Ready"` / `"Attested"` literals every writer
    /// and reader restated inline. A regression that lower-cased
    /// one variant, added a whitespace prefix, or a future
    /// `#[derive(Serialize)]` `serde(rename)` drift on this type
    /// would surface HERE, not as silent operator-facing wire-form
    /// skew across every ProcessCondition writer and reader in the
    /// workspace.
    #[test]
    fn as_wire_str_matches_pre_lift_literals_bytewise() {
        assert_eq!(ProcessConditionType::Ready.as_wire_str(), "Ready");
        assert_eq!(ProcessConditionType::Attested.as_wire_str(), "Attested");
    }

    /// Round-trip through `from_wire_str(v.as_wire_str())` yields
    /// `Some(v)` for every variant. Pins the invariant that the
    /// writer + reader compose invertibly at the closed-set boundary
    /// — a writer's `as_wire_str` output is always accepted by the
    /// reader's `from_wire_str` on the SAME variant.
    #[test]
    fn wire_form_round_trip_holds_for_every_variant() {
        for v in [ProcessConditionType::Ready, ProcessConditionType::Attested] {
            assert_eq!(
                ProcessConditionType::from_wire_str(v.as_wire_str()),
                Some(v),
            );
        }
    }

    /// Inputs outside the closed set — case-drift, whitespace-
    /// wrapped variants, empty string, unrelated K8s wire-form
    /// literals (K8s ConditionStatus values, K8s built-in condition
    /// types, ProcessCondition reason slot values) — all reject with
    /// `None`. Pins the closed-set nature: `from_wire_str` is a total
    /// function over the ProcessCondition type alphabet, not a
    /// permissive parser that accepts synonyms.
    #[test]
    fn from_wire_str_rejects_case_drift_and_unknown() {
        for bad in [
            "",
            "ready",
            "READY",
            "attested",
            "ATTESTED",
            " Ready",
            "Ready ",
            "Attested\n",
            "True",
            "False",
            "Unknown",
            "KustomizationHealthy",
            "HelmReleaseReleased",
            "ObservedRunning",
            "AttestationWritten",
            "1",
        ] {
            assert_eq!(
                ProcessConditionType::from_wire_str(bad),
                None,
                "expected `{bad:?}` outside the ProcessCondition type closed set, but from_wire_str accepted it",
            );
        }
    }

    /// `Display` composes through `as_wire_str` — the two
    /// projections are byte-identical so `format!("{v}")` and
    /// `v.as_wire_str()` are interchangeable at every consumer.
    /// Pins the invariant that a caller who reaches for the stdlib
    /// `Display` conversion path (via `.to_string()`,
    /// `format!("{v}")`, a `write!` macro) gets the same wire-form
    /// bytes as a direct `.as_wire_str()` call.
    #[test]
    fn display_composes_through_as_wire_str_bytewise() {
        for v in [ProcessConditionType::Ready, ProcessConditionType::Attested] {
            assert_eq!(v.to_string(), v.as_wire_str());
            assert_eq!(format!("{v}"), v.as_wire_str());
        }
    }

    /// Closed-set completeness: the two variants exhaust the
    /// ProcessCondition type alphabet as of this crate's revision.
    /// A future ProcessCondition constructor that added a third
    /// condition type (e.g. `"Reconverging"` for the SIGHUP re-
    /// convergence path, `"Terminated"` for the Zombie/Reaped gate)
    /// would land as one new variant here, and every consumer that
    /// pattern-matched exhaustively against the closed set gets a
    /// compile-time error until it handles the new arm — the fifth
    /// invariant of the Rust+Lisp pattern. Compiler-verified below
    /// with an exhaustive match; a regression that added a
    /// `#[non_exhaustive]` attribute or a private constructor arm
    /// would break the exhaustiveness proof at this test.
    #[test]
    fn closed_set_exhausts_the_process_condition_type_alphabet() {
        fn describe(v: ProcessConditionType) -> &'static str {
            match v {
                ProcessConditionType::Ready => "Ready",
                ProcessConditionType::Attested => "Attested",
            }
        }
        assert_eq!(describe(ProcessConditionType::Ready), "Ready");
        assert_eq!(describe(ProcessConditionType::Attested), "Attested");
    }

    /// `Copy` + `Clone` + `Eq` + `Hash` — pins the trait derives at
    /// compile time so a regression that dropped one (a future
    /// `#[derive(Serialize, Deserialize)]` addition that reshaped
    /// the enum, a manual `impl Clone` that dropped `Copy`) surfaces
    /// here rather than at a downstream consumer that stored the
    /// value in a `HashMap` key or copied it across an `if` arm.
    #[test]
    fn value_semantics_hold_at_compile_time() {
        fn assert_copy<T: Copy>() {}
        fn assert_hash<T: std::hash::Hash>() {}
        fn assert_eq<T: Eq>() {}
        assert_copy::<ProcessConditionType>();
        assert_hash::<ProcessConditionType>();
        assert_eq::<ProcessConditionType>();
    }

    /// Sibling-axis coherence with [`crate::k8s_condition::
    /// K8sConditionStatus`] on the K8s-Condition wire-form axis-
    /// family — the two primitives partition the `metav1.Condition`
    /// wire alphabet by SLOT (`type` here, `status` at the sibling),
    /// so a caller that accidentally passed a ProcessConditionType
    /// wire-form to `K8sConditionStatus::from_wire_str` (or vice
    /// versa) MUST reject. Pins the invariant that the two closed
    /// sets are DISJOINT — a case-flip or a copy-paste that swapped
    /// the two `from_wire_str` calls surfaces HERE as a `None` at
    /// the wrong reader, not as silent classification skew.
    #[test]
    fn from_wire_str_rejects_sibling_axis_wire_forms() {
        use crate::k8s_condition::K8sConditionStatus;
        for status_wire in [
            K8sConditionStatus::True.as_wire_str(),
            K8sConditionStatus::False.as_wire_str(),
            K8sConditionStatus::Unknown.as_wire_str(),
        ] {
            assert_eq!(
                ProcessConditionType::from_wire_str(status_wire),
                None,
                "ProcessConditionType::from_wire_str accepted the sibling-axis \
                 K8sConditionStatus wire-form `{status_wire:?}` — the two closed \
                 sets MUST stay disjoint",
            );
        }
        for type_wire in [
            ProcessConditionType::Ready.as_wire_str(),
            ProcessConditionType::Attested.as_wire_str(),
        ] {
            assert_eq!(
                K8sConditionStatus::from_wire_str(type_wire),
                None,
                "K8sConditionStatus::from_wire_str accepted the sibling-axis \
                 ProcessConditionType wire-form `{type_wire:?}` — the two closed \
                 sets MUST stay disjoint",
            );
        }
    }
}
