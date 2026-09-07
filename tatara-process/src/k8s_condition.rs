//! Substrate primitive over the K8s `metav1.Condition.status` wire-
//! form axis — the workspace-wide ONE substrate owner of the
//! exact-case ASCII `"True"` / `"False"` / `"Unknown"` closed set
//! every writer AND reader hand-authored on opposite sides of the
//! `status.conditions[]` wire.
//!
//! ## Why the substrate lives here
//!
//! Pre-lift the same three-literal set was hand-authored at FIVE
//! production sites across two crates past the ★★ PRIME-DIRECTIVE
//! ≥ 2 duplication threshold — each pair of writer + reader sites
//! silently coupled by exact-case ASCII agreement:
//!
//! * `tatara-process::status::ProcessCondition::ready` — writer,
//!   `status: "True".into()` on the `Ready` type row.
//! * `tatara-process::status::ProcessCondition::not_ready` — writer,
//!   `status: "False".into()` on the `Ready` type row.
//! * `tatara-process::status::ProcessCondition::attested` — writer,
//!   `status: "True".into()` on the `Attested` type row.
//! * `tatara-reconciler::ssapply::ready_condition_value` — reader,
//!   `Some("True") => ReadyState::Ready` at the Deployment /
//!   HelmRelease / Kustomization / StatefulSet condition classifier.
//! * `tatara-reconciler::ssapply::ready_condition_value` — reader,
//!   `Some("False") => ReadyState::NotReady(...)` at the same
//!   classifier.
//!
//! Every site restated the SAME `&'static str` byte-literal (`"True"`,
//! `"False"`) — three writer sites and two reader sites. A copy-paste
//! that lower-cased one letter (`"true"` — silently invalid; the K8s
//! API server rejects it as non-conformant), swapped the pair
//! semantics (writer emits `"False"` where semantic is `"True"`), or
//! introduced an alternate spelling drifts the wire-form at ONE end
//! and leaves the other end unable to classify the condition — the
//! reader falls through to `ReadyState::Unknown` and every Flux /
//! Deployment readiness gate silently reports "not observed" for the
//! remainder of the resource's life. Post-lift each writer composes
//! with `K8sConditionStatus::<V>.as_wire_str()` and each reader binds
//! through `K8sConditionStatus::from_wire_str(...)`; the wire-form
//! literal lives at ONE substrate owner and a drift at either end
//! becomes unrepresentable at the closed-set level.
//!
//! ## Closed-set completeness
//!
//! The K8s API defines exactly three ConditionStatus values —
//! [`ConditionStatus`][cs] — corresponding to the three enum variants
//! here. A future K8s revision that added a fourth wire-form literal
//! would land as one new variant at this ONE substrate owner, and
//! every consumer (writer + reader) that pattern-matched exhaustively
//! against the closed set gets a compile-time error until it handles
//! the new arm — the fifth invariant of the pattern (composition
//! preserves proofs) plays out mechanically at the exhaustiveness
//! check.
//!
//! [cs]: https://pkg.go.dev/k8s.io/apimachinery/pkg/apis/meta/v1#ConditionStatus
//!
//! ## Byte-shape parity
//!
//! `as_wire_str` returns the EXACT-CASE ASCII the K8s API server
//! accepts — the same literal every pre-lift site restated. Pinned
//! bytewise at [`tests::as_wire_str_matches_pre_lift_literals_bytewise`]
//! against a hand-authored fixture-table of the pre-lift strings. A
//! regression that lower-cased a variant (a `"true"` spelling, an
//! accidental `to_lowercase` pass, a `serde(rename = "…")` drift at a
//! future `#[derive(Serialize)]` impl on this type) surfaces at the
//! pin rather than as silent operator-facing wire-form skew.
//!
//! `from_wire_str` is the invertible partner — a round-trip through
//! `from_wire_str(v.as_wire_str())` yields `Some(v)` for every
//! variant, pinned at
//! [`tests::wire_form_round_trip_holds_for_every_variant`]. Any input
//! outside the closed set (case-drift, whitespace, empty string,
//! unrelated literals) returns `None`; the closed-set nature is
//! pinned at [`tests::from_wire_str_rejects_case_drift_and_unknown`].
//!
//! ## Naming — `as_wire_str`, not `as_str`
//!
//! Same discipline as the [`crate::k8s_builtin_resource::K8sBuiltinResource`]
//! and [`crate::phase::ProcessPhase::as_str`] siblings — the method
//! signals that the returned `&'static str` is the K8s WIRE FORM (the
//! exact byte-shape the API server accepts on the `status` slot of a
//! `metav1.Condition`), not a debug-print or `Display` projection. A
//! caller that reads `.as_wire_str()` immediately understands the
//! return value is safe to write into a JSON payload without any
//! further normalization; a call spelled `.as_str()` reads as a
//! generic string projection and invites callers to reach for
//! `.to_lowercase()` / `.trim()` normalizations that would break the
//! wire form.
//!
//! ## `#[must_use]` on `as_wire_str`
//!
//! Every consumer feeds the returned `&'static str` into either a
//! `String::from(...)` composition (writer side, going into
//! `ProcessCondition.status`) or a pattern-match arm (reader side).
//! Dropping the return means the wire-form projection was computed
//! for no observable reason — the attribute surfaces that as a
//! warning at every consumer site.
//!
//! Theory anchor: THEORY.md §II.1 invariant 5 (composition preserves
//! proofs — the wire-form literal at ONE substrate owner means the
//! writer + reader sides of the K8s `status.conditions[]` wire agree
//! bytewise by construction; a drift at either end becomes
//! unrepresentable at the closed-set level, not "detected at runtime
//! by a mismatched log line"). THEORY.md §III (typescape — the K8s
//! ConditionStatus closed set is a first-class Rust enum, not a
//! stringly-typed wire-form). THEORY.md §VI.1 (generation over
//! composition — the three-literal closed set recurred at FIVE hand-
//! authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger,
//! and is lifted to ONE substrate owner here on the K8s-Condition
//! wire-form axis).

