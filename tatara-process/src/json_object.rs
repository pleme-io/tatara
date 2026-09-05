//! Substrate primitive over `serde_json::Value` — the ONE substrate
//! owner of the `.as_object_mut().ok_or_else(|| anyhow::anyhow!(
//! "<slot> is not an object"))` guard-shape every JSON-mutating helper
//! restates by hand at the "walk this `Value` slot into its
//! `serde_json::Map` interior or fail loud" boundary.
//!
//! Peer of the trait family that already lives in this crate on the
//! wrap-shape axis:
//!
//! * [`crate::kube_error::KubeResultExt`] — the `kube::Error → anyhow`
//!   display-prefix wrap.
//! * [`crate::hostname::HostnameResultExt`] — the `HostnameError →
//!   anyhow` display-prefix wrap.
//! * [`crate::anyhow_flatten::FlattenCtxExt`] — the `anyhow::Error →
//!   anyhow` display-prefix flatten.
//! * This module — the `Option<&mut Map> → anyhow::Result<&mut Map>`
//!   type-guard, partitioned from the three above by SOURCE (`None`
//!   from the slot-typecheck, not a lifted error type) but sharing the
//!   `.map_err(|_| anyhow!("<slug>: …"))?` display-prefix wire format.
//!
//! Pre-lift the shape was hand-authored at THREE adjacent private
//! helpers in `tatara-reconciler::ssapply` past the ★★ PRIME-DIRECTIVE
//! ≥ 2 duplication threshold:
//!
//! * `metadata_object_mut(resource)` — the root-guard step
//!   (`resource.as_object_mut().ok_or_else(|| anyhow!("resource is not
//!   an object"))?`) that opens the SSA-time
//!   `resource → &mut metadata` walk shared by `inject_owner_reference`
//!   + `inject_annotations`.
//! * `metadata_object_mut(resource)` — the metadata-slot type-check
//!   step (`metadata.as_object_mut().ok_or_else(|| anyhow!("metadata
//!   is not an object"))?`) that closes the same walk — a resource
//!   whose author mistyped the `metadata` slot as an array / string
//!   surfaces as an error rather than as a silent
//!   `.as_object_mut() → None → skip` no-op.
//! * `inject_annotations(resource, process)` — the annotations-slot
//!   type-check step (`annot.as_object_mut().ok_or_else(|| anyhow!(
//!   "annotations is not an object"))?`) that opens the SSA-time
//!   `metadata → &mut annotations` walk before the ownership tag +
//!   observed-* primitive family drops its keys into the map.
//!
//! All three restated the SAME 2-line shape verbatim: `.as_object_mut()`
//! on a `serde_json::Value` handle already known to be non-null, then
//! `.ok_or_else(|| anyhow!("<slot-name> is not an object"))` wrap
//! whose slot name matched the walk step's semantic role (`"resource"`
//! / `"metadata"` / `"annotations"`). THREE byte-for-byte identical
//! guard blocks past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold,
//! differing only in the `&'static str` slot name each callsite
//! stamped.
//!
//! Post-lift each callsite reads
//! `<value>.as_object_mut_or("<slot>")?` and the guard-shape lives at
//! ONE substrate owner here. The composed `anyhow::Error`'s `Display`
//! is byte-identical to the pre-lift chain (`"<slot> is not an
//! object"`), so operator-facing log output and any error-chain greps
//! still match bytewise. A regression that drifts the message (a
//! `"<slot> is not a JSON object"` synonym, a swapped `<slot>` slot,
//! a promotion to a chain-form `source` that only surfaces via the
//! alternate `{e:#}` formatter) surfaces at the tests below rather
//! than as silent operator-facing drift across the three pre-lift
//! consumers.
//!
//! ### Naming — `as_object_mut_or`, not `as_object_mut`
//!
//! Same discipline as the three sibling traits above — the trait
//! method deliberately does NOT share a name with the inherent
//! `serde_json::Value::as_object_mut` method (which returns
//! `Option<&mut Map>`), because a name collision would let a caller
//! who has `ValueObjectExt` in scope resolve to the inherent method
//! by accident (inherent methods win over trait methods in method
//! resolution) and silently drop the type-guard wrap altogether. The
//! `_or` suffix names the intent: guard the `Option → Result` step
//! at the same call, matching the pre-lift `.as_object_mut().
//! ok_or_else(...)` chain.
//!
//! ### `#[must_use]`
//!
//! Every consumer threads the `?` short-circuit onto its handler's
//! `Result<_, anyhow::Error>` return — dropping the guard swallows
//! the underlying type-mismatch entirely, which is never the intended
//! semantic at any of the three pre-lift consumers (each downstream
//! `md.entry(...).or_insert_with(...)` / `annot.insert(...)` mutation
//! depends on the returned `&mut Map` reference).
//!
//! Theory anchor: THEORY.md §VI.1 (generation over composition — the
//! `.as_object_mut().ok_or_else(|| anyhow!("<slot> is not an
//! object"))` guard-shape recurred at three hand-authored sites past
//! the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to
//! ONE substrate owner here). THEORY.md §II.1 invariant 5 (composition
//! preserves proofs — a regression that drifts the guard message
//! wording at ONE site surfaces here at the substrate pin rather than
//! as silent operator-facing skew across every SSA-time
//! `metadata_object_mut` + `inject_annotations` mutation).

