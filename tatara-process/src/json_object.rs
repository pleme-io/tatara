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
//!   The module also owns the READ-side [`ValueGetExt`] projector
//!   (`.get_i64(<key>) -> Option<i64>`) — sibling of the three
//!   MUTATION-side traits below on the (read, mutate) axis, closing
//!   the READ half of the `serde_json::Value` substrate the four
//!   traits jointly own.
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

/// Substrate extension trait over `serde_json::Value` — the ONE
/// substrate owner of the paired `.get(<key>).and_then(|v| v.as_<T>())`
/// two-link READ chain every downstream projection walks to pull a
/// typed leaf off a Kubernetes-status blob (or an equivalent
/// rendered-resource JSON object) without asserting the slot is
/// present, without asserting its variant, and without asserting the
/// slot fits the target scalar type.
///
/// The trait carries ONE method per typed READ axis; the axis-family
/// is [`Self::get_i64`] (integer counters) + [`Self::get_str`]
/// (string slots) + [`Self::get_array`] (JSON array slots). Adding a
/// new axis (a `get_bool` for `Value::Bool`, a `get_object` for
/// `Value::Object`, a `get_f64` for `Value::Number` truncated to
/// `f64`) lands as ONE new method here + ONE impl arm, inheriting
/// the naming, `#[must_use]`, and inline discipline the existing
/// axes pin. Never open a peer trait for a new axis — keep every
/// READ projection on the ONE substrate owner so a caller who
/// imports `ValueGetExt` reaches every axis through the same trait
/// handle.
///
/// READ-side counterpart to the three MUTATION-side siblings already in
/// this module — [`ValueObjectExt::as_object_mut_or`],
/// [`JsonMapStrExt::insert_str`], [`JsonMapObjectEntryExt::object_slot_mut_or`]
/// — partitioning the substrate along the (read, mutate) axis on the
/// same `serde_json::Value` / `serde_json::Map<String, Value>` carrier
/// pair.
///
/// Pre-lift the two-link chain was hand-authored at THREE adjacent
/// slots inside `tatara-reconciler::boundary::fetch_job_status`, each
/// projecting one `batch/v1::Job` `status.<counter>` field out of the
/// fetched `serde_json::Value` object into a private `JobStatusView`
/// row past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold:
///
/// * `status.get("succeeded").and_then(|v| v.as_i64())` — the
///   Job-completion counter every `JobAttested` + `ClosedLoopAuth`
///   postcondition evaluator gates on (`succeeded < 1` short-circuits
///   to `Satisfaction::Unsatisfied("… still running (…)")`).
/// * `status.get("failed").and_then(|v| v.as_i64())` — the
///   Job-failure counter the same evaluators gate on
///   (`failed > 0` short-circuits to
///   `Satisfaction::Unsatisfied("… failed (status.failed={n})")`).
/// * `status.get("active").and_then(|v| v.as_i64())` — the
///   Job-in-flight counter the "still running" diagnostic tail
///   reports as `(succeeded={s}, active={a})`.
///
/// All THREE sites walked the SAME two-link chain — `.get(<key>)` on a
/// `serde_json::Value` already known to be the status object, then
/// `.and_then(|v| v.as_i64())` on the returned `Option<&Value>` — and
/// each was followed by an `if let Some(...)` write into the
/// [`JobStatusView`] row initialised from `Default::default()`. Post-
/// lift each callsite reads `status.get_i64(<key>)` and the two-link
/// READ chain lives at ONE substrate owner here.
///
/// ### Naming — `get_i64`, not `as_i64` or `i64_at`
///
/// Same discipline as the three sibling traits above — the trait method
/// deliberately does NOT collide with `serde_json::Value::as_i64` (the
/// inherent projection on a single `Value` handle) nor with
/// `serde_json::Value::get` (the inherent slot-lookup returning
/// `Option<&Value>`). A name collision would let a caller who has
/// `ValueGetExt` in scope resolve to one of the inherent methods by
/// accident (inherent methods win over trait methods in method
/// resolution) and silently drop half of the paired chain. The
/// `get_i64(<key>)` shape names the intent: look up the slot at
/// `<key>`, project the returned handle to `i64`, in ONE call.
///
/// ### `#[must_use]`
///
/// Every consumer either binds the returned `Option<i64>` into a
/// downstream `if let Some(n) = ...` / `.unwrap_or_default()` / struct-
/// field construction. Dropping the return silently discards the
/// projection entirely, which is never the intended semantic at the
/// three pre-lift consumers (each downstream write depends on the
/// returned counter).
///
/// ### Composability
///
/// * Key slot is `&str` — matches every pre-lift `.get("<literal>")`
///   callsite and the inherent `serde_json::Value::get`'s primary
///   `str`-index arm. A caller with a runtime-computed key (a
///   `String` produced by a template composer) reaches through
///   `.get_i64(&s)` mechanically via `Deref<Target = str>`.
/// * Returns `Option<i64>` matching the composed inherent chain's own
///   return; a consumer wanting the "absent or non-integer → 0"
///   fallback composes `.unwrap_or_default()` (or `.unwrap_or(0)`) at
///   the callsite, keeping the "should this counter default to 0 or
///   fail loud" decision at the caller rather than baking it into the
///   primitive.
/// * Non-object receivers (a `Value::String`, a `Value::Null`) return
///   `None` verbatim via the inherent `Value::get`'s own non-object-
///   arm behaviour, matching the pre-lift chain's semantics on the
///   corner where the caller's status blob is malformed.
///
/// A future normalization — a per-fleet clamp that rejects negative
/// counters (the K8s API server never emits them, but a fixture
/// authoring bug could), a `Value::Number` fallback that accepts
/// `f64` counters truncated to `i64`, a `checked` overflow arm that
/// promotes an out-of-range integer to a diagnostic rather than a
/// silent `None` — lands at THIS ONE substrate primitive and every
/// downstream Job-status / Deployment-replica / HPA-desired-count
/// counter reader inherits the upgrade mechanically. No per-site edit
/// at any of the 3 listed callers or at future consumers (a
/// Deployment `readyReplicas` projection, an HPA `currentReplicas`
/// gate, a StatefulSet `updatedReplicas` freshness check).
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// two-link `.get(<key>).and_then(|v| v.as_i64())` chain recurred at
/// three hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
/// duplication trigger, and is lifted to ONE substrate owner here).
/// THEORY.md §II.1 invariant 5 (composition preserves proofs — a
/// regression that drifted the projection axis at ONE site — a swap
/// of `as_i64` for `as_u64` narrowing the accepted range, a swap of
/// `.get(<key>)` for `.pointer("<key>")` losing the direct-child
/// semantics — would silently pass every downstream `JobStatusView`
/// composition and surface as a wrong counter at operator-facing
/// diagnostic wording; post-lift the projection lives at ONE typed
/// owner so a regression surfaces at [`tests::get_i64_null_arm_returns_none`]
/// / peers rather than as silent operator-facing drift).
pub trait ValueGetExt {
    /// Look up `key` on this JSON object and project the returned
    /// handle to `i64`; returns `None` when the slot is absent, when
    /// the receiver is not a JSON object, or when the slot's variant
    /// is not integer-shaped.
    #[must_use = "a JSON i64 projection that isn't bound swallows the counter entirely"]
    fn get_i64(&self, key: &str) -> Option<i64>;

