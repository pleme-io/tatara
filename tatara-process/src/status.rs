//! `ProcessStatus` sub-structures — conditions, checked boundaries, Flux refs.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::boundary::Condition;
use crate::crd::Process;

/// Standard K8s Condition (shape of `metav1.Condition`).
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessCondition {
    #[serde(rename = "type")]
    pub type_: String,
    pub status: String,
    pub last_transition_time: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ProcessCondition {
    pub fn ready(reason: impl Into<String>, message: Option<String>) -> Self {
        Self {
            type_: "Ready".into(),
            status: "True".into(),
            last_transition_time: Utc::now(),
            reason: Some(reason.into()),
            message,
        }
    }

    pub fn not_ready(reason: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            type_: "Ready".into(),
            status: "False".into(),
            last_transition_time: Utc::now(),
            reason: Some(reason.into()),
            message: Some(message.into()),
        }
    }

    pub fn attested(root: &str) -> Self {
        Self {
            type_: "Attested".into(),
            status: "True".into(),
            last_transition_time: Utc::now(),
            reason: Some("AttestationWritten".into()),
            message: Some(format!("composed_root={root}")),
        }
    }
}

/// Reference to a FluxCD resource emitted as part of this Process.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FluxResourceRef {
    pub api_version: String,
    pub kind: String,
    pub name: String,
    pub namespace: String,
    #[serde(default)]
    pub ready: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<DateTime<Utc>>,
}

impl FluxResourceRef {
    /// Pure typed projection of the four fetch coordinates
    /// `(namespace, api_version, kind, name)` every consumer that
    /// dispatches this persisted reference through kube-rs's dynamic-
    /// object surface splats by hand pre-lift. The 4-tuple binds the
    /// slot order at ONE typed accessor so a copy-paste at any downstream
    /// consumer cannot swap two adjacent `&str` slots in the fetch call.
    ///
    /// Peer projection to
    /// [`crate::k8s_wire_identity::K8sWireIdentity`] on the static-
    /// identity axis: [`K8sWireIdentity`] carries a
    /// `(&'static str, &'static str)` closed-set variant's pair for
    /// emit-time (RENDER phase) composition; this method carries the
    /// full `(ns, apiVersion, kind, name)` 4-slot borrow for fetch-time
    /// (VERIFY / ATTEST-heartbeat) composition where the ref's payload
    /// comes back off the persisted `ProcessStatus.flux_resources`
    /// slice with owned `String`s rather than static literals. The two
    /// primitives partition the fetch axis by whether the caller starts
    /// from a closed-set variant (emit-time) or a persisted status
    /// slice (fetch-time).
    ///
    /// Pre-lift the 5-slot `ssapply::fetch(client, &r.namespace,
    /// &r.api_version, &r.kind, &r.name)` splat was hand-authored at
    /// TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold
    /// in `tatara-reconciler::phase_machine`:
    /// * `handle_running` — the VERIFY-phase per-ref readiness probe
    ///   that populates the updated `FluxResourceRef` slice with
    ///   `ready` + `message` + `last_check`.
    /// * `handle_attested` — the ATTEST-heartbeat drift detector that
    ///   short-circuits on the first non-Ready ref.
    ///
    /// Both sites splatted the SAME four `&r.X` field borrows in the
    /// SAME order into raw `ssapply::fetch`. A copy-paste that swapped
    /// two adjacent `&str` slots (`&r.api_version` and `&r.kind` are
    /// both strings that look interchangeable to a mechanical
    /// substitution) would silently 404 at wire time and diagnose as a
    /// broken CRD rather than as slot skew at the callsite. Post-lift
    /// each site names the ref ONCE and unpacks it through this ONE
    /// projection; the slot order binds structurally at the tuple
    /// return so a caller cannot desync one axis.
    ///
    /// A future addition (a case-fold normalization on the group, a
    /// virtual-cluster prefix rewrite for multi-tenancy, a
    /// `generateName` fallback on the name slot, a cluster-cache
    /// short-circuit inserted between the projection and the fetch
    /// call) lands at this ONE method and every downstream fetch
    /// consumer inherits the upgrade mechanically — no per-site edit
    /// at `handle_running` / `handle_attested` / any future kenshi-
    /// runner / mirror-audit / drift-probe consumer that grows a third
    /// consumer.
    ///
    /// Return-order pin lives at
    /// [`tests::flux_resource_ref_fetch_coords_binds_slots_by_position`]
    /// so a regression that swapped `namespace` and `api_version`
    /// (both `String`, same type) inside the tuple constructor fails-
    /// loudly here rather than as a silent wire-time 404 at every
    /// downstream fetch consumer.
    ///
    /// Theory grounding: THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the 4-tuple slot order binds at ONE typed
    /// projection so a regression across the two fields of the same
    /// `String` type fails at the projection's positional pin rather
    /// than at every downstream fetch consumer). THEORY.md §VI.1
    /// (generation over composition — the 5-slot splat recurred at
    /// two hand-authored sites past the ≥ 2 duplication trigger, and
    /// is lifted to ONE typed borrow-projection here).
    pub fn fetch_coords(&self) -> (&str, &str, &str, &str) {
        (&self.namespace, &self.api_version, &self.kind, &self.name)
    }
}

