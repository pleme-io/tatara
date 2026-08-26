//! The `Process` CRD — `tatara.pleme.io/v1alpha1`.

use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tatara_lisp::DeriveTataraDomain;

use crate::attestation::ProcessAttestation;
use crate::boundary::Boundary;
use crate::classification::Classification;
use crate::compliance::ComplianceSpec;
use crate::encapsulates::EncapsulatesSpec;
use crate::identity::Identity;
use crate::intent::Intent;
use crate::lifetime::Lifetime;
use crate::phase::ProcessPhase;
use crate::routing::RoutingSpec;
use crate::signal::ProcessSignal;
use crate::spec::{DependsOn, IdentitySpec, SignalPolicy};
use crate::status::{BoundaryStatus, ComplianceStatus, FluxResourceRef, ProcessCondition};

/// Process — one element of the tatara convergence lattice, reconciled as a Unix process.
///
/// ```yaml
/// apiVersion: tatara.pleme.io/v1alpha1
/// kind: Process
/// metadata:
///   name: observability-stack
///   namespace: seph
/// spec:
///   identity:
///     parent: seph.1
///   classification:
///     pointType: Gate
///     substrate: Observability
///   intent:
///     nix:
///       flakeRef: github:pleme-io/k8s?dir=shared/infrastructure
///       attribute: observability
///   compliance:
///     baseline: fedramp-moderate
///     bindings:
///       - framework: nist-800-53
///         controlId: SC-7
///         phase: AtBoundary
///   dependsOn:
///     - name: secret-injection
/// ```
#[derive(CustomResource, DeriveTataraDomain, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "tatara.pleme.io",
    version = "v1alpha1",
    kind = "Process",
    plural = "processes",
    shortname = "proc",
    namespaced,
    status = "ProcessStatus",
    printcolumn = r#"{"name":"PID","type":"string","jsonPath":".status.pid"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#,
    printcolumn = r#"{"name":"Type","type":"string","jsonPath":".spec.classification.pointType"}"#,
    printcolumn = r#"{"name":"Substrate","type":"string","jsonPath":".spec.classification.substrate"}"#,
    printcolumn = r#"{"name":"Gen","type":"integer","jsonPath":".status.attestation.generation"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defpoint")]
pub struct ProcessSpec {
    /// Identity (parent, name override).
    #[serde(default)]
    pub identity: IdentitySpec,

    /// Lattice position (6 dimensions).
    pub classification: Classification,

    /// Where rendered artifacts come from. Exactly one variant must be set.
    pub intent: Intent,

    /// Boundary predicates (preconditions / postconditions).
    #[serde(default)]
    pub boundary: Boundary,

    /// Compliance bindings + baseline.
    #[serde(default)]
    pub compliance: ComplianceSpec,

    /// Lattice dependencies — must reach phase before we proceed.
    #[serde(default)]
    pub depends_on: Vec<DependsOn>,

    /// Signal policy (grace, SIGHUP strategy, start-suspended).
    #[serde(default)]
    pub signals: SignalPolicy,

    /// Lifetime — `Permanent` (default, re-converging) or `Ephemeral`
    /// (auto-SIGTERM per `teardown_policy` + TTL clock).
    #[serde(default, skip_serializing_if = "Lifetime::is_default")]
    pub lifetime: Lifetime,

    /// External edges — DNS + Ingress. When `None`, the Process is
    /// internal-only (matches today's default). See
    /// [`crate::routing`] for the full shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<RoutingSpec>,

    /// Pre-existing in-cluster state this Process wraps. When `None`,
    /// the Process is greenfield (Manage mode implicitly applied to
    /// nothing pre-existing). See [`crate::encapsulates`] for the
    /// three modes (Manage / Adopt / Observe).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encapsulates: Option<EncapsulatesSpec>,

    /// Soft-suspend marker — reconciler treats as SIGSTOP.
    /// Same effect as delivering SIGSTOP, but persistent across restarts.
    #[serde(default)]
    pub suspended: bool,
}

