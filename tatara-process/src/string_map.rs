//! Substrate primitive over `std::collections::BTreeMap<String, String>`
//! — the ONE substrate owner of the `<map>.insert(<k>.to_string(),
//! <v>.to_string())` string-string insertion shape every K8s-carrier
//! writer (ObjectMeta `annotations` / `labels`, ConfigMap `data`)
//! restates by hand at the `&str × &str → BTreeMap` write boundary.
//!
//! Receiver-shape peer of [`crate::json_object::JsonMapStrExt::insert_str`]
//! on the "insert a string at a string key" write axis, partitioned by
//! CARRIER TYPE:
//!
//! * [`crate::json_object::JsonMapStrExt::insert_str`] — the
//!   `serde_json::Map<String, Value>` receiver used by every JSON-shaped
//!   `metadata.annotations` / label map / spec-body slot the reconciler
//!   emits to K8s through SSA-time `serde_json::Value` bodies.
//! * [`BTreeMapStrExt::insert_str`] (this trait) — the
//!   `BTreeMap<String, String>` receiver used by every K8s-canonical
//!   ObjectMeta annotations / labels slot AND every ConfigMap `.data`
//!   slot the workspace stamps through the `kube-rs` typed API surface
//!   (which reifies `ObjectMeta.annotations: Option<BTreeMap<String,
//!   String>>` verbatim).
//!
//! The two traits deliberately share the `insert_str` method name AND
//! the `(impl Into<String>, impl Into<String>)` argument shape so a
//! caller who imports either substrate reaches the same-shape write
//! call at the same-name method, regardless of whether the receiver is
//! the JSON-side `Map<String, Value>` or the K8s-typed-API-side
//! `BTreeMap<String, String>`. A future new carrier (e.g. a
//! `HashMap<String, String>` receiver for a lightweight fixture map,
//! or a `secrecy::Secret<String>` value-slot for encrypted secret
//! payloads) adds one impl arm here without splitting the substrate
//! into a third trait.
//!
//! Pre-lift the shape was hand-authored at THREE production sites
//! across two workspace crates past the ★★ PRIME-DIRECTIVE ≥ 2
//! duplication threshold:
//!
//! * `tatara-pool-reconciler::controller_pool::build_member_process`
//!   × 2 — the pool-membership annotation seed stamping
//!   `annotations::POOL` + `annotations::POOL_SLOT` into the fresh
//!   member Process's `metadata.annotations` map right after `Process::
//!   new`. Both restated the same `<map>.insert(<key>.to_string(),
//!   <val>.to_string())` shape.
//! * `tatara-export-worker::write_receipt` × 1 — the receipt-CM `.data`
//!   seed stamping the `(configmap-key, payload)` pair into the fresh
//!   `BTreeMap` right before the [`crate::configmap::with_data`]
//!   composer wraps it as a `ConfigMap` wire body.
//!
//! All three sites walked the SAME two-position insert shape verbatim,
//! differing only in the `&str` / `&'static str` key + the `&str` /
//! numeric-`.to_string()` value at each callsite. Post-lift each
//! callsite reads `<map>.insert_str(<key>, <val>)` and the string-
//! string write shape lives at ONE substrate owner here.
//!
//! ### Naming — `insert_str`, not `insert`
//!
//! Same discipline as the sibling
//! [`crate::json_object::JsonMapStrExt::insert_str`] — the trait method
//! deliberately does NOT collide with the inherent `BTreeMap::insert`
//! (which takes `(String, String)` positionally). A name collision
//! would let a caller who has [`BTreeMapStrExt`] in scope resolve to
//! the inherent method by accident (inherent methods win over trait
//! methods in method resolution) and silently drop the `Into<String>`
//! coerce on either slot. The `_str` suffix names the intent: both
//! slots project the caller's borrowed handle into an owned `String`
//! at the substrate, not at every callsite.
//!
//! Theory anchor: THEORY.md §VI.1 (generation over composition — the
//! `<map>.insert(<k>.to_string(), <v>.to_string())` shape recurred at
//! three hand-authored production sites across two workspace crates
//! past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted
//! to ONE substrate owner here). THEORY.md §II.1 invariant 5
//! (composition preserves proofs — a regression that drifts the write
//! shape at ONE consumer surfaces at the substrate pin rather than as
//! silent per-emit skew across every K8s-carrier annotations / labels
//! / ConfigMap-data writer).