/// Identifying coordinates of a rendered K8s resource — the
/// `(apiVersion, kind, metadata.name, metadata.namespace)` 4-tuple
/// every consumer that walks a rendered `serde_json::Value` resource
/// unwraps by hand pre-lift.
///
/// The three K8s API-path segments (`apiVersion`, `kind`,
/// `metadata.name`) are REQUIRED — a rendered resource missing any
/// of them cannot be applied via kube-rs's dynamic API surface, so
/// the extraction fails fast at the boundary rather than as a
/// downstream `Api::patch` panic. `metadata.namespace` is
/// intentionally kept as `Option<String>` because different consumers
/// resolve the fallback differently: `apply_owned` uses the
/// caller-supplied `namespace: &str` argument (the reconciler already
/// resolved the target namespace upstream), while `flux_ref_from_json`
/// records the K8s canonical `"default"` fallback into the persisted
/// `FluxResourceRef.namespace` slot. The peer method
/// [`Self::namespace_or_default`] applies the K8s canonical fallback
/// (`Process::DEFAULT_NAMESPACE = "default"`) for consumers wanting
/// the same shape [`FluxResourceRef.namespace`] carries.
///
/// Pre-lift the 3+1 slot extraction was hand-authored at TWO sites
/// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold across
/// `tatara-reconciler`:
/// * `tatara-reconciler::phase_machine::flux_ref_from_json` — the
///   post-SSA `FluxResourceRef` builder that persists into
///   `ProcessStatus.flux_resources`; namespace half fallback-
///   defaulted to `"default"`.
/// * `tatara-reconciler::ssapply::apply_owned` — the SSA entry
///   point that extracts (apiVersion, kind, name) for the
///   [`kube::Api::patch`] call; namespace half discarded (the
///   `namespace: &str` argument comes from the caller upstream).
///
/// Both callsites restated the same three
/// `.get(K).and_then(|v| v.as_str()).ok_or_else(|| anyhow!(...))?
/// .to_string()` incantations with subtly different error wording
/// (`"resource missing X"` vs `"rendered resource missing X"`); post-
/// lift both route through this ONE substrate owner with the
/// canonical `"rendered resource missing X"` wording. A future
/// addition (case-fold on the group, a rename of the namespace
/// fallback, a stricter kind gate, a Unicode-safe collation step,
/// support for `metadata.generateName` as a name fallback) lands at
/// the primitive's body on the substrate, not at 2 independent
/// hand-writes across 2 reconciler files.
///
/// Namespace fallback const is shared with
/// [`Process::DEFAULT_NAMESPACE`] — a rename of the K8s canonical
/// default namespace lands at that ONE workspace-wide const, not at
/// per-primitive local literals that would drift silently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedResourceCoords {
    /// `apiVersion` — the group+version pair kube-rs uses to resolve
    /// the `ApiResource` for the SSA call.
    pub api_version: String,
    /// `kind` — the resource kind (Kustomization, HelmRelease, …).
    pub kind: String,
    /// `metadata.name` — the API-path leaf segment.
    pub name: String,
    /// `metadata.namespace` — raw from the resource, `None` when the
    /// slot is absent (a cluster-scoped resource, or a namespaced
    /// resource whose namespace was left for the API server to
    /// substitute). Consumers apply their own fallback:
    /// [`Self::namespace_or_default`] applies the K8s canonical
    /// `"default"` (matching what [`FluxResourceRef.namespace`]
    /// records); other consumers substitute a caller-supplied string
    /// (see `tatara-reconciler::ssapply::apply_owned`).
    pub namespace: Option<String>,
}