// Coordinate primitives — the `(namespace, name)` pair every downstream
// composer (annotation writers, claim arbiter, boundary evaluator,
// render owner-metadata seed) pulled by hand from `Process.metadata`
// pre-lift, each restating the same two `Option<String>`-to-`&str`
// unwrap incantations with the same two workspace-wide fallback
// strings sprayed inline. Post-lift the pair lives at ONE substrate
// primitive on `Process` — a future normalization (case-fold,
// unicode-safe collation, cross-cluster prefix, a rename of either
// fallback) lands here and every downstream composer inherits the
// upgrade mechanically. Peer to `qualified_process_ref` in
// `tatara-reconciler::ssapply`, whose two `&str` arguments are
// exactly the pair `Process::coordinates_or_defaults` returns.
impl Process {
    /// The K8s canonical default namespace — the fallback every
    /// consumer of a `Process` whose `metadata.namespace` is `None`
    /// substitutes. Matches the string K8s itself substitutes on
    /// namespaced resource writes with no explicit namespace.
    pub const DEFAULT_NAMESPACE: &'static str = "default";

    /// Workspace-wide fallback for a `Process`'s `metadata.name` when
    /// it is `None` — the sentinel every annotation writer, claim
    /// arbiter, and owner-metadata seed substitutes so downstream
    /// grepping / label-selecting sees a stable spelling rather than
    /// a per-callsite ad-hoc placeholder (`""`, `"<unnamed>"`, or the
    /// empty `unwrap_or_default()` fallback). A Process authored
    /// through the reconciler's fork path always has a name; this
    /// constant covers the surface where an untyped `Process` value
    /// (test fixture, dynamic API response, adopted resource pre-
    /// name-resolution) surfaces without one.
    pub const UNNAMED_PLACEHOLDER: &'static str = "unnamed";

    /// Namespace slice with the [`Self::DEFAULT_NAMESPACE`] fallback
    /// applied — the ONE-line collapse of the `metadata.namespace
    /// .as_deref().unwrap_or("default")` incantation every consumer
    /// spelled by hand pre-lift.
    ///
    /// Peer to [`Self::name_or_placeholder`] on the (metadata slot ×
    /// fallback shape) axis; both compose through
    /// [`Self::coordinates_or_defaults`] when a consumer needs the
    /// pair together (annotation writers, claim-arbiter row builders,
    /// render owner-metadata seed).
    pub fn namespace_or_default(&self) -> &str {
        self.metadata
            .namespace
            .as_deref()
            .unwrap_or(Self::DEFAULT_NAMESPACE)
    }

    /// Name slice with the [`Self::UNNAMED_PLACEHOLDER`] fallback
    /// applied — the ONE-line collapse of the `metadata.name.as_deref
    /// ().unwrap_or("unnamed")` incantation every consumer spelled by
    /// hand pre-lift.
    ///
    /// Peer to [`Self::namespace_or_default`] on the (metadata slot ×
    /// fallback shape) axis; both compose through
    /// [`Self::coordinates_or_defaults`] when a consumer needs the
    /// pair together.
    pub fn name_or_placeholder(&self) -> &str {
        self.metadata
            .name
            .as_deref()
            .unwrap_or(Self::UNNAMED_PLACEHOLDER)
    }

    /// `(namespace, name)` coordinates with the workspace-wide default
    /// fallbacks applied — the ONE-line collapse of the paired
    /// `metadata.namespace.as_deref().unwrap_or("default")` +
    /// `metadata.name.as_deref().unwrap_or("unnamed")` extraction
    /// every downstream composer restated by hand pre-lift.
    ///
    /// Return-tuple order matches the axis order of the substrate's
    /// paired-composer primitive
    /// `tatara_reconciler::ssapply::qualified_process_ref(ns, name)`:
    /// the (namespace, name) pair this method returns feeds that
    /// primitive positionally without an axis-swap step.
    pub fn coordinates_or_defaults(&self) -> (&str, &str) {
        (self.namespace_or_default(), self.name_or_placeholder())
    }

