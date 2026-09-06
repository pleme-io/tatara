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

/// Substrate extension trait over `serde_json::Map<String, Value>` —
/// the ONE substrate owner of the `map.insert(<key>.into(),
/// Value::String(<val>.into()))` string-slot insertion shape every
/// JSON-mutating helper in the workspace hand-authored at each callsite.
///
/// Peer of [`ValueObjectExt`] above on the JSON-mutation axis, split
/// by SHAPE: [`ValueObjectExt::as_object_mut_or`] owns the "walk into
/// this `Value`'s object-shape interior or fail loud" guard;
/// [`JsonMapStrExt::insert_str`] owns the "stamp a `Value::String` at
/// a string-typed key" write shape that every consumer downstream of
/// the guard uses to populate the returned `&mut Map`.
///
/// Pre-lift the shape was hand-authored at THIRTEEN production emit
/// sites across `tatara-reconciler` past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication threshold:
///
/// * `ssapply::inject_annotations` × 4 — the SSA-time observed-*
///   annotation stamp family (`PID`, `CONTENT_HASH`, `GENERATION`,
///   `ATTESTATION_ROOT`) each restated the 2-line `annot.insert(
///   <annotation-const>.to_string(), Value::String(<val>.<coerce>))`
///   shape verbatim.
/// * `render::render_flux` × 3 — the Flux `Kustomization.spec` seeds
///   (`interval`, `path`, `targetNamespace`), each restating the same
///   `spec.insert("<key>".into(), Value::String(<val>))` shape.
/// * `render::render_aplicacao` × 3 — the Flux `HelmRelease.spec`
///   seeds (`releaseName`, `targetNamespace`) plus the values-overlay
///   `profile` slot, each restating the same insert shape.
/// * `render::render_export_job` × 2 — the export-Job outer label map
///   (`ROLE`, `EXPORT_INDEX`) each restating the same insert shape.
/// * `edges::IngressEdge::render` × 1 — the cert-manager
///   `cluster-issuer` annotation, restating the same insert shape.
///
/// All THIRTEEN pre-lift sites restated the SAME 2-line shape verbatim,
/// differing only in the `&'static str` / `String` key + the `&str` /
/// `String` value at each callsite. A copy-paste that dropped the
/// `Value::String(...)` wrap (a caller who reached for
/// `.insert(k, v)` after refactoring from a `Value` slot to a plain
/// `String` value slot) would type-check silently at every callsite —
/// `Map<String, Value>::insert` expects a `Value`, and `String:
/// Into<Value>` is provided by `serde_json` via the `Value::String`
/// arm's `From` impl, so the naive `.insert(k, v.to_string())` compiles
/// AND writes the byte-identical JSON. Post-lift each callsite reads
/// `<map>.insert_str(<key>, <val>)` and the string-slot write shape
/// lives at ONE substrate owner here.
///
/// ### Composability
///
/// * Key slot accepts any `impl Into<String>`: `&str` (via
///   `String::from`), `String` (identity), `Cow<'_, str>`, so a
///   callsite with a static `annotations::PID` (`&'static str`) reads
///   `insert_str(annotations::PID, …)` with no `.to_string()` per site.
/// * Value slot accepts any `impl Into<String>`: `&str`, `String`,
///   `Cow<'_, str>`. Numeric or non-string values still need an
///   explicit `.to_string()` at the callsite — same as pre-lift, so
///   the wrapping shape stays visible in the caller's grep footprint.
/// * Returns `Option<Value>` matching the inherent
///   `Map<String, Value>::insert` return semantics: `None` on new-key,
///   `Some(prev)` on overwrite of an existing slot.
///
/// ### Naming — `insert_str`, not `insert`
///
/// Same discipline as [`ValueObjectExt::as_object_mut_or`] above — the
/// trait method deliberately does NOT collide with the inherent
/// `Map::insert` (which takes `(String, Value)` positionally). A name
/// collision would let a caller who has `JsonMapStrExt` in scope
/// resolve to the inherent method by accident (inherent methods win
/// over trait methods in method resolution) and silently drop the
/// `Value::String` wrap, stamping the value bytes straight into the
/// map under a different `Value` variant. The `_str` suffix names the
/// intent: the value slot IS the `Value::String` arm at this write.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// `.insert(<k>.into(), Value::String(<v>.into()))` shape recurred at
/// THIRTEEN hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication trigger, and is lifted to ONE substrate owner here).
/// THEORY.md §II.1 invariant 5 (composition preserves proofs — a
/// regression that drifts the string-slot write shape at ONE consumer
/// surfaces at the substrate pin rather than as silent per-emit skew
/// across every ssapply / render / edges JSON emit site).
pub trait JsonMapStrExt {
    /// Insert a `Value::String(<val>.into())` at `<key>.into()` into
    /// this JSON object map. Returns `Option<Value>` matching the
    /// underlying `Map::insert` semantics — `None` for a new key,
    /// `Some(prev)` for an overwrite.
    fn insert_str(&mut self, key: impl Into<String>, value: impl Into<String>) -> Option<Value>;
}