    /// Look up `key` on this JSON object and project the returned
    /// handle to `&str`; returns `None` when the slot is absent, when
    /// the receiver is not a JSON object, or when the slot's variant
    /// is not `Value::String`.
    ///
    /// String-axis sibling of [`Self::get_i64`] on the same
    /// `.get(<key>).and_then(|v| v.as_<T>())` READ-chain lift. Pre-lift
    /// the two-link chain was hand-authored at SEVEN production sites
    /// across two crates past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// threshold:
    ///
    /// * `tatara-process::status::RenderedResourceCoords::from_json`
    ///   — FOUR paired reads (`apiVersion`, `kind`, `metadata.name`,
    ///   `metadata.namespace`) that project the four rendered-resource
    ///   coordinate slots off a `serde_json::Value` rendered manifest
    ///   into the typed `RenderedResourceCoords` row; the required
    ///   three (`apiVersion` / `kind` / `metadata.name`) compose with
    ///   `.ok_or_else(|| anyhow!("rendered resource missing X"))?
    ///   .to_string()`, and the optional `metadata.namespace` composes
    ///   with `.map(str::to_string)`.
    /// * `tatara-reconciler::ssapply::ready_condition_value` — THREE
    ///   paired reads (`type`, `status`, `message`) inside the
    ///   condition-walker's per-condition classifier, each pulling a
    ///   `Value::String` slot off a K8s Condition object off the
    ///   `status.conditions[]` array.
    ///
    /// All seven sites walked the SAME two-link chain — `.get(<key>)`
    /// on a `serde_json::Value` already known to be an object, then
    /// `.and_then(|v| v.as_str())` on the returned `Option<&Value>` —
    /// and each composed different downstream tails (fallible
    /// `.ok_or_else(...)?.to_string()`, optional `.map(String::from)`,
    /// pattern-match `Some("True")` / `Some("False")` / `_`). Post-lift
    /// each callsite reads `<value>.get_str(<key>)` and the two-link
    /// READ chain lives at ONE substrate owner here.
    ///
    /// ### Naming — `get_str`, not `as_str` or `str_at`
    ///
    /// Same discipline as [`Self::get_i64`] — the trait method
    /// deliberately does NOT collide with `serde_json::Value::as_str`
    /// (the inherent projection on a single `Value` handle) nor with
    /// `serde_json::Value::get` (the inherent slot-lookup returning
    /// `Option<&Value>`). A name collision would let a caller who has
    /// [`ValueGetExt`] in scope resolve to one of the inherent methods
    /// by accident (inherent methods win over trait methods in method
    /// resolution) and silently drop half of the paired chain. The
    /// `get_str(<key>)` shape names the intent: look up the slot at
    /// `<key>`, project the returned handle to `&str`, in ONE call.
    ///
    /// ### `#[must_use]`
    ///
    /// Every pre-lift consumer binds the returned `Option<&str>` into
    /// a downstream `.ok_or_else(...)?.to_string()` / `.map(String::from)`
    /// / `.map(str::to_string)` / pattern-match arm. Dropping the
    /// return silently discards the projection entirely, which is
    /// never the intended semantic at any of the seven pre-lift
    /// consumers.
    ///
    /// ### Return lifetime
    ///
    /// The `&str` borrows the same buffer the underlying
    /// `Value::String` variant owns; the `Option<&str>` is bounded by
    /// the receiver's lifetime (`&'_ self`), so a caller holding onto
    /// the returned slice keeps the receiver borrowed. Matches the
    /// pre-lift chain's own borrow shape (`v.as_str()` borrows through
    /// the `&Value`).
    ///
    /// A future normalization on the projection — a Unicode
    /// normalization pass (NFC-folding annotation values), a
    /// per-fleet trim of leading/trailing whitespace, a rejection of
    /// empty-string arms as "the caller meant absent" — lands at THIS
    /// ONE substrate primitive and every downstream `apiVersion` /
    /// `kind` / `metadata.name` / K8s-condition-string reader
    /// inherits the upgrade mechanically.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the two-link `.get(<key>).and_then(|v| v.as_str())` chain
    /// recurred at SEVEN production sites across two crates past the
    /// ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to
    /// ONE substrate owner here on the string axis of the same
    /// READ-chain axis-family the `get_i64` sibling opened for the
    /// integer axis). THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — a regression that drifted the projection
    /// axis at ONE site would silently pass every downstream
    /// composition and surface as a wrong slot at operator-facing
    /// diagnostic wording; post-lift the projection lives at ONE
    /// typed owner so a regression surfaces at
    /// [`tests::get_str_present_string_slot_returns_the_slice`] /
    /// peers rather than as silent operator-facing drift).
    #[must_use = "a JSON &str projection that isn't bound swallows the slot entirely"]
    fn get_str(&self, key: &str) -> Option<&str>;