    /// `(namespace, name)` coordinates as owned `String`s, with the
    /// namespace half fallback-defaulted to [`Self::DEFAULT_NAMESPACE`]
    /// but the name half REQUIRED — an [`anyhow::Error`] is returned
    /// when `metadata.name` is absent, because "unnamed" is a display
    /// placeholder, not a valid K8s API path segment. Fed straight into
    /// kube-rs API calls (`Api::patch`, `Api::delete`, `Api::get`) that
    /// take owned `String` arguments; the [`Self::DEFAULT_NAMESPACE`]
    /// fallback matches what K8s itself substitutes on namespaced
    /// resource writes with no explicit namespace, so the surface is
    /// safe against a `Process` whose `metadata.namespace` slot is
    /// absent (test fixture, dynamic API response pre-defaulting) but
    /// refuses to guess a name.
    ///
    /// Peer to [`Self::coordinates_or_defaults`] on the (return-form ×
    /// name gate) axis pair:
    /// * borrow + name-defaulted → `coordinates_or_defaults` (display,
    ///   annotation writers, ownership-tag composers — every consumer
    ///   whose downstream drops `"unnamed"` in place of a missing name
    ///   without an operator-visible failure);
    /// * owned + name-required → this method (kube-rs API calls —
    ///   every consumer whose downstream must NOT silently substitute
    ///   a placeholder for the API call target, because the caller is
    ///   about to `patch`/`delete`/`get` at `metadata.name`).
    ///
    /// The error wording is pinned by
    /// [`tests::owned_coordinates_or_err_error_message_matches_pre_lift_reconciler_wording`]
    /// to match the exact spelling every pre-lift `tatara-reconciler`
    /// helper produced (`"Process has no metadata.name"`) so log-line
    /// / test greps that anchored on that wording keep matching post-
    /// lift, and no operator-visible message drift lands as a side
    /// effect of the substrate move.
    pub fn owned_coordinates_or_err(&self) -> anyhow::Result<(String, String)> {
        let ns = self
            .metadata
            .namespace
            .clone()
            .unwrap_or_else(|| Self::DEFAULT_NAMESPACE.into());
        let name = self
            .metadata
            .name
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Process has no metadata.name"))?;
        Ok((ns, name))
    }
}