use serde_json::{Map, Value};

/// Substrate extension trait over `serde_json::Value` — the ONE
/// substrate owner of the `.as_object_mut().ok_or_else(|| anyhow!(
/// "<slot> is not an object"))` guard-shape. See the module docs for
/// the full callsite audit + the naming rationale (why
/// `as_object_mut_or` and not `as_object_mut`).
pub trait ValueObjectExt {
    /// Borrow the [`Value`] as a mutable JSON object [`Map`], or fail
    /// loud with an [`anyhow::Error`] whose `Display` reads exactly
    /// `"<slot> is not an object"` — the pre-lift wire format every
    /// consumer's `tracing::error!(error = %e, ...)` log line already
    /// encoded.
    #[must_use = "an object-guard that isn't threaded via `?` swallows the underlying type mismatch"]
    fn as_object_mut_or(&mut self, slot: &'static str) -> anyhow::Result<&mut Map<String, Value>>;
}

impl ValueObjectExt for Value {
    #[inline]
    fn as_object_mut_or(&mut self, slot: &'static str) -> anyhow::Result<&mut Map<String, Value>> {
        self.as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("{slot} is not an object"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ─── ValueObjectExt::as_object_mut_or substrate pins ─────────────
    //
    // Fail-before-pass-after granularity: the `ValueObjectExt::
    // as_object_mut_or` trait method did not exist before this commit,
    // so each test below fails to compile pre-lift. Post-lift they
    // collectively pin the object-guard shape at ONE substrate owner —
    // a regression that drifts the error message wording, swaps the
    // `<slot>` slot, wraps the source in a chain-form `source` (which
    // would change `Display` output when downstream tracing formatters
    // interpolate `{e}` rather than the chain-walking `{e:#}`), or
    // promotes the pass-through arm to synthesis (a `None → Ok(&mut
    // Map::default())` fallthrough that silently swallows a mistyped
    // slot) surfaces HERE rather than as silent operator-facing skew
    // across the three `ssapply.rs` pre-lift consumers whose log
    // output already encoded the flat `"<slot> is not an object"`
    // shape.

    #[test]
    fn as_object_mut_or_object_arm_returns_the_inner_map_mutably() {
        // Ok-arm invariant: a `Value::Object` handle threaded through
        // `as_object_mut_or("<slot>")` MUST return `Ok(&mut Map)`
        // whose interior is the SAME `serde_json::Map` the underlying
        // `serde_json::Value::as_object_mut` would return — no clone,
        // no reshape, no synthesis. The `&mut` return is load-bearing
        // at every consumer (each threads a downstream `.entry(...).
        // or_insert_with(...)` / `.insert(...)` mutation onto the
        // returned reference), so a regression that returned a fresh
        // owned `Map` here would silently drop every downstream write.
        let mut v = json!({ "existing_key": "existing_value" });
        let map = v.as_object_mut_or("resource").expect("Value::Object");
        map.insert("new_key".to_string(), json!("new_value"));
        assert_eq!(v["existing_key"], "existing_value");
        assert_eq!(v["new_key"], "new_value");
    }

    #[test]
    fn as_object_mut_or_null_arm_errors_with_pre_lift_display_bytewise() {
        // Byte-shape parity pin: the wrap output of `as_object_mut_or
        // ("<slot>")` on a `Value::Null` handle MUST be `Display`-
        // identical to the pre-lift hand-authored `.as_object_mut().
        // ok_or_else(|| anyhow!("<slot> is not an object"))?` chain.
        // A regression that inserted a synonym (`"<slot> is not a
        // JSON object"`), reshaped the slot position (`"not an
        // object: <slot>"`), or dropped the leading `<slot>` slot
        // surfaces HERE rather than as silent drift at every
        // downstream log-output consumer.
        let mut v = Value::Null;
        let err = v.as_object_mut_or("resource").unwrap_err();
        assert_eq!(format!("{err}"), "resource is not an object");
    }

    #[test]
    fn as_object_mut_or_array_arm_errors_with_pre_lift_display_bytewise() {
        // Sibling to the null-arm byte-shape pin — a mistyped
        // `metadata` slot authored as a JSON array (kubectl accepts
        // `metadata: []` in a YAML manifest with no schema, though the
        // apiserver later rejects it) surfaces the same guard error.
        // Pins the "non-object variants ALL error via the same wire
        // format" invariant — a regression that special-cased the
        // array variant (returning a fresh empty map, silently
        // coercing) surfaces HERE.
        let mut v = json!(["not", "an", "object"]);
        let err = v.as_object_mut_or("metadata").unwrap_err();
        assert_eq!(format!("{err}"), "metadata is not an object");
    }

    #[test]
    fn as_object_mut_or_string_arm_errors_with_pre_lift_display_bytewise() {
        // Sibling to the null / array pins — a mistyped `annotations`
        // slot authored as a JSON string (a common apiserver-layer
        // authoring bug in kubectl-generated manifests where a
        // stringified JSON object leaks through) surfaces the same
        // guard error. Pins the "every non-object variant errors via
        // the same wire format" invariant across the full
        // `serde_json::Value` sum.
        let mut v = json!("stringified");
        let err = v.as_object_mut_or("annotations").unwrap_err();
        assert_eq!(format!("{err}"), "annotations is not an object");
    }

    #[test]
    fn as_object_mut_or_threads_the_slot_slug_verbatim_across_all_three_pre_lift_labels() {
        // Cross-slot coherence pin: the three pre-lift consumers in
        // `tatara-reconciler::ssapply` stamped THREE distinct slot
        // slugs (`"resource"` / `"metadata"` / `"annotations"`), and
        // the wrap-shape MUST honor each one verbatim as the leading
        // slot in the `Display` output. A regression that hard-coded
        // one slug (say `"resource"`) across every callsite would
        // pass the first pin above and fail HERE — the three
        // downstream error-stream greps operators run to bisect a
        // "which SSA-time mutation faulted" alert would ALL collapse
        // to the same slug.
        for slot in ["resource", "metadata", "annotations"] {
            let mut v = Value::Null;
            let err = v.as_object_mut_or(slot).unwrap_err();
            assert_eq!(format!("{err}"), format!("{slot} is not an object"));
        }
    }

    #[test]
    fn as_object_mut_or_object_arm_matches_inherent_as_object_mut_bytewise() {
        // Cross-substrate coherence pin: on the Ok arm the trait
        // method MUST return the SAME `&mut Map` the inherent
        // `serde_json::Value::as_object_mut` would — no diverging
        // view, no clone, no key-order reshape. A regression that
        // introduced a normalization pass here (sorting keys,
        // stripping a null-valued entry, coercing a nested string
        // to a JSON scalar) would surface as silent per-consumer
        // schema drift at the SSA-time mutation — an ownerReferences
        // append that no longer landed in the same slot the apiserver
        // reads, an annotations insert whose key ordering diverged
        // from kubectl's canonical form.
        let mut via_trait = json!({ "key": "value", "nested": { "inner": 1 } });
        let mut via_inherent = via_trait.clone();
        assert_eq!(
            via_trait
                .as_object_mut_or("resource")
                .expect("Value::Object")
                .clone(),
            via_inherent.as_object_mut().expect("Value::Object").clone(),
        );
    }
}