use std::collections::BTreeMap;

/// Substrate extension trait over `BTreeMap<String, String>` — the ONE
/// substrate owner of the `<map>.insert(<k>.to_string(), <v>.to_string
/// ())` string-string insertion shape every K8s-carrier writer
/// (ObjectMeta annotations / labels, ConfigMap `.data`) hand-authored
/// at the callsite pre-lift.
///
/// Peer of [`crate::json_object::JsonMapStrExt`] on the same
/// "insert a string at a string key" write axis, split by receiver
/// type (JSON-shaped `Map<String, Value>` vs K8s-canonical
/// `BTreeMap<String, String>`). See the module docs for the naming
/// rationale (why `insert_str` and not `insert`) and the callsite
/// audit.
pub trait BTreeMapStrExt {
    /// Insert an owned `String` value at an owned `String` key into
    /// this K8s-canonical string-string map. Returns `Option<String>`
    /// matching the underlying [`BTreeMap::insert`] semantics — `None`
    /// for a new key, `Some(prev)` for an overwrite of an existing
    /// slot.
    ///
    /// Both slots accept any `impl Into<String>` — `&str` (via
    /// `String::from`), `String` (identity), `Cow<'_, str>`, so a
    /// callsite with a static `annotations::POOL` (`&'static str`)
    /// reads `insert_str(annotations::POOL, pool_name)` with no
    /// `.to_string()` per-site. Numeric or non-string values still
    /// need an explicit `.to_string()` at the callsite — same as
    /// pre-lift, so the wrapping shape stays visible in the caller's
    /// grep footprint.
    fn insert_str(&mut self, key: impl Into<String>, value: impl Into<String>) -> Option<String>;
}