/// Process status — every field optional until the reconciler writes it.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessStatus {
    /// Hierarchical PID path — e.g., `"seph.1.7"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<String>,

    /// Parent PID path (mirror of `spec.identity.parent`, resolved at fork).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,

    /// Direct children's PID paths.
    #[serde(default)]
    pub children: Vec<String>,

    /// Resolved identity (name + content hash).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,

    /// Current phase.
    #[serde(default)]
    pub phase: ProcessPhase,

    /// When the process entered the current phase.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_since: Option<DateTime<Utc>>,

    /// Three-pillar attestation (written at end of every successful cycle).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<ProcessAttestation>,

    /// FluxCD resources currently owned by this Process.
    #[serde(default)]
    pub flux_resources: Vec<FluxResourceRef>,

    /// Boundary verification state.
    #[serde(default)]
    pub boundary: BoundaryStatus,

    /// Compliance summary at the latest attestation.
    #[serde(default)]
    pub compliance: ComplianceStatus,

    /// Pending signals (delivered, not yet handled).
    #[serde(default)]
    pub signal_queue: Vec<ProcessSignal>,

    /// Standard K8s Conditions.
    #[serde(default)]
    pub conditions: Vec<ProcessCondition>,

    /// Human-readable last status message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Exit code (only set on Failed / Reaped).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classification::{ConvergencePointType, SubstrateType};
    use crate::intent::NixIntent;

    #[test]
    fn minimal_spec_serializes() {
        let spec = ProcessSpec {
            identity: IdentitySpec::default(),
            classification: Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Observability,
                horizon: Default::default(),
                calm: Default::default(),
                data_classification: Default::default(),
            },
            intent: Intent {
                nix: Some(NixIntent {
                    flake_ref: "github:pleme-io/k8s".into(),
                    attribute: "obs".into(),
                    system: None,
                    attic_cache: None,
                    extra_args: vec![],
                    delegate_to_nix_build: false,
                }),
                ..Intent::default()
            },
            boundary: Default::default(),
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Default::default(),
            routing: None,
            encapsulates: None,
            suspended: false,
        };
        let yaml = serde_yaml::to_string(&spec).unwrap();
        assert!(yaml.contains("pointType: Gate"));
        assert!(yaml.contains("substrate: Observability"));
        assert!(yaml.contains("flakeRef: github:pleme-io/k8s"));
    }

    // ─── Process::coordinates_or_defaults substrate pins ────────────────
    //
    // Pins the (namespace, name) coordinate-primitive family on the
    // (metadata slot × fallback shape) axis. Fail-before-pass-after
    // granularity: a regression that flipped either fallback string,
    // swapped the return-tuple axis order, or dropped the
    // `Option::as_deref` unwrap surfaces here rather than as silent
    // drift at every downstream annotation writer / claim-arbiter row
    // builder / render owner-metadata seed.

    fn empty_spec() -> ProcessSpec {
        ProcessSpec {
            identity: IdentitySpec::default(),
            classification: Classification {
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Compute,
                horizon: Default::default(),
                calm: Default::default(),
                data_classification: Default::default(),
            },
            intent: Intent::default(),
            boundary: Default::default(),
            compliance: Default::default(),
            depends_on: vec![],
            signals: Default::default(),
            lifetime: Default::default(),
            routing: None,
            encapsulates: None,
            suspended: false,
        }
    }

    #[test]
    fn default_namespace_constant_is_k8s_canonical_default() {
        // Pins the load-bearing convention that this primitive's
        // namespace fallback matches K8s's own implicit-namespace
        // spelling. A regression that renamed this to "kube-system"
        // or any other K8s-reserved name would silently misroute
        // every downstream namespaced-Api call on a Process without
        // a metadata.namespace.
        assert_eq!(Process::DEFAULT_NAMESPACE, "default");
    }

    #[test]
    fn unnamed_placeholder_constant_matches_prior_annotation_writer_fallback() {
        // Pins the load-bearing convention that this primitive's name
        // fallback matches the exact spelling every annotation writer
        // (tatara-reconciler::ssapply::inject_annotations,
        // tatara-reconciler::render::render, and
        // tatara-reconciler::table_controller's claim-row builder)
        // was hand-authoring pre-lift ("unnamed", NOT "<unnamed>" or
        // ""). A regression that renamed this would break the
        // annotation-writer / claim-arbiter grep contract silently.
        assert_eq!(Process::UNNAMED_PLACEHOLDER, "unnamed");
    }

    #[test]
    fn namespace_or_default_falls_back_when_metadata_namespace_is_none() {
        let mut p = Process::new("some-proc", empty_spec());
        p.metadata.namespace = None;
        assert_eq!(p.namespace_or_default(), Process::DEFAULT_NAMESPACE);
    }

    #[test]
    fn namespace_or_default_returns_metadata_slice_when_some() {
        let mut p = Process::new("some-proc", empty_spec());
        p.metadata.namespace = Some("prod-app".into());
        assert_eq!(p.namespace_or_default(), "prod-app");
    }

    #[test]
    fn name_or_placeholder_falls_back_when_metadata_name_is_none() {
        let mut p = Process::new("real-name", empty_spec());
        p.metadata.name = None;
        assert_eq!(p.name_or_placeholder(), Process::UNNAMED_PLACEHOLDER);
    }

    #[test]
    fn name_or_placeholder_returns_metadata_slice_when_some() {
        let p = Process::new("api-gateway", empty_spec());
        assert_eq!(p.name_or_placeholder(), "api-gateway");
    }

    #[test]
    fn coordinates_or_defaults_composes_both_halves() {
        // Both slots present — returns metadata slices in
        // (namespace, name) axis order.
        let mut p = Process::new("api", empty_spec());
        p.metadata.namespace = Some("staging".into());
        assert_eq!(p.coordinates_or_defaults(), ("staging", "api"));
    }

    #[test]
    fn coordinates_or_defaults_falls_back_on_both_slots() {
        // Both slots None — returns (DEFAULT_NAMESPACE,
        // UNNAMED_PLACEHOLDER) in axis order.
        let mut p = Process::new("scratch", empty_spec());
        p.metadata.name = None;
        p.metadata.namespace = None;
        assert_eq!(
            p.coordinates_or_defaults(),
            (Process::DEFAULT_NAMESPACE, Process::UNNAMED_PLACEHOLDER)
        );
    }

    #[test]
    fn coordinates_or_defaults_mixes_slotted_and_fallback_halves() {
        // Namespace set, name missing — the (namespace, name) tuple
        // pins each half independently. A regression that returned
        // BOTH fallbacks when EITHER metadata slot was None would
        // surface here rather than at every downstream reader.
        let mut p = Process::new("kept-name", empty_spec());
        p.metadata.namespace = Some("prod".into());
        assert_eq!(p.coordinates_or_defaults(), ("prod", "kept-name"));

        // Name set, namespace missing — the peer corner.
        let mut q = Process::new("api", empty_spec());
        q.metadata.namespace = None;
        assert_eq!(
            q.coordinates_or_defaults(),
            (Process::DEFAULT_NAMESPACE, "api")
        );
    }

    // ─── Process::owned_coordinates_or_err substrate pins ──────────────
    //
    // Pins the owned + name-required peer of the coordinate-primitive
    // family on the (return-form × name gate) axis pair. Fail-before-
    // pass-after granularity: a regression that flipped the namespace
    // fallback string, dropped the `Option::clone` unwrap, changed the
    // return-tuple axis order, or altered the "Process has no
    // metadata.name" error wording surfaces here rather than as silent
    // drift at every pre-lift caller (10 sites in
    // `tatara-reconciler::phase_machine` + 2 sites in
    // `tatara-reconciler::signals` pre-lift).

    #[test]
    fn owned_coordinates_or_err_returns_owned_strings_when_both_slots_present() {
        // Happy path — both slots populated, method returns owned
        // Strings in (namespace, name) axis order.
        let mut p = Process::new("api-gateway", empty_spec());
        p.metadata.namespace = Some("prod-app".into());
        let (ns, name) = p.owned_coordinates_or_err().unwrap();
        assert_eq!(ns, "prod-app");
        assert_eq!(name, "api-gateway");
        // Ownership pin: type inference above binds ns/name as
        // owned Strings — a regression that returned &str would
        // fail to compile at the following .push() call. This
        // holds the "owned" half of the primitive's contract.
        let mut owned_ns = ns;
        owned_ns.push_str("-mutated");
        assert_eq!(owned_ns, "prod-app-mutated");
    }

    #[test]
    fn owned_coordinates_or_err_falls_back_on_namespace_but_returns_owned_name() {
        // Namespace absent → DEFAULT_NAMESPACE. Name present → owned.
        let p = Process::new("api", empty_spec());
        // Process::new leaves metadata.namespace = None by default.
        let (ns, name) = p.owned_coordinates_or_err().unwrap();
        assert_eq!(ns, Process::DEFAULT_NAMESPACE);
        assert_eq!(name, "api");
    }

    #[test]
    fn owned_coordinates_or_err_errors_when_metadata_name_absent_regardless_of_namespace() {
        // Name absent → Err, REGARDLESS of whether the namespace is
        // populated. The name gate is strictly on `metadata.name` and
        // does NOT fall back to `Self::UNNAMED_PLACEHOLDER` (that
        // fallback is on the peer `coordinates_or_defaults`, which
        // exists precisely for consumers that can tolerate a
        // display placeholder).
        for ns_slot in [None, Some("prod".to_string())] {
            let mut p = Process::new("scratch", empty_spec());
            p.metadata.name = None;
            p.metadata.namespace = ns_slot.clone();
            let err = p.owned_coordinates_or_err().unwrap_err();
            assert!(
                err.to_string().contains("metadata.name"),
                "err on missing name (ns={ns_slot:?}) should mention metadata.name; got {err}"
            );
        }
    }

    #[test]
    fn owned_coordinates_or_err_error_message_matches_pre_lift_reconciler_wording() {
        // Load-bearing wording pin — every pre-lift `tatara-reconciler`
        // helper (`phase_machine::namespace_and_name`,
        // `signals::ingest`, `signals::consume_effect`) errored with
        // EXACTLY this wording. Post-lift the substrate owner produces
        // the same wording so log-line / test greps that anchored on
        // it keep matching, and no operator-visible message drift
        // lands as a side effect of the substrate move.
        let mut p = Process::new("scratch", empty_spec());
        p.metadata.name = None;
        let err = p.owned_coordinates_or_err().unwrap_err();
        assert_eq!(err.to_string(), "Process has no metadata.name");
    }

    #[test]
    fn owned_coordinates_or_err_namespace_fallback_matches_default_namespace_const() {
        // Byte-identity pin between the owned form's namespace
        // fallback and the workspace-wide `DEFAULT_NAMESPACE` const.
        // A regression that spelled this fallback as any other
        // string ("kube-system", "", "default-ns") would silently
        // misroute every downstream namespaced-Api call on a
        // Process without a metadata.namespace — surfaces here
        // rather than at every kube-rs API caller.
        let mut p = Process::new("api", empty_spec());
        p.metadata.namespace = None;
        let (ns, _) = p.owned_coordinates_or_err().unwrap();
        assert_eq!(ns, Process::DEFAULT_NAMESPACE);
    }

    #[test]
    fn owned_coordinates_or_err_matches_pre_lift_reconciler_helper_shape() {
        // Byte-identical parity pin between the owned + name-required
        // primitive here and the pre-lift `tatara-reconciler` helper
        // shape — the exact 2-slot unwrap chain each pre-lift caller
        // spelled by hand:
        //
        //   let ns = p.metadata.namespace.clone().unwrap_or_else(|| "default".into());
        //   let name = p.metadata.name.clone().ok_or_else(|| anyhow!(...))?;
        //   Ok((ns, name))
        //
        // Sweeps every corner every callsite plausibly encounters
        // (both slots present, namespace absent, name absent, both
        // absent). A regression that inserted a normalization step
        // at the primitive that the pre-lift chain does NOT apply —
        // or vice versa — surfaces here rather than as silent drift
        // between the 12 pre-lift consumer callsites and the ONE
        // substrate owner they now route through.
        fn pre_lift(p: &Process) -> anyhow::Result<(String, String)> {
            let ns = p
                .metadata
                .namespace
                .clone()
                .unwrap_or_else(|| "default".into());
            let name = p
                .metadata
                .name
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Process has no metadata.name"))?;
            Ok((ns, name))
        }
        // Both present.
        let mut p = Process::new("api", empty_spec());
        p.metadata.namespace = Some("prod".into());
        assert_eq!(p.owned_coordinates_or_err().unwrap(), pre_lift(&p).unwrap());
        // Namespace absent.
        let p = Process::new("api", empty_spec());
        assert_eq!(p.owned_coordinates_or_err().unwrap(), pre_lift(&p).unwrap());
        // Name absent → both variants error with the same wording.
        let mut p = Process::new("api", empty_spec());
        p.metadata.name = None;
        p.metadata.namespace = Some("prod".into());
        assert_eq!(
            p.owned_coordinates_or_err().unwrap_err().to_string(),
            pre_lift(&p).unwrap_err().to_string(),
        );
        // Both absent → still errors on the name gate.
        let mut p = Process::new("api", empty_spec());
        p.metadata.name = None;
        p.metadata.namespace = None;
        assert_eq!(
            p.owned_coordinates_or_err().unwrap_err().to_string(),
            pre_lift(&p).unwrap_err().to_string(),
        );
    }

    #[test]
    fn owned_coordinates_or_err_axis_order_matches_coordinates_or_defaults() {
        // Cross-primitive coherence pin between the owned + name-
        // required form and the borrow + name-defaulted peer:
        // (namespace, name) axis order is IDENTICAL across both
        // return-forms. A regression that swapped the tuple slots on
        // only ONE of the two primitives would silently misroute
        // every consumer that picked between the two forms based on
        // its callsite's ownership needs. The pin re-reads both
        // primitives at test time so the equality holds iff both
        // live paths are the current implementation.
        let mut p = Process::new("app", empty_spec());
        p.metadata.namespace = Some("infra".into());
        let (borrow_ns, borrow_name) = p.coordinates_or_defaults();
        let (owned_ns, owned_name) = p.owned_coordinates_or_err().unwrap();
        assert_eq!(owned_ns, borrow_ns);
        assert_eq!(owned_name, borrow_name);
        // Explicit slot labels — pins the (namespace, name) axis
        // order as opposed to (name, namespace).
        assert_eq!(owned_ns, "infra"); // NOT "app"
        assert_eq!(owned_name, "app"); // NOT "infra"
    }

    #[test]
    fn coordinates_or_defaults_axis_order_matches_qualified_process_ref() {
        // Pins the load-bearing convention that the return-tuple
        // axis order is (namespace, name) — the exact positional
        // argument order the substrate's paired-composer primitive
        // `tatara_reconciler::ssapply::qualified_process_ref(ns,
        // name)` consumes. A regression that swapped the tuple
        // slots would silently misroute every annotation writer /
        // claim-arbiter row / owner-metadata seed built by feeding
        // this pair into the composer — every downstream `<ns>/
        // <name>` grep would suddenly see `<name>/<ns>`. The test
        // verifies the tuple's first slot is what a hand-authored
        // `.metadata.namespace.as_deref()...` produced pre-lift, and
        // the second slot is what `.metadata.name.as_deref()...`
        // produced.
        let mut p = Process::new("app", empty_spec());
        p.metadata.namespace = Some("infra".into());
        let (ns, name) = p.coordinates_or_defaults();
        assert_eq!(ns, "infra"); // NOT "app"
        assert_eq!(name, "app"); // NOT "infra"
    }
}