    /// Look up `key` on this JSON object and project the returned
    /// handle to `&Vec<Value>`; returns `None` when the slot is
    /// absent, when the receiver is not a JSON object, or when the
    /// slot's variant is not `Value::Array`.
    ///
    /// Array-axis sibling of [`Self::get_i64`] + [`Self::get_str`]
    /// on the same `.get(<key>).and_then(|v| v.as_<T>())` READ-chain
    /// axis-family. Pre-lift the two-link chain was hand-authored at
    /// TWO production sites across two crates past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication threshold:
    ///
    /// * `tatara-reconciler::ssapply::ready_condition_value` — the
    ///   tail of the `data.get("status").and_then(|s|
    ///   s.get("conditions")).and_then(|c| c.as_array())` walker that
    ///   opens the K8s Condition classifier every DynamicObject
    ///   readiness probe rides through.
    /// * `tatara-closed-loop-probe::probe::count_jwks_keys` — the
    ///   JWKS-response walker that counts issuer-side public keys off
    ///   the `keys` slot for the closed-loop probe's per-run
    ///   `jwks_key_count` diagnostic.
    ///
    /// Both sites walked the SAME two-link chain — `.get(<key>)` on a
    /// `serde_json::Value` already known to be an object, then
    /// `.and_then(|v| v.as_array())` on the returned `Option<&Value>`
    /// — and composed different downstream tails (`Some(conditions)`
    /// pattern-match on the reconciler side, `.map(|xs| xs.len() as
    /// u64)` on the probe side). Post-lift each callsite reads
    /// `<value>.get_array(<key>)` and the two-link READ chain lives at
    /// ONE substrate owner here. The probe-side variant additionally
    /// sheds the pre-lift `.get("keys").cloned()` allocation because
    /// this primitive borrows through the receiver rather than
    /// cloning.
    ///
    /// ### Naming — `get_array`, not `as_array` or `array_at`
    ///
    /// Same discipline as [`Self::get_i64`] + [`Self::get_str`] — the
    /// trait method deliberately does NOT collide with
    /// `serde_json::Value::as_array` (the inherent projection on a
    /// single `Value` handle) nor with `serde_json::Value::get` (the
    /// inherent slot-lookup returning `Option<&Value>`). A name
    /// collision would let a caller who has [`ValueGetExt`] in scope
    /// resolve to one of the inherent methods by accident (inherent
    /// methods win over trait methods in method resolution) and
    /// silently drop half of the paired chain. The `get_array(<key>)`
    /// shape names the intent: look up the slot at `<key>`, project
    /// the returned handle to `&Vec<Value>`, in ONE call.
    ///
    /// ### `#[must_use]`
    ///
    /// Every pre-lift consumer binds the returned `Option<&Vec<Value>>`
    /// into a downstream `let Some(...) = ... else { return ... }`
    /// short-circuit or a `.map(|xs| xs.len() as u64).unwrap_or(0)`
    /// counter composition. Dropping the return silently discards the
    /// projection entirely, which is never the intended semantic at
    /// either pre-lift consumer.
    ///
    /// ### Return lifetime
    ///
    /// The `&Vec<Value>` borrows the same buffer the underlying
    /// `Value::Array` variant owns; the `Option<&Vec<Value>>` is
    /// bounded by the receiver's lifetime (`&'_ self`), so a caller
    /// iterating the returned slice keeps the receiver borrowed.
    /// Matches the pre-lift chain's own borrow shape (`v.as_array()`
    /// borrows through the `&Value`), and in the probe.rs case
    /// eliminates the pre-lift `.cloned()` on the intermediate
    /// `Value` that only existed to sidestep the borrow.
    ///
    /// A future normalization on the projection — a rejection of
    /// empty arrays as "the caller meant absent", an accept-scalar
    /// coercion (a `Value::String` promoted to a one-element array),
    /// a per-fleet cap on array length that short-circuits pathological
    /// payloads — lands at THIS ONE substrate primitive and every
    /// downstream K8s-Condition classifier / JWKS-array counter /
    /// future array-slot reader inherits the upgrade mechanically.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the two-link `.get(<key>).and_then(|v| v.as_array())` chain
    /// recurred at two production sites across two crates past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to ONE
    /// substrate owner here on the array axis of the same READ-chain
    /// axis-family the `get_i64` + `get_str` siblings already own).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs — a
    /// regression that drifted the projection axis at ONE site would
    /// silently pass every downstream composition and surface as a
    /// wrong slot at operator-facing diagnostic wording; post-lift the
    /// projection lives at ONE typed owner so a regression surfaces
    /// at [`tests::get_array_present_array_slot_returns_the_slice`] /
    /// peers rather than as silent operator-facing drift).
    #[must_use = "a JSON array projection that isn't bound swallows the slot entirely"]
    fn get_array(&self, key: &str) -> Option<&Vec<Value>>;
}