impl RenderedResourceCoords {
    /// Extract the 4-tuple from a rendered K8s resource JSON `Value`.
    ///
    /// Fails with a canonical `"rendered resource missing X"` message
    /// when any of the three required slots (`apiVersion`, `kind`,
    /// `metadata.name`) is absent or non-string; `metadata.namespace`
    /// is optional and captured as `None` when absent.
    ///
    /// The error wording is pinned by
    /// [`tests::rendered_resource_coords_error_wording_is_canonical`]
    /// so a regression that reshaped the message surfaces at the test
    /// surface rather than as silent drift between the two pre-lift
    /// call sites (which used subtly different wording — `"resource
    /// missing X"` in `apply_owned` vs `"rendered resource missing
    /// X"` in `flux_ref_from_json`).
    pub fn from_json(res: &Value) -> anyhow::Result<Self> {
        let api_version = res
            .get("apiVersion")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("rendered resource missing apiVersion"))?
            .to_string();
        let kind = res
            .get("kind")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("rendered resource missing kind"))?
            .to_string();
        let metadata = res.get("metadata");
        let name = metadata
            .and_then(|m| m.get("name"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("rendered resource missing metadata.name"))?
            .to_string();
        let namespace = metadata
            .and_then(|m| m.get("namespace"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        Ok(Self {
            api_version,
            kind,
            name,
            namespace,
        })
    }

    /// `metadata.namespace` slice with the K8s canonical `"default"`
    /// fallback applied — matching what [`Process::DEFAULT_NAMESPACE`]
    /// spells for the `Process`-borne coordinate primitive family
    /// and what [`FluxResourceRef.namespace`] records into
    /// `ProcessStatus.flux_resources`.
    pub fn namespace_or_default(&self) -> &str {
        self.namespace
            .as_deref()
            .unwrap_or(Process::DEFAULT_NAMESPACE)
    }
}

/// A boundary condition paired with its current satisfaction state.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CheckedCondition {
    #[serde(flatten)]
    pub condition: Condition,
    pub satisfied: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Summary of boundary verification.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoundaryStatus {
    #[serde(default)]
    pub preconditions: Vec<CheckedCondition>,
    #[serde(default)]
    pub postconditions: Vec<CheckedCondition>,
    /// Absolute deadline for VERIFY (derived from `spec.boundary.timeout`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<DateTime<Utc>>,
}

/// Summary of compliance checks at the latest attestation.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ComplianceStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline: Option<String>,
    pub satisfied: u32,
    pub violated: u32,
    pub total: u32,
    #[serde(default)]
    pub violations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ─── RenderedResourceCoords substrate pins ──────────────────────

    #[test]
    fn rendered_resource_coords_from_json_extracts_all_four_slots_when_present() {
        let res = json!({
            "apiVersion": "kustomize.toolkit.fluxcd.io/v1",
            "kind": "Kustomization",
            "metadata": {
                "name": "observability-stack",
                "namespace": "flux-system",
            },
        });
        let c = RenderedResourceCoords::from_json(&res).expect("extract");
        assert_eq!(c.api_version, "kustomize.toolkit.fluxcd.io/v1");
        assert_eq!(c.kind, "Kustomization");
        assert_eq!(c.name, "observability-stack");
        assert_eq!(c.namespace.as_deref(), Some("flux-system"));
    }

    #[test]
    fn rendered_resource_coords_from_json_captures_absent_namespace_as_none() {
        // Cluster-scoped resource — `metadata.namespace` intentionally absent.
        let res = json!({
            "apiVersion": "v1",
            "kind": "Namespace",
            "metadata": {"name": "demo-test"},
        });
        let c = RenderedResourceCoords::from_json(&res).expect("extract");
        assert_eq!(c.namespace, None);
        assert_eq!(c.name, "demo-test");
    }

    #[test]
    fn rendered_resource_coords_from_json_errors_on_missing_api_version() {
        let res = json!({"kind": "K", "metadata": {"name": "n"}});
        let e = RenderedResourceCoords::from_json(&res).expect_err("must error");
        assert_eq!(e.to_string(), "rendered resource missing apiVersion");
    }

    #[test]
    fn rendered_resource_coords_from_json_errors_on_missing_kind() {
        let res = json!({"apiVersion": "v1", "metadata": {"name": "n"}});
        let e = RenderedResourceCoords::from_json(&res).expect_err("must error");
        assert_eq!(e.to_string(), "rendered resource missing kind");
    }

    #[test]
    fn rendered_resource_coords_from_json_errors_on_missing_metadata_name() {
        let res = json!({"apiVersion": "v1", "kind": "K", "metadata": {}});
        let e = RenderedResourceCoords::from_json(&res).expect_err("must error");
        assert_eq!(e.to_string(), "rendered resource missing metadata.name");
    }

    #[test]
    fn rendered_resource_coords_from_json_errors_on_missing_metadata_object() {
        // `metadata` absent entirely — same failure as `metadata.name` missing,
        // because the API-path leaf segment cannot be resolved.
        let res = json!({"apiVersion": "v1", "kind": "K"});
        let e = RenderedResourceCoords::from_json(&res).expect_err("must error");
        assert_eq!(e.to_string(), "rendered resource missing metadata.name");
    }

    #[test]
    fn rendered_resource_coords_from_json_errors_on_non_string_slot() {
        // A numeric `apiVersion` slot falls through the `.as_str()` gate and
        // triggers the same missing-slot failure as absence — the API-path
        // segment is not a string.
        let res = json!({
            "apiVersion": 42,
            "kind": "K",
            "metadata": {"name": "n"},
        });
        let e = RenderedResourceCoords::from_json(&res).expect_err("must error");
        assert_eq!(e.to_string(), "rendered resource missing apiVersion");
    }

    #[test]
    fn rendered_resource_coords_error_wording_is_canonical() {
        // Pins the exact spelling every downstream consumer sees.
        // Pre-lift wording differed across the two call sites (`"resource
        // missing X"` in `apply_owned` vs `"rendered resource missing X"` in
        // `flux_ref_from_json`); post-lift the canonical wording is
        // `"rendered resource missing X"` at every site.
        let cases = [
            (
                "apiVersion",
                json!({"kind": "K", "metadata": {"name": "n"}}),
            ),
            (
                "kind",
                json!({"apiVersion": "v1", "metadata": {"name": "n"}}),
            ),
            (
                "metadata.name",
                json!({"apiVersion": "v1", "kind": "K", "metadata": {}}),
            ),
        ];
        for (slot, res) in cases {
            let e = RenderedResourceCoords::from_json(&res).expect_err("must error");
            assert_eq!(
                e.to_string(),
                format!("rendered resource missing {slot}"),
                "slot {slot} error must be canonical"
            );
        }
    }

    #[test]
    fn rendered_resource_coords_namespace_or_default_returns_slice_when_some() {
        let c = RenderedResourceCoords {
            api_version: "v1".into(),
            kind: "K".into(),
            name: "n".into(),
            namespace: Some("prod".into()),
        };
        assert_eq!(c.namespace_or_default(), "prod");
    }

    #[test]
    fn rendered_resource_coords_namespace_or_default_falls_back_when_none() {
        let c = RenderedResourceCoords {
            api_version: "v1".into(),
            kind: "K".into(),
            name: "n".into(),
            namespace: None,
        };
        assert_eq!(c.namespace_or_default(), Process::DEFAULT_NAMESPACE);
        assert_eq!(c.namespace_or_default(), "default");
    }

    // ─── FluxResourceRef::fetch_coords substrate pins ─────────────
    //
    // The 4-slot `(&namespace, &api_version, &kind, &name)` borrow
    // projection lifts the pre-existing 5-slot `ssapply::fetch(client,
    // &r.namespace, &r.api_version, &r.kind, &r.name)` splat that
    // recurred at TWO hand-authored sites in
    // `tatara-reconciler::phase_machine` (`handle_running`,
    // `handle_attested`) past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    // trigger. These pins bind the slot order at fail-before-pass-
    // after granularity so a regression that swapped `namespace` and
    // `api_version` (both `String`, mechanically interchangeable to
    // a bad refactor) surfaces HERE rather than as a silent wire-time
    // 404 at every downstream Flux fetch consumer.

    fn sample_flux_ref() -> FluxResourceRef {
        // Slot values are deliberately distinct so a swap between any
        // two adjacent tuple positions surfaces as an equality
        // failure at the assertion site — a slot-inversion regression
        // cannot masquerade as identity by accident.
        FluxResourceRef {
            api_version: "kustomize.toolkit.fluxcd.io/v1".to_string(),
            kind: "Kustomization".to_string(),
            name: "observability-stack".to_string(),
            namespace: "flux-system".to_string(),
            ready: true,
            message: None,
            last_check: None,
        }
    }

    #[test]
    fn flux_resource_ref_fetch_coords_binds_slots_by_position() {
        // Positional pin: the 4-tuple return binds
        // `(namespace, api_version, kind, name)` in THAT order,
        // matching the raw `ssapply::fetch(client, ns, av, kind,
        // name)` positional signature every pre-lift callsite splatted
        // into. A regression that swapped ANY pair of adjacent slots
        // (all four axes are `String` and mechanically
        // indistinguishable at the type level) would surface here
        // rather than as an operator-visible wire-form 404 at every
        // downstream fetch consumer.
        let r = sample_flux_ref();
        let (ns, av, kind, name) = r.fetch_coords();
        assert_eq!(ns, "flux-system", "position 0 must be namespace");
        assert_eq!(
            av, "kustomize.toolkit.fluxcd.io/v1",
            "position 1 must be api_version"
        );
        assert_eq!(kind, "Kustomization", "position 2 must be kind");
        assert_eq!(name, "observability-stack", "position 3 must be name");
    }

    #[test]
    fn flux_resource_ref_fetch_coords_returns_borrows_of_owned_slots() {
        // Borrow-discipline pin: the 4-tuple returns `&str` borrows
        // of the enclosing `FluxResourceRef`'s owned `String` slots —
        // NOT a fresh allocation or a clone. A regression that
        // switched the projection to owned strings (via `.clone()` or
        // `format!`) would defeat the zero-copy contract and would
        // surface here via pointer-identity comparison.
        let r = sample_flux_ref();
        let (ns, av, kind, name) = r.fetch_coords();
        assert!(std::ptr::eq(ns.as_ptr(), r.namespace.as_ptr()));
        assert!(std::ptr::eq(av.as_ptr(), r.api_version.as_ptr()));
        assert!(std::ptr::eq(kind.as_ptr(), r.kind.as_ptr()));
        assert!(std::ptr::eq(name.as_ptr(), r.name.as_ptr()));
    }

    #[test]
    fn flux_resource_ref_fetch_coords_is_a_pure_borrow_projection() {
        // Purity pin: calling the projection twice on the same ref
        // returns byte-identical slices (same pointer, same length).
        // A regression that introduced state — a lazy-cached slot
        // computed on first call, a normalization step that ran once
        // and cached — would surface here rather than as silent drift
        // between the VERIFY-phase and ATTEST-heartbeat consumers on
        // the SAME ref within one reconcile pass.
        let r = sample_flux_ref();
        let a = r.fetch_coords();
        let b = r.fetch_coords();
        assert!(std::ptr::eq(a.0.as_ptr(), b.0.as_ptr()));
        assert!(std::ptr::eq(a.1.as_ptr(), b.1.as_ptr()));
        assert!(std::ptr::eq(a.2.as_ptr(), b.2.as_ptr()));
        assert!(std::ptr::eq(a.3.as_ptr(), b.3.as_ptr()));
    }

    #[test]
    fn flux_resource_ref_fetch_coords_ignores_status_slots() {
        // Coverage pin: the projection exposes ONLY the four API-path
        // slots the fetch call requires; the ref's status slots
        // (`ready`, `message`, `last_check`) are deliberately absent
        // from the tuple. The fetch signature admits four `&str`
        // slots, and the projection carries EXACTLY those four — no
        // silent widening that would surface as an arity mismatch at
        // every downstream `fetch(...)` call.
        let r = sample_flux_ref();
        let coords = r.fetch_coords();
        assert_eq!(
            std::mem::size_of_val(&coords),
            std::mem::size_of::<(&str, &str, &str, &str)>(),
            "the 4-tuple width must match the raw fetch signature's four `&str` slots"
        );
    }

    #[test]
    fn rendered_resource_coords_namespace_fallback_shares_process_default_const() {
        // Byte-identity between the namespace fallback and the workspace-
        // wide `Process::DEFAULT_NAMESPACE` const. A regression that spelled
        // the fallback as any other string ("kube-system", "", "default-ns")
        // would silently drift between the coord-primitive family here and
        // the `Process`-borne family in `crd.rs` — surfaces here rather than
        // as operator-observed namespace routing skew between the two
        // primitive families.
        let c = RenderedResourceCoords {
            api_version: "v1".into(),
            kind: "K".into(),
            name: "n".into(),
            namespace: None,
        };
        assert_eq!(c.namespace_or_default(), Process::DEFAULT_NAMESPACE);
    }
}