use std::fmt;

/// The K8s API's `metav1.Condition.status` closed set — the three
/// values that live in every `status.conditions[].status` field on
/// every K8s object (built-in + CRD), per
/// [ConditionStatus][cs]. Wire-form is the exact-case ASCII literal
/// (`"True"`, `"False"`, `"Unknown"`); the K8s API server rejects any
/// other casing.
///
/// Substrate primitive over the wire-form literal every writer AND
/// reader hand-authors on opposite sides of `status.conditions[]`.
/// See the [module docs][crate::k8s_condition] for the pre-lift lift
/// audit + closed-set completeness argument.
///
/// [cs]: https://pkg.go.dev/k8s.io/apimachinery/pkg/apis/meta/v1#ConditionStatus
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum K8sConditionStatus {
    /// The condition holds (`"True"` on the wire).
    True,
    /// The condition does not hold (`"False"` on the wire).
    False,
    /// The condition's state cannot be determined (`"Unknown"` on the
    /// wire). This variant is NOT emitted by any writer in the
    /// workspace today — pre-lift the writer sites only ever emitted
    /// `"True"` / `"False"` — but it IS part of the K8s closed set,
    /// and the reader side falls through to it (`_ => ReadyState::
    /// Unknown` arm) when the wire-form does not match either `True`
    /// or `False`. Included here so the exhaustive-match discipline
    /// downstream compilers can enforce holds against the full K8s
    /// closed set, not a two-arm subset.
    Unknown,
}

impl K8sConditionStatus {
    /// The K8s wire-form literal for this variant — exact-case ASCII,
    /// safe to write directly into a `metav1.Condition.status` slot
    /// without further normalization. Byte-identical to the pre-lift
    /// hand-authored `"True"` / `"False"` / `"Unknown"` literals every
    /// writer + reader restated inline.
    #[must_use = "a K8s ConditionStatus wire-form projection that isn't bound swallows the composition"]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::True => "True",
            Self::False => "False",
            Self::Unknown => "Unknown",
        }
    }

    /// Parse a K8s wire-form ConditionStatus literal into its typed
    /// variant. Returns `None` for any input outside the closed set
    /// — case-drift (`"true"`), whitespace-wrapped variants (`" True"`),
    /// unrelated literals (`""`, `"Ready"`), all reject silently and
    /// the caller falls through to its own `_ => ...` arm.
    ///
    /// Invertible with [`Self::as_wire_str`]: a round-trip through
    /// `from_wire_str(v.as_wire_str())` yields `Some(v)` for every
    /// variant. Pinned at
    /// [`tests::wire_form_round_trip_holds_for_every_variant`].
    #[must_use = "a K8s ConditionStatus parse result that isn't bound swallows the classification"]
    pub fn from_wire_str(s: &str) -> Option<Self> {
        match s {
            "True" => Some(Self::True),
            "False" => Some(Self::False),
            "Unknown" => Some(Self::Unknown),
            _ => None,
        }
    }
}