impl ValueGetExt for Value {
    #[inline]
    fn get_i64(&self, key: &str) -> Option<i64> {
        self.get(key).and_then(Value::as_i64)
    }

    #[inline]
    fn get_str(&self, key: &str) -> Option<&str> {
        self.get(key).and_then(Value::as_str)
    }

    #[inline]
    fn get_array(&self, key: &str) -> Option<&Vec<Value>> {
        self.get(key).and_then(Value::as_array)
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

    // ─── ValueGetExt::get_i64 substrate pins ─────────────────────────
    //
    // Fail-before-pass-after granularity: the `ValueGetExt::get_i64`
    // trait method did not exist before this commit, so each test below
    // fails to compile pre-lift. Post-lift they collectively pin the
    // paired READ-shape at ONE substrate owner — a regression that
    // narrowed the projection to `as_u64` (silently losing every
    // negative counter K8s fixtures can carry for a JSON authoring
    // bug), swapped the slot lookup to `.pointer(<key>)` (losing the
    // direct-child semantics), promoted a present-but-non-integer
    // corner to `Some(0)` (silently paving over a malformed status
    // blob), or drifted the receiver-non-object arm from `None → Some(default)`
    // (silently synthesising a zero counter on a null status blob)
    // surfaces HERE rather than as silent operator-facing skew across
    // the three `boundary.rs::fetch_job_status` pre-lift consumers
    // whose JobStatusView row initialised at `Default::default()` and
    // conditionally overwrote each field on `Some(i64)`.

    #[test]
    fn get_i64_present_integer_slot_returns_the_value() {
        // Primary Ok-arm invariant: a `Value::Number(i)` present at the
        // slot projects to `Some(i)`. Sweeps the three representative
        // counters every pre-lift `JobStatusView` field carried (a
        // completed Job's `succeeded=1`, a failed Job's `failed=3`, a
        // freshly-scheduled Job's `active=5`) so a regression at ONE
        // counter axis surfaces here rather than at the downstream
        // diagnostic.
        let status = json!({ "succeeded": 1, "failed": 3, "active": 5 });
        assert_eq!(status.get_i64("succeeded"), Some(1));
        assert_eq!(status.get_i64("failed"), Some(3));
        assert_eq!(status.get_i64("active"), Some(5));
    }

    #[test]
    fn get_i64_absent_slot_returns_none() {
        // Absent-slot corner: a fresh `batch/v1::Job` before its
        // controller has stamped any counter into `status` (the JSON
        // is `{}` or missing the counter key). Every pre-lift consumer
        // routed this corner through the `if let Some(...)` guard so
        // the `JobStatusView` field kept its `Default::default()` `0`
        // seed. A regression that returned `Some(0)` on the absent
        // corner would collapse the "not yet reported" ↔ "reported
        // zero" distinction the K8s status protocol keeps.
        let status = json!({});
        assert_eq!(status.get_i64("succeeded"), None);
        assert_eq!(status.get_i64("any_missing_key"), None);
    }

    #[test]
    fn get_i64_present_but_non_integer_slot_returns_none() {
        // Present-but-non-integer corner: a `Value::String`, a
        // `Value::Bool`, a `Value::Object`, or a `Value::Array` at the
        // slot ALL fall through to `None` — matches the pre-lift
        // `.and_then(|v| v.as_i64())` chain exactly. A regression that
        // promoted a `Value::String("1")` to `Some(1)` (adding a
        // parse-string fallback) would silently accept a malformed
        // status blob whose author stringified a counter.
        let status = json!({
            "stringy": "1",
            "boolean": true,
            "object": {},
            "array": [],
            "null_valued": null,
        });
        assert_eq!(status.get_i64("stringy"), None);
        assert_eq!(status.get_i64("boolean"), None);
        assert_eq!(status.get_i64("object"), None);
        assert_eq!(status.get_i64("array"), None);
        assert_eq!(status.get_i64("null_valued"), None);
    }

    #[test]
    fn get_i64_negative_counter_survives_the_projection() {
        // Negative-integer corner: `as_i64` accepts negatives; `as_u64`
        // does not. A regression that narrowed the projection to
        // `as_u64` under a mistaken "K8s counters are always non-
        // negative" refactor would silently drop every negative
        // counter a JSON authoring bug could stamp — hiding the bug
        // rather than surfacing it as a counter the diagnostic reports
        // verbatim.
        let status = json!({ "n": -1 });
        assert_eq!(status.get_i64("n"), Some(-1));
    }

    #[test]
    fn get_i64_non_object_receiver_returns_none_verbatim() {
        // Non-object receiver corner: a caller who reached this
        // primitive on a `Value::Null` / `Value::Bool` / `Value::Array`
        // handle (a malformed fetch response, an upstream default-value
        // fallback) MUST get `None` back rather than a panic or a
        // synthesized `Some(default)`. Matches the pre-lift chain's
        // behaviour: `Value::get` on a non-object receiver returns
        // `None`, `and_then` short-circuits.
        assert_eq!(Value::Null.get_i64("any"), None);
        assert_eq!(Value::Bool(true).get_i64("any"), None);
        assert_eq!(json!([1, 2, 3]).get_i64("any"), None);
        assert_eq!(json!("scalar").get_i64("any"), None);
    }

    #[test]
    fn get_i64_matches_pre_lift_hand_authored_chain_shape() {
        // Byte-shape parity pin: `<value>.get_i64(<key>)` MUST return
        // the SAME `Option<i64>` the pre-lift hand-authored
        // `.get(<key>).and_then(|v| v.as_i64())` chain produced.
        // Sweeps the six pre-lift-reachable input corners (the three
        // "value present" + three "value absent/malformed" arms every
        // fetch_job_status callsite reached) so a regression at the
        // primitive that broke byte identity with the pre-lift chain at
        // ONE corner surfaces here rather than as a per-counter
        // divergence at the fetched-Job projection.
        let status = json!({
            "succeeded": 2,
            "failed": 0,
            "active": 7,
            "stringy": "1",
            "null_valued": null,
        });
        for key in [
            "succeeded",
            "failed",
            "active",
            "stringy",
            "null_valued",
            "missing",
        ] {
            let via_primitive = status.get_i64(key);
            let via_pre_lift = status.get(key).and_then(|v| v.as_i64());
            assert_eq!(
                via_primitive, via_pre_lift,
                "corner `{key}` must round-trip through both shapes",
            );
        }
    }

    #[test]
    fn get_i64_composes_with_unwrap_or_default_at_default_seed_shape() {
        // Downstream composition pin: the canonical caller shape
        // post-lift is `<status>.get_i64(<key>).unwrap_or_default()` —
        // matches the pre-lift `JobStatusView::default()` seed +
        // conditional `if let Some(n)` write pattern. A regression that
        // reshaped the return form (an `i64` bare default, a
        // `Result<i64, _>` fallible arm) would break this composition.
        let status = json!({ "succeeded": 4 });
        // Absent slot composes to the type default (0 for i64).
        assert_eq!(status.get_i64("missing").unwrap_or_default(), 0_i64);
        // Present slot composes to the projected counter.
        assert_eq!(status.get_i64("succeeded").unwrap_or_default(), 4_i64);
    }

    // ─── ValueGetExt::get_str substrate pins ─────────────────────────
    //
    // Fail-before-pass-after granularity: the `ValueGetExt::get_str`
    // trait method did not exist before this commit, so each test below
    // fails to compile pre-lift. Post-lift they collectively pin the
    // paired READ-shape at ONE substrate owner — a regression that
    // narrowed the projection to the wrong variant (accepting
    // `Value::Number`-stringified slots via a fallback, or accepting
    // `Value::Null` as `Some("")`), swapped the slot lookup to
    // `.pointer(<key>)` (losing the direct-child semantics), promoted
    // an absent slot to `Some("")` (silently paving over a missing
    // required slot), or drifted the receiver-non-object arm from
    // `None` (silently synthesising an empty string on a null status
    // blob) surfaces HERE rather than as silent operator-facing skew
    // across the SEVEN pre-lift consumers (`status::from_json`'s four
    // rendered-resource coordinate reads + `ssapply::ready_condition_value`'s
    // three K8s Condition slot reads).

    #[test]
    fn get_str_present_string_slot_returns_the_slice() {
        // Primary Ok-arm invariant: a `Value::String(s)` present at the
        // slot projects to `Some(s.as_str())`. Sweeps the four
        // representative slots the pre-lift `RenderedResourceCoords::
        // from_json` consumer walked (`apiVersion`, `kind`,
        // `metadata.name`, `metadata.namespace`) so a regression at
        // ONE axis surfaces here rather than at the downstream
        // typed row's coordinate.
        let manifest = json!({
            "apiVersion": "helm.toolkit.fluxcd.io/v2",
            "kind": "HelmRelease",
            "name": "demo-app",
            "namespace": "demo",
        });
        assert_eq!(
            manifest.get_str("apiVersion"),
            Some("helm.toolkit.fluxcd.io/v2"),
        );
        assert_eq!(manifest.get_str("kind"), Some("HelmRelease"));
        assert_eq!(manifest.get_str("name"), Some("demo-app"));
        assert_eq!(manifest.get_str("namespace"), Some("demo"));
    }

    #[test]
    fn get_str_absent_slot_returns_none() {
        // Absent-slot corner: a rendered manifest whose author forgot
        // the `apiVersion` slot (a common authoring bug) MUST return
        // `None` so `RenderedResourceCoords::from_json` fails loud
        // rather than silently synthesising an empty apiVersion. A
        // regression that returned `Some("")` on the absent corner
        // would collapse the "not authored" ↔ "authored empty"
        // distinction the fail-loud gate depends on.
        let manifest = json!({ "kind": "HelmRelease" });
        assert_eq!(manifest.get_str("apiVersion"), None);
        assert_eq!(manifest.get_str("any_missing_key"), None);
    }

    #[test]
    fn get_str_present_but_non_string_slot_returns_none() {
        // Present-but-non-string corner: a `Value::Number`,
        // `Value::Bool`, `Value::Object`, `Value::Array`, or
        // `Value::Null` at the slot ALL fall through to `None` —
        // matches the pre-lift `.and_then(|v| v.as_str())` chain
        // exactly. A regression that stringified a `Value::Number`
        // (adding a `to_string()` fallback) would silently accept a
        // malformed manifest whose author numeric-typed a
        // conventionally-string slot.
        let manifest = json!({
            "numeric": 1,
            "boolean": true,
            "object": {},
            "array": [],
            "null_valued": null,
        });
        assert_eq!(manifest.get_str("numeric"), None);
        assert_eq!(manifest.get_str("boolean"), None);
        assert_eq!(manifest.get_str("object"), None);
        assert_eq!(manifest.get_str("array"), None);
        assert_eq!(manifest.get_str("null_valued"), None);
    }

    #[test]
    fn get_str_empty_string_slot_survives_the_projection() {
        // Empty-string corner: a `Value::String("")` present at the
        // slot MUST project to `Some("")` — matches the pre-lift
        // `.and_then(|v| v.as_str())` chain exactly, keeping the
        // "authored empty" arm distinct from the "not authored" arm
        // upstream. A regression that promoted `Some("")` to `None`
        // under a "reject empty strings" refactor would silently
        // collapse the two arms and turn a valid empty `metadata.
        // namespace` (a cluster-scoped resource) into a fail-loud
        // error at the required-slot gates.
        let manifest = json!({ "namespace": "" });
        assert_eq!(manifest.get_str("namespace"), Some(""));
    }

    #[test]
    fn get_str_non_object_receiver_returns_none_verbatim() {
        // Non-object receiver corner: a caller who reached this
        // primitive on a `Value::Null` / `Value::Bool` / `Value::Array`
        // handle (a malformed fetch response, an upstream default-value
        // fallback, a `serde_json::Value::Null` metadata slot chained
        // through `.and_then`) MUST get `None` back rather than a
        // panic or a synthesized `Some("")`. Matches the pre-lift
        // chain's behaviour: `Value::get` on a non-object receiver
        // returns `None`, `and_then` short-circuits.
        assert_eq!(Value::Null.get_str("any"), None);
        assert_eq!(Value::Bool(true).get_str("any"), None);
        assert_eq!(json!([1, 2, 3]).get_str("any"), None);
        assert_eq!(json!("scalar").get_str("any"), None);
    }

    #[test]
    fn get_str_matches_pre_lift_hand_authored_chain_shape() {
        // Byte-shape parity pin: `<value>.get_str(<key>)` MUST return
        // the SAME `Option<&str>` the pre-lift hand-authored
        // `.get(<key>).and_then(|v| v.as_str())` chain produced.
        // Sweeps every pre-lift-reachable input corner (three
        // "value present" + three "value absent/malformed" arms every
        // status.rs / ssapply.rs callsite reached) so a regression at
        // the primitive that broke byte identity with the pre-lift
        // chain at ONE corner surfaces here rather than as a
        // per-slot divergence downstream.
        let manifest = json!({
            "apiVersion": "v1",
            "kind": "ConfigMap",
            "type": "Ready",
            "numeric": 1,
            "null_valued": null,
        });
        for key in [
            "apiVersion",
            "kind",
            "type",
            "numeric",
            "null_valued",
            "missing",
        ] {
            let via_primitive = manifest.get_str(key);
            let via_pre_lift = manifest.get(key).and_then(|v| v.as_str());
            assert_eq!(
                via_primitive, via_pre_lift,
                "corner `{key}` must round-trip through both shapes",
            );
        }
    }

    #[test]
    fn get_str_composes_with_ok_or_else_at_from_json_shape() {
        // Downstream composition pin: the canonical caller shape at
        // `RenderedResourceCoords::from_json` is
        // `<manifest>.get_str(<key>).ok_or_else(|| anyhow!("rendered
        // resource missing X"))?.to_string()`. A regression that
        // reshaped the return form (an `&str` bare default, a
        // `Result<&str, _>` fallible arm) would break this
        // composition. Additionally sweeps the peer
        // `.map(String::from)` / `.map(str::to_string)` optional-slot
        // arm the `namespace` slot uses.
        let manifest = json!({ "apiVersion": "v1" });
        let ok_arm: String = manifest
            .get_str("apiVersion")
            .ok_or_else(|| anyhow::anyhow!("missing"))
            .unwrap()
            .to_string();
        assert_eq!(ok_arm, "v1");
        let err_arm = manifest
            .get_str("kind")
            .ok_or_else(|| anyhow::anyhow!("rendered resource missing kind"))
            .unwrap_err();
        assert_eq!(format!("{err_arm}"), "rendered resource missing kind");
        let opt_present: Option<String> = manifest.get_str("apiVersion").map(str::to_string);
        assert_eq!(opt_present.as_deref(), Some("v1"));
        let opt_absent: Option<String> = manifest.get_str("kind").map(String::from);
        assert!(opt_absent.is_none());
    }

    #[test]
    fn get_str_return_lifetime_borrows_receiver_not_owned() {
        // Return-lifetime pin: the `&str` MUST borrow the receiver's
        // buffer rather than a fresh owned `String`. A regression that
        // reshaped the return to `Option<String>` (adding a
        // `to_string()` inside the primitive) would inflate every
        // callsite's allocation count and break `metadata.and_then(|m|
        // m.get_str("name"))`'s per-lookup zero-alloc guarantee. Bind
        // the invariant structurally: the borrow reaches back through
        // the receiver.
        let manifest = json!({ "apiVersion": "helm.toolkit.fluxcd.io/v2" });
        let s: &str = manifest.get_str("apiVersion").unwrap();
        let raw: &str = manifest.get("apiVersion").and_then(|v| v.as_str()).unwrap();
        assert!(std::ptr::eq(s.as_ptr(), raw.as_ptr()));
    }

    #[test]
    fn get_str_axis_family_reaches_i64_and_str_through_one_trait_import() {
        // Axis-family pin: a caller who imports `ValueGetExt` reaches
        // BOTH the string axis (`get_str`) and the integer axis
        // (`get_i64`) through the SAME trait handle. A regression that
        // opened a peer `ValueGetStrExt` (or a peer trait per axis)
        // would break this — the caller would have to import each
        // trait separately and a partial import would silently miss
        // one axis at method-resolution time.
        //
        // Structurally: a bound `T: ValueGetExt` reaches both methods.
        fn probe<T: ValueGetExt>(t: &T) -> (Option<i64>, Option<&str>) {
            (t.get_i64("n"), t.get_str("s"))
        }
        let mixed = json!({ "n": 7, "s": "hello" });
        let (n, s) = probe(&mixed);
        assert_eq!(n, Some(7));
        assert_eq!(s, Some("hello"));
    }

    // ─── ValueGetExt::get_array substrate pins ───────────────────────
    //
    // Fail-before-pass-after granularity: the `ValueGetExt::get_array`
    // trait method did not exist before this commit, so each test below
    // fails to compile pre-lift. Post-lift they collectively pin the
    // paired READ-shape at ONE substrate owner — a regression that
    // narrowed the projection to the wrong variant (accepting an
    // object slot via a `.values().collect()` synthesis, promoting an
    // absent slot to `Some(&Vec::new())`), swapped the slot lookup to
    // `.pointer(<key>)` (losing the direct-child semantics), or
    // drifted the receiver-non-object arm from `None` (silently
    // synthesising an empty array on a null status blob) surfaces
    // HERE rather than as silent operator-facing skew across the two
    // pre-lift consumers (`ssapply::ready_condition_value`'s
    // `status.conditions` walker + `probe::count_jwks_keys`'s `keys`
    // counter).

    #[test]
    fn get_array_present_array_slot_returns_the_slice() {
        // Primary Ok-arm invariant: a `Value::Array` present at the
        // slot projects to `Some(&Vec::new())`-shaped borrow. Sweeps
        // the two representative shapes the pre-lift consumers walked
        // (a K8s `status.conditions` array of Condition objects on the
        // reconciler side; a JWKS `keys` array of key objects on the
        // probe side).
        let status = json!({
            "conditions": [
                { "type": "Ready", "status": "True" },
                { "type": "Progressing", "status": "False" },
            ],
        });
        let via = status.get_array("conditions").expect("Value::Array");
        assert_eq!(via.len(), 2);
        assert_eq!(via[0]["type"], "Ready");

        let jwks = json!({
            "keys": [
                { "kty": "RSA", "kid": "1" },
                { "kty": "RSA", "kid": "2" },
                { "kty": "EC",  "kid": "3" },
            ],
        });
        assert_eq!(
            jwks.get_array("keys").map(Vec::len),
            Some(3),
            "probe count_jwks_keys composition must reach the same tail as pre-lift",
        );
    }

    #[test]
    fn get_array_absent_slot_returns_none() {
        // Absent-slot corner: a fresh K8s status blob whose controller
        // has not stamped `conditions` yet (the `data.get("status")`
        // walker yields an object without the slot) MUST return
        // `None` so the caller's `let Some(...) = ... else { return
        // ReadyState::Unknown }` short-circuit fires. A regression that
        // returned `Some(&Vec::new())` on the absent corner would
        // silently drive the caller into an empty for-loop and skip
        // the fail-safe.
        let status = json!({});
        assert_eq!(status.get_array("conditions"), None);
        assert_eq!(status.get_array("any_missing_key"), None);
    }

    #[test]
    fn get_array_present_but_non_array_slot_returns_none() {
        // Present-but-non-array corner: a `Value::String`,
        // `Value::Number`, `Value::Bool`, `Value::Object`, or
        // `Value::Null` at the slot ALL fall through to `None` —
        // matches the pre-lift `.and_then(|v| v.as_array())` chain
        // exactly. A regression that wrapped a scalar in a single-
        // element array under a "tolerant" refactor would silently
        // accept a malformed status blob whose author collapsed the
        // conditions array to a single scalar.
        let status = json!({
            "stringy": "ready",
            "numeric": 1,
            "boolean": true,
            "object": { "nested": true },
            "null_valued": null,
        });
        assert_eq!(status.get_array("stringy"), None);
        assert_eq!(status.get_array("numeric"), None);
        assert_eq!(status.get_array("boolean"), None);
        assert_eq!(status.get_array("object"), None);
        assert_eq!(status.get_array("null_valued"), None);
    }

    #[test]
    fn get_array_empty_array_slot_survives_the_projection() {
        // Empty-array corner: a `Value::Array` with zero elements at
        // the slot MUST project to `Some(&Vec::new())` — matches the
        // pre-lift chain exactly, keeping the "authored empty" arm
        // distinct from the "not authored" arm upstream. The probe
        // consumer's `.map(|xs| xs.len() as u64).unwrap_or(0)` tail
        // depends on this: an authored-empty JWKS array reports 0
        // keys, distinct from a JWKS response missing the `keys` slot
        // altogether (which the caller could later choose to log
        // differently).
        let jwks = json!({ "keys": [] });
        let arr = jwks.get_array("keys").expect("Value::Array");
        assert!(arr.is_empty());
        assert_eq!(jwks.get_array("keys").map(Vec::len), Some(0));
    }

    #[test]
    fn get_array_non_object_receiver_returns_none_verbatim() {
        // Non-object receiver corner: a caller who reached this
        // primitive on a `Value::Null` / `Value::Bool` / `Value::Array`
        // handle (a malformed fetch response, an upstream default-value
        // fallback, a `serde_json::Value::Null` intermediate chained
        // through `.and_then`) MUST get `None` back rather than a
        // panic or a synthesized `Some(&Vec::new())`. Matches the
        // pre-lift chain's behaviour: `Value::get` on a non-object
        // receiver returns `None`, `and_then` short-circuits.
        assert_eq!(Value::Null.get_array("any"), None);
        assert_eq!(Value::Bool(true).get_array("any"), None);
        assert_eq!(json!([1, 2, 3]).get_array("any"), None);
        assert_eq!(json!("scalar").get_array("any"), None);
    }

    #[test]
    fn get_array_matches_pre_lift_hand_authored_chain_shape() {
        // Byte-shape parity pin: `<value>.get_array(<key>)` MUST return
        // the SAME `Option<&Vec<Value>>` the pre-lift hand-authored
        // `.get(<key>).and_then(|v| v.as_array())` chain produced.
        // Sweeps every pre-lift-reachable input corner (three
        // "value present" + three "value absent/malformed" arms
        // covering the two pre-lift consumers) so a regression at the
        // primitive that broke byte identity with the pre-lift chain
        // at ONE corner surfaces here rather than as a per-slot
        // divergence downstream.
        let manifest = json!({
            "conditions": [{ "type": "Ready" }],
            "keys": [{ "kid": "1" }, { "kid": "2" }],
            "empty": [],
            "stringy": "not-an-array",
            "null_valued": null,
        });
        for key in [
            "conditions",
            "keys",
            "empty",
            "stringy",
            "null_valued",
            "missing",
        ] {
            let via_primitive = manifest.get_array(key);
            let via_pre_lift = manifest.get(key).and_then(|v| v.as_array());
            assert_eq!(
                via_primitive, via_pre_lift,
                "corner `{key}` must round-trip through both shapes",
            );
        }
    }

    #[test]
    fn get_array_composes_with_len_map_at_probe_count_jwks_keys_shape() {
        // Downstream composition pin: the canonical caller shape at
        // `probe::count_jwks_keys` is `<body_val>.get_array(<key>).
        // map(|xs| xs.len() as u64).unwrap_or(0)` — matches the
        // pre-lift `.get(<key>).cloned().and_then(|k| k.as_array().
        // map(|xs| xs.len() as u64)).unwrap_or(0)` chain shed of its
        // pre-lift `.cloned()` allocation. A regression that reshaped
        // the return form (an `Option<Vec<Value>>` owned, a
        // `Result<...>` fallible arm) would break this composition
        // AND reintroduce the eliminated allocation.
        let jwks = json!({ "keys": [{ "kid": "1" }, { "kid": "2" }, { "kid": "3" }] });
        let n: u64 = jwks
            .get_array("keys")
            .map(|xs| xs.len() as u64)
            .unwrap_or(0);
        assert_eq!(n, 3);
        // Missing slot composes to 0 through the same unwrap_or arm.
        let empty = json!({});
        let z: u64 = empty
            .get_array("keys")
            .map(|xs| xs.len() as u64)
            .unwrap_or(0);
        assert_eq!(z, 0);
    }

    #[test]
    fn get_array_composes_with_let_else_short_circuit_at_ready_condition_shape() {
        // Downstream composition pin: the canonical caller shape at
        // `ssapply::ready_condition_value` is `let Some(conditions) =
        // <data>.get("status").and_then(|s| s.get_array("conditions"))
        // else { return ReadyState::Unknown; }` — the walker rides
        // the `get_array` primitive on the tail of a nested walk. A
        // regression that changed the return to `Option<Vec<Value>>`
        // owned would break the `for c in conditions` borrow-iterate
        // pattern downstream (each `c` borrows through the receiver).
        let data = json!({
            "status": {
                "conditions": [
                    { "type": "Ready",       "status": "True" },
                    { "type": "Progressing", "status": "False" },
                ],
            },
        });
        let conditions = data
            .get("status")
            .and_then(|s| s.get_array("conditions"))
            .expect("nested walk resolves");
        assert_eq!(conditions.len(), 2);
        // Verifies borrow-through-receiver: iterate without cloning.
        let types: Vec<&str> = conditions
            .iter()
            .filter_map(|c| c.get_str("type"))
            .collect();
        assert_eq!(types, vec!["Ready", "Progressing"]);
    }

    #[test]
    fn get_array_return_lifetime_borrows_receiver_not_owned() {
        // Return-lifetime pin: the `&Vec<Value>` MUST borrow the
        // receiver's buffer rather than a fresh owned `Vec`. A
        // regression that reshaped the return to `Option<Vec<Value>>`
        // (adding a `.clone()` inside the primitive) would inflate
        // every callsite's allocation count and — for the
        // ssapply.rs caller — reintroduce a per-reconcile clone of
        // every K8s Condition on every DynamicObject readiness probe.
        // Bind the invariant structurally: the borrow reaches back
        // through the receiver.
        let manifest = json!({ "keys": [{ "kid": "1" }, { "kid": "2" }] });
        let via_primitive: &Vec<Value> = manifest.get_array("keys").unwrap();
        let via_raw: &Vec<Value> = manifest.get("keys").and_then(|v| v.as_array()).unwrap();
        assert!(std::ptr::eq(via_primitive.as_ptr(), via_raw.as_ptr()));
    }

    #[test]
    fn get_array_axis_family_reaches_i64_str_and_array_through_one_trait_import() {
        // Axis-family pin: a caller who imports `ValueGetExt` reaches
        // the integer axis (`get_i64`), the string axis (`get_str`),
        // AND the array axis (`get_array`) through the SAME trait
        // handle. A regression that opened a peer `ValueGetArrayExt`
        // (or a peer trait per axis) would break this — the caller
        // would have to import each trait separately and a partial
        // import would silently miss one axis at method-resolution
        // time.
        //
        // Structurally: a bound `T: ValueGetExt` reaches all three
        // methods. This test extends the pre-existing
        // `get_str_axis_family_reaches_i64_and_str_through_one_trait_import`
        // sibling to cover the new axis; either drops means the
        // axis-family invariant no longer holds.
        fn probe<T: ValueGetExt>(t: &T) -> (Option<i64>, Option<&str>, Option<&Vec<Value>>) {
            (t.get_i64("n"), t.get_str("s"), t.get_array("a"))
        }
        let mixed = json!({ "n": 7, "s": "hello", "a": [1, 2, 3] });
        let (n, s, a) = probe(&mixed);
        assert_eq!(n, Some(7));
        assert_eq!(s, Some("hello"));
        assert_eq!(a.map(Vec::len), Some(3));
    }
}