impl JsonMapStrExt for Map<String, Value> {
    #[inline]
    fn insert_str(&mut self, key: impl Into<String>, value: impl Into<String>) -> Option<Value> {
        self.insert(key.into(), Value::String(value.into()))
    }
}

/// Substrate extension trait over `serde_json::Map<String, Value>` —
/// the ONE substrate owner of the `.entry(<key>).or_insert_with(||
/// Value::Object(<empty>))` seed-then-guard shape every JSON-mutating
/// helper hand-authored at the "walk into this object slot on the
/// parent map, seeding an empty object if the slot is absent, or fail
/// loud if the slot exists but is a non-object" boundary.
///
/// Peer of [`ValueObjectExt::as_object_mut_or`] and
/// [`JsonMapStrExt::insert_str`] on the JSON-mutation axis; split by
/// SHAPE + SITE. [`ValueObjectExt::as_object_mut_or`] owns the "guard
/// a `Value` handle into its object interior" step at ONE level;
/// [`JsonMapStrExt::insert_str`] owns the "stamp a `Value::String` at
/// a string-typed key" write shape; this trait owns the compound
/// "get-or-seed the object at a slot, then guard" step every SSA-time
/// re-injection walks when the caller intends to reach a nested
/// object slot without asserting whether the parent has already
/// populated it (a caller composing a fresh resource-body carries
/// no `metadata` / `metadata.annotations` slot pre-seed; a caller
/// composing atop a pre-populated resource does — both paths reach
/// the same primitive).
///
/// Pre-lift the compound shape was hand-authored at TWO adjacent
/// private helpers in `tatara-reconciler::ssapply` past the ★★
/// PRIME-DIRECTIVE ≥ 2 duplication threshold, both walking the SAME
/// 3-step `let X = <map>.entry(<slot>).or_insert_with(|| Value::Object
/// (<empty>)); X.as_object_mut_or(<slot>)?` incantation:
///
/// * `metadata_object_mut(resource)` — the `metadata` slot seed-then-
///   guard step at the root of every SSA-time re-injection walk
///   (`inject_owner_reference` + `inject_annotations` reach it).
/// * `inject_annotations(resource, process)` — the `annotations`
///   slot seed-then-guard step nested one level deeper under the
///   `metadata` object the primitive above returned.
///
/// Both restated the SAME 3-line shape verbatim: `.entry(<slot>)` on
/// a `Map<String, Value>` handle known to be an object, then
/// `.or_insert_with(|| Value::Object(<empty>))` to synthesize an
/// empty object at the slot when absent, then a `.as_object_mut_or
/// (<slot>)?` guard on the returned `&mut Value` to fail loud when
/// the existing slot is a non-object. TWO byte-for-byte identical
/// blocks past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold,
/// differing only in the `&'static str` slot name each callsite
/// stamped (`"metadata"` / `"annotations"`) — and the slot name is
/// used at BOTH the entry key AND the guard error message so a
/// regression that drifted the two apart at one callsite (a typo
/// stamping `"metadata"` into the entry key + `"metadatas"` into
/// the error message) would silently pass one pin and fail the
/// other. Post-lift each callsite reads `<map>.object_slot_mut_or
/// (<slot>)?` and the compound shape lives at ONE substrate owner
/// here — the slot name is stamped ONCE per call and reaches both
/// the entry key and the guard error slot mechanically.
///
/// ### Composability
///
/// * Slot name is `&'static str` — pre-lift both callsites stamped
///   `&'static str` literals (`"metadata"` / `"annotations"`); a
///   dynamic-slot caller (a callsite that reached this primitive
///   with a `String` key computed at runtime) has no pre-lift
///   precedent in the ssapply/render axis, so the `&'static str`
///   bound stays honest to the pre-lift shape. A future caller
///   needing a runtime slot name can widen this to
///   `impl Into<String>` at the substrate; the pre-lift consumers
///   inherit it mechanically.
/// * Returns `anyhow::Result<&mut Map<String, Value>>` — matches the
///   sibling [`ValueObjectExt::as_object_mut_or`] shape so the
///   downstream `.entry(...).or_insert_with(...)` / `.insert(...)`
///   mutation threads through `?` onto the caller's
///   `Result<_, anyhow::Error>` return exactly as pre-lift.
/// * Ok-arm returns the SAME `&mut Map<String, Value>` the pre-lift
///   `.as_object_mut_or(<slot>)` step returned — no clone, no key-
///   order reshape, no synthesis.
///
/// ### Naming — `object_slot_mut_or`, not `entry_object` or
/// `get_or_insert_object_mut`
///
/// Same discipline as the two sibling traits above — the trait method
/// deliberately does NOT collide with the inherent `Map::entry` /
/// `Map::get_mut` / `Map::insert` methods (any of which a caller who
/// has this trait in scope could resolve to by accident, silently
/// dropping the type-guard step). The `_or` suffix names the intent
/// (guard the `Option → Result` step at the same call, matching the
/// pre-lift `.as_object_mut_or(<slot>)?` guard); `object_slot_mut`
/// names the target shape (return an `&mut` object-typed `Map` at
/// the slot). Together they read as "guard the slot into a mutable
/// object interior or fail loud", matching the pre-lift semantics
/// exactly.
///
/// ### `#[must_use]`
///
/// Every consumer threads the `?` short-circuit onto its handler's
/// `Result<_, anyhow::Error>` return — dropping the guard swallows
/// the underlying type-mismatch entirely, which is never the intended
/// semantic at either pre-lift consumer (each downstream
/// `.entry(...).or_insert_with(...)` / `.insert(...)` mutation
/// depends on the returned `&mut Map` reference).
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 3-line `.entry(<slot>).or_insert_with(|| Value::Object(<empty>))
/// .as_object_mut_or(<slot>)?` compound shape recurred at two
/// hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// trigger, and is lifted to ONE substrate owner here). THEORY.md
/// §II.1 invariant 5 (composition preserves proofs — a regression
/// that drifted the entry-key slot vs. the guard-error slot at ONE
/// site would silently pass one downstream pin and fail the other;
/// post-lift the primitive stamps the slot ONCE per call so the
/// substrate itself owns the entry-key ↔ guard-error name coherence).
pub trait JsonMapObjectEntryExt {
    /// Get-or-seed the object at `slot` in this JSON map, then guard
    /// that the resulting handle is an object; returns
    /// `&mut Map<String, Value>` on the object arm, and an
    /// [`anyhow::Error`] whose `Display` reads
    /// `"<slot> is not an object"` on the non-object arm (byte-
    /// identical to the pre-lift `.as_object_mut_or(<slot>)?` guard,
    /// sourced from the sibling [`ValueObjectExt::as_object_mut_or`]).
    #[must_use = "an object-slot guard that isn't threaded via `?` swallows the underlying type mismatch"]
    fn object_slot_mut_or(&mut self, slot: &'static str)
        -> anyhow::Result<&mut Map<String, Value>>;
}

impl JsonMapObjectEntryExt for Map<String, Value> {
    #[inline]
    fn object_slot_mut_or(
        &mut self,
        slot: &'static str,
    ) -> anyhow::Result<&mut Map<String, Value>> {
        self.entry(slot)
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut_or(slot)
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

    // ─── JsonMapStrExt::insert_str substrate pins ─────────────────
    //
    // Fail-before-pass-after granularity: the `JsonMapStrExt::insert_str`
    // trait method did not exist before this commit, so each test below
    // fails to compile pre-lift. Post-lift they collectively pin the
    // string-slot write shape at ONE substrate owner — a regression that
    // dropped the `Value::String` wrap (silently coercing to a bare
    // `Value::from(&str)` — byte-identical in the `Object` arm today but
    // divergent for any future non-`&str` numeric caller who reached for
    // `insert_str(k, n.to_string())`), swapped the key + value slot
    // orientation, or drifted the return semantics from the inherent
    // `Map::insert` (which returns the previous value on overwrite —
    // load-bearing at any future caller that inspects the return) would
    // surface HERE rather than as silent per-emit skew across the
    // thirteen pre-lift `ssapply` + `render` + `edges` consumers.

    #[test]
    fn insert_str_new_key_returns_none_and_stamps_value_string() {
        // New-key arm: matches inherent `Map::insert` return
        // semantics — `None` for a fresh key — and stamps a
        // `Value::String` (NOT `Value::from(&str)`, though they're
        // byte-identical today) at the slot.
        let mut m = Map::new();
        let prev = m.insert_str("key", "value");
        assert!(prev.is_none(), "new key returns None");
        assert_eq!(m.get("key"), Some(&Value::String("value".to_string())));
        assert!(matches!(m.get("key"), Some(Value::String(_))));
    }

    #[test]
    fn insert_str_overwrite_returns_prior_value_and_stamps_new() {
        // Overwrite arm: matches inherent `Map::insert` return
        // semantics — `Some(prev)` on overwrite. Load-bearing for
        // any future consumer that inspects the return to detect a
        // slot collision (a fleet-wide sweep that flagged a
        // duplicate SSA-time annotation stamp, for example).
        let mut m = Map::new();
        m.insert_str("key", "old");
        let prev = m.insert_str("key", "new");
        assert_eq!(prev, Some(Value::String("old".to_string())));
        assert_eq!(m.get("key"), Some(&Value::String("new".to_string())));
    }

    #[test]
    fn insert_str_accepts_str_and_owned_string_at_both_slots() {
        // Composability pin: both slots MUST accept `&str` and
        // `String` interchangeably — the pre-lift callsite inventory
        // mixes both (SSA-time `annotations::PID` static + a
        // `pid.to_string()` runtime String at the value slot;
        // `spec.insert("interval".into(), Value::String("1m".into()))`
        // with two `&str` slots). A regression that constrained
        // either slot to one shape would break the callsite parity
        // that motivated this substrate primitive.
        let mut m1 = Map::new();
        m1.insert_str("a", "b");
        let mut m2 = Map::new();
        m2.insert_str(String::from("a"), String::from("b"));
        let mut m3 = Map::new();
        m3.insert_str("a", String::from("b"));
        let mut m4 = Map::new();
        m4.insert_str(String::from("a"), "b");
        assert_eq!(m1, m2);
        assert_eq!(m2, m3);
        assert_eq!(m3, m4);
    }

    #[test]
    fn insert_str_matches_pre_lift_hand_authored_shape_bytewise() {
        // Byte-shape parity pin: `insert_str(k, v)` MUST emit the
        // SAME `Map` entry the pre-lift hand-authored `.insert(
        // <k>.into(), Value::String(<v>.into()))` chain produced.
        // Sweeps the four (str × String) × (str × String) key/value
        // shape quadrants so a regression at the primitive that
        // broke the byte identity with the pre-lift shape at ONE
        // quadrant surfaces here rather than as a subtle per-emit
        // divergence at that quadrant.
        for (k_str, v_str) in [("a", "b"), ("x", ""), ("", "y"), ("", "")] {
            // (str, str) quadrant
            let mut via_primitive = Map::new();
            via_primitive.insert_str(k_str, v_str);
            let mut via_pre_lift = Map::new();
            via_pre_lift.insert(k_str.into(), Value::String(v_str.into()));
            assert_eq!(via_primitive, via_pre_lift);

            // (String, String) quadrant
            let mut via_primitive = Map::new();
            via_primitive.insert_str(String::from(k_str), String::from(v_str));
            let mut via_pre_lift = Map::new();
            via_pre_lift.insert(String::from(k_str), Value::String(String::from(v_str)));
            assert_eq!(via_primitive, via_pre_lift);
        }
    }

    #[test]
    fn insert_str_empty_value_stamps_empty_string_not_null() {
        // Semantic pin: an empty value slot MUST stamp
        // `Value::String("")`, NEVER `Value::Null`. Load-bearing at
        // any callsite that stamps a placeholder empty-string
        // annotation (say a `content_hash` slot pre-derive) where a
        // `Null` slot would fail-loud at the K8s apiserver's
        // annotation-value type check.
        let mut m = Map::new();
        m.insert_str("empty", "");
        assert_eq!(m.get("empty"), Some(&Value::String(String::new())));
        assert!(!matches!(m.get("empty"), Some(Value::Null)));
    }

    // ─── JsonMapObjectEntryExt::object_slot_mut_or substrate pins ─────
    //
    // Fail-before-pass-after granularity: the
    // `JsonMapObjectEntryExt::object_slot_mut_or` trait method did not
    // exist before this commit, so each test below fails to compile
    // pre-lift. Post-lift they collectively pin the compound
    // seed-then-guard shape at ONE substrate owner — a regression that
    // dropped the seed step (leaving an absent slot to fall through the
    // guard as `None → Err`), skipped the guard step (silently returning
    // an `&mut Value` when the existing slot is a non-object variant),
    // drifted the entry-key slot vs. the guard-error slot (a copy-paste
    // typo that stamped `"metadata"` into the entry and `"metadatas"`
    // into the guard error message), or drifted the empty-seed shape
    // (a `Value::Null` fallback where `Value::Object(Map::new())` is
    // load-bearing at the downstream `.entry(...).or_insert_with(...)`
    // / `.insert(...)` mutation) would surface HERE rather than as
    // silent per-emit skew across the two pre-lift `ssapply.rs`
    // consumers.

    #[test]
    fn object_slot_mut_or_absent_slot_seeds_empty_object_and_returns_it() {
        // Absent-slot arm: the pre-lift `.entry(<slot>).or_insert_with
        // (|| Value::Object(Default::default()))` step MUST seed the
        // slot with an EMPTY `Value::Object` when the slot is not
        // present in the parent map. The returned handle is the fresh
        // empty map, MUTABLY, so a downstream `.insert(...)` writes
        // land in the parent map's `<slot>` object post-return.
        let mut parent = Map::new();
        {
            let child = parent
                .object_slot_mut_or("metadata")
                .expect("absent slot seeds an object");
            assert!(child.is_empty(), "fresh-seeded slot is an empty object");
            child.insert("name".into(), Value::String("demo".into()));
        }
        // The write landed in the parent map's metadata slot.
        assert_eq!(parent["metadata"]["name"], "demo");
        assert!(matches!(parent.get("metadata"), Some(Value::Object(_))));
    }

    #[test]
    fn object_slot_mut_or_present_object_slot_returns_existing_interior_mutably() {
        // Present-object-slot arm: when the slot is already populated
        // with a `Value::Object`, the primitive MUST return the
        // EXISTING map interior mutably — no synthesis, no reshape, no
        // key-order rewrite. The downstream `.insert(...)` writes MUST
        // merge into the pre-existing keys rather than replace them.
        let mut parent = Map::new();
        parent.insert(
            "metadata".into(),
            serde_json::json!({ "existing_key": "existing_value" }),
        );
        {
            let child = parent
                .object_slot_mut_or("metadata")
                .expect("present-object slot returns Ok");
            assert_eq!(
                child.get("existing_key"),
                Some(&Value::String("existing_value".into()))
            );
            child.insert("new_key".into(), Value::String("new_value".into()));
        }
        assert_eq!(parent["metadata"]["existing_key"], "existing_value");
        assert_eq!(parent["metadata"]["new_key"], "new_value");
    }

    #[test]
    fn object_slot_mut_or_present_non_object_slot_errors_with_pre_lift_display() {
        // Fail-loud arm: when the slot is present but holds a non-
        // object variant (a `Value::String` from a hand-authored
        // YAML manifest where `metadata: "malformed"` slipped past
        // kubectl's schema check), the primitive MUST fail with a
        // `Display` byte-identical to the pre-lift
        // `.as_object_mut_or(<slot>)?` guard — the sibling
        // [`ValueObjectExt::as_object_mut_or`] guard's wire format.
        // A regression that special-cased this arm (overwriting the
        // slot with a fresh empty object, silently coercing) would
        // silently swallow the operator's authoring error at the
        // SSA-time re-injection step.
        let mut parent = Map::new();
        parent.insert("metadata".into(), Value::String("malformed".into()));
        let err = parent.object_slot_mut_or("metadata").unwrap_err();
        assert_eq!(format!("{err}"), "metadata is not an object");
    }

    #[test]
    fn object_slot_mut_or_threads_the_slot_slug_verbatim_across_both_pre_lift_labels() {
        // Cross-slot coherence pin: the TWO pre-lift consumers in
        // `tatara-reconciler::ssapply` stamped TWO distinct slot slugs
        // (`"metadata"` at the resource root, `"annotations"` at the
        // metadata child), and the wrap-shape MUST honor each one
        // verbatim as the leading slot in the `Display` output. A
        // regression that hard-coded one slug across every callsite
        // would pass the fail-loud pin above (on the `"metadata"` slug)
        // and fail HERE — the two downstream error-stream greps
        // operators run to bisect a "which SSA-time slot mutation
        // faulted" alert would ALL collapse to the same slug, hiding
        // whether the fault was at the resource-root object walk or
        // the metadata-child annotations walk.
        for slot in ["metadata", "annotations"] {
            let mut parent = Map::new();
            parent.insert(slot.into(), Value::Null);
            let err = parent.object_slot_mut_or(slot).unwrap_err();
            assert_eq!(format!("{err}"), format!("{slot} is not an object"));
        }
    }

    #[test]
    fn object_slot_mut_or_present_empty_object_returns_existing_reference_not_synthesized() {
        // Precedence pin: a present slot holding an EMPTY
        // `Value::Object` MUST return the pre-existing empty map
        // interior — not a freshly-synthesized replacement. The
        // pre-lift `.entry(<slot>).or_insert_with(||...)` step's
        // short-circuit on the present-slot arm skips the closure
        // entirely; a regression that always evaluated the closure
        // (unconditionally overwriting an existing empty-object slot
        // with a fresh empty object) would type-check silently at
        // every callsite AND write byte-identical JSON at the empty-
        // slot corner, but it would break a hypothetical future
        // consumer that reached the primitive on a map whose slot
        // was seeded upstream with metadata (a caller intending to
        // preserve any keys the parent-composer already dropped in).
        let mut parent = Map::new();
        parent.insert("metadata".into(), Value::Object(Map::new()));
        let addr_before = parent.get("metadata").unwrap() as *const Value;
        {
            let _child = parent.object_slot_mut_or("metadata").unwrap();
        }
        let addr_after = parent.get("metadata").unwrap() as *const Value;
        assert_eq!(
            addr_before, addr_after,
            "present empty-object slot must return the pre-existing reference, not a fresh synthesis",
        );
    }

    #[test]
    fn object_slot_mut_or_matches_pre_lift_hand_authored_compound_shape_bytewise() {
        // Byte-shape parity pin: `object_slot_mut_or(<slot>)?` MUST
        // produce the SAME `&mut Map` (and, on the non-object arm, the
        // SAME `Display`-shaped error) the pre-lift 3-line `.entry
        // (<slot>).or_insert_with(|| Value::Object(Default::default()))
        // .as_object_mut_or(<slot>)?` chain produced. Sweeps the three
        // pre-lift-reachable input corners (absent slot / present
        // object / present non-object) so a regression at the primitive
        // that broke byte identity with the pre-lift chain at ONE
        // corner surfaces here rather than as a subtle per-emit
        // divergence.
        for slot in ["metadata", "annotations"] {
            // (1) Absent-slot corner: both routes seed empty-object at
            //     the slot AND return the same empty map interior.
            let mut via_primitive = Map::new();
            let mut via_pre_lift = Map::new();
            {
                let _ = via_primitive.object_slot_mut_or(slot).unwrap();
                let _ = via_pre_lift
                    .entry(slot.to_string())
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut_or(slot)
                    .unwrap();
            }
            assert_eq!(via_primitive, via_pre_lift);

            // (2) Present-object corner: both routes read back the
            //     same pre-populated interior mutably.
            let mut via_primitive = Map::new();
            via_primitive.insert(slot.into(), serde_json::json!({ "k": "v" }));
            let mut via_pre_lift = via_primitive.clone();
            {
                let a = via_primitive.object_slot_mut_or(slot).unwrap();
                let b = via_pre_lift
                    .entry(slot.to_string())
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut_or(slot)
                    .unwrap();
                assert_eq!(a, b);
            }

            // (3) Present-non-object corner: both routes fail loud
            //     with the same wire-format Display shape.
            let mut via_primitive = Map::new();
            via_primitive.insert(slot.into(), Value::Bool(true));
            let mut via_pre_lift = via_primitive.clone();
            let err_primitive = via_primitive.object_slot_mut_or(slot).unwrap_err();
            let err_pre_lift = via_pre_lift
                .entry(slot.to_string())
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut_or(slot)
                .unwrap_err();
            assert_eq!(format!("{err_primitive}"), format!("{err_pre_lift}"));
        }
    }
}