impl BTreeMapStrExt for BTreeMap<String, String> {
    #[inline]
    fn insert_str(&mut self, key: impl Into<String>, value: impl Into<String>) -> Option<String> {
        self.insert(key.into(), value.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── BTreeMapStrExt::insert_str substrate pins ──────────────────
    //
    // Fail-before-pass-after granularity: the `BTreeMapStrExt::insert_str`
    // trait method did not exist before this commit, so each test below
    // fails to compile pre-lift. Post-lift they collectively pin the
    // string-string insert shape at ONE substrate owner — a regression
    // that widened the return shape (e.g. `Result<Option<String>>`),
    // dropped the `Into<String>` coerce on either slot, or promoted a
    // silent-write arm that swallows overwrites surfaces HERE rather
    // than as silent operator-facing skew across the three pre-lift
    // consumer callsites whose observable K8s annotations / labels /
    // ConfigMap-data payload already encoded the flat `String → String`
    // write shape.

    #[test]
    fn insert_str_new_key_returns_none_and_writes_the_slot() {
        // New-key invariant: an insert onto a fresh map returns `None`
        // (byte-identical to the inherent `BTreeMap::insert` semantic
        // the pre-lift chain rode). Post-lift the composed return
        // matches the pre-lift chain bytewise so every downstream
        // consumer that binds the return (a caller sensing overwrite
        // vs new-key via `if let Some(prev) = ...` or `.is_some()`
        // gate) inherits the pre-lift semantics verbatim.
        let mut m: BTreeMap<String, String> = BTreeMap::new();
        let prev = m.insert_str("tatara.pleme.io/pool", "primary");
        assert_eq!(prev, None, "new-key insert must return None");
        assert_eq!(
            m.get("tatara.pleme.io/pool").map(String::as_str),
            Some("primary")
        );
    }

    #[test]
    fn insert_str_existing_key_returns_previous_and_overwrites() {
        // Overwrite invariant: an insert onto an already-populated
        // slot returns the previous `String` value (byte-identical to
        // the inherent `BTreeMap::insert` semantic). A regression that
        // dropped the overwrite return (returning `None` even on
        // pre-populated slots) would silently pass every new-key pin
        // above and surface HERE.
        let mut m: BTreeMap<String, String> = BTreeMap::new();
        m.insert("tatara.pleme.io/pool".to_string(), "old".to_string());
        let prev = m.insert_str("tatara.pleme.io/pool", "new");
        assert_eq!(
            prev,
            Some("old".to_string()),
            "overwrite must return the prior value"
        );
        assert_eq!(
            m.get("tatara.pleme.io/pool").map(String::as_str),
            Some("new")
        );
    }

    #[test]
    fn insert_str_matches_pre_lift_chain_bytewise_across_both_slot_shapes() {
        // Byte-identical parity witness — the substrate composer's
        // returned `BTreeMap` state MUST match the pre-lift `.insert
        // (<k>.to_string(), <v>.to_string())` chain's returned state
        // bytewise on every slot shape a pre-lift caller threaded.
        // Sweeps the two production shapes:
        //
        //   1. `&'static str` key + `&str` value — the
        //      `annotations::POOL` + `pool_name` shape at
        //      `tatara-pool-reconciler::controller_pool::
        //      build_member_process`.
        //   2. `&'static str` key + `String` value (from numeric
        //      `.to_string()`) — the `annotations::POOL_SLOT` + `slot
        //      .to_string()` shape at the same production site (the
        //      `slot: u32` argument coerces via `.to_string()` at the
        //      callsite before reaching the primitive).
        //
        // A regression that drifted either slot's `Into<String>` arm
        // (a caller who reached for `.insert(k, v)` after refactoring
        // from a `String` value slot to a plain `&str` value slot)
        // would type-mismatch at the substrate rather than silently
        // slip through with byte-different `String` content.
        let key: &'static str = "tatara.pleme.io/pool";
        let value_borrowed: &str = "primary";
        let value_owned: String = 7u32.to_string();

        let mut via_composer: BTreeMap<String, String> = BTreeMap::new();
        via_composer.insert_str(key, value_borrowed);
        via_composer.insert_str("tatara.pleme.io/pool-slot", value_owned.clone());

        let mut via_pre_lift: BTreeMap<String, String> = BTreeMap::new();
        via_pre_lift.insert(key.to_string(), value_borrowed.to_string());
        via_pre_lift.insert("tatara.pleme.io/pool-slot".to_string(), value_owned);

        assert_eq!(
            via_composer, via_pre_lift,
            "BTreeMapStrExt::insert_str must be byte-identical to the pre-lift .insert(<k>.to_string(), <v>.to_string()) chain",
        );
    }

    #[test]
    fn insert_str_accepts_owned_string_on_both_slots() {
        // The `impl Into<String>` bound on both slots must accept an
        // owned `String` (identity `Into` impl) verbatim — a fixture
        // caller that composes both slots dynamically (as
        // `String::from_utf8_lossy` output, format!-produced payloads,
        // etc.) reaches the same primitive without a per-callsite
        // borrow detour. A regression that narrowed either bound to
        // `&str` only (via an accidental `impl AsRef<str>` swap) would
        // reject the owned-`String` corner and fail here.
        let k: String = "tatara.pleme.io/pool".to_string();
        let v: String = "primary".to_string();
        let mut m: BTreeMap<String, String> = BTreeMap::new();
        let prev = m.insert_str(k, v);
        assert_eq!(prev, None);
        assert_eq!(
            m.get("tatara.pleme.io/pool").map(String::as_str),
            Some("primary")
        );
    }
}