impl fmt::Display for K8sConditionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_wire_str())
    }
}

#[cfg(test)]
mod tests {
    use super::K8sConditionStatus;

    /// Fail-before-pass-after: the substrate's `as_wire_str`
    /// projection produces byte-identical output to the pre-lift
    /// hand-authored `"True"` / `"False"` / `"Unknown"` literals
    /// every writer + reader restated inline. A regression that
    /// lower-cased one variant, added a whitespace prefix, or a
    /// future `#[derive(Serialize)]` `serde(rename)` drift on this
    /// type would surface HERE, not as silent operator-facing wire-
    /// form skew across every K8s `status.conditions[]` writer +
    /// reader in the workspace.
    #[test]
    fn as_wire_str_matches_pre_lift_literals_bytewise() {
        assert_eq!(K8sConditionStatus::True.as_wire_str(), "True");
        assert_eq!(K8sConditionStatus::False.as_wire_str(), "False");
        assert_eq!(K8sConditionStatus::Unknown.as_wire_str(), "Unknown");
    }

    /// Round-trip through `from_wire_str(v.as_wire_str())` yields
    /// `Some(v)` for every variant. Pins the invariant that the
    /// writer + reader compose invertibly at the closed-set boundary
    /// — a writer's `as_wire_str` output is always accepted by the
    /// reader's `from_wire_str` on the SAME variant.
    #[test]
    fn wire_form_round_trip_holds_for_every_variant() {
        for v in [
            K8sConditionStatus::True,
            K8sConditionStatus::False,
            K8sConditionStatus::Unknown,
        ] {
            assert_eq!(K8sConditionStatus::from_wire_str(v.as_wire_str()), Some(v));
        }
    }

    /// Inputs outside the closed set — case-drift, whitespace-
    /// wrapped variants, empty string, unrelated K8s wire-form
    /// literals — all reject with `None`. Pins the closed-set
    /// nature: `from_wire_str` is a total function over the K8s
    /// wire-form alphabet, not a permissive parser that accepts
    /// synonyms.
    #[test]
    fn from_wire_str_rejects_case_drift_and_unknown() {
        for bad in [
            "", "true", "false", "unknown", " True", "True ", "Ready", "Attested", "yes", "1",
        ] {
            assert_eq!(
                K8sConditionStatus::from_wire_str(bad),
                None,
                "expected `{bad:?}` outside the K8s ConditionStatus closed set, but from_wire_str accepted it",
            );
        }
    }

    /// `Display` composes through `as_wire_str` — the two
    /// projections are byte-identical so `format!("{v}")` and
    /// `v.as_wire_str()` are interchangeable at every consumer.
    /// Pins the invariant that a caller who reaches for the
    /// stdlib `Display` conversion path (via `.to_string()`,
    /// `format!("{v}")`, a `write!` macro) gets the same wire-
    /// form bytes as a direct `.as_wire_str()` call.
    #[test]
    fn display_composes_through_as_wire_str_bytewise() {
        for v in [
            K8sConditionStatus::True,
            K8sConditionStatus::False,
            K8sConditionStatus::Unknown,
        ] {
            assert_eq!(v.to_string(), v.as_wire_str());
            assert_eq!(format!("{v}"), v.as_wire_str());
        }
    }

    /// Closed-set completeness: the three variants exhaust the K8s
    /// ConditionStatus alphabet. A future K8s revision that added a
    /// fourth wire-form literal would land as one new variant here,
    /// and every consumer that pattern-matched exhaustively against
    /// the closed set gets a compile-time error until it handles the
    /// new arm — the fifth invariant of the Rust+Lisp pattern.
    /// Compiler-verified below with an exhaustive match; a regression
    /// that added a `#[non_exhaustive]` or a private constructor arm
    /// would break the exhaustiveness proof at this test.
    #[test]
    fn closed_set_exhausts_the_k8s_condition_status_alphabet() {
        fn describe(v: K8sConditionStatus) -> &'static str {
            match v {
                K8sConditionStatus::True => "True",
                K8sConditionStatus::False => "False",
                K8sConditionStatus::Unknown => "Unknown",
            }
        }
        assert_eq!(describe(K8sConditionStatus::True), "True");
        assert_eq!(describe(K8sConditionStatus::False), "False");
        assert_eq!(describe(K8sConditionStatus::Unknown), "Unknown");
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
        assert_copy::<K8sConditionStatus>();
        assert_hash::<K8sConditionStatus>();
        assert_eq::<K8sConditionStatus>();
    }
}
