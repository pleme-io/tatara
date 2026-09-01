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

    /// `(namespace, name)` coordinates in the BORROW + NAME-REQUIRED
    /// corner of the primitive family — namespace half falls back to
    /// [`Self::DEFAULT_NAMESPACE`], but the name half is REQUIRED
    /// (`None` on a `Process` whose `metadata.name` is absent, so the
    /// caller stops with an `else { continue; }` / `else { return
    /// …; }` guard rather than proceeding with the empty-string
    /// sentinel every pre-lift consumer had to spell inline).
    ///
    /// Peer to [`Self::coordinates_or_defaults`] +
    /// [`Self::owned_coordinates_or_err`] on the (return-form ×
    /// name-gate) axis pair — closes the corner the family previously
    /// left open:
    ///
    /// * borrow + name-defaulted → [`Self::coordinates_or_defaults`]
    ///   (annotation writers, render owner-metadata seed — consumers
    ///   whose downstream tolerates the `"unnamed"` display placeholder
    ///   without operator-visible failure);
    /// * borrow + name-required → **this method** (claim-arbiter
    ///   probes, child-Process delete-fan-out — consumers that need a
    ///   real API-path leaf and cleanly SKIP the row when the name is
    ///   absent rather than issuing a K8s call with an empty-string
    ///   name argument);
    /// * owned + name-required → [`Self::owned_coordinates_or_err`]
    ///   (kube-rs API-path calls — consumers whose downstream requires
    ///   owned `String` arguments and rejects the missing-name corner
    ///   with a load-bearing error message).
    ///
    /// The primitive family's `None`-on-missing-name semantics
    /// intentionally differs from [`Self::owned_coordinates_or_err`]'s
    /// error-on-missing-name semantics: the caller sites for this form
    /// (child-Process fan-out, claim-arbiter row probes) are non-fatal
    /// SKIPS rather than reportable failures — an `Option::None` at
    /// the primitive lets the caller thread that "skip" through a
    /// let-else without stringifying / logging an anyhow chain per
    /// missing-name occurrence.
    ///
    /// The namespace fallback matches [`Self::coordinates_or_defaults`]
    /// (via [`Self::namespace_or_default`]), so a consumer that
    /// switches between the two borrow-form primitives based on its
    /// name-gate need never sees a different namespace-fallback string
    /// as a side effect.
    pub fn coordinates_or_none(&self) -> Option<(&str, &str)> {
        let name = self.metadata.name.as_deref()?;
        Some((self.namespace_or_default(), name))
    }

    /// Canonical `<ns>/<name>` **namespace-qualified process reference**
    /// composed straight off the live [`Process`] — the ONE-liner
    /// collapse of the paired
    /// `let (ns, name) = process.coordinates_or_defaults(); let r =
    /// qualified_process_ref(ns, name);` incantation every consumer
    /// whose downstream keys a Process by "which cluster location owns
    /// it" hand-authored at scattered sites across `tatara-reconciler`.
    ///
    /// Pre-lift the 2-step `coordinates_or_defaults() →
    /// qualified_process_ref(ns, name)` composition was hand-authored
    /// at THREE sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// threshold in `tatara-reconciler`, each restating the SAME
    /// paired projection + `<ns>/<name>` shape:
    /// * `render::render_routing` — routing-graph `PROCESS=<ref>`
    ///   annotation seed on every emitted Ingress / DNSEndpoint,
    ///   feeding [`crate::status::FluxResourceRef`] downstream.
    /// * `render::render_export_jobs` — export-Job `PROCESS=<ref>`
    ///   annotation seed on every emitted export `batch/v1` Job.
    /// * `table_controller::reconcile` — claim-arbiter row-key +
    ///   `Candidate.process_ref` seed on the stable-name claim
    ///   registry (the reference lands verbatim in
    ///   [`crate::table::ClaimRecord.holder`], where every downstream
    ///   claim query greps it).
    ///
    /// All THREE sites walked the SAME 2-step chain — pull the
    /// `(ns, name)` pair through [`Self::coordinates_or_defaults`],
    /// then feed the pair positionally into
    /// [`crate::qualified_process_ref`]. Post-lift each caller reads
    /// `process.qualified_ref()` — the paired projection + shape
    /// composer now sit at ONE substrate owner, so a rename of either
    /// workspace-wide fallback (`"default"` / `"unnamed"`), a swap of
    /// the `<ns>/<name>` separator, a normalization pass inserted
    /// between the paired projection and the shape composer, or a
    /// future `<ns>/<name>@<gen>` / `<cluster>/<ns>/<name>` cross-
    /// cluster extension lands here exactly once and every consumer
    /// (annotation seed, claim-row key, holder-slot writer, export-
    /// Job seed, `Candidate` composer) inherits the upgrade
    /// mechanically.
    ///
    /// Peer to [`Self::coordinates_or_defaults`] on the (return-form ×
    /// composition-depth) axis pair:
    /// * pair + defaulted → [`Self::coordinates_or_defaults`]
    ///   (consumers that thread each half into a separate positional
    ///   slot — `Api::namespaced(client, &ns) + Api::patch(&name, …)`,
    ///   `one_export_job(ns, name, …)`, `EdgeContext { process_name,
    ///   process_namespace, … }`);
    /// * shape + defaulted → **this method** (consumers that key on
    ///   the composed `<ns>/<name>` reference directly — the
    ///   `PROCESS=<ref>` annotation seed, the `ClaimRecord.holder`
    ///   slot, the label-selector composer).
    ///
    /// The namespace-fallback discipline matches
    /// [`Self::coordinates_or_defaults`] (via
    /// [`Self::namespace_or_default`]) and the name-fallback discipline
    /// matches [`Self::name_or_placeholder`], so a consumer that
    /// switches between the pair-returning primitive and this shape-
    /// composing primitive never sees a different fallback string as
    /// a side effect. The composed reference is byte-identical to the
    /// pre-lift hand-authored `format!("{ns}/{name}")` with `ns` /
    /// `name` supplied by the pair-returning primitive, so downstream
    /// greps keyed on the reference shape (`PROCESS=<ref>` on emitted
    /// resources, `holder = <ref>` on claim-registry queries) match
    /// bytewise post-lift.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the 2-step paired-projection + shape-composer chain recurred at
    /// three hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted onto ONE workspace-wide
    /// owner here). THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — a regression that inserted a normalization step at
    /// only two of three sites, or that drifted the fallback strings
    /// between the paired projection and the shape composer, surfaces
    /// at [`tests::qualified_ref_*`] rather than as silent operator-
    /// visible skew across the three annotation / claim-key /
    /// export-Job seed writers).
    #[must_use]
    pub fn qualified_ref(&self) -> String {
        let (ns, name) = self.coordinates_or_defaults();
        crate::qualified_process_ref(ns, name)
    }

    /// Borrowed lookup of ONE key in `metadata.annotations`, with
    /// BOTH the missing-`annotations` corner AND the missing-key
    /// corner collapsed to `None` — the ONE-liner collapse of the
    /// paired `self.metadata.annotations.as_ref().and_then(|m|
    /// m.get(key)).map(String::as_str)` incantation every consumer
    /// restated by hand pre-lift.
    ///
    /// Pre-lift the 3-line `.metadata.annotations.as_ref().and_then
    /// (|m| m.get(KEY))` chain (in three tail variants — `.cloned()`,
    /// `.cloned().unwrap_or_default()`, `.map(String::as_str)`) was
    /// hand-authored at THREE sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold across the workspace:
    /// * `tatara-reconciler::signals::ingest` — SIGNAL annotation
    ///   lookup (pre-lift `.cloned()` for owned parsing).
    /// * `tatara-reconciler::phase_machine::released_from_annotation`
    ///   — RELEASED_FROM annotation lookup (pre-lift `.cloned()
    ///   .unwrap_or_default()` for `match v.as_str()`).
    /// * `tatara-pool-reconciler::controller_pool::process_belongs_to_pool`
    ///   — POOL annotation lookup (pre-lift `.map(String::as_str)`
    ///   for `== Some(pool_name)`).
    ///
    /// All THREE sites walked the SAME 3-line chain — read the
    /// annotations map, gate on presence, index by key — differing
    /// only in the tail that shaped the result. Post-lift each
    /// caller routes through the ONE substrate primitive here and
    /// applies its own tail at its own site (`.map(str::to_string)`
    /// / bare match / `==`).
    ///
    /// Return-form axis: `Option<&str>` mirrors the existing borrow-
    /// first discipline of the peer metadata primitives
    /// [`Self::namespace_or_default`], [`Self::name_or_placeholder`],
    /// [`Self::coordinates_or_none`]. The two corners the chain
    /// swallowed pre-lift (missing `metadata.annotations` map,
    /// missing key inside the map) BOTH collapse to `None` so
    /// `.is_some()` / `if let Some(_)` / `Option::map` behave
    /// identically on a `Process` whose annotations block is `None`
    /// and on one whose annotations block is populated but omits the
    /// key — matching what the pre-lift `.and_then(...)` chain
    /// produced.
    ///
    /// A future normalization step (a key-canonicalization pass,
    /// a case-fold lookup, a per-key alias table for renamed
    /// annotations across API versions, a per-namespace override
    /// substrate) lands at ONE substrate method here and all three
    /// downstream consumers pick up the upgrade mechanically — no
    /// per-callsite hand-edit at `ingest` / `released_from_annotation`
    /// / `process_belongs_to_pool`.
    ///
    /// Sibling to the peer metadata primitives
    /// ([`Self::namespace_or_default`], [`Self::name_or_placeholder`],
    /// [`Self::coordinates_or_defaults`], [`Self::coordinates_or_none`],
    /// [`Self::owned_coordinates_or_err`]) on the metadata axis;
    /// this method opens the borrow-form peer on the ANNOTATION
    /// axis. Future annotation projections (a paired
    /// `label(&str) -> Option<&str>` on `metadata.labels`, a
    /// `has_annotation(&str) -> bool` boolean gate for presence-
    /// only consumers) land as peer methods on this same axis.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition
    /// — the 3-line annotation-lookup chain recurred at three
    /// hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the missing-`annotations` corner + the
    /// missing-key corner + the borrow-form `&str` lifetime + the
    /// byte-identical parity with the pre-lift 3-line chain, so a
    /// regression that drifted any surface at
    /// `tests::annotation_*` rather than as silent operator-facing
    /// skew between the SIGNAL / RELEASED_FROM / POOL annotation
    /// readers).
    pub fn annotation(&self, key: &str) -> Option<&str> {
        self.metadata
            .annotations
            .as_ref()
            .and_then(|m| m.get(key))
            .map(String::as_str)
    }

    /// Borrow-form metadata-projection primitive on the `metadata.uid`
    /// axis: returns the K8s-API-server-assigned uid as a `&str`, with
    /// the missing-uid corner collapsed to the load-bearing empty-string
    /// sentinel — the ONE-liner collapse of the paired
    /// `self.metadata.uid.as_deref().unwrap_or("")` incantation every
    /// owner-reference-emitting consumer restated by hand pre-lift.
    ///
    /// The empty-string fallback is NOT arbitrary — it is the exact
    /// sentinel value the sibling substrate composer
    /// [`crate::owner_references_json`] gates on (`if uid.is_empty()
    /// { vec![] } else { vec![owner_reference_json(name, uid)] }`) to
    /// stamp `metadata.ownerReferences: []` on a resource whose owning
    /// Process pre-dates the API server's `metadata.uid` assignment
    /// (test fixture, mid-Forking snapshot before the first `patch`
    /// round-trip, dynamic API response pre-uid-resolution). Pre-lift
    /// each consumer spelled the fallback as `.unwrap_or("")` at its
    /// callsite; the two literals in two files could drift silently to
    /// `.unwrap_or_default()`, `.unwrap_or("<unknown>")`, or an
    /// `if let Some(u) = &process.metadata.uid` gate that returned a
    /// different owner-refs shape for the missing-uid corner. Post-lift
    /// the sentinel value is composed at ONE substrate site so the
    /// empty-uid gate at `owner_references_json` and its per-callsite
    /// producers share the SAME `""` byte-string, and a rename of the
    /// sentinel would land at ONE substrate site rather than at every
    /// downstream `owner_references_json(name, uid)` call.
    ///
    /// Peer to [`Self::namespace_or_default`] +
    /// [`Self::name_or_placeholder`] on the metadata-slot × fallback-
    /// shape axis: `namespace_or_default` returns the K8s-canonical
    /// `"default"` fallback (matching what the API server substitutes
    /// on namespaced writes with no explicit namespace);
    /// `name_or_placeholder` returns the workspace-wide `"unnamed"`
    /// sentinel (a display placeholder for downstream grepping /
    /// label-selecting); this method returns the empty-string sentinel
    /// (a load-bearing gate value that composes with
    /// [`crate::owner_references_json`]'s `is_empty` check). The three
    /// primitives partition the metadata-slot family by whether the
    /// consumer wants a K8s-canonical fallback (namespace), a display
    /// placeholder (name), or a gate sentinel (uid).
    ///
    /// Pre-lift the `.metadata.uid.as_deref().unwrap_or("")` chain was
    /// hand-authored at TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold in `tatara-reconciler::render`, both
    /// feeding a downstream owner-reference emitter:
    /// * `render_routing` — the routing-edge seed that binds
    ///   `process_uid` into every routing-form `EdgeContext` (Ingress +
    ///   DNSEndpoint) built inside the fanout loop over
    ///   `RoutingSpec::hostnames`; each `Edge::render` impl then walks
    ///   its `EdgeContext` through `build_owner_refs` →
    ///   [`crate::owner_references_json`] to stamp
    ///   `metadata.ownerReferences` on the emitted resource.
    /// * `render_export_jobs` — the ephemeral-export Job builder that
    ///   passes the same uid slice to `tatara_process::
    ///   owner_references_json(name, uid)` per rendered Job, stamping
    ///   the export-Job's `metadata.ownerReferences` back at the
    ///   owning Process.
    ///
    /// Both sites walked the SAME `.as_deref().unwrap_or("")` chain and
    /// both wanted the `&str` form the primitive returns — as the
    /// second positional argument to `owner_references_json(name, uid)`
    /// on the ownership-tag axis. Post-lift each callsite reads
    /// `let uid = process.uid_or_empty();` and the produced slice feeds
    /// the same downstream composer unchanged.
    ///
    /// Return-form axis: `&str` mirrors the existing borrow-first
    /// discipline of the peer metadata-fallback primitives
    /// ([`Self::namespace_or_default`], [`Self::name_or_placeholder`]);
    /// all three return owned-metadata borrows with a slot-specific
    /// fallback baked in so downstream consumers compose the slice
    /// directly into their next call without re-spelling the fallback.
    ///
    /// A future normalization step (a canonicalization pass that
    /// rejects a malformed uid before the owner-ref stamp, a cross-
    /// cluster uid rewrite for multi-tenant control planes, a stale-
    /// uid warning annotation for a Process whose uid changed under
    /// the reconciler mid-generation) lands at ONE substrate method
    /// here and both downstream `owner_references_json` consumers
    /// pick up the upgrade mechanically — no per-callsite hand-edit
    /// at `render_routing` / `render_export_jobs`.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.metadata.uid.as_deref().unwrap_or("")` chain recurred at
    /// two hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the missing-uid corner + the empty-string
    /// sentinel byte-shape + the borrow-form `&str` lifetime + the
    /// byte-identical parity with the pre-lift chain + the composition
    /// coherence with [`crate::owner_references_json`]'s `is_empty`
    /// gate, so a regression that drifted any surface at
    /// `tests::uid_or_empty_*` rather than as silent operator-facing
    /// skew between the two owner-reference emitters on the SAME
    /// Process).
    pub fn uid_or_empty(&self) -> &str {
        self.metadata.uid.as_deref().unwrap_or("")
    }

    /// Owned-form metadata-projection primitive on the `metadata.name`
    /// axis: returns an owned `String` copy of the K8s object name, with
    /// the missing-name corner collapsed to the load-bearing empty-string
    /// sentinel — the ONE-liner collapse of the paired
    /// `self.metadata.name.clone().unwrap_or_default()` incantation every
    /// keying / row-builder consumer restated by hand pre-lift.
    ///
    /// Pre-lift the `.metadata.name.clone().unwrap_or_default()` chain
    /// was hand-authored at TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold in `tatara-pool-reconciler::controller_pool`,
    /// both stamping the `PoolMember` / `PoolMemberSnapshot`
    /// `process_name: String` slot inside a struct-literal fanout over
    /// pool-owned `Process`es:
    /// * `reconcile_pool`'s pool-member seed (annotation-matched Process
    ///   list → `PoolMember { process_name, state, entered_state_at, .. }`)
    ///   — the row every operator sees on the pool's status page.
    /// * `reconcile_pool`'s desired-count snapshot seed
    ///   (`PoolMemberSnapshot { process_name, phase, created_at }`)
    ///   — the row fed into `decide_pool_convergence`.
    ///
    /// Both sites walked the SAME `.clone().unwrap_or_default()` chain
    /// and both wanted the `String` form the primitive returns — as the
    /// owned-form `process_name: String` slot on a struct literal
    /// composed inside a `.iter().map(...)` fanout over the same
    /// pool-owned `Process` list. Post-lift each callsite reads
    /// `process_name: p.owned_name_or_empty()` and the produced value
    /// feeds the same struct-literal slot unchanged.
    ///
    /// The empty-string fallback is the SAME sentinel the sibling
    /// borrow-form primitive [`Self::uid_or_empty`] returns — the two
    /// primitives partition the owned-form × borrow-form corner of the
    /// metadata-slot family on identical fallback semantics (empty
    /// string means "the slot is unset"), so a consumer that switches
    /// between them based on downstream ownership requirements never
    /// sees a different missing-slot spelling as a side effect.
    ///
    /// Peer to [`Self::name_or_placeholder`] on the (return-form ×
    /// fallback-value) axis pair — closes the corner the family
    /// previously left open:
    ///
    /// * borrow + display placeholder → [`Self::name_or_placeholder`]
    ///   (log lines, annotation writers, ownership-tag composers —
    ///   consumers whose downstream drops `"unnamed"` in place of a
    ///   missing name without operator-visible failure);
    /// * owned + empty sentinel → **this method** (row-builder /
    ///   HashMap-key / struct-literal fanout consumers whose downstream
    ///   fills a `String` field with the load-bearing `""` sentinel to
    ///   flag "no name to key by" rather than substituting a display
    ///   placeholder that would misalign a downstream lookup);
    /// * owned + name-required → [`Self::owned_coordinates_or_err`] (kube-rs
    ///   API-path calls — consumers whose downstream must NOT silently
    ///   substitute a placeholder for the API call target).
    ///
    /// The primitive family's `""`-on-missing-name semantics
    /// intentionally differs from [`Self::name_or_placeholder`]'s
    /// `"unnamed"` semantics: the caller sites for this form (pool
    /// membership row seeds, HashMap keys) are load-bearing keys — a
    /// display placeholder like `"unnamed"` would silently alias every
    /// missing-name Process to the same key, collapsing distinct rows
    /// in the pool's member list. The empty-string sentinel keeps the
    /// pre-lift byte-shape and lets downstream consumers gate on
    /// `String::is_empty` if they need to filter the missing-name
    /// corner explicitly.
    ///
    /// A future normalization step (a name-canonicalization pass, a
    /// case-fold key builder, a per-pool alias table for renamed
    /// Processes across generations) lands at ONE substrate method
    /// here and both downstream `PoolMember` / `PoolMemberSnapshot`
    /// seeds pick up the upgrade mechanically — no per-callsite hand-
    /// edit at `reconcile_pool`.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.metadata.name.clone().unwrap_or_default()` chain recurred
    /// at two hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the missing-name corner + the empty-string
    /// sentinel byte-shape + the owned-form `String` return type +
    /// the byte-identical parity with the pre-lift chain + the
    /// fallback-value coherence with the sibling [`Self::uid_or_empty`]
    /// on the metadata-slot × empty-sentinel axis, so a regression
    /// that drifted any surface at `tests::owned_name_or_empty_*`
    /// rather than as silent operator-facing skew between the pool-
    /// member seed and the desired-count snapshot seed on the SAME
    /// pool).
    pub fn owned_name_or_empty(&self) -> String {
        self.metadata.name.clone().unwrap_or_default()
    }

    /// Borrow-form spec-projection primitive on the declared parent-PID
    /// axis: returns the hierarchical PID path (e.g. `"seph.1"`) the
    /// author declared at `spec.identity.parent`, with the empty-slot
    /// corner collapsed to `None` — the ONE-liner collapse of the
    /// paired `self.spec.identity.parent.as_deref()` incantation every
    /// consumer restated by hand pre-lift.
    ///
    /// Pre-lift the `.spec.identity.parent.as_deref()` chain was hand-
    /// authored at TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold in `tatara-reconciler::phase_machine`:
    /// * `handle_forking` — the ALLOCATE-PID composer that threads the
    ///   declared parent PID into [`pid::allocate_pid`] and also into
    ///   the status patch payload (`{ "pid": new_pid, "parent":
    ///   parent_pid }`), so the reconciler-observed
    ///   [`ProcessStatus::parent`] slot mirrors the author-declared
    ///   [`IdentitySpec::parent`] at fork time. The `info!` tracing
    ///   span also reads the same slice as the `parent` field on the
    ///   PID-assigned log line.
    /// * `handle_exiting` — the SIGTERM cascade's child-fan-out filter
    ///   that enumerates every Process cluster-wide and picks children
    ///   whose `spec.identity.parent` equals this Process's currently-
    ///   observed PID (`.filter(|c| c.spec.identity.parent.as_deref()
    ///   == Some(pid))`). The filter runs per candidate child, so the
    ///   borrow-form projection avoids allocating one `String` clone
    ///   per non-matching row in the cluster-wide list.
    ///
    /// Both sites walked the SAME `.as_deref()` chain and both wanted
    /// the `Option<&str>` form the primitive returns — the
    /// `handle_forking` site to feed positionally into
    /// `pid::allocate_pid(&identity, parent_pid, next_seq)` and the
    /// tracing span's `parent = ?parent_pid` debug print + the JSON
    /// payload's `"parent": parent_pid` slot; the `handle_exiting`
    /// filter to compare directly against `Some(pid)` where `pid:
    /// &str` came off the borrow-form peer [`Self::observed_pid`].
    ///
    /// Return-form axis: `Option<&str>` mirrors the borrow-first
    /// discipline of every peer primitive on the metadata / status
    /// slot family ([`Self::namespace_or_default`],
    /// [`Self::name_or_placeholder`], [`Self::observed_pid`],
    /// [`Self::annotation`]). The empty-slot corner
    /// (`spec.identity.parent = None`, matching `init` / PID 1 with
    /// no parent) collapses to `None` so `.is_some()` / `if let
    /// Some(_)` / `.map(...)` behave identically on a `Process`
    /// authored at cluster init (PID 1, parent absent) and on any
    /// PID-N child (parent present) — matching the pre-lift
    /// `.as_deref()` chain's `None` byte-identically.
    ///
    /// Peer to [`Self::observed_pid`] on the (spec-declared ×
    /// status-observed) axis pair: `observed_pid` returns the PID
    /// path this Process currently OWNS (the reconciler-persisted
    /// child position in the hierarchy), while `declared_parent_pid`
    /// returns the PID path this Process's parent OWNS (the author-
    /// declared upstream position). The SIGTERM cascade at
    /// `handle_exiting` composes both: it reads its own
    /// [`Self::observed_pid`] and matches each candidate child's
    /// [`Self::declared_parent_pid`] against that value — the child-
    /// fan-out relation IS the spec-declared × status-observed axis
    /// pair collapsed to a single comparator, both sides routed
    /// through the same borrow-form skeleton.
    ///
    /// A future normalization step (a per-slot canonicalization pass
    /// that rejects malformed hierarchical PIDs, a case-fold lookup
    /// against a table of renamed identities, a cross-cluster prefix
    /// stripper, an alias-table lookup that maps a legacy PID to its
    /// current spelling) lands at ONE substrate method here and both
    /// downstream consumers pick up the upgrade mechanically — no
    /// per-callsite hand-edit at `handle_forking` / `handle_exiting`.
    ///
    /// Sibling to the peer metadata-projection primitives
    /// ([`Self::namespace_or_default`], [`Self::name_or_placeholder`],
    /// [`Self::coordinates_or_defaults`], [`Self::coordinates_or_none`],
    /// [`Self::owned_coordinates_or_err`], [`Self::annotation`]) on the
    /// metadata axis; this method opens the borrow-form peer on the
    /// declared-identity axis. Future identity projections
    /// (`declared_name_override` on the `spec.identity.name_override`
    /// axis, a paired `declared_identity` composite that returns both
    /// halves) land as peer methods on this same axis.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.spec.identity.parent.as_deref()` chain recurred at two
    /// hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// trigger, and is lifted to ONE owner here). THEORY.md §II.1
    /// invariant 5 (composition preserves proofs — the pins bind the
    /// empty-slot corner + the borrow-form `&str` lifetime + the
    /// byte-identical parity with the pre-lift `.as_deref()` chain,
    /// so a regression that drifted any surface at
    /// `tests::declared_parent_pid_*` rather than as silent operator-
    /// facing skew between the ALLOCATE-PID composer and the SIGTERM
    /// cascade's child-fan-out filter on the SAME parent-child pair).
    pub fn declared_parent_pid(&self) -> Option<&str> {
        self.spec.identity.parent.as_deref()
    }

    /// Borrow-form spec-projection primitive on the declared
    /// name-override axis: returns the human name the author declared
    /// at `spec.identity.name_override` (used verbatim instead of the
    /// content-hash-derived name in [`derive_identity`]), with the
    /// empty-slot corner collapsed to `None` — the ONE-liner collapse
    /// of the paired `self.spec.identity.name_override.as_deref()`
    /// incantation every consumer restated by hand pre-lift.
    ///
    /// Pre-lift the `.spec.identity.name_override.as_deref()` chain
    /// was hand-authored at TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold in `tatara-reconciler::phase_machine`,
    /// both feeding the second positional argument of
    /// [`derive_identity`]:
    /// * `handle_pending` — the DECLARE composer that computes the
    ///   Process's [`Identity`] on entry to the state machine (before
    ///   `patch::phase_status` writes it into `status.identity`).
    /// * `handle_forking` — the ALLOCATE-PID composer that recomputes
    ///   the same [`Identity`] on a rehydration path (status may
    ///   already carry an identity from a prior reconcile, in which
    ///   case the `.and_then(|s| s.identity.clone())` short-circuit
    ///   takes it; otherwise this `.unwrap_or_else` branch fires and
    ///   recomputes the identity fresh from the spec) so `pid::
    ///   allocate_pid` sees the SAME [`Identity`] the DECLARE phase
    ///   produced.
    ///
    /// Both sites walked the SAME `.as_deref()` chain and both wanted
    /// the `Option<&str>` form the primitive returns — as the second
    /// positional argument to `derive_identity(&self.spec, …)`, which
    /// internally trims + filters empty strings + dispatches on
    /// `Some(non_empty)` (verbatim name, `name_override: true`) vs
    /// `None | Some(empty | whitespace)` (content-hash-derived name,
    /// `name_override: false`). The primitive itself preserves the
    /// raw slot byte-identically (the trim happens IN
    /// `derive_identity`, not at the borrow site), so the two live
    /// paths compose through the SAME borrow-form skeleton.
    ///
    /// Return-form axis: `Option<&str>` mirrors the borrow-first
    /// discipline of every peer primitive on the metadata / status /
    /// spec-identity slot family ([`Self::namespace_or_default`],
    /// [`Self::name_or_placeholder`], [`Self::observed_pid`],
    /// [`Self::annotation`], [`Self::declared_parent_pid`]). The
    /// empty-slot corner (`spec.identity.name_override = None`,
    /// matching a Process authored WITHOUT the human-name-override
    /// escape hatch — the default; `derive_identity` then computes
    /// the name from the content hash) collapses to `None` so
    /// `.is_some()` / `if let Some(_)` / `.map(...)` behave
    /// identically on the two Process shapes an operator can author.
    ///
    /// Peer to [`Self::declared_parent_pid`] on the (parent × name-
    /// override) sub-axis of the declared-identity axis: both
    /// primitives project a `Option<String>` slot on `IdentitySpec`
    /// through the SAME borrow-form skeleton, so a future
    /// `declared_identity` composite that returns both halves
    /// together (e.g. as a `(Option<&str>, Option<&str>)` tuple or a
    /// borrow-form `DeclaredIdentityView<'_>` newtype) lands as ONE
    /// method that COMPOSES the two peer primitives, not as three
    /// hand-authored `.as_deref()` chains restated at each callsite.
    ///
    /// A future normalization step (a per-slot canonicalization pass
    /// that rejects malformed names, a case-fold lookup against a
    /// table of renamed identities, an alias-table lookup that maps
    /// a legacy name-override to its current spelling, a whitespace-
    /// trim lift OUT of `derive_identity` INTO the primitive so both
    /// consumers see the trimmed form) lands at ONE substrate method
    /// here and both downstream consumers pick up the upgrade
    /// mechanically — no per-callsite hand-edit at `handle_pending` /
    /// `handle_forking`.
    ///
    /// Sibling to the peer spec-identity projection
    /// [`Self::declared_parent_pid`] on the declared-identity axis;
    /// this method opens the borrow-form peer on the name-override
    /// sub-axis of the same closed set (`IdentitySpec { parent,
    /// name_override }`). Future identity projections (a paired
    /// `declared_identity` composite that returns both halves
    /// together) land as peer methods on this same axis.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.spec.identity.name_override.as_deref()` chain recurred
    /// at two hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the empty-slot corner + the borrow-form `&str`
    /// lifetime + the byte-identical parity with the pre-lift
    /// `.as_deref()` chain + the invariance under
    /// [`derive_identity`]'s internal trim/filter step, so a
    /// regression that drifted any surface at
    /// `tests::declared_name_override_*` rather than as silent
    /// operator-facing skew between the DECLARE composer and the
    /// ALLOCATE-PID rehydration branch on the SAME Process spec).
    pub fn declared_name_override(&self) -> Option<&str> {
        self.spec.identity.name_override.as_deref()
    }

    /// Borrowed slice of the FluxCD resources this Process's status
    /// currently persists at `status.flux_resources`, with the
    /// missing-`status` corner collapsed to an empty slice — the ONE-
    /// line collapse of the paired `self.status.as_ref().map(|s|
    /// s.flux_resources.clone()).unwrap_or_default()` incantation
    /// every VERIFY-phase / ATTEST-heartbeat consumer restated by hand
    /// pre-lift.
    ///
    /// Pre-lift the 5-line `.status.as_ref().map(|s| s.flux_resources
    /// .clone()).unwrap_or_default()` chain was hand-authored at TWO
    /// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold in
    /// `tatara-reconciler::phase_machine`:
    /// * `handle_running` — the VERIFY-phase per-ref readiness probe
    ///   seed that walks every ref through
    ///   [`crate::status::FluxResourceRef::fetch_coords`] via
    ///   `ssapply::fetch_flux_ref` and rebuilds an updated
    ///   `Vec<FluxResourceRef>` with `ready` + `message` + `last_check`
    ///   observed at reconcile time.
    /// * `handle_attested` — the ATTEST-heartbeat drift detector that
    ///   short-circuits on the first non-Ready ref via
    ///   `ssapply::fetch_flux_ref` + `ssapply::ready_condition`.
    ///
    /// Both sites walked the SAME 5-line chain — clone the vector
    /// eagerly for the length of the reconcile pass, then iterate it
    /// by reference — even though neither site ever mutates the vector
    /// nor keeps it alive past the enclosing async fn. Post-lift both
    /// callers borrow the slice directly from `self.status`; the two
    /// pre-lift `.clone()` calls disappear because the slice lives for
    /// the borrow of `&self`, and both call sites' subsequent
    /// downstream calls (`ssapply::fetch_flux_ref` / the
    /// `patch::patch_process_status` write) do not touch the borrowed
    /// `p: &Process`, so the borrow lifetime holds.
    ///
    /// Return-form axis: `&[FluxResourceRef]` mirrors the existing
    /// borrow-first discipline every pre-lift consumer already
    /// iterated by reference (`for r in &refs`), and the shape of
    /// [`crate::status::FluxResourceRef::fetch_coords`]'s per-ref
    /// borrow projection extends mechanically to the slice-level
    /// projection here. The missing-`status` corner collapses to the
    /// empty slice `&[]` so `.is_empty()` / `.len()` / iteration all
    /// behave identically on a `Process` whose status is `None` and
    /// on one whose status carries an empty `flux_resources` slot —
    /// matching what the pre-lift `.unwrap_or_default()` produced
    /// (an empty `Vec`).
    ///
    /// A future normalization step (a per-ref canonicalization pass
    /// that skips duplicated refs, an owner-filter that returns only
    /// refs stamped with the CURRENT `metadata.generation`, a
    /// staleness gate that drops refs whose `last_check` predates a
    /// reconcile deadline) lands at ONE substrate method here and
    /// both downstream consumers pick up the upgrade mechanically —
    /// no per-callsite hand-edit at `handle_running` /
    /// `handle_attested`.
    ///
    /// Sibling to the [`Self::coordinates_or_none`] borrow-first
    /// primitive on the metadata axis; this method opens the
    /// analogous borrow-first primitive on the status-projection
    /// axis. Future status projections (`observed_attestation` on
    /// the attestation-chain axis, `observed_pid` on the PID axis,
    /// `observed_children` on the child-fan-out axis) land as peer
    /// methods on this same axis.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the 5-line status-projection chain recurred at two hand-
    /// authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// trigger, and is lifted to ONE owner here). THEORY.md §II.1
    /// invariant 5 (composition preserves proofs — the pins bind the
    /// missing-`status` corner + the slice-lifetime borrow discipline
    /// + the byte-identical parity with the pre-lift 5-line chain, so
    /// a regression that drifted any of the three surfaces at
    /// `tests::observed_flux_resources_*` rather than as silent
    /// operator-facing skew between the VERIFY-phase and ATTEST-
    /// heartbeat consumers).
    pub fn observed_flux_resources(&self) -> &[FluxResourceRef] {
        self.status
            .as_ref()
            .map(|s| s.flux_resources.as_slice())
            .unwrap_or(&[])
    }

    /// The borrow-form status-projection primitive on the PID axis:
    /// returns the hierarchical PID path (e.g. `"seph.1.7"`) the
    /// reconciler currently persists at `status.pid`, with BOTH the
    /// missing-`status` corner AND the empty-slot corner collapsed
    /// to `None` — the ONE-liner collapse of the paired
    /// `self.status.as_ref().and_then(|s| s.pid.clone())` incantation
    /// every consumer restated by hand pre-lift.
    ///
    /// Pre-lift the 3-line `.status.as_ref().and_then(|s| s.pid
    /// .clone())` chain was hand-authored at TWO sites past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication threshold in
    /// `tatara-reconciler::phase_machine`:
    /// * `handle_forking` — the ALLOCATE-PID gate that short-
    ///   circuits the PID allocator when the reconciler already
    ///   assigned a PID on a prior reconcile pass (pre-lift the
    ///   chain composed with `.is_some()` and threw the clone away
    ///   without ever reading the string).
    /// * `handle_exiting` — the SIGTERM cascade that enumerates
    ///   child Processes and terminates them by matching each
    ///   child's `spec.identity.parent` against the PID this Process
    ///   currently owns (pre-lift the chain bound an owned
    ///   `Option<String>` and threaded `pid.as_str()` into the
    ///   downstream `.as_deref() == Some(...)` comparator).
    ///
    /// Both sites walked the SAME 3-line chain — clone the `String`
    /// eagerly, then either drop it (the `handle_forking` gate) or
    /// re-borrow it through `.as_str()` (the `handle_exiting`
    /// comparator) — even though neither site ever mutates the PID
    /// nor keeps it alive past the enclosing async fn. Post-lift
    /// both callers borrow the PID directly from `self.status`; the
    /// pre-lift `.clone()` at both sites disappears because the
    /// `&str` lives for the borrow of `&self`, and both call sites'
    /// subsequent downstream calls (the K8s API list/patch, the
    /// child-Process comparator) do not touch the borrowed
    /// `p: &Process`, so the borrow lifetime holds.
    ///
    /// Return-form axis: `Option<&str>` mirrors the existing
    /// borrow-first discipline every pre-lift consumer already
    /// re-borrowed through `.as_str()` before use, and the shape of
    /// [`Self::coordinates_or_none`]'s `Option<(&str, &str)>`
    /// projection extends mechanically to the single-slot
    /// projection here. The missing-`status` corner AND the
    /// populated-status-with-`pid=None` corner BOTH collapse to
    /// `None` so `.is_some()` / `if let Some(_)` / `.map(...)`
    /// behave identically on a `Process` whose status is `None`
    /// and on one whose status carries an unpopulated `pid` slot —
    /// matching what the pre-lift `.and_then(...)` chain produced.
    ///
    /// A future normalization step (a per-slot canonicalization
    /// pass that rejects malformed hierarchical PIDs, a
    /// generation-filter that returns `None` for a PID stamped
    /// with a stale `metadata.generation`, a staleness gate that
    /// drops a PID whose observing `phase_since` predates a
    /// reconcile deadline) lands at ONE substrate method here and
    /// both downstream consumers pick up the upgrade mechanically
    /// — no per-callsite hand-edit at `handle_forking` /
    /// `handle_exiting`.
    ///
    /// Sibling to the peer [`Self::observed_flux_resources`]
    /// borrow-first primitive on the flux-resources axis; both
    /// methods compose the same missing-`status` fallback +
    /// borrow-form return-shape skeleton on distinct
    /// `ProcessStatus` slots. Future status projections
    /// (`observed_parent` on the parent-pointer axis,
    /// `observed_message` on the human-readable-status axis,
    /// `observed_attestation` on the attestation-chain axis) land
    /// as peer methods on this same axis.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over
    /// composition — the 3-line status-projection chain recurred
    /// at two hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the missing-`status` corner + the empty-slot
    /// corner + the borrow-form `&str` lifetime + the
    /// byte-identical parity with the pre-lift 3-line chain, so a
    /// regression that drifted any surface at
    /// `tests::observed_pid_*` rather than as silent operator-
    /// facing skew between the ALLOCATE-PID gate and the SIGTERM
    /// cascade on the SAME `Process`).
    pub fn observed_pid(&self) -> Option<&str> {
        self.status.as_ref().and_then(|s| s.pid.as_deref())
    }

    /// The borrow-form status-projection primitive on the
    /// attestation-chain axis: returns the last
    /// [`ProcessAttestation`] the reconciler persisted at
    /// `status.attestation`, with the missing-`status` corner AND the
    /// empty-slot corner BOTH collapsed to `None` — the ONE-liner
    /// collapse of the paired `self.status.as_ref().and_then(|s|
    /// s.attestation.as_ref())` incantation every consumer restated
    /// by hand pre-lift.
    ///
    /// Pre-lift the 3-line `.status.as_ref().and_then(|s| s
    /// .attestation.as_ref())` chain was hand-authored at TWO sites
    /// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold in
    /// `tatara-reconciler`:
    /// * `phase_machine::advance_to_attested` — the ATTEST composer
    ///   that chains `prior.next(pillars)` when a prior attestation
    ///   is persisted and seeds with `ProcessAttestation::initial`
    ///   otherwise.
    /// * `render::render_export_jobs` — the ephemeral-export Job
    ///   builder that pulls the prior `composed_root` off the last
    ///   persisted attestation and threads it into every rendered
    ///   Job's `previousRoot` env var, so the export receipt chains
    ///   into the Process's BLAKE3 attestation tree at the correct
    ///   generation boundary.
    ///
    /// Both sites walked the SAME 3-line chain — the borrow-form
    /// `Option<&ProcessAttestation>` shape both consumers wanted
    /// already — even though neither site ever mutated the
    /// attestation nor kept it alive past the enclosing async fn.
    /// Post-lift both callers borrow the attestation directly from
    /// `self.status`; the pre-lift 3-line chain shrinks to a single
    /// method call at both sites, and both consumers' subsequent
    /// downstream calls (`ProcessAttestation::next` for the ATTEST
    /// composer, `.composed_root.clone()` for the export Job builder)
    /// do not touch the borrowed `p: &Process`, so the borrow
    /// lifetime holds.
    ///
    /// Return-form axis: `Option<&ProcessAttestation>` mirrors the
    /// existing borrow-first discipline every pre-lift consumer
    /// already re-borrowed through `.as_ref()`, and the shape of the
    /// peer [`Self::observed_pid`] projection extends mechanically
    /// to the whole-attestation-record projection here. The missing-
    /// `status` corner AND the populated-status-with-`attestation
    /// =None` corner BOTH collapse to `None` so `.is_some()` / `if
    /// let Some(_)` / `.map(...)` behave identically on a `Process`
    /// whose status is `None` and on one whose status carries an
    /// unpopulated `attestation` slot — matching what the pre-lift
    /// `.and_then(...)` chain produced.
    ///
    /// A future normalization step (a per-slot canonicalization pass
    /// that rejects a persisted attestation whose `composed_root`
    /// fails `verify`, a generation-filter that returns `None` for
    /// an attestation stamped with a stale `metadata.generation`, a
    /// staleness gate that drops an attestation whose `attested_at`
    /// predates a reconcile deadline) lands at ONE substrate method
    /// here and both downstream consumers pick up the upgrade
    /// mechanically — no per-callsite hand-edit at
    /// `advance_to_attested` / `render_export_jobs`.
    ///
    /// Sibling to the peer [`Self::observed_pid`] +
    /// [`Self::observed_flux_resources`] borrow-first primitives on
    /// the PID + flux-resources axes; all three methods compose the
    /// same missing-`status` fallback + borrow-form return-shape
    /// skeleton on distinct `ProcessStatus` slots. Future status
    /// projections (`observed_parent` on the parent-pointer axis,
    /// `observed_message` on the human-readable-status axis) land
    /// as peer methods on this same axis.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition
    /// — the 3-line status-projection chain recurred at two hand-
    /// authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// trigger, and is lifted to ONE owner here). THEORY.md §II.1
    /// invariant 5 (composition preserves proofs — the pins bind
    /// the missing-`status` corner + the empty-slot corner + the
    /// borrow-form `&ProcessAttestation` lifetime + the byte-
    /// identical parity with the pre-lift 3-line chain, so a
    /// regression that drifted any surface at
    /// `tests::observed_attestation_*` rather than as silent
    /// operator-facing skew between the ATTEST composer and the
    /// ephemeral-export receipt chain on the SAME `Process`).
    pub fn observed_attestation(&self) -> Option<&ProcessAttestation> {
        self.status.as_ref().and_then(|s| s.attestation.as_ref())
    }

    /// The borrow-form status-projection primitive on the resolved-
    /// identity axis: returns the [`Identity`] the reconciler
    /// currently persists at `status.identity` (name + content hash +
    /// override flag), with the missing-`status` corner AND the
    /// empty-slot corner BOTH collapsed to `None` — the ONE-liner
    /// collapse of the paired `self.status.as_ref().and_then(|s|
    /// s.identity.as_ref())` incantation every consumer restated by
    /// hand pre-lift.
    ///
    /// Pre-lift the paired `.status.as_ref().and_then(|s|
    /// s.identity.<clone|as_ref>())` chain was hand-authored at TWO
    /// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold
    /// in `tatara-reconciler`:
    /// * `phase_machine::handle_forking` — the FORK-time identity
    ///   seed that reuses the reconciler-persisted `Identity` if
    ///   present and falls back to a fresh `derive_identity(&spec,
    ///   name_override)` otherwise. Pre-lift the site cloned the
    ///   whole `Identity` off the borrow before threading it through
    ///   `.unwrap_or_else(...)` even though the fallback path
    ///   allocates its own owned `Identity` — the pre-lift clone
    ///   allocated a fresh `Identity` on the happy path just so the
    ///   `Option`'s shape matched the fallback's `Identity` return
    ///   type.
    /// * `ssapply::inject_annotations` — the SSA-time annotation
    ///   composer that stamps the content-hash annotation onto every
    ///   owned resource. Pre-lift the site nested the identity
    ///   borrow-form check inside a manual `if let Some(status) =
    ///   &process.status { … }` guard alongside sibling `status.pid`
    ///   and `status.attestation` accesses — three siblings the peer
    ///   primitives [`Self::observed_pid`] and
    ///   [`Self::observed_attestation`] already own, so the outer
    ///   status guard was the last hand-authored `.status.as_ref()`
    ///   destructure at this composer.
    ///
    /// Both sites walked the SAME 3-line chain (one via `.clone()`,
    /// one via `.as_ref()`) — the borrow-form
    /// `Option<&Identity>` shape both consumers wanted already, even
    /// though the FORK-time seed then had to `.clone()` off the
    /// borrow to compose with the owned-`Identity` fallback. Post-
    /// lift the seed calls `.observed_identity().cloned()` at the
    /// exact composition point where the owned value is required
    /// (the empty-borrow corner clones nothing, since
    /// `Option::cloned` on `None` is `None`), and the SSA-time
    /// consumer drops the outer status guard entirely — the
    /// three-sibling primitive family (pid + identity + attestation)
    /// now peers through `observed_pid` +
    /// `observed_identity` + `observed_attestation` at ONE call each
    /// with no shared status destructure between them.
    ///
    /// Return-form axis: `Option<&Identity>` mirrors the
    /// existing borrow-first discipline every pre-lift consumer
    /// already re-borrowed through `.as_ref()` / re-cloned through
    /// `.clone()`, and the shape of the peer
    /// [`Self::observed_attestation`] projection extends
    /// mechanically to the whole-`Identity`-record projection here.
    /// The missing-`status` corner AND the populated-status-with-
    /// `identity=None` corner BOTH collapse to `None` so
    /// `.is_some()` / `if let Some(_)` / `.map(...)` behave
    /// identically on a `Process` whose status is `None` and on one
    /// whose status carries an unpopulated `identity` slot —
    /// matching what the pre-lift `.and_then(...)` chain produced.
    ///
    /// A future normalization step (a per-slot canonicalization
    /// pass that rejects an `Identity` whose `content_hash` fails
    /// re-derivation against the current spec, a generation-filter
    /// that returns `None` for an identity stamped with a stale
    /// `metadata.generation`, a staleness gate that drops an
    /// identity whose observing `phase_since` predates a reconcile
    /// deadline) lands at ONE substrate method here and both
    /// downstream consumers pick up the upgrade mechanically — no
    /// per-callsite hand-edit at `handle_forking` /
    /// `inject_annotations`.
    ///
    /// Sibling to the peer [`Self::observed_pid`] +
    /// [`Self::observed_attestation`] +
    /// [`Self::observed_flux_resources`] borrow-first primitives on
    /// the PID + attestation-chain + flux-resources axes; all four
    /// methods compose the same missing-`status` fallback +
    /// borrow-form return-shape skeleton on distinct `ProcessStatus`
    /// slots. Future status projections (`observed_parent` on the
    /// parent-pointer axis, `observed_message` on the human-
    /// readable-status axis) land as peer methods on this same
    /// axis.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition
    /// — the 3-line status-projection chain recurred at two hand-
    /// authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// trigger, and is lifted to ONE owner here). THEORY.md §II.1
    /// invariant 5 (composition preserves proofs — the pins bind
    /// the missing-`status` corner + the empty-slot corner + the
    /// borrow-form `&Identity` lifetime + the byte-identical parity
    /// with the pre-lift 3-line chain, so a regression that drifted
    /// any surface at `tests::observed_identity_*` rather than as
    /// silent operator-facing skew between the FORK-time identity
    /// seed and the SSA-time content-hash annotation stamp on the
    /// SAME `Process`).
    pub fn observed_identity(&self) -> Option<&Identity> {
        self.status.as_ref().and_then(|s| s.identity.as_ref())
    }

    /// The copy-form status-projection primitive on the phase axis:
    /// returns the [`ProcessPhase`] the reconciler currently persists
    /// at `status.phase`, wrapped in an `Option` so the missing-
    /// `status` corner collapses to `None` — the ONE-liner collapse
    /// of the paired `self.status.as_ref().map(|s| s.phase)`
    /// incantation every consumer restated by hand pre-lift.
    ///
    /// Peer to the borrow-form projections
    /// [`Self::observed_pid`] (PID axis, `Option<&str>`),
    /// [`Self::observed_flux_resources`] (flux-resources axis,
    /// `&[FluxResourceRef]`), and [`Self::observed_attestation`]
    /// (attestation-chain axis, `Option<&ProcessAttestation>`); this
    /// method opens the copy-form peer for `ProcessPhase` — a
    /// `Copy` scalar with a `Default` impl (`Pending`), so the
    /// return is `Option<ProcessPhase>` rather than
    /// `Option<&ProcessPhase>` (borrow would give the caller
    /// nothing over the copy for a 1-byte enum) and neither the
    /// missing-`status` corner nor a "empty slot" corner is
    /// meaningful — the underlying slot is a bare `ProcessPhase`,
    /// not `Option<ProcessPhase>`, so the primitive returns `None`
    /// iff `status: None`.
    ///
    /// Pre-lift the 3-line `.status.as_ref().map(|s| s.phase)`
    /// chain was hand-authored at FIVE sites past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication threshold in
    /// `tatara-reconciler`:
    /// * `controller::reconcile` — the top-level dispatcher's
    ///   `current_phase` seed that feeds the deletion-preempt +
    ///   signal-ingestion gates + the per-phase handler dispatch.
    ///   Pre-lift `.unwrap_or(ProcessPhase::Pending)`.
    /// * `boundary::evaluate_process_phase` — the boundary
    ///   evaluator's `ProcessPhase` condition (a peer-Process
    ///   `phase`-reached postcondition). Pre-lift
    ///   `.unwrap_or(ProcessPhase::Pending)`.
    /// * `boundary::check_depends_on` — the `depends_on`
    ///   pre-condition audit that stashes the observed phase into
    ///   the `UnmetDependency::actual: Option<ProcessPhase>` slot
    ///   (keeps the `Option` form). Pre-lift the raw
    ///   `.map(|s| s.phase)` shape.
    /// * `phase_machine::p_current_phase_str` — the released-from
    ///   annotation composer that emits `"Attested"` for every
    ///   non-`Failed` phase (SIGSTOP/SIGCONT release gate).
    ///   Pre-lift `.unwrap_or(ProcessPhase::Attested)` — the ONE
    ///   site whose default is not `Pending`; the primitive
    ///   returns the raw `Option` so the caller's `.unwrap_or`
    ///   default choice stays local rather than baked in.
    /// * `table_controller::stable_name_group_key` — the routing-
    ///   groupby seed that pairs the phase with the PID + creation
    ///   timestamp when partitioning Processes claiming the same
    ///   stable name. Pre-lift `.unwrap_or(ProcessPhase::Pending)`.
    ///
    /// All FIVE sites walked the SAME 3-line `.status.as_ref()
    /// .map(|s| s.phase)` chain — three closed with `unwrap_or
    /// (ProcessPhase::Pending)` (the `Default`), one closed with
    /// `unwrap_or(ProcessPhase::Attested)`, one kept the raw
    /// `Option<ProcessPhase>` — so the ONE substrate accessor
    /// returns the raw `Option<ProcessPhase>` and each consumer
    /// keeps its `.unwrap_or(...)` default choice at its own site.
    ///
    /// A future normalization step (a generation-filter that
    /// returns `None` for a phase stamped with a stale
    /// `metadata.generation`, a staleness gate that drops a phase
    /// whose observing `phase_since` predates a reconcile
    /// deadline, a canonicalization pass that maps a phase that
    /// no longer belongs to the CRD's closed set to `None`) lands
    /// at ONE substrate method here and all five consumers pick
    /// up the upgrade mechanically — no per-callsite hand-edit at
    /// `reconcile` / `evaluate_process_phase` / `check_depends_on`
    /// / `p_current_phase_str` / `stable_name_group_key`.
    ///
    /// Future status projections (`observed_parent` on the
    /// parent-pointer axis, `observed_message` on the human-
    /// readable-status axis, `observed_children` on the child
    /// fan-out axis, `observed_exit_code` on the terminal-exit
    /// axis) land as peer methods on this same axis.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over
    /// composition — the 3-line status-projection chain recurred
    /// at FIVE hand-authored sites past the ★★ PRIME-DIRECTIVE
    /// ≥ 2 duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs
    /// — the pins bind the missing-`status` corner + the
    /// per-variant enum round-trip + the byte-identical parity
    /// with the pre-lift 3-line chain, so a regression that
    /// drifted any surface at `tests::observed_phase_*` rather
    /// than as silent operator-facing skew between the
    /// controller's dispatch seed and the boundary evaluator's
    /// depends-on audit on the SAME `Process` within one
    /// reconcile pass).
    pub fn observed_phase(&self) -> Option<ProcessPhase> {
        self.status.as_ref().map(|s| s.phase)
    }

    /// The copy-form status-projection primitive on the phase axis
    /// with the `Pending` sink applied — the ONE-liner collapse of
    /// the paired `self.observed_phase().unwrap_or(ProcessPhase::
    /// Pending)` incantation every reconciler consumer restated by
    /// hand at the `Option`-flattening tail of the `observed_phase`
    /// call. Sibling to [`Self::observed_phase`] on the (return-form
    /// × fallback shape) axis pair — the raw-`Option` corner stays
    /// as `observed_phase`, this method opens the `Pending`-defaulted
    /// corner that four of the five hand-authored `observed_phase`
    /// consumers chose (the fifth chose `Attested`; it keeps the raw
    /// `Option` accessor because a `Pending` sink would silently drop
    /// its released-from-annotation branch into the wrong label).
    ///
    /// The primitive returns [`ProcessPhase::Pending`] on any missing
    /// `status` slot — the same sentinel [`ProcessPhase::default`]
    /// returns, and the same fallback all four pre-lift consumers
    /// wrote by hand. `ProcessPhase::Pending` is load-bearing as the
    /// "not yet observed" default because the top-level dispatcher's
    /// `Pending → Forking` transition, the boundary evaluator's
    /// per-Process phase-reached postcondition, the routing groupby's
    /// stable-name claim-arbiter row seed, and the pool controller's
    /// desired-count snapshot all read a freshly-forked Process (no
    /// `status` yet stamped by the reconciler) as being at the
    /// entrypoint phase of the closed lifecycle. A caller with a
    /// different default choice (currently only the SIGSTOP/SIGCONT
    /// release gate's `Attested` fallback in
    /// `phase_machine::p_current_phase_str`) keeps the raw
    /// [`Self::observed_phase`] accessor at its own site.
    ///
    /// Pre-lift the two-link `.observed_phase().unwrap_or
    /// (ProcessPhase::Pending)` chain was hand-authored at FOUR
    /// sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold
    /// across the workspace:
    /// * `tatara-reconciler::controller::reconcile` — the top-level
    ///   dispatcher's `current_phase` seed that feeds the
    ///   deletion-preempt + signal-ingestion gates + the per-phase
    ///   handler dispatch.
    /// * `tatara-reconciler::boundary::evaluate_process_phase` — the
    ///   boundary evaluator's [`ConditionKind::ProcessPhase`]
    ///   evaluator that compares a peer-Process's observed phase
    ///   against the operator-declared `phase`-reached postcondition.
    /// * `tatara-reconciler::table_controller::stable_name_group_key`
    ///   — the routing-groupby seed that pairs the phase with the
    ///   PID + creation timestamp when partitioning Processes
    ///   claiming the same stable name.
    /// * `tatara-pool-reconciler::controller_pool::reconcile_pool` —
    ///   the desired-count loop's per-member snapshot seed that feeds
    ///   `decide_pool_convergence` with each owned Process's
    ///   `(phase, created_at)` pair.
    ///
    /// All FOUR sites walked the SAME two-link chain and all four
    /// closed with `ProcessPhase::Pending` as the sink; post-lift
    /// each callsite reads `process.observed_phase_or_pending()` and
    /// the produced `ProcessPhase` feeds the same downstream branch
    /// (dispatch on the `current_phase` value, comparison against a
    /// declared threshold, groupby-key composition, member-state
    /// snapshot construction) unchanged.
    ///
    /// Return-form axis: `ProcessPhase` matches the copy discipline
    /// of [`Self::observed_phase`] (a `Copy` scalar one byte wide),
    /// with the [`Option`] wrapper collapsed at the primitive rather
    /// than at every consumer. A caller that needs the missing-`status`
    /// corner as a distinguishable value keeps the raw
    /// [`Self::observed_phase`] accessor.
    ///
    /// A future normalization step (a generation-filter that
    /// treats a phase stamped with a stale `metadata.generation` as
    /// unobserved and therefore `Pending`, a staleness gate that
    /// drops a phase whose observing `phase_since` predates a
    /// reconcile deadline, a canonicalization pass that maps a phase
    /// that no longer belongs to the CRD's closed set to `Pending`)
    /// lands at ONE substrate method here — because this primitive
    /// composes on top of [`Self::observed_phase`], the normalization
    /// applies to both the raw-`Option` and the `Pending`-sinked
    /// return through the SAME upstream body — and all four
    /// downstream consumers pick up the upgrade mechanically.
    ///
    /// Peer to the sibling defaulted-fallback primitive family
    /// [`Self::namespace_or_default`] +
    /// [`Self::name_or_placeholder`] + [`Self::uid_or_empty`] on the
    /// (return-shape × fallback-value) axis — those three open the
    /// borrow-form defaulted corner for the metadata slots; this
    /// method opens the copy-form defaulted corner for the phase
    /// slot on `status`. Future defaulted-fallback status
    /// projections (an `observed_pid_or_empty` on the PID axis, an
    /// `observed_exit_code_or_zero` on the terminal-exit axis) land
    /// as peer methods on this same axis.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the two-link `.observed_phase().unwrap_or(Pending)` chain
    /// recurred at four hand-authored sites past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to ONE
    /// owner here). THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the pins bind the missing-`status` sink to
    /// `Pending` + populated-status pass-through + every
    /// `ProcessPhase` variant round-trip + byte-identical parity
    /// with the pre-lift two-link chain, so a regression that
    /// drifted any surface at `tests::observed_phase_or_pending_*`
    /// rather than as silent operator-facing skew between the
    /// top-level dispatcher's `Pending → Forking` seed and the
    /// boundary evaluator's per-Process phase-reached postcondition
    /// on the SAME `Process` within one reconcile pass).
    pub fn observed_phase_or_pending(&self) -> ProcessPhase {
        self.observed_phase().unwrap_or(ProcessPhase::Pending)
    }

    /// Copy-form metadata-projection primitive on the deletion-tombstone
    /// axis: returns `true` iff the K8s API server has stamped a
    /// `metadata.deletionTimestamp` on this Process (the moment the
    /// object entered the "being deleted" corner of its lifecycle,
    /// after which further mutating writes are refused and finalizers
    /// are drained before the object is actually removed) — the ONE-
    /// liner collapse of the paired `self.metadata.deletion_timestamp
    /// .is_some()` incantation every consumer restated by hand
    /// pre-lift.
    ///
    /// Pre-lift the `.metadata.deletion_timestamp.is_some()` chain
    /// was hand-authored at TWO sites past the ★★ PRIME-DIRECTIVE
    /// ≥ 2 duplication threshold in `tatara-reconciler`, both
    /// projecting the SAME tombstone-presence predicate on a
    /// `Process` value:
    /// * `controller::reconcile` — the top-level dispatcher's
    ///   deletion-preempt gate that forces the SIGTERM cascade
    ///   (`→ Exiting`) as soon as the API server stamps the
    ///   tombstone, before the phase handler for the current
    ///   [`ProcessPhase`] gets a chance to run. Composed with
    ///   [`ProcessPhase::is_alive`] so the preempt only fires on a
    ///   Process still in an alive phase — a Process already in
    ///   `Zombie` / `Reaped` / `Failed` runs its normal handler.
    /// * `phase_machine::handle_exiting` — the SIGTERM cascade's
    ///   child-fan-out loop that enumerates every child Process and
    ///   skips ones the API server has already tombstoned (so the
    ///   reconciler does not re-issue a `DELETE` against a child
    ///   whose deletion the API server is already draining through
    ///   its own finalizer). The skip composes with
    ///   [`Self::coordinates_or_none`]'s name-required probe so a
    ///   child missing either its tombstone-absent gate or its
    ///   `metadata.name` slot is a clean `continue` rather than an
    ///   attempted `child_api.delete("")` no-op.
    ///
    /// Both sites walked the SAME `.metadata.deletion_timestamp
    /// .is_some()` chain and both wanted the `bool` form the
    /// primitive returns — the `controller::reconcile` site to gate
    /// the SIGTERM preempt with `&& current_phase.is_alive()` and
    /// the `handle_exiting` site to gate the DELETE-skip with a
    /// bare `if child.is_being_deleted() { continue; }`. Post-lift
    /// each callsite reads `process.is_being_deleted()` and the
    /// produced `bool` feeds the same downstream gate unchanged.
    ///
    /// Return-form axis: `bool` matches the copy-form discipline of
    /// [`Self::observed_phase`] (an `Option<Copy>` scalar) — the
    /// underlying slot is a wire-format `Option<Time>` that carries
    /// only presence information at this axis (the RFC-3339 timestamp
    /// payload itself is not what the two consumers read; both only
    /// probe presence to detect the tombstone-stamped state).
    /// Returning the raw `Option<&Time>` would push the `.is_some()`
    /// probe back to every callsite, restating the pre-lift chain
    /// one link shorter without collapsing the primitive.
    ///
    /// Peer to the metadata-fallback primitives
    /// [`Self::namespace_or_default`], [`Self::name_or_placeholder`],
    /// [`Self::uid_or_empty`], [`Self::coordinates_or_defaults`],
    /// [`Self::coordinates_or_none`], [`Self::owned_coordinates_or_err`],
    /// [`Self::annotation`] on the metadata axis; this method opens
    /// the copy-form peer for the presence-probe corner. Future
    /// metadata-presence projections (an `is_being_finalized`
    /// projection on `metadata.finalizers.is_empty()`'s negation,
    /// a `has_owner` projection on `metadata.owner_references.is_empty()`'s
    /// negation) land as peer methods on this same axis.
    ///
    /// A future normalization step (a per-tombstone staleness gate
    /// that returns `false` for a tombstone older than the reconciler's
    /// grace-period budget, a canonicalization pass that treats a
    /// tombstone from a paused controller as absent, a cross-cluster
    /// tombstone-observation clock skew guard) lands at ONE substrate
    /// method here and both downstream consumers pick up the upgrade
    /// mechanically — no per-callsite hand-edit at
    /// `controller::reconcile` / `phase_machine::handle_exiting`.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.metadata.deletion_timestamp.is_some()` chain recurred at
    /// two hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the missing-tombstone corner + the present-
    /// tombstone corner + the copy-form `bool` return + the byte-
    /// identical parity with the pre-lift `.is_some()` chain, so a
    /// regression that drifted any surface at
    /// `tests::is_being_deleted_*` rather than as silent operator-
    /// facing skew between the top-level dispatcher's SIGTERM
    /// preempt and the SIGTERM cascade's child-fan-out DELETE-skip
    /// on the SAME `Process` within one reconcile pass).
    pub fn is_being_deleted(&self) -> bool {
        self.metadata.deletion_timestamp.is_some()
    }

    /// Copy-form metadata-projection primitive on the
    /// `metadata.creationTimestamp` axis: returns the K8s-API-server-
    /// assigned creation moment as a `DateTime<Utc>`, hiding the wire-
    /// format `k8s_openapi::apimachinery::pkg::apis::meta::v1::Time`
    /// newtype behind an inherent projection — the ONE-liner collapse
    /// of the paired `self.metadata.creation_timestamp.as_ref().map(|t|
    /// t.0)` incantation every timestamp-driven consumer restated by
    /// hand pre-lift.
    ///
    /// Pre-lift the paired `.metadata.creation_timestamp.as_ref()` +
    /// `t.0` unwrap chain was hand-authored at THREE sites past the
    /// ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold across the
    /// workspace, all projecting the SAME creation-moment `DateTime<Utc>`
    /// on a `Process`:
    /// * `tatara-process::lifetime_clock::evaluate` — TTL-expiry gate
    ///   in the ephemeral-lifetime decision (`elapsed = now
    ///   .signed_duration_since(creation.0)`), inside the non-terminal-
    ///   phase guard that fires the `AutoTerminate::Now { TtlExpired }`
    ///   branch. Pre-lift the site read `if let Some(creation) = process
    ///   .metadata.creation_timestamp.as_ref() { ... creation.0 ... }`.
    /// * `tatara-process::lifetime_clock::requeue_with_ttl` — sleep-
    ///   budget picker for the reconciler's next requeue, choosing the
    ///   smaller of HEARTBEAT and TTL-remaining so the reconciler
    ///   doesn't oversleep past a TTL boundary. Pre-lift the site read
    ///   `let Some(creation) = process.metadata.creation_timestamp
    ///   .as_ref() else { return default; };` + `creation.0`.
    /// * `tatara-reconciler::table_controller::reconcile_process_table`
    ///   — stable-name claim-arbiter row builder, seeding each
    ///   candidate row's `created_at` for the tie-break ordering
    ///   (oldest wins). Pre-lift the site read `p.metadata
    ///   .creation_timestamp.as_ref().map(|t| t.0).unwrap_or_else(Utc
    ///   ::now)`.
    ///
    /// All THREE sites walked the SAME two-link chain — read the
    /// `Option<Time>` slot as a borrow, then unwrap the `Time` newtype
    /// to its inner `DateTime<Utc>` — differing only in the tail
    /// (`if-let-Some` guard, `let-else` short-circuit, `Utc::now`
    /// fallback). Post-lift each callsite reads
    /// `process.created_at()` and applies its own tail at its own site
    /// (`if let Some(creation) = ...`, `let Some(creation) = ... else`,
    /// `.unwrap_or_else(Utc::now)`).
    ///
    /// Return-form axis: `Option<DateTime<Utc>>` matches the copy-form
    /// discipline of the sibling status-projection primitive
    /// [`Self::observed_phase`] — both return `Option<T>` where `T:
    /// Copy` and hide the wire-format wrapper (`ProcessStatus` on the
    /// status side; `Time` on the metadata side). Returning the raw
    /// `Option<&Time>` would push the `.0` unwrap back to every
    /// callsite, restating the pre-lift chain one link shorter without
    /// collapsing the primitive; returning owned `Option<Time>` would
    /// force a `Time` import at every consumer for a projection every
    /// consumer immediately discards past `.0`.
    ///
    /// Peer to the metadata-fallback + presence-probe primitives
    /// [`Self::namespace_or_default`], [`Self::name_or_placeholder`],
    /// [`Self::uid_or_empty`], [`Self::coordinates_or_defaults`],
    /// [`Self::coordinates_or_none`], [`Self::owned_coordinates_or_err`],
    /// [`Self::annotation`], [`Self::is_being_deleted`] on the metadata
    /// axis; this method opens the copy-form timestamp corner. Future
    /// metadata-timestamp projections (a
    /// `deletion_at() -> Option<DateTime<Utc>>` peer on the
    /// tombstone-payload axis for staleness gates that need the
    /// timestamp value alongside the presence bit) land as peer
    /// methods on this same axis.
    ///
    /// A future normalization step (a per-cluster clock-skew guard
    /// that offsets the returned timestamp by the observing controller's
    /// measured skew, a canonicalization pass that maps a suspiciously-
    /// zero creation moment to `None`, a per-namespace override that
    /// substitutes a `spec.identity`-declared creation anchor for the
    /// metadata slot on adopted resources) lands at ONE substrate
    /// method here and all three downstream consumers pick up the
    /// upgrade mechanically — no per-callsite hand-edit at `evaluate`
    /// / `requeue_with_ttl` / `reconcile_process_table`.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.metadata.creation_timestamp.as_ref().map(|t| t.0)` chain
    /// recurred at three hand-authored sites past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to ONE
    /// owner here). THEORY.md §II.1 invariant 5 (composition preserves
    /// proofs — the pins bind the missing-timestamp corner + the
    /// present-timestamp corner + the copy-form `DateTime<Utc>` return
    /// + the byte-identical parity with the pre-lift `.as_ref().map(|t|
    /// t.0)` chain, so a regression that drifted any surface at
    /// `tests::created_at_*` rather than as silent operator-facing
    /// skew between the TTL-expiry gate, the requeue-budget picker,
    /// and the stable-name claim-arbiter tie-break on the SAME
    /// `Process` within one reconcile pass).
    pub fn created_at(&self) -> Option<DateTime<Utc>> {
        self.metadata.creation_timestamp.as_ref().map(|t| t.0)
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

    // ─── Process::qualified_ref substrate pins ─────────────────────────
    //
    // Pins the paired-projection + shape-composer chain
    // `coordinates_or_defaults() → qualified_process_ref(ns, name)` on
    // the (return-form × composition-depth) axis pair. Fail-before-
    // pass-after granularity: a regression that swapped the `<ns>/<name>`
    // axis order, dropped either half, drifted the fallback strings
    // between the paired-projection primitive and the shape composer, or
    // inserted a normalization step at only the composed site and not
    // the pair-returning primitive (or vice versa) surfaces here rather
    // than as silent operator-visible skew across the three pre-lift
    // `tatara-reconciler` sites (`render::render_routing`,
    // `render::render_export_jobs`, `table_controller::reconcile`)
    // whose downstream greps the reference shape verbatim (the
    // `PROCESS=<ref>` annotation seed on every emitted Ingress /
    // DNSEndpoint / export Job, the `ClaimRecord.holder` slot on the
    // stable-name claim registry).

    #[test]
    fn qualified_ref_composes_ns_and_name_with_slash_when_both_slots_present() {
        // Happy path — both metadata slots populated. The composed
        // reference is EXACTLY `<ns>/<name>`, in that order, joined by
        // a single `/`. A regression that swapped the two axes at
        // this primitive would silently break every downstream
        // `PROCESS=<ref>` annotation grep + claim-registry lookup.
        let mut p = Process::new("api-gateway", empty_spec());
        p.metadata.namespace = Some("prod-app".into());
        assert_eq!(p.qualified_ref(), "prod-app/api-gateway");
    }

    #[test]
    fn qualified_ref_falls_back_to_default_namespace_when_metadata_namespace_is_none() {
        // Namespace-fallback pin: an absent `metadata.namespace` rides
        // through `namespace_or_default()` → `DEFAULT_NAMESPACE`, so
        // the composed reference lands as `default/<name>`. Matches
        // what a pre-lift `qualified_process_ref(process.
        // coordinates_or_defaults())` composition produced.
        let mut p = Process::new("api-gateway", empty_spec());
        p.metadata.namespace = None;
        assert_eq!(p.qualified_ref(), "default/api-gateway");
    }

    #[test]
    fn qualified_ref_falls_back_to_unnamed_placeholder_when_metadata_name_is_none() {
        // Name-fallback pin: an absent `metadata.name` rides through
        // `name_or_placeholder()` → `UNNAMED_PLACEHOLDER`, so the
        // composed reference lands as `<ns>/unnamed`. A pre-lift
        // consumer whose paired projection returned the placeholder
        // (annotation writer, render owner-metadata seed) sees the
        // exact same `<ns>/unnamed` shape post-lift, so downstream
        // greps keyed on the pre-metadata Process's reference match
        // bytewise.
        let mut p = Process::new("ignored", empty_spec());
        p.metadata.namespace = Some("staging".into());
        p.metadata.name = None;
        assert_eq!(p.qualified_ref(), "staging/unnamed");
    }

    #[test]
    fn qualified_ref_falls_back_on_both_slots_when_both_metadata_are_none() {
        // Both slots absent → both fallbacks land in the composed
        // reference. The `default/unnamed` shape is what every pre-
        // lift caller produced when a Process fixture (test or
        // dynamic API response) surfaced without populated metadata;
        // pinning it here holds the primitive's contract against a
        // regression that dropped either fallback at only the
        // composed site.
        let mut p = Process::new("ignored", empty_spec());
        p.metadata.namespace = None;
        p.metadata.name = None;
        assert_eq!(
            p.qualified_ref(),
            format!(
                "{}/{}",
                Process::DEFAULT_NAMESPACE,
                Process::UNNAMED_PLACEHOLDER
            )
        );
    }

    #[test]
    fn qualified_ref_matches_pre_lift_paired_composition_bytewise() {
        // Byte-identical parity with the exact pre-lift 2-step
        // composition every `tatara-reconciler` site hand-authored:
        // `let (ns, name) = process.coordinates_or_defaults(); let r
        // = qualified_process_ref(ns, name);`. Sweeps every metadata-
        // slot combination the three pre-lift consumers plausibly
        // encountered — both slots populated (steady state), one
        // slot absent (Process mid-fork before API-server metadata
        // stamp), both slots absent (dynamic API response / test
        // fixture) — so a regression that reshaped the composition at
        // the substrate primitive would surface here rather than as
        // silent drift at the three consumer sites.
        let fixtures: [(Option<&str>, Option<&str>); 4] = [
            (Some("prod-app"), Some("api-gateway")),
            (None, Some("api-gateway")),
            (Some("staging"), None),
            (None, None),
        ];
        for (ns_slot, name_slot) in fixtures {
            let mut p = Process::new(name_slot.unwrap_or("seed"), empty_spec());
            p.metadata.namespace = ns_slot.map(str::to_string);
            p.metadata.name = name_slot.map(str::to_string);
            let via_primitive = p.qualified_ref();
            let (ns, name) = p.coordinates_or_defaults();
            let via_paired = crate::qualified_process_ref(ns, name);
            assert_eq!(
                via_primitive, via_paired,
                "qualified_ref must be byte-identical to the pre-lift \
                 paired composition on (ns={ns_slot:?}, name={name_slot:?})"
            );
        }
    }

    #[test]
    fn qualified_ref_composes_from_the_shared_coordinates_or_defaults_owner() {
        // Composition invariant: the composed reference decomposes at
        // the single `/` separator into EXACTLY the (ns, name) pair
        // `coordinates_or_defaults` returns. A regression that
        // introduced a per-callsite normalization at the shape
        // composer (URL-escape, case-fold, path-normalize) or that
        // pulled the pair from a different metadata source than the
        // paired-projection primitive would surface here rather than
        // at every downstream reference-shape grep.
        let mut p = Process::new("api-gateway", empty_spec());
        p.metadata.namespace = Some("prod-app".into());
        let composed = p.qualified_ref();
        let (ns, name) = p.coordinates_or_defaults();
        let (composed_ns, composed_name) = composed.split_once('/').unwrap();
        assert_eq!(composed_ns, ns);
        assert_eq!(composed_name, name);
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

    // ─── Process::coordinates_or_none substrate pins ──────────────────
    //
    // Pins the borrow + name-required peer of the coordinate-primitive
    // family on the (return-form × name-gate) axis pair. Closes the
    // corner previously left open (borrow + name-required) so the
    // three consumer shapes (child-Process delete-fan-out at
    // `phase_machine::handle_exiting`, claim-arbiter probe at
    // `phase_machine::process_holds_any_claim`, any future non-fatal
    // skip site) route through ONE primitive rather than three hand-
    // authored empty-string / `unwrap_or_default()` sentinel chains.
    // Fail-before-pass-after granularity: a regression that flipped
    // the namespace fallback, swapped the return-tuple axis order,
    // returned an owned form, or promoted a missing name to an error
    // rather than `None` surfaces here rather than as silent drift at
    // every borrow + name-required consumer.

    #[test]
    fn coordinates_or_none_returns_slices_when_both_slots_present() {
        // Happy path — both slots populated, method returns borrowed
        // (&str, &str) in (namespace, name) axis order wrapped in
        // `Some`.
        let mut p = Process::new("api-gateway", empty_spec());
        p.metadata.namespace = Some("prod-app".into());
        let (ns, name) = p.coordinates_or_none().expect("Some when name set");
        assert_eq!(ns, "prod-app");
        assert_eq!(name, "api-gateway");
    }

    #[test]
    fn coordinates_or_none_falls_back_on_namespace_but_returns_name_slice() {
        // Namespace absent → DEFAULT_NAMESPACE (shared with the peer
        // `coordinates_or_defaults` + `namespace_or_default`). Name
        // present → the metadata slice, wrapped in `Some`.
        let mut p = Process::new("api", empty_spec());
        p.metadata.namespace = None;
        let (ns, name) = p.coordinates_or_none().expect("Some when name set");
        assert_eq!(ns, Process::DEFAULT_NAMESPACE);
        assert_eq!(name, "api");
    }

    #[test]
    fn coordinates_or_none_returns_none_when_metadata_name_absent_regardless_of_namespace() {
        // Name absent → `None`, REGARDLESS of whether the namespace
        // slot is populated. The name gate is strictly on
        // `metadata.name` and does NOT fall back to
        // `Self::UNNAMED_PLACEHOLDER` (that fallback is on the peer
        // `coordinates_or_defaults`, which exists precisely for
        // consumers that tolerate a display placeholder). Peer to
        // `owned_coordinates_or_err_errors_when_metadata_name_absent_regardless_of_namespace`
        // on the sibling primitive; a regression that widened THIS
        // form to substitute the placeholder while leaving the owned
        // form strict would silently drift the two borrow-form
        // primitives out of the coherence the family carries.
        for ns_slot in [None, Some("prod".to_string())] {
            let mut p = Process::new("scratch", empty_spec());
            p.metadata.name = None;
            p.metadata.namespace = ns_slot.clone();
            assert!(
                p.coordinates_or_none().is_none(),
                "coordinates_or_none must be None on missing name (ns={ns_slot:?})",
            );
        }
    }

    #[test]
    fn coordinates_or_none_namespace_fallback_matches_default_namespace_const() {
        // Byte-identity pin between the borrow + name-required form's
        // namespace fallback and the workspace-wide `DEFAULT_NAMESPACE`
        // const. Sibling to
        // `owned_coordinates_or_err_namespace_fallback_matches_default_namespace_const`
        // on the peer primitive — the two forms MUST substitute the
        // same fallback string, else a consumer that switches between
        // them based on its ownership need silently observes a
        // different namespace-fallback shape as a side effect.
        let mut p = Process::new("api", empty_spec());
        p.metadata.namespace = None;
        let (ns, _) = p.coordinates_or_none().unwrap();
        assert_eq!(ns, Process::DEFAULT_NAMESPACE);
    }

    #[test]
    fn coordinates_or_none_axis_order_matches_coordinates_or_defaults_when_name_present() {
        // Cross-primitive coherence pin between the two borrow-form
        // primitives: when the name is present, the (namespace, name)
        // return-tuple axis order is IDENTICAL across the two forms,
        // and the returned slices are the SAME `&str` view onto the
        // same metadata slots. A regression that swapped the tuple
        // slots on ONE form would silently misroute every consumer
        // that picked between the two forms based on its name-gate
        // need. The pin re-reads both primitives at test time so the
        // equality holds iff both live paths are the current
        // implementation.
        let mut p = Process::new("app", empty_spec());
        p.metadata.namespace = Some("infra".into());
        let (defaulted_ns, defaulted_name) = p.coordinates_or_defaults();
        let (required_ns, required_name) = p.coordinates_or_none().unwrap();
        assert_eq!(defaulted_ns, required_ns);
        assert_eq!(defaulted_name, required_name);
        // Explicit slot labels — pins the (namespace, name) axis order
        // as opposed to (name, namespace).
        assert_eq!(required_ns, "infra"); // NOT "app"
        assert_eq!(required_name, "app"); // NOT "infra"
    }

    #[test]
    fn coordinates_or_none_axis_pair_diverges_from_coordinates_or_defaults_on_missing_name() {
        // Divergence pin between the two borrow-form primitives when
        // the name gate fires: `coordinates_or_defaults` substitutes
        // the display placeholder AND still returns a tuple;
        // `coordinates_or_none` returns `None`. A regression that
        // collapsed the two behaviors (either by dropping the gate
        // from the required form or by adding a `None` corner to the
        // defaulted form) would blur the axis pair's whole reason to
        // exist as two peer primitives.
        let mut p = Process::new("scratch", empty_spec());
        p.metadata.name = None;
        p.metadata.namespace = Some("prod".into());
        // Defaulted form: substitutes placeholder, no gate.
        assert_eq!(
            p.coordinates_or_defaults(),
            ("prod", Process::UNNAMED_PLACEHOLDER)
        );
        // Required form: gate fires, `None`.
        assert!(p.coordinates_or_none().is_none());
    }

    #[test]
    fn coordinates_or_none_matches_pre_lift_reconciler_helper_shape() {
        // Byte-identical parity pin between the borrow + name-required
        // primitive here and the pre-lift `tatara-reconciler` helper
        // shapes — the exact 2-slot unwrap + gate chains each pre-lift
        // caller spelled by hand (`phase_machine::process_holds_any_claim`
        // spelled it as `unwrap_or("")` + `is_empty` early-return;
        // `phase_machine::handle_exiting`'s child-fan-out spelled it
        // as `unwrap_or_default()` + implicit no-op delete on the
        // empty API-path). Sweeps every corner every callsite plausibly
        // encounters (both slots present, namespace absent, name
        // absent + ns present, both absent). A regression that
        // inserted a normalization step at the primitive the pre-lift
        // chain does NOT apply — or vice versa — surfaces here rather
        // than as silent drift between the pre-lift consumer sites
        // and the ONE substrate owner they now route through.
        fn pre_lift_holds_any_claim(p: &Process) -> Option<(&str, &str)> {
            let ns = p.metadata.namespace.as_deref().unwrap_or("default");
            let name = p.metadata.name.as_deref().unwrap_or("");
            if name.is_empty() {
                return None;
            }
            Some((ns, name))
        }
        // Both present.
        let mut p = Process::new("api", empty_spec());
        p.metadata.namespace = Some("prod".into());
        assert_eq!(p.coordinates_or_none(), pre_lift_holds_any_claim(&p));
        // Namespace absent.
        let p = Process::new("api", empty_spec());
        assert_eq!(p.coordinates_or_none(), pre_lift_holds_any_claim(&p));
        // Name absent → both variants return `None` regardless of ns.
        let mut p = Process::new("api", empty_spec());
        p.metadata.name = None;
        p.metadata.namespace = Some("prod".into());
        assert_eq!(p.coordinates_or_none(), pre_lift_holds_any_claim(&p));
        // Both absent → still `None` on the name gate.
        let mut p = Process::new("api", empty_spec());
        p.metadata.name = None;
        p.metadata.namespace = None;
        assert_eq!(p.coordinates_or_none(), pre_lift_holds_any_claim(&p));
    }

    #[test]
    fn coordinates_or_none_axis_order_matches_owned_coordinates_or_err_on_happy_path() {
        // Cross-primitive coherence pin at the sibling corner: when
        // BOTH slots are present, the borrow + name-required form
        // (this method) and the owned + name-required peer
        // (`owned_coordinates_or_err`) return the SAME `(ns, name)`
        // pair — the axis order is IDENTICAL and neither primitive
        // silently applies a normalization the other omits. A
        // regression that skewed one form's normalization would
        // surface here rather than as silent drift between the two
        // name-required corners of the primitive family.
        let mut p = Process::new("app", empty_spec());
        p.metadata.namespace = Some("infra".into());
        let (borrow_ns, borrow_name) = p.coordinates_or_none().unwrap();
        let (owned_ns, owned_name) = p.owned_coordinates_or_err().unwrap();
        assert_eq!(borrow_ns, owned_ns.as_str());
        assert_eq!(borrow_name, owned_name.as_str());
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

    // ─── Process::annotation substrate pins ────────────────────────────
    //
    // Pins the borrow-form annotation-lookup primitive that owns the
    // 3-line `.metadata.annotations.as_ref().and_then(|m| m.get(KEY))`
    // chain three hand-authored sites restated by hand pre-lift:
    // `tatara-reconciler::signals::ingest` (SIGNAL),
    // `tatara-reconciler::phase_machine::released_from_annotation`
    // (RELEASED_FROM), and
    // `tatara-pool-reconciler::controller_pool::process_belongs_to_pool`
    // (POOL). Fail-before-pass-after granularity: a regression that
    // widened the missing-`annotations` corner (returning `Some("")`
    // instead of `None`), promoted a missing key to an error, dropped
    // the borrow-form return, or changed the two swallowed corners'
    // shared collapse to `None` surfaces here rather than as silent
    // drift at the three consumer sites.
    fn process_with_annotation(key: &str, value: &str) -> Process {
        let mut p = Process::new("some-proc", empty_spec());
        let mut anns = std::collections::BTreeMap::new();
        anns.insert(key.to_string(), value.to_string());
        p.metadata.annotations = Some(anns);
        p
    }

    #[test]
    fn annotation_returns_none_when_metadata_annotations_is_none() {
        // Missing-`annotations` corner: a Process with no annotations
        // block at all returns `None` for every key. Peer to
        // `observed_flux_resources_returns_empty_slice_when_status_is_none`
        // on the status-projection axis; both primitives collapse the
        // outer `Option` corner rather than requiring each consumer
        // to spell the guard by hand.
        let mut p = Process::new("scratch", empty_spec());
        p.metadata.annotations = None;
        assert!(p.annotation("tatara.pleme.io/signal").is_none());
        assert!(p.annotation("tatara.pleme.io/pool").is_none());
        assert!(p.annotation("").is_none());
    }

    #[test]
    fn annotation_returns_none_when_key_absent_from_populated_map() {
        // Missing-key corner: annotations block populated with OTHER
        // keys returns `None` for the queried key. Symmetric with the
        // missing-`annotations` corner — both corners collapse to the
        // same `None`, matching the pre-lift `.and_then(...)`
        // behavior every consumer relied on.
        let p = process_with_annotation("tatara.pleme.io/other", "value");
        assert!(p.annotation("tatara.pleme.io/signal").is_none());
        assert!(p.annotation("").is_none());
    }

    #[test]
    fn annotation_returns_borrowed_slice_when_key_present() {
        // Happy path: annotations block populated + key present →
        // `Some(&str)` borrowed from the underlying `String` in the
        // map. A regression that returned an owned `String` (defeating
        // the primitive's role as a zero-copy projection) would
        // surface at the lifetime of the returned reference — the
        // `&str` outlives the borrow of `&p` here.
        let p = process_with_annotation("tatara.pleme.io/signal", "SIGHUP");
        assert_eq!(p.annotation("tatara.pleme.io/signal"), Some("SIGHUP"));
    }

    #[test]
    fn annotation_returns_borrowed_empty_string_slice_when_value_is_empty() {
        // Edge corner between the missing-key `None` and the present-
        // key `Some("")` — a Process whose annotation is EXPLICITLY
        // set to an empty string returns `Some("")`, NOT `None`. A
        // regression that normalized the empty-string value to `None`
        // (a plausible "defensive" simplification) would silently
        // reshape the corner every callsite pre-lift kept distinct via
        // `.cloned().unwrap_or_default()` (which collapses BOTH to
        // `""`) or `.map(String::as_str)` (which keeps them distinct
        // as `None` vs `Some("")`).
        let p = process_with_annotation("tatara.pleme.io/signal", "");
        assert_eq!(p.annotation("tatara.pleme.io/signal"), Some(""));
    }

    #[test]
    fn annotation_is_a_pure_projection() {
        // Purity pin — repeated calls return equal results and the
        // primitive does not mutate `self`. Peer to
        // `observed_flux_resources_is_a_pure_projection` on the
        // status-projection axis.
        let p = process_with_annotation("tatara.pleme.io/released-from", "Attested");
        let a = p.annotation("tatara.pleme.io/released-from");
        let b = p.annotation("tatara.pleme.io/released-from");
        assert_eq!(a, b);
        assert_eq!(a, Some("Attested"));
    }

    #[test]
    fn annotation_matches_pre_lift_reconciler_chain_shape() {
        // Byte-identical parity pin between the borrow-form primitive
        // here and the pre-lift `tatara-reconciler` / `tatara-pool-
        // reconciler` chain shape — the exact 3-line
        // `.metadata.annotations.as_ref().and_then(|m| m.get(KEY))
        // .map(String::as_str)` incantation each pre-lift caller
        // spelled by hand (three variants of tail collapsed onto ONE
        // borrow-form primitive here; each caller reapplies its own
        // tail at its own site). Sweeps every corner (missing
        // annotations map, missing key, present key with value,
        // present key with empty value) so a regression that inserted
        // a normalization at the primitive the pre-lift chain does
        // NOT apply — or vice versa — surfaces here rather than as
        // silent drift between the ONE substrate owner and the three
        // consumer sites.
        fn pre_lift<'a>(p: &'a Process, key: &str) -> Option<&'a str> {
            p.metadata
                .annotations
                .as_ref()
                .and_then(|m| m.get(key))
                .map(String::as_str)
        }
        // Missing annotations map.
        let mut p = Process::new("x", empty_spec());
        p.metadata.annotations = None;
        assert_eq!(p.annotation("k"), pre_lift(&p, "k"));
        // Missing key in populated map.
        let p = process_with_annotation("other", "v");
        assert_eq!(p.annotation("k"), pre_lift(&p, "k"));
        // Present key with non-empty value.
        let p = process_with_annotation("k", "v");
        assert_eq!(p.annotation("k"), pre_lift(&p, "k"));
        // Present key with explicitly-empty value — the corner
        // `.cloned().unwrap_or_default()` collapses to `""` post-tail
        // but the primitive-level shape stays `Some("")`.
        let p = process_with_annotation("k", "");
        assert_eq!(p.annotation("k"), pre_lift(&p, "k"));
    }

    #[test]
    fn annotation_composes_owned_tail_matching_pre_lift_signals_ingest() {
        // Pins the exact tail shape `tatara-reconciler::signals::
        // ingest` composed pre-lift: an `Option<String>` for the
        // downstream `let Some(raw) = raw else { ... }` guard.
        // Post-lift the callsite composes `.map(str::to_string)` at
        // its own site; this test pins the composition matches the
        // pre-lift `.cloned()` tail byte-for-byte on both corners the
        // consumer's downstream distinguishes (annotation present →
        // `Some(String)`; absent → `None`).
        let p = process_with_annotation("tatara.pleme.io/signal", "SIGUSR1");
        assert_eq!(
            p.annotation("tatara.pleme.io/signal").map(str::to_string),
            Some("SIGUSR1".to_string())
        );
        let mut q = Process::new("y", empty_spec());
        q.metadata.annotations = None;
        assert_eq!(
            q.annotation("tatara.pleme.io/signal").map(str::to_string),
            None
        );
    }

    #[test]
    fn annotation_composes_default_tail_matching_pre_lift_released_from() {
        // Pins the exact tail shape
        // `tatara-reconciler::phase_machine::released_from_annotation`
        // composed pre-lift: a bare `String` via `.cloned()
        // .unwrap_or_default()` for the downstream
        // `match v.as_str()` dispatch. Post-lift the callsite matches
        // directly on `Option<&str>` (Some("Failed") vs _); this test
        // pins that the borrow-form primitive plus the `.unwrap_or("")`
        // fallback reproduces the pre-lift bare-string shape on both
        // corners.
        let p = process_with_annotation("tatara.pleme.io/released-from", "Failed");
        assert_eq!(
            p.annotation("tatara.pleme.io/released-from").unwrap_or(""),
            "Failed"
        );
        let mut q = Process::new("y", empty_spec());
        q.metadata.annotations = None;
        assert_eq!(
            q.annotation("tatara.pleme.io/released-from").unwrap_or(""),
            ""
        );
    }

    #[test]
    fn annotation_composes_borrow_equality_tail_matching_pre_lift_pool() {
        // Pins the exact tail shape `tatara-pool-reconciler::
        // controller_pool::process_belongs_to_pool` composed pre-lift:
        // an `Option<&str>` compared with `== Some(pool_name)` for the
        // membership gate. Post-lift the callsite composes
        // `p.annotation(POOL) == Some(pool_name)` verbatim; this test
        // pins that the borrow-form primitive returns exactly the
        // shape the equality gate expects.
        let p = process_with_annotation("tatara.pleme.io/pool", "demo-pool");
        assert_eq!(
            p.annotation("tatara.pleme.io/pool") == Some("demo-pool"),
            true
        );
        assert_eq!(p.annotation("tatara.pleme.io/pool") == Some("other"), false);
    }

    // ─── Process::uid_or_empty substrate pins ──────────────────────────
    //
    // Pins the borrow-form metadata-projection primitive on the
    // `metadata.uid` axis that owns the `.metadata.uid.as_deref()
    // .unwrap_or("")` chain the two hand-authored
    // `tatara-reconciler::render` sites (`render_routing` +
    // `render_export_jobs`) restated by hand pre-lift. Peer to the
    // sibling `namespace_or_default_*` + `name_or_placeholder_*` pin
    // families on the metadata-slot × fallback-shape axis; all three
    // primitives return borrows of an owned-metadata slot with a slot-
    // specific fallback baked in (`"default"` for namespace, `"unnamed"`
    // for name, `""` for uid — the load-bearing gate value for
    // `owner_references_json`'s `is_empty` check). Fail-before-pass-
    // after granularity: `uid_or_empty` did not exist pre-lift, so any
    // test invoking it fails to compile pre-lift and passes post-lift.

    #[test]
    fn uid_or_empty_returns_empty_string_when_metadata_uid_is_none() {
        // Empty-slot corner pin: the primitive collapses the no-uid
        // case to `""`, matching the pre-lift `.as_deref().unwrap_or("")`
        // chain's `""` byte-identically at both render consumer sites.
        // Semantically corresponds to a Process pre-metadata (fixtured
        // in tests, or caught mid-Forking before the API server has
        // stamped a `uid`); the downstream `owner_references_json`
        // composer gates on this exact `""` sentinel to stamp
        // `metadata.ownerReferences: []` rather than emit an owner-ref
        // pointing at a placeholder uid.
        let mut p = Process::new("scratch", empty_spec());
        p.metadata.uid = None;
        assert_eq!(p.uid_or_empty(), "");
    }

    #[test]
    fn uid_or_empty_returns_borrowed_str_when_slot_is_populated() {
        // Happy-path pin: with a populated `metadata.uid` slot, the
        // primitive returns a borrowed `&str` whose contents match the
        // persisted `String`. A regression that reshaped / normalized
        // / cross-cluster-stripped the uid without touching this pin
        // would surface here rather than as silent skew at the two
        // `owner_references_json(name, uid)` emitters on the SAME
        // Process.
        let mut p = Process::new("owned-proc", empty_spec());
        p.metadata.uid = Some("uid-abc-123".into());
        assert_eq!(p.uid_or_empty(), "uid-abc-123");
    }

    #[test]
    fn uid_or_empty_returns_empty_string_when_slot_is_explicitly_empty_string() {
        // Corner between the missing-slot `None` and the explicitly-
        // empty-string `Some("")` — both collapse to `""` at the
        // primitive because the downstream gate at
        // `owner_references_json` treats `.is_empty()` uniformly (the
        // empty-slot posture is what the whole primitive family
        // encodes: "no admissible owner reference, stamp `[]`"). A
        // regression that discriminated the two corners (returning a
        // sentinel `"<none>"` for the missing slot but `""` for the
        // explicit slot) would break the composition with
        // `owner_references_json` at the exactly-two-corner gate.
        let mut p = Process::new("owned-proc", empty_spec());
        p.metadata.uid = Some(String::new());
        assert_eq!(p.uid_or_empty(), "");
    }

    #[test]
    fn uid_or_empty_is_a_zero_copy_borrow_projection() {
        // Borrow-discipline pin: the returned `&str` borrows the
        // persisted `String`'s underlying byte buffer in place — NOT
        // a fresh allocation or a clone. A regression that switched
        // the projection to an owned `String` (via `.clone()` or a
        // `format!` wrap) would defeat the zero-copy contract the
        // lift's primary strict-widening delivers, and would surface
        // here via pointer-identity comparison.
        let mut p = Process::new("owned-proc", empty_spec());
        p.metadata.uid = Some("uid-borrow-pin".into());
        let slice = p.uid_or_empty();
        assert!(std::ptr::eq(
            slice.as_ptr(),
            p.metadata.uid.as_ref().unwrap().as_ptr()
        ));
    }

    #[test]
    fn uid_or_empty_is_a_pure_projection() {
        // Purity pin — repeated calls return byte-identical slices
        // (same pointer, same length). A regression that introduced
        // state (a lazy-cached normalized slot, a first-call
        // canonicalization pass) would surface here rather than as
        // silent drift between the two render consumer sites on the
        // SAME Process within one render pass.
        let mut p = Process::new("owned-proc", empty_spec());
        p.metadata.uid = Some("uid-pure".into());
        let a = p.uid_or_empty();
        let b = p.uid_or_empty();
        assert!(std::ptr::eq(a.as_ptr(), b.as_ptr()));
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn uid_or_empty_matches_pre_lift_render_chain_shape() {
        // Byte-identical parity pin between the borrow-form primitive
        // here and the pre-lift `tatara-reconciler::render` chain shape
        // — the exact `.metadata.uid.as_deref().unwrap_or("")`
        // incantation both `render_routing` (line 514) and
        // `render_export_jobs` (line 653) spelled by hand pre-lift.
        // Sweeps every corner (missing uid slot, populated uid slot,
        // explicitly-empty uid slot) so a regression that inserted a
        // normalization the pre-lift chain does NOT apply — or vice
        // versa — surfaces here rather than as silent drift between
        // the ONE substrate owner and the two consumer sites.
        fn pre_lift(p: &Process) -> &str {
            p.metadata.uid.as_deref().unwrap_or("")
        }
        // Missing slot.
        let mut p = Process::new("x", empty_spec());
        p.metadata.uid = None;
        assert_eq!(p.uid_or_empty(), pre_lift(&p));
        // Populated slot.
        let mut p = Process::new("x", empty_spec());
        p.metadata.uid = Some("uid-42".into());
        assert_eq!(p.uid_or_empty(), pre_lift(&p));
        // Explicitly-empty slot.
        let mut p = Process::new("x", empty_spec());
        p.metadata.uid = Some(String::new());
        assert_eq!(p.uid_or_empty(), pre_lift(&p));
    }

    #[test]
    fn uid_or_empty_composes_with_owner_references_json_empty_gate() {
        // Cross-primitive composition pin — the empty-string sentinel
        // this primitive returns for the missing-uid corner is EXACTLY
        // the sentinel the sibling substrate composer
        // `owner_references_json(name, uid)` gates on to stamp
        // `metadata.ownerReferences: []`. A regression that changed
        // the sentinel at either end (this primitive returning
        // `"<none>"`, `owner_references_json` gating on `uid == "0"`
        // instead of `uid.is_empty()`) would break the composition
        // and surface here rather than as an operator-observed
        // orphan resource after apply.
        let mut p = Process::new("x", empty_spec());
        p.metadata.uid = None;
        let refs = crate::owner_references_json("some-name", p.uid_or_empty());
        assert!(
            refs.is_empty(),
            "empty-uid corner must produce empty owner-refs array"
        );

        p.metadata.uid = Some("real-uid".into());
        let refs = crate::owner_references_json("some-name", p.uid_or_empty());
        assert_eq!(
            refs.len(),
            1,
            "populated-uid corner must produce one owner-ref entry"
        );
    }

    // ─── Process::owned_name_or_empty substrate pins ─────────────────
    //
    // Pins the owned-form metadata-projection primitive on the
    // `metadata.name` axis that owns the
    // `.metadata.name.clone().unwrap_or_default()` chain the two hand-
    // authored `tatara-pool-reconciler::controller_pool` sites (the
    // `PoolMember` seed at line 68 + the `PoolMemberSnapshot` desired-
    // count seed at line 108) restated by hand pre-lift. Peer to the
    // sibling `uid_or_empty` pin family on the (return-form × fallback-
    // value) axis pair — `uid_or_empty` owns the BORROW + empty-sentinel
    // corner (`&str` for owner-ref emitters gating on `.is_empty()`);
    // this method owns the OWNED + empty-sentinel corner (`String` for
    // struct-literal / HashMap-key row-builder consumers whose
    // downstream fills a `String` field with the load-bearing `""`
    // sentinel). Fail-before-pass-after granularity: `owned_name_or_empty`
    // did not exist pre-lift, so any test invoking it fails to compile
    // pre-lift and passes post-lift.

    #[test]
    fn owned_name_or_empty_returns_empty_string_when_metadata_name_is_none() {
        // Empty-slot corner pin: the primitive collapses the no-name
        // case to `String::new()`, matching the pre-lift
        // `.clone().unwrap_or_default()` chain's empty `String` byte-
        // identically at both pool-reconciler consumer sites.
        // Semantically corresponds to a Process pre-metadata-name (test
        // fixture, dynamic API response pre-name-resolution); the
        // downstream `PoolMember { process_name, .. }` slot then holds
        // `""` as a stable "no name to key by" signal rather than a
        // display placeholder that would silently alias distinct rows.
        let mut p = Process::new("scratch", empty_spec());
        p.metadata.name = None;
        assert_eq!(p.owned_name_or_empty(), String::new());
    }

    #[test]
    fn owned_name_or_empty_returns_owned_string_when_slot_is_populated() {
        // Happy-path pin: with a populated `metadata.name` slot, the
        // primitive returns an owned `String` whose contents match the
        // persisted `String`. A regression that reshaped / normalized
        // / case-folded the name without touching this pin would surface
        // here rather than as silent skew between the two pool-member
        // seeds keying on the SAME Process's name.
        let p = Process::new("api", empty_spec());
        assert_eq!(p.owned_name_or_empty(), "api");
    }

    #[test]
    fn owned_name_or_empty_returns_empty_string_when_slot_is_explicitly_empty_string() {
        // Corner between the missing-slot `None` and the explicitly-
        // empty-string `Some(String::new())` — both collapse to `""` at
        // the primitive because the downstream pool-member consumers
        // treat both corners uniformly (no name, no key). A regression
        // that discriminated the two corners (returning a sentinel
        // `"<none>"` for the missing slot but `""` for the explicit
        // slot) would break `String::is_empty` gating at the row-builder
        // callsites without moving this pin.
        let mut p = Process::new("scratch", empty_spec());
        p.metadata.name = Some(String::new());
        assert_eq!(p.owned_name_or_empty(), String::new());
        assert!(p.owned_name_or_empty().is_empty());
    }

    #[test]
    fn owned_name_or_empty_is_a_pure_projection() {
        // Purity pin — repeated calls return byte-identical `String`
        // values. A regression that introduced state (a lazy-cached
        // normalized slot, a first-call canonicalization pass) would
        // surface here rather than as silent drift between the pool-
        // member seed and the desired-count snapshot seed on the SAME
        // Process within one reconcile pass.
        let p = Process::new("stable-name", empty_spec());
        assert_eq!(p.owned_name_or_empty(), p.owned_name_or_empty());
    }

    #[test]
    fn owned_name_or_empty_returns_independent_owned_string() {
        // Owned-discipline pin: the returned `String` is an independent
        // allocation the caller may consume, `.push_str` into, or move
        // into a struct-literal `process_name: String` slot — NOT a
        // shared reference into `metadata.name`. A regression that
        // switched the projection to a `Cow`-shaped variant or a slice-
        // form projection would defeat the owned-form contract the two
        // pool-reconciler struct-literal consumers depend on (a slice
        // cannot land in a `process_name: String` slot without a re-
        // clone), and would surface here at compile time via the mutate-
        // in-place test below.
        let p = Process::new("owned-proc", empty_spec());
        let mut owned = p.owned_name_or_empty();
        owned.push_str("-mutated");
        assert_eq!(owned, "owned-proc-mutated");
        // The Process's own slot is unchanged — the returned String
        // owns its own byte buffer, disjoint from `metadata.name`.
        assert_eq!(p.metadata.name.as_deref(), Some("owned-proc"));
    }

    #[test]
    fn owned_name_or_empty_matches_pre_lift_controller_pool_chain_shape() {
        // Byte-identical parity pin between the owned-form primitive
        // here and the pre-lift `tatara-pool-reconciler::controller_pool`
        // chain shape — the exact `.metadata.name.clone().unwrap_or_default()`
        // incantation both `PoolMember` seed (line 68) and
        // `PoolMemberSnapshot` seed (line 108) spelled by hand pre-lift.
        // Sweeps every corner (missing name slot, populated name slot,
        // explicitly-empty name slot) so a regression that inserted a
        // normalization the pre-lift chain does NOT apply — or vice
        // versa — surfaces here rather than as silent drift between
        // the ONE substrate owner and the two consumer sites.
        fn pre_lift(p: &Process) -> String {
            p.metadata.name.clone().unwrap_or_default()
        }
        // Missing slot.
        let mut p = Process::new("x", empty_spec());
        p.metadata.name = None;
        assert_eq!(p.owned_name_or_empty(), pre_lift(&p));
        // Populated slot.
        let p = Process::new("real-name", empty_spec());
        assert_eq!(p.owned_name_or_empty(), pre_lift(&p));
        // Explicitly-empty slot.
        let mut p = Process::new("x", empty_spec());
        p.metadata.name = Some(String::new());
        assert_eq!(p.owned_name_or_empty(), pre_lift(&p));
    }

    #[test]
    fn owned_name_or_empty_shares_empty_sentinel_with_uid_or_empty() {
        // Cross-primitive coherence pin — the empty-string fallback this
        // primitive returns for the missing-name corner is the SAME
        // sentinel the sibling borrow-form primitive `uid_or_empty`
        // returns for the missing-uid corner. Both partition the OWNED
        // × BORROW corner of the metadata-slot family on identical
        // fallback semantics ("the slot is unset"), so a consumer that
        // switches between them based on downstream ownership
        // requirements never sees a different missing-slot spelling as
        // a side effect. A regression that drifted either sentinel
        // (this primitive returning `"<unnamed>"`, `uid_or_empty`
        // returning `"<none>"`) would break the partition and surface
        // here rather than as silent shape drift across the family.
        let mut p = Process::new("scratch", empty_spec());
        p.metadata.name = None;
        p.metadata.uid = None;
        assert_eq!(p.owned_name_or_empty(), p.uid_or_empty());
        assert!(p.owned_name_or_empty().is_empty());
        assert!(p.uid_or_empty().is_empty());
    }

    #[test]
    fn owned_name_or_empty_returns_distinct_fallback_from_name_or_placeholder() {
        // Axis-partition pin — the owned + empty-sentinel primitive here
        // and the borrow + display-placeholder primitive
        // [`Self::name_or_placeholder`] MUST return distinct fallback
        // values on the missing-name corner. The distinction is load-
        // bearing: `owned_name_or_empty` is for HashMap-key / row-builder
        // consumers that need distinct keys for missing-name Processes
        // (empty string collides only with other missing-name rows,
        // never with a real "unnamed" Process); `name_or_placeholder`
        // is for log-line / display consumers that render the
        // `"unnamed"` word to operators. A regression that unified the
        // two fallbacks (either primitive returning the other's
        // sentinel) would silently collapse missing-name pool members
        // into a display-string key or expose the empty sentinel to
        // operator log lines. This pin catches either drift.
        let mut p = Process::new("scratch", empty_spec());
        p.metadata.name = None;
        assert_eq!(p.owned_name_or_empty(), "");
        assert_eq!(p.name_or_placeholder(), Process::UNNAMED_PLACEHOLDER);
        assert_ne!(p.owned_name_or_empty(), p.name_or_placeholder());
    }

    // ─── Process::declared_parent_pid substrate pins ─────────────────
    //
    // Pins the borrow-form spec-projection primitive on the declared
    // parent-PID axis that owns the `.spec.identity.parent.as_deref()`
    // chain the two hand-authored `tatara-reconciler::phase_machine`
    // sites (`handle_forking` ALLOCATE-PID composer + `handle_exiting`
    // SIGTERM-cascade child-fan-out filter) restated by hand pre-lift.
    // Peer to the sibling `observed_pid_*` pin family on the (spec-
    // declared × status-observed) axis pair; both compose the same
    // borrow-form `Option<&str>` return-shape skeleton on distinct
    // slots (`spec.identity.parent` vs. `status.pid`). Fail-before-
    // pass-after granularity: `declared_parent_pid` did not exist
    // pre-lift, so any test invoking it fails to compile pre-lift and
    // passes post-lift.
    fn process_with_declared_parent(parent: Option<&str>) -> Process {
        let mut spec = empty_spec();
        spec.identity.parent = parent.map(str::to_string);
        Process::new("child-proc", spec)
    }

    #[test]
    fn declared_parent_pid_returns_none_when_slot_is_none() {
        // Empty-slot corner pin: the primitive collapses the no-
        // parent case to `None`, matching the pre-lift `.as_deref()`
        // chain's `None` byte-identically at both reconciler consumer
        // sites. Semantically corresponds to a Process authored at
        // cluster init (PID 1) with no upstream parent — the
        // ALLOCATE-PID composer feeds `None` into `pid::allocate_pid`
        // to signal "no prefix", and the SIGTERM cascade's filter
        // never matches such a Process because a child's declared
        // parent can never equal `Some(pid)` when the slot is `None`.
        let p = process_with_declared_parent(None);
        assert!(p.declared_parent_pid().is_none());
    }

    #[test]
    fn declared_parent_pid_returns_borrowed_str_when_slot_is_populated() {
        // Happy-path pin: with a populated `spec.identity.parent`
        // slot, the primitive returns a borrowed `&str` whose
        // contents match the persisted `String`. A regression that
        // filtered / reshaped / canonicalized the string would
        // surface here rather than as silent skew at the child-fan-
        // out filter's `.declared_parent_pid() == Some(pid)`
        // equality check on the SAME parent-child pair.
        let p = process_with_declared_parent(Some("seph.1"));
        assert_eq!(p.declared_parent_pid(), Some("seph.1"));
    }

    #[test]
    fn declared_parent_pid_is_a_zero_copy_borrow_projection() {
        // Borrow-discipline pin: the returned `&str` borrows the
        // persisted `String`'s underlying byte buffer in place —
        // NOT a fresh allocation or a clone. A regression that
        // switched the projection to an owned `String` (via
        // `.clone()` or `.to_owned()`) would defeat the zero-copy
        // contract the lift's primary strict-widening delivers.
        // The `handle_exiting` cascade filter runs per candidate
        // child across the cluster-wide Process list; a per-row
        // `String::clone` would allocate one heap block per non-
        // matching row, so the borrow-form primitive is load-
        // bearing for large clusters. Peer to the sibling
        // `observed_pid_is_a_zero_copy_borrow_projection` pin on
        // the status-observed side of the axis pair.
        let p = process_with_declared_parent(Some("seph.1"));
        let borrowed = p.declared_parent_pid().expect("populated slot");
        let persisted = p.spec.identity.parent.as_ref().unwrap();
        assert!(std::ptr::eq(borrowed.as_ptr(), persisted.as_ptr()));
    }

    #[test]
    fn declared_parent_pid_is_a_pure_projection() {
        // Purity pin: calling the projection twice on the same
        // `Process` returns byte-identical `&str`s (same pointer,
        // same length). A regression that introduced state — a
        // lazy-cached slice materialized on first call, a
        // normalization step that ran once and cached — would
        // surface here rather than as silent drift between the
        // ALLOCATE-PID composer and the SIGTERM cascade's child-
        // fan-out filter within one reconcile pass.
        let p = process_with_declared_parent(Some("seph.1.3"));
        let a = p.declared_parent_pid().expect("populated slot");
        let b = p.declared_parent_pid().expect("populated slot");
        assert!(std::ptr::eq(a.as_ptr(), b.as_ptr()));
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn declared_parent_pid_matches_pre_lift_reconciler_chain_shape() {
        // Byte-identical parity pin between the borrow-form primitive
        // here and the pre-lift `tatara-reconciler::phase_machine`
        // `.spec.identity.parent.as_deref()` chain shape. Sweeps
        // every corner every callsite plausibly encounters (empty
        // slot, populated with a hierarchical PID). A regression
        // that inserted a normalization step at the primitive the
        // pre-lift chain does NOT apply — or vice versa — surfaces
        // here rather than as silent drift between the pre-lift
        // consumer sites and the ONE substrate owner they now route
        // through. Peer to
        // `observed_pid_matches_pre_lift_reconciler_chain_shape` on
        // the sibling axis's borrow-form primitive.
        fn pre_lift(p: &Process) -> Option<&str> {
            p.spec.identity.parent.as_deref()
        }
        // Empty slot.
        let p = process_with_declared_parent(None);
        assert_eq!(p.declared_parent_pid(), pre_lift(&p));
        // Populated with a hierarchical PID.
        let p = process_with_declared_parent(Some("seph.1"));
        assert_eq!(p.declared_parent_pid(), pre_lift(&p));
        // Populated with a deeper hierarchical PID.
        let p = process_with_declared_parent(Some("seph.1.7.42"));
        assert_eq!(p.declared_parent_pid(), pre_lift(&p));
    }

    #[test]
    fn declared_parent_pid_preserves_hierarchical_pid_format() {
        // Format-preservation pin: the hierarchical PID path
        // (dotted-segment form `seph.1.7`, matching the ported
        // `convergence-controller/src/identity.rs` scheme) reaches
        // the caller with segments and separators byte-identical
        // to the persisted `String`. A regression that inserted a
        // canonicalization pass (a segment-count validator, a
        // separator swap `.` → `/`, a leading/trailing whitespace
        // trim) would silently misroute the SIGTERM cascade's
        // `declared_parent_pid() == Some(pid)` comparator against
        // children whose `parent` field was authored in the ported
        // scheme's exact form — the SAME children the observed_pid
        // primitive is pinned to match on the other side of the
        // axis pair.
        for parent in ["seph", "seph.1", "seph.1.7", "seph.1.7.42"] {
            let p = process_with_declared_parent(Some(parent));
            assert_eq!(p.declared_parent_pid(), Some(parent));
        }
    }

    #[test]
    fn declared_parent_pid_composes_with_observed_pid_for_child_fanout_filter() {
        // Cross-axis coherence pin against the sibling
        // [`Self::observed_pid`] on the (spec-declared × status-
        // observed) axis pair: a child's `.declared_parent_pid()`
        // and its parent's `.observed_pid()` compose through the
        // SAME borrow-form `Option<&str>` skeleton so the
        // `handle_exiting` cascade filter's equality gate holds
        // structurally. A regression that skewed EITHER primitive's
        // return-form (return-shape, borrow discipline, empty-slot
        // collapse) would silently misroute every SIGTERM cascade
        // on the parent-child pair. This pin re-reads both primitives
        // at test time so the composition holds iff both live paths
        // are the current implementation.
        // Parent Process: has an observed PID.
        let mut parent = Process::new("parent-proc", empty_spec());
        parent.status = Some(ProcessStatus {
            pid: Some("seph.1".to_string()),
            ..Default::default()
        });
        // Child Process: declared parent matches parent's observed PID.
        let child = process_with_declared_parent(Some("seph.1"));
        // The `handle_exiting` filter's equality gate:
        // `child.declared_parent_pid() == Some(parent.observed_pid()?)`.
        let parent_pid = parent.observed_pid().expect("parent has PID");
        assert_eq!(child.declared_parent_pid(), Some(parent_pid));
        // Sibling Process with an unrelated declared parent must NOT
        // match the same parent — pins that the filter's SKIP branch
        // holds on the other side of the axis pair.
        let sibling = process_with_declared_parent(Some("seph.2"));
        assert_ne!(sibling.declared_parent_pid(), Some(parent_pid));
    }

    // ─── Process::declared_name_override substrate pins ──────────────
    //
    // Pins the borrow-form spec-projection primitive on the declared
    // name-override sub-axis of the declared-identity axis that owns
    // the `.spec.identity.name_override.as_deref()` chain the two
    // hand-authored `tatara-reconciler::phase_machine` sites
    // (`handle_pending` DECLARE composer + `handle_forking` ALLOCATE-
    // PID rehydration branch) restated by hand pre-lift. Peer to the
    // sibling `declared_parent_pid_*` pin family on the (parent ×
    // name-override) sub-axis pair; both compose the same borrow-form
    // `Option<&str>` return-shape skeleton on distinct slots
    // (`spec.identity.name_override` vs `spec.identity.parent`).
    // Fail-before-pass-after granularity: `declared_name_override`
    // did not exist pre-lift, so any test invoking it fails to
    // compile pre-lift and passes post-lift.
    fn process_with_declared_name_override(name_override: Option<&str>) -> Process {
        let mut spec = empty_spec();
        spec.identity.name_override = name_override.map(str::to_string);
        Process::new("some-proc", spec)
    }

    #[test]
    fn declared_name_override_returns_none_when_slot_is_none() {
        // Empty-slot corner pin: the primitive collapses the no-
        // override case to `None`, matching the pre-lift `.as_deref()`
        // chain's `None` byte-identically at both reconciler consumer
        // sites. Semantically corresponds to a Process authored
        // WITHOUT the human-name-override escape hatch — the default;
        // `derive_identity` then computes the name from the content
        // hash and stamps `name_override: false` on the resulting
        // [`Identity`].
        let p = process_with_declared_name_override(None);
        assert!(p.declared_name_override().is_none());
    }

    #[test]
    fn declared_name_override_returns_borrowed_str_when_slot_is_populated() {
        // Happy-path pin: with a populated `spec.identity
        // .name_override` slot, the primitive returns a borrowed
        // `&str` whose contents match the persisted `String`. A
        // regression that filtered / reshaped / canonicalized the
        // string at the primitive (as opposed to inside
        // `derive_identity`, where the trim/empty-filter lives today)
        // would surface here rather than as silent skew between the
        // DECLARE composer and the ALLOCATE-PID rehydration branch on
        // the SAME Process spec.
        let p = process_with_declared_name_override(Some("observability-stack"));
        assert_eq!(p.declared_name_override(), Some("observability-stack"));
    }

    #[test]
    fn declared_name_override_is_a_zero_copy_borrow_projection() {
        // Borrow-discipline pin: the returned `&str` borrows the
        // persisted `String`'s underlying byte buffer in place —
        // NOT a fresh allocation or a clone. Peer to the sibling
        // `declared_parent_pid_is_a_zero_copy_borrow_projection` pin
        // on the other side of the (parent × name-override) sub-axis
        // pair; the borrow discipline holds structurally on BOTH
        // sub-axes so a future `declared_identity` composite that
        // returns both halves together can compose them without
        // dropping into an owning form.
        let p = process_with_declared_name_override(Some("observability-stack"));
        let borrowed = p.declared_name_override().expect("populated slot");
        let persisted = p.spec.identity.name_override.as_ref().unwrap();
        assert!(std::ptr::eq(borrowed.as_ptr(), persisted.as_ptr()));
    }

    #[test]
    fn declared_name_override_is_a_pure_projection() {
        // Purity pin: calling the projection twice on the same
        // `Process` returns byte-identical `&str`s (same pointer,
        // same length). A regression that introduced state — a
        // lazy-cached slice materialized on first call, a
        // normalization step that ran once and cached — would
        // surface here rather than as silent drift between the
        // DECLARE composer and the ALLOCATE-PID rehydration branch
        // within one reconcile pass.
        let p = process_with_declared_name_override(Some("gateway-primary"));
        let a = p.declared_name_override().expect("populated slot");
        let b = p.declared_name_override().expect("populated slot");
        assert!(std::ptr::eq(a.as_ptr(), b.as_ptr()));
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn declared_name_override_matches_pre_lift_reconciler_chain_shape() {
        // Byte-identical parity pin between the borrow-form primitive
        // here and the pre-lift `tatara-reconciler::phase_machine`
        // `.spec.identity.name_override.as_deref()` chain shape.
        // Sweeps every corner every callsite plausibly encounters
        // (empty slot, populated with a bare name, populated with a
        // whitespace-containing name that `derive_identity`'s
        // internal trim would collapse, populated with an explicitly
        // empty string that `derive_identity`'s internal
        // `!s.is_empty()` filter would reject). A regression that
        // inserted a normalization step at the primitive the pre-
        // lift chain does NOT apply — or vice versa — surfaces here
        // rather than as silent drift between the pre-lift consumer
        // sites and the ONE substrate owner they now route through.
        // Peer to
        // `declared_parent_pid_matches_pre_lift_reconciler_chain_shape`
        // on the sibling sub-axis's borrow-form primitive.
        fn pre_lift(p: &Process) -> Option<&str> {
            p.spec.identity.name_override.as_deref()
        }
        // Empty slot.
        let p = process_with_declared_name_override(None);
        assert_eq!(p.declared_name_override(), pre_lift(&p));
        // Populated with a bare name.
        let p = process_with_declared_name_override(Some("observability-stack"));
        assert_eq!(p.declared_name_override(), pre_lift(&p));
        // Populated with a whitespace-containing name.
        let p = process_with_declared_name_override(Some("  observability-stack  "));
        assert_eq!(p.declared_name_override(), pre_lift(&p));
        // Populated with an explicitly empty string. Distinct from
        // the missing-slot `None` corner both at the primitive here
        // and at the pre-lift chain (the trim/filter that collapses
        // these two into the same `false`-branched
        // `Identity { name_override: false, .. }` lives INSIDE
        // `derive_identity`, NOT at the borrow site) — the primitive
        // MUST preserve the distinction so a future lift of the trim/
        // filter OUT of `derive_identity` INTO the primitive is a
        // conscious substrate change, not a silent one.
        let p = process_with_declared_name_override(Some(""));
        assert_eq!(p.declared_name_override(), pre_lift(&p));
    }

    #[test]
    fn declared_name_override_preserves_raw_slot_verbatim() {
        // Invariance-under-`derive_identity`-normalization pin: the
        // primitive returns the slot's raw byte contents verbatim —
        // no trim, no empty-string filter, no case fold, no
        // normalization of any kind. `derive_identity` internally
        // applies `.map(str::trim).filter(|s| !s.is_empty())` before
        // dispatching on `Some(non_empty)` vs `None | Some(empty |
        // whitespace)`, but that transform lives IN `derive_identity`,
        // NOT at the borrow site. A regression that pulled the trim/
        // filter forward INTO the primitive would silently collapse
        // three currently-distinct corners at the borrow site (bare
        // populated → `Some(name)`; whitespace-only → `Some("   ")`;
        // empty → `Some("")`) into two (bare → `Some(name)`; the
        // other two → `None`). That collapse might be an intentional
        // substrate change some future run wants to make; if so, it
        // lands as a conscious edit here (with this pin updated in
        // the same commit) rather than as silent behavior drift.
        for value in ["bare", "  padded  ", "\ttabs\t", "   ", ""] {
            let p = process_with_declared_name_override(Some(value));
            assert_eq!(
                p.declared_name_override(),
                Some(value),
                "declared_name_override must preserve raw slot verbatim for value {value:?}"
            );
        }
    }

    #[test]
    fn declared_name_override_composes_with_derive_identity_call_shape() {
        // Cross-primitive coherence pin against the [`derive_identity`]
        // consumer: the two live `tatara-reconciler::phase_machine`
        // callsites feed `p.declared_name_override()` as the second
        // positional argument to `derive_identity(&p.spec, …)`. This
        // pin exercises that exact call shape at test time so a
        // regression that skewed the primitive's return-form (return-
        // shape, borrow discipline, empty-slot collapse) surfaces
        // here as a shape mismatch at the [`derive_identity`] call
        // site rather than as silent operator-facing skew between the
        // DECLARE composer and the ALLOCATE-PID rehydration branch.
        // Populated with a bare non-empty name: `derive_identity`
        // dispatches on `Some(non_empty)` and stamps
        // `name_override: true` on the resulting [`Identity`], with
        // the resulting `.name` equal to the raw slot value.
        let p = process_with_declared_name_override(Some("gateway-primary"));
        let id = crate::identity::derive_identity(&p.spec, p.declared_name_override());
        assert!(id.name_override);
        assert_eq!(id.name, "gateway-primary");
        // Empty slot: `derive_identity` dispatches on `None` and
        // stamps `name_override: false` on the resulting [`Identity`],
        // with the resulting `.name` derived from the content hash
        // (NOT equal to any operator-authored slot value).
        let p = process_with_declared_name_override(None);
        let id = crate::identity::derive_identity(&p.spec, p.declared_name_override());
        assert!(!id.name_override);
    }

    // ─── Process::observed_flux_resources substrate pins ───────────────
    //
    // Pins the borrow-form status-projection primitive that owns the
    // 5-line `.status.as_ref().map(|s| s.flux_resources.clone())
    // .unwrap_or_default()` chain the two hand-authored
    // `tatara-reconciler::phase_machine` sites (`handle_running` +
    // `handle_attested`) restated by hand pre-lift. Fail-before-pass-
    // after granularity: a regression that widened the missing-`status`
    // corner, dropped the slot, or drifted the borrow discipline
    // surfaces here rather than as silent operator-facing skew between
    // the VERIFY-phase readiness probe and the ATTEST-heartbeat drift
    // detector.

    fn sample_flux_ref(name: &str) -> FluxResourceRef {
        // Distinct slot values so a swap between adjacent tuple
        // positions surfaces as an equality failure at the assertion
        // site — a slot-inversion regression cannot masquerade as
        // identity by accident. Peer to the sibling
        // `tatara_process::status::tests::sample_flux_ref` discipline
        // on the fetch-coords axis.
        FluxResourceRef {
            api_version: "kustomize.toolkit.fluxcd.io/v1".to_string(),
            kind: "Kustomization".to_string(),
            name: name.to_string(),
            namespace: "flux-system".to_string(),
            ready: false,
            message: None,
            last_check: None,
        }
    }

    fn process_with_flux_resources(refs: Vec<FluxResourceRef>) -> Process {
        let mut p = Process::new("api-gateway", empty_spec());
        p.metadata.namespace = Some("prod".into());
        let mut status = ProcessStatus::default();
        status.flux_resources = refs;
        p.status = Some(status);
        p
    }

    #[test]
    fn observed_flux_resources_returns_empty_slice_when_status_is_none() {
        // Missing-`status` corner pin: the primitive collapses the
        // no-status case to `&[]` so downstream `.is_empty()` /
        // `.len()` / iteration behave identically on a `Process`
        // whose status field is `None` and on one whose status
        // carries an empty `flux_resources` slot. Matches the
        // pre-lift `.unwrap_or_default()`'s empty-`Vec` corner
        // byte-identically at every reconciler consumer's downstream
        // shape.
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert!(p.observed_flux_resources().is_empty());
        assert_eq!(p.observed_flux_resources().len(), 0);
    }

    #[test]
    fn observed_flux_resources_returns_empty_slice_when_flux_resources_is_empty() {
        // Zero-refs-under-populated-status corner pin: the primitive
        // returns an empty slice, matching the missing-`status`
        // corner byte-identically. A regression that treated the two
        // corners differently (a `None`-vs-empty signal that
        // downstream consumers could grep on) would silently promote
        // an internal representation detail (whether the reconciler
        // has ever written a status subresource) into observable
        // behavior.
        let p = process_with_flux_resources(vec![]);
        assert!(p.observed_flux_resources().is_empty());
        assert_eq!(p.observed_flux_resources().len(), 0);
    }

    #[test]
    fn observed_flux_resources_returns_slice_of_persisted_vec() {
        // Happy-path pin: with a populated `status.flux_resources`
        // slot, the primitive returns a borrowed slice whose length
        // and per-element identity match the persisted vector. A
        // regression that filtered / reshaped / deduplicated the
        // slice would surface here rather than as silent skew at the
        // downstream fetch consumers.
        let refs = vec![
            sample_flux_ref("observability-stack"),
            sample_flux_ref("gateway"),
        ];
        let p = process_with_flux_resources(refs.clone());
        let observed = p.observed_flux_resources();
        assert_eq!(observed.len(), 2);
        assert_eq!(observed[0].name, "observability-stack");
        assert_eq!(observed[1].name, "gateway");
    }

    #[test]
    fn observed_flux_resources_is_a_zero_copy_borrow_projection() {
        // Borrow-discipline pin: the returned slice borrows the
        // persisted `Vec<FluxResourceRef>` in place — NOT a fresh
        // allocation or a clone. A regression that switched the
        // projection to owned refs (via `.clone()` or `.to_vec()`)
        // would defeat the zero-copy contract the lift's primary
        // strict-widening delivers (the pre-lift 5-line chain
        // eagerly cloned the whole vector per reconcile pass; the
        // post-lift primitive borrows). Peer to the sibling
        // `flux_resource_ref_fetch_coords_returns_borrows_of_owned_slots`
        // pin on the per-ref borrow-projection axis.
        let refs = vec![sample_flux_ref("observability-stack")];
        let p = process_with_flux_resources(refs);
        let observed = p.observed_flux_resources();
        let persisted = &p.status.as_ref().unwrap().flux_resources;
        assert!(std::ptr::eq(observed.as_ptr(), persisted.as_ptr()));
    }

    #[test]
    fn observed_flux_resources_is_a_pure_projection() {
        // Purity pin: calling the projection twice on the same
        // `Process` returns byte-identical slices (same pointer,
        // same length). A regression that introduced state — a
        // lazy-cached slice materialized on first call, a
        // normalization step that ran once and cached — would
        // surface here rather than as silent drift between the
        // VERIFY-phase and ATTEST-heartbeat consumers on the SAME
        // `Process` within one reconcile pass.
        let refs = vec![sample_flux_ref("observability-stack")];
        let p = process_with_flux_resources(refs);
        let a = p.observed_flux_resources();
        let b = p.observed_flux_resources();
        assert!(std::ptr::eq(a.as_ptr(), b.as_ptr()));
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn observed_flux_resources_matches_pre_lift_reconciler_chain_shape() {
        // Byte-identical parity pin between the borrow-form primitive
        // here and the pre-lift `tatara-reconciler::phase_machine`
        // 5-line chain shape. Sweeps every corner every callsite
        // plausibly encounters (missing status, empty flux_resources,
        // populated flux_resources with one ref, populated with
        // multiple refs). A regression that inserted a normalization
        // step at the primitive the pre-lift chain does NOT apply —
        // or vice versa — surfaces here rather than as silent drift
        // between the pre-lift consumer sites and the ONE substrate
        // owner they now route through. Peer to
        // `coordinates_or_none_matches_pre_lift_reconciler_helper_shape`
        // on the metadata axis's borrow-form primitive.
        // `FluxResourceRef` does not derive `PartialEq` — the parity
        // check walks the per-ref fetch-coords tuple (the same 4-slot
        // borrow projection every downstream fetch consumer routes
        // through) so a regression that reshaped ANY slot at ANY
        // index surfaces here through the sibling
        // `FluxResourceRef::fetch_coords` typed projection.
        fn pre_lift(p: &Process) -> Vec<FluxResourceRef> {
            p.status
                .as_ref()
                .map(|s| s.flux_resources.clone())
                .unwrap_or_default()
        }
        fn coord_shape(refs: &[FluxResourceRef]) -> Vec<(String, String, String, String)> {
            refs.iter()
                .map(|r| {
                    let (ns, av, kind, name) = r.fetch_coords();
                    (
                        ns.to_string(),
                        av.to_string(),
                        kind.to_string(),
                        name.to_string(),
                    )
                })
                .collect()
        }
        // Missing status.
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert_eq!(
            coord_shape(p.observed_flux_resources()),
            coord_shape(&pre_lift(&p))
        );
        // Populated status, empty slot.
        let p = process_with_flux_resources(vec![]);
        assert_eq!(
            coord_shape(p.observed_flux_resources()),
            coord_shape(&pre_lift(&p))
        );
        // Populated status, one ref.
        let p = process_with_flux_resources(vec![sample_flux_ref("obs")]);
        assert_eq!(
            coord_shape(p.observed_flux_resources()),
            coord_shape(&pre_lift(&p))
        );
        // Populated status, multiple refs.
        let p = process_with_flux_resources(vec![
            sample_flux_ref("obs"),
            sample_flux_ref("gw"),
            sample_flux_ref("api"),
        ]);
        assert_eq!(
            coord_shape(p.observed_flux_resources()),
            coord_shape(&pre_lift(&p))
        );
    }

    #[test]
    fn observed_flux_resources_missing_status_and_empty_slot_collapse_to_the_same_slice_shape() {
        // Cross-corner coherence pin: the missing-`status` corner and
        // the populated-empty-slot corner return slices whose
        // `.is_empty()` / `.len()` observations are IDENTICAL. A
        // regression that promoted the missing-`status` corner to
        // returning `None` (via a signature change) — or that widened
        // the empty-slot corner to a synthetic single-element slice
        // — would surface here rather than as silent operator-facing
        // divergence between a never-status-written Process and a
        // status-emptied Process.
        let mut p_no_status = Process::new("api", empty_spec());
        p_no_status.status = None;
        let p_empty_status = process_with_flux_resources(vec![]);
        assert_eq!(
            p_no_status.observed_flux_resources().len(),
            p_empty_status.observed_flux_resources().len()
        );
        assert_eq!(
            p_no_status.observed_flux_resources().is_empty(),
            p_empty_status.observed_flux_resources().is_empty()
        );
    }

    #[test]
    fn observed_flux_resources_slice_preserves_persisted_ordering() {
        // Ordering-preservation pin: the borrowed slice preserves
        // the exact insertion order of the persisted vector — no
        // sort, no dedup, no reshape. A regression that inserted a
        // sort or reordering would silently misroute per-ref
        // observations at the downstream VERIFY-phase / ATTEST-
        // heartbeat consumers, both of which walk the slice
        // positionally and correlate the position to the observed
        // readiness.
        let refs = vec![
            sample_flux_ref("z-last"),
            sample_flux_ref("a-first"),
            sample_flux_ref("m-middle"),
        ];
        let p = process_with_flux_resources(refs);
        let observed = p.observed_flux_resources();
        assert_eq!(observed[0].name, "z-last");
        assert_eq!(observed[1].name, "a-first");
        assert_eq!(observed[2].name, "m-middle");
    }

    // ─── Process::observed_pid substrate pins ─────────────────────────
    //
    // Pins the borrow-form status-projection primitive on the PID axis
    // that owns the 3-line `.status.as_ref().and_then(|s| s.pid.clone())`
    // chain the two hand-authored `tatara-reconciler::phase_machine`
    // sites (`handle_forking` ALLOCATE-PID gate + `handle_exiting`
    // SIGTERM cascade) restated by hand pre-lift. Peer to the sibling
    // `observed_flux_resources_*` pin family on the flux-resources
    // axis; both compose the missing-`status` fallback + borrow-form
    // return-shape skeleton on distinct `ProcessStatus` slots. Fail-
    // before-pass-after granularity: `observed_pid` did not exist
    // pre-lift, so any test invoking it fails to compile pre-lift and
    // passes post-lift.

    fn process_with_pid(pid: Option<&str>) -> Process {
        let mut p = Process::new("api-gateway", empty_spec());
        p.metadata.namespace = Some("prod".into());
        let mut status = ProcessStatus::default();
        status.pid = pid.map(str::to_string);
        p.status = Some(status);
        p
    }

    #[test]
    fn observed_pid_returns_none_when_status_is_none() {
        // Missing-`status` corner pin: the primitive collapses the
        // no-status case to `None` so downstream `.is_some()` /
        // `if let Some(_)` / `.map(...)` behave identically on a
        // `Process` whose status field is `None` and on one whose
        // status carries an unpopulated `pid` slot. Matches the
        // pre-lift `.and_then(...)` chain's `None` byte-identically
        // at every reconciler consumer's downstream shape.
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert!(p.observed_pid().is_none());
    }

    #[test]
    fn observed_pid_returns_none_when_pid_slot_is_none() {
        // Empty-slot-under-populated-status corner pin: the
        // primitive returns `None`, matching the missing-`status`
        // corner byte-identically. A regression that treated the
        // two corners differently (a `None`-vs-`Some("")` signal
        // that downstream consumers could grep on) would silently
        // promote an internal representation detail (whether the
        // reconciler has ever written a status subresource) into
        // observable behavior at the ALLOCATE-PID gate.
        let p = process_with_pid(None);
        assert!(p.observed_pid().is_none());
    }

    #[test]
    fn observed_pid_returns_borrowed_str_when_pid_slot_is_populated() {
        // Happy-path pin: with a populated `status.pid` slot, the
        // primitive returns a borrowed `&str` whose contents match
        // the persisted `String`. A regression that filtered /
        // reshaped / canonicalized the string would surface here
        // rather than as silent skew at the downstream cascade
        // comparator's `.as_deref() == Some(...)` equality check.
        let p = process_with_pid(Some("seph.1.7"));
        assert_eq!(p.observed_pid(), Some("seph.1.7"));
    }

    #[test]
    fn observed_pid_is_a_zero_copy_borrow_projection() {
        // Borrow-discipline pin: the returned `&str` borrows the
        // persisted `String`'s underlying byte buffer in place —
        // NOT a fresh allocation or a clone. A regression that
        // switched the projection to an owned `String` (via
        // `.clone()` or `.to_owned()`) would defeat the zero-copy
        // contract the lift's primary strict-widening delivers
        // (the pre-lift 3-line chain eagerly cloned the `String`
        // per reconcile pass at BOTH call sites even though the
        // ALLOCATE-PID gate immediately dropped the clone and the
        // SIGTERM cascade only re-borrowed it via `.as_str()`; the
        // post-lift primitive borrows). Peer to the sibling
        // `observed_flux_resources_is_a_zero_copy_borrow_projection`
        // pin on the flux-resources borrow-projection axis.
        let p = process_with_pid(Some("seph.1.7"));
        let observed = p.observed_pid().expect("populated slot");
        let persisted = p.status.as_ref().unwrap().pid.as_ref().unwrap();
        assert!(std::ptr::eq(observed.as_ptr(), persisted.as_ptr()));
    }

    #[test]
    fn observed_pid_is_a_pure_projection() {
        // Purity pin: calling the projection twice on the same
        // `Process` returns byte-identical `&str`s (same pointer,
        // same length). A regression that introduced state — a
        // lazy-cached slice materialized on first call, a
        // normalization step that ran once and cached — would
        // surface here rather than as silent drift between the
        // ALLOCATE-PID gate and the SIGTERM cascade on the SAME
        // `Process` within one reconcile pass.
        let p = process_with_pid(Some("seph.1.7"));
        let a = p.observed_pid().expect("populated slot");
        let b = p.observed_pid().expect("populated slot");
        assert!(std::ptr::eq(a.as_ptr(), b.as_ptr()));
        assert_eq!(a.len(), b.len());
    }

    #[test]
    fn observed_pid_matches_pre_lift_reconciler_chain_shape() {
        // Byte-identical parity pin between the borrow-form
        // primitive here and the pre-lift `tatara-reconciler
        // ::phase_machine` 3-line chain shape. Sweeps every corner
        // every callsite plausibly encounters (missing status,
        // empty pid slot, populated pid slot). A regression that
        // inserted a normalization step at the primitive the pre-
        // lift chain does NOT apply — or vice versa — surfaces
        // here rather than as silent drift between the pre-lift
        // consumer sites and the ONE substrate owner they now
        // route through. Peer to
        // `observed_flux_resources_matches_pre_lift_reconciler_chain_shape`
        // on the flux-resources axis's borrow-form primitive.
        fn pre_lift(p: &Process) -> Option<String> {
            p.status.as_ref().and_then(|s| s.pid.clone())
        }
        // Missing status.
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert_eq!(p.observed_pid().map(str::to_string), pre_lift(&p));
        // Populated status, empty pid slot.
        let p = process_with_pid(None);
        assert_eq!(p.observed_pid().map(str::to_string), pre_lift(&p));
        // Populated status, populated pid slot.
        let p = process_with_pid(Some("seph.1.7"));
        assert_eq!(p.observed_pid().map(str::to_string), pre_lift(&p));
    }

    #[test]
    fn observed_pid_missing_status_and_empty_slot_collapse_to_the_same_option_shape() {
        // Cross-corner coherence pin: the missing-`status` corner
        // and the populated-empty-slot corner return `Option`s whose
        // `.is_none()` observations are IDENTICAL. A regression
        // that promoted the missing-`status` corner to returning a
        // typed error (via a signature change to `Result<_, _>`) —
        // or that widened the empty-slot corner to a synthetic
        // `Some("")` — would surface here rather than as silent
        // operator-facing divergence between a never-status-
        // written Process and a status-emptied Process on the
        // ALLOCATE-PID gate.
        let mut p_no_status = Process::new("api", empty_spec());
        p_no_status.status = None;
        let p_empty_slot = process_with_pid(None);
        assert_eq!(
            p_no_status.observed_pid().is_none(),
            p_empty_slot.observed_pid().is_none()
        );
        assert_eq!(
            p_no_status.observed_pid().is_some(),
            p_empty_slot.observed_pid().is_some()
        );
    }

    #[test]
    fn observed_pid_preserves_hierarchical_pid_format() {
        // Format-preservation pin: the hierarchical PID path
        // (dotted-segment form `seph.1.7`, matching the ported
        // `convergence-controller/src/identity.rs` scheme) reaches
        // the caller with segments and separators byte-identical
        // to the persisted `String`. A regression that inserted a
        // canonicalization pass (a segment-count validator, a
        // separator swap `.` → `/`, a leading/trailing whitespace
        // trim) would silently misroute the SIGTERM cascade's
        // `spec.identity.parent == Some(pid)` comparator against
        // children whose `parent` field was authored in the ported
        // scheme's exact form.
        for pid in ["seph", "seph.1", "seph.1.7", "seph.1.7.42"] {
            let p = process_with_pid(Some(pid));
            assert_eq!(p.observed_pid(), Some(pid));
        }
    }

    // ─── Process::observed_attestation substrate pins ─────────────────
    //
    // Pins the borrow-form status-projection primitive on the
    // attestation-chain axis that owns the 3-line
    // `.status.as_ref().and_then(|s| s.attestation.as_ref())` chain
    // the two hand-authored `tatara-reconciler` sites
    // (`phase_machine::advance_to_attested` ATTEST composer +
    // `render::render_export_jobs` export-Job builder) restated by
    // hand pre-lift. Peer to the sibling `observed_pid_*` +
    // `observed_flux_resources_*` pin families; all three compose
    // the missing-`status` fallback + borrow-form return-shape
    // skeleton on distinct `ProcessStatus` slots. Fail-before-pass-
    // after granularity: `observed_attestation` did not exist
    // pre-lift, so any test invoking it fails to compile pre-lift
    // and passes post-lift.

    fn sample_attestation(artifact: &str, intent: &str) -> ProcessAttestation {
        // Distinct pillar strings so a regression that swapped the
        // artifact / intent pillars silently surfaces as an
        // equality failure at the composed-root parity pin.
        ProcessAttestation::initial(artifact.to_string(), None, intent.to_string())
    }

    fn process_with_attestation(attestation: Option<ProcessAttestation>) -> Process {
        let mut p = Process::new("api-gateway", empty_spec());
        p.metadata.namespace = Some("prod".into());
        let mut status = ProcessStatus::default();
        status.attestation = attestation;
        p.status = Some(status);
        p
    }

    #[test]
    fn observed_attestation_returns_none_when_status_is_none() {
        // Missing-`status` corner pin: the primitive collapses the
        // no-status case to `None` so downstream `.is_some()` /
        // `if let Some(_)` / `.map(...)` behave identically on a
        // `Process` whose status field is `None` and on one whose
        // status carries an unpopulated `attestation` slot.
        // Matches the pre-lift `.and_then(...)` chain's `None`
        // byte-identically at every reconciler consumer's
        // downstream shape.
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert!(p.observed_attestation().is_none());
    }

    #[test]
    fn observed_attestation_returns_none_when_attestation_slot_is_none() {
        // Empty-slot-under-populated-status corner pin: the
        // primitive returns `None`, matching the missing-`status`
        // corner byte-identically. A regression that treated the
        // two corners differently (a `None`-vs-`Some(_)` signal
        // that downstream consumers could grep on) would silently
        // promote an internal representation detail (whether the
        // reconciler has ever written a status subresource) into
        // observable behavior at the ATTEST composer's
        // seed-vs-chain branch.
        let p = process_with_attestation(None);
        assert!(p.observed_attestation().is_none());
    }

    #[test]
    fn observed_attestation_returns_borrow_when_slot_is_populated() {
        // Happy-path pin: with a populated `status.attestation`
        // slot, the primitive returns a borrowed
        // `&ProcessAttestation` whose fields match the persisted
        // record. A regression that filtered / reshaped /
        // canonicalized the record would surface here rather than
        // as silent skew at the downstream `prior.next(pillars)`
        // chain composer + the ephemeral-export receipt's
        // `previous_root` linker.
        let att = sample_attestation("art-1", "int-1");
        let composed_root = att.composed_root.clone();
        let p = process_with_attestation(Some(att));
        let observed = p.observed_attestation().expect("populated slot");
        assert_eq!(observed.artifact_hash, "art-1");
        assert_eq!(observed.intent_hash, "int-1");
        assert_eq!(observed.composed_root, composed_root);
        assert_eq!(observed.generation, 0);
        assert!(observed.previous_root.is_none());
    }

    #[test]
    fn observed_attestation_is_a_zero_copy_borrow_projection() {
        // Borrow-discipline pin: the returned reference points at
        // the persisted `ProcessAttestation` in place — NOT a fresh
        // allocation or a clone. A regression that switched the
        // projection to an owned `ProcessAttestation` (via
        // `.clone()`) would defeat the zero-copy contract the
        // lift's primary strict-widening delivers (the pre-lift
        // 3-line chain returned a borrow, but the export-Job
        // builder then cloned `composed_root` off it; the post-
        // lift primitive preserves the borrow all the way to the
        // consumer's own cloning choice). Peer to the sibling
        // `observed_pid_is_a_zero_copy_borrow_projection` +
        // `observed_flux_resources_is_a_zero_copy_borrow_projection`
        // pins on the PID + flux-resources borrow-projection axes.
        let att = sample_attestation("art-1", "int-1");
        let p = process_with_attestation(Some(att));
        let observed = p.observed_attestation().expect("populated slot") as *const _;
        let persisted = p.status.as_ref().unwrap().attestation.as_ref().unwrap() as *const _;
        assert!(std::ptr::eq(observed, persisted));
    }

    #[test]
    fn observed_attestation_is_a_pure_projection() {
        // Purity pin: calling the projection twice on the same
        // `Process` returns byte-identical borrows (same pointer).
        // A regression that introduced state — a lazy-cached
        // reference materialized on first call, a normalization
        // step that ran once and cached — would surface here
        // rather than as silent drift between the ATTEST composer
        // and the ephemeral-export receipt chain on the SAME
        // `Process` within one reconcile pass.
        let att = sample_attestation("art-1", "int-1");
        let p = process_with_attestation(Some(att));
        let a = p.observed_attestation().expect("populated slot") as *const _;
        let b = p.observed_attestation().expect("populated slot") as *const _;
        assert!(std::ptr::eq(a, b));
    }

    #[test]
    fn observed_attestation_matches_pre_lift_reconciler_chain_shape() {
        // Byte-identical parity pin between the borrow-form
        // primitive here and the pre-lift `tatara-reconciler`
        // 3-line chain shape. Sweeps every corner every callsite
        // plausibly encounters (missing status, empty attestation
        // slot, populated attestation slot). A regression that
        // inserted a normalization step at the primitive the pre-
        // lift chain does NOT apply — or vice versa — surfaces
        // here rather than as silent drift between the pre-lift
        // consumer sites and the ONE substrate owner they now
        // route through. Peer to
        // `observed_pid_matches_pre_lift_reconciler_chain_shape` +
        // `observed_flux_resources_matches_pre_lift_reconciler_chain_shape`
        // on the PID + flux-resources axes.
        // `ProcessAttestation` does not derive `PartialEq` — the
        // parity check walks the `composed_root` field (the
        // byte-string every downstream consumer keys off) so a
        // regression that reshaped the record without touching
        // the composed-root observation surfaces here through
        // the receipt-chain projection.
        fn pre_lift(p: &Process) -> Option<String> {
            p.status
                .as_ref()
                .and_then(|s| s.attestation.as_ref())
                .map(|a| a.composed_root.clone())
        }
        // Missing status.
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert_eq!(
            p.observed_attestation().map(|a| a.composed_root.clone()),
            pre_lift(&p)
        );
        // Populated status, empty attestation slot.
        let p = process_with_attestation(None);
        assert_eq!(
            p.observed_attestation().map(|a| a.composed_root.clone()),
            pre_lift(&p)
        );
        // Populated status, populated attestation slot.
        let p = process_with_attestation(Some(sample_attestation("art-1", "int-1")));
        assert_eq!(
            p.observed_attestation().map(|a| a.composed_root.clone()),
            pre_lift(&p)
        );
    }

    #[test]
    fn observed_attestation_missing_status_and_empty_slot_collapse_to_the_same_option_shape() {
        // Cross-corner coherence pin: the missing-`status` corner
        // and the populated-empty-slot corner return `Option`s
        // whose `.is_none()` observations are IDENTICAL. A
        // regression that promoted the missing-`status` corner to
        // returning a typed error (via a signature change to
        // `Result<_, _>`) — or that widened the empty-slot corner
        // to a synthetic `Some(default_attestation)` — would
        // surface here rather than as silent operator-facing
        // divergence between a never-status-written Process and
        // an attestation-emptied Process on the ATTEST composer's
        // seed-vs-chain branch.
        let mut p_no_status = Process::new("api", empty_spec());
        p_no_status.status = None;
        let p_empty_slot = process_with_attestation(None);
        assert_eq!(
            p_no_status.observed_attestation().is_none(),
            p_empty_slot.observed_attestation().is_none()
        );
        assert_eq!(
            p_no_status.observed_attestation().is_some(),
            p_empty_slot.observed_attestation().is_some()
        );
    }

    #[test]
    fn observed_attestation_preserves_chain_generation_field() {
        // Generation-preservation pin: a chained attestation
        // (`prior.next(...)` at generation N ≥ 1 with a
        // `previous_root` linked to `prior.composed_root`) reaches
        // the caller with its `generation` counter + `previous_root`
        // link byte-identical to the persisted record. The pre-lift
        // ATTEST composer discriminated exactly on this borrow's
        // `Some(prior)` vs `None` arm; a regression that dropped
        // the chain's `generation` counter (say, by folding
        // `next(...)` into a fresh `initial(...)` on every
        // reconcile pass) would silently reset every chain and
        // orphan every downstream `previous_root` link, but that
        // drift is invisible to a Process CRD reader who only
        // observes the LATEST composed_root.
        let prior = sample_attestation("art-0", "int-0");
        let chained = prior.next("art-1".to_string(), None, "int-1".to_string());
        let expected_generation = chained.generation;
        let expected_previous = chained.previous_root.clone();
        let p = process_with_attestation(Some(chained));
        let observed = p.observed_attestation().expect("populated slot");
        assert_eq!(observed.generation, expected_generation);
        assert_eq!(observed.generation, 1);
        assert_eq!(observed.previous_root, expected_previous);
        assert_eq!(
            observed.previous_root.as_deref(),
            Some(prior.composed_root.as_str())
        );
    }

    // ─── Process::observed_identity substrate pins ────────────────────
    //
    // The borrow-form status-projection primitive on the resolved-
    // identity axis. Collapses the paired 3-line `.status.as_ref()
    // .and_then(|s| s.identity.<clone|as_ref>())` chain every
    // consumer in `tatara-reconciler` restated by hand pre-lift at
    // TWO sites (`phase_machine::handle_forking` seed +
    // `ssapply::inject_annotations` content-hash annotation
    // composer). Peer to the sibling `observed_pid_*` +
    // `observed_attestation_*` + `observed_flux_resources_*` pin
    // families; all four compose the same missing-`status` fallback
    // + borrow-form return-shape skeleton on distinct
    // `ProcessStatus` slots. Each pin fails-before-pass-after
    // granularity: `observed_identity` did not exist pre-lift, so
    // any test invoking it fails to compile pre-lift and passes
    // post-lift.

    fn sample_identity(name: &str) -> Identity {
        // Distinct name + content_hash + override flag so a
        // regression that reshaped one slot surfaces at the
        // populated-slot pin's field-equality check without
        // aliasing the sibling slots.
        Identity {
            name: name.to_string(),
            content_hash: "a".repeat(26),
            name_override: true,
        }
    }

    fn process_with_identity(identity: Option<Identity>) -> Process {
        let mut p = Process::new("api-gateway", empty_spec());
        p.metadata.namespace = Some("prod".into());
        let mut status = ProcessStatus::default();
        status.identity = identity;
        p.status = Some(status);
        p
    }

    #[test]
    fn observed_identity_returns_none_when_status_is_none() {
        // Missing-`status` corner pin: the primitive collapses the
        // no-status case to `None` so downstream `.is_some()` /
        // `if let Some(_)` / `.cloned().unwrap_or_else(...)` behave
        // identically on a `Process` whose status field is `None`
        // and on one whose status carries an unpopulated `identity`
        // slot. Matches the pre-lift `.and_then(...)` chain's `None`
        // byte-identically at every reconciler consumer's
        // downstream shape.
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert!(p.observed_identity().is_none());
    }

    #[test]
    fn observed_identity_returns_none_when_identity_slot_is_none() {
        // Empty-slot-under-populated-status corner pin: the
        // primitive returns `None`, matching the missing-`status`
        // corner byte-identically. A regression that treated the
        // two corners differently (a `None`-vs-`Some(_)` signal
        // that downstream consumers could grep on) would silently
        // promote an internal representation detail (whether the
        // reconciler has ever written a status subresource) into
        // observable behavior at the FORK-time `derive_identity`
        // fallback branch.
        let p = process_with_identity(None);
        assert!(p.observed_identity().is_none());
    }

    #[test]
    fn observed_identity_returns_borrow_when_slot_is_populated() {
        // Happy-path pin: with a populated `status.identity` slot,
        // the primitive returns a borrowed `&Identity` whose fields
        // match the persisted record. A regression that filtered /
        // reshaped / canonicalized the record would surface here
        // rather than as silent skew at the FORK-time seed's
        // `.cloned().unwrap_or_else(derive_identity)` composition
        // + the SSA-time content-hash annotation stamp on the SAME
        // Process.
        let id = sample_identity("seph");
        let expected = id.clone();
        let p = process_with_identity(Some(id));
        let observed = p.observed_identity().expect("populated slot");
        assert_eq!(observed, &expected);
        assert_eq!(observed.name, "seph");
        assert_eq!(observed.content_hash, "a".repeat(26));
        assert!(observed.name_override);
    }

    #[test]
    fn observed_identity_is_a_zero_copy_borrow_projection() {
        // Borrow-discipline pin: the returned reference points at
        // the persisted `Identity` in place — NOT a fresh
        // allocation or a clone. A regression that switched the
        // projection to an owned `Identity` (via `.clone()`) would
        // defeat the zero-copy contract the lift's primary strict-
        // widening delivers (the SSA-time consumer never clones the
        // whole `Identity`, only the `content_hash` field it stamps
        // onto the annotation map, so the borrow-form return
        // shape's happy-path allocation count is exactly ZERO).
        // Peer to the sibling
        // `observed_attestation_is_a_zero_copy_borrow_projection`
        // + `observed_pid_is_a_zero_copy_borrow_projection` +
        // `observed_flux_resources_is_a_zero_copy_borrow_projection`
        // pins on the attestation-chain + PID + flux-resources
        // borrow-projection axes.
        let id = sample_identity("seph");
        let p = process_with_identity(Some(id));
        let observed = p.observed_identity().expect("populated slot") as *const _;
        let persisted = p.status.as_ref().unwrap().identity.as_ref().unwrap() as *const _;
        assert!(std::ptr::eq(observed, persisted));
    }

    #[test]
    fn observed_identity_is_a_pure_projection() {
        // Purity pin: calling the projection twice on the same
        // `Process` returns byte-identical borrows (same pointer).
        // A regression that introduced state — a lazy-cached
        // reference materialized on first call, a normalization
        // step that ran once and cached — would surface here
        // rather than as silent drift between the FORK-time
        // identity seed and the SSA-time content-hash annotation
        // stamp on the SAME `Process` within one reconcile pass.
        let p = process_with_identity(Some(sample_identity("seph")));
        let a = p.observed_identity().expect("populated slot") as *const _;
        let b = p.observed_identity().expect("populated slot") as *const _;
        assert!(std::ptr::eq(a, b));
    }

    #[test]
    fn observed_identity_matches_pre_lift_reconciler_chain_shape() {
        // Byte-identical parity pin between the borrow-form
        // primitive here and the pre-lift `tatara-reconciler`
        // 3-line chain shape. Sweeps every corner every callsite
        // plausibly encounters (missing status, empty identity
        // slot, populated identity slot). A regression that
        // inserted a normalization step at the primitive the pre-
        // lift chain does NOT apply — or vice versa — surfaces
        // here rather than as silent drift between the pre-lift
        // consumer sites and the ONE substrate owner they now
        // route through. Peer to
        // `observed_attestation_matches_pre_lift_reconciler_chain_shape`
        // + `observed_pid_matches_pre_lift_reconciler_chain_shape`
        // + `observed_flux_resources_matches_pre_lift_reconciler_chain_shape`
        // on the attestation-chain + PID + flux-resources axes.
        fn pre_lift(p: &Process) -> Option<Identity> {
            p.status.as_ref().and_then(|s| s.identity.clone())
        }
        // Missing status.
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert_eq!(p.observed_identity().cloned(), pre_lift(&p));
        // Populated status, empty identity slot.
        let p = process_with_identity(None);
        assert_eq!(p.observed_identity().cloned(), pre_lift(&p));
        // Populated status, populated identity slot.
        let p = process_with_identity(Some(sample_identity("seph")));
        assert_eq!(p.observed_identity().cloned(), pre_lift(&p));
    }

    #[test]
    fn observed_identity_missing_status_and_empty_slot_collapse_to_the_same_option_shape() {
        // Cross-corner coherence pin: the missing-`status` corner
        // and the populated-empty-slot corner return `Option`s
        // whose `.is_none()` observations are IDENTICAL. A
        // regression that promoted the missing-`status` corner to
        // returning a typed error (via a signature change to
        // `Result<_, _>`) — or that widened the empty-slot corner
        // to a synthetic `Some(derive_identity(default_spec))` —
        // would surface here rather than as silent operator-facing
        // divergence between a never-status-written Process and an
        // identity-cleared Process on the FORK-time seed branch.
        let mut p_no_status = Process::new("api", empty_spec());
        p_no_status.status = None;
        let p_empty_slot = process_with_identity(None);
        assert_eq!(
            p_no_status.observed_identity().is_none(),
            p_empty_slot.observed_identity().is_none()
        );
        assert_eq!(
            p_no_status.observed_identity().is_some(),
            p_empty_slot.observed_identity().is_some()
        );
    }

    #[test]
    fn observed_identity_cloned_composes_with_derive_identity_fallback() {
        // Cross-primitive composition pin: the borrow-form
        // primitive threaded through `.cloned().unwrap_or_else(||
        // derive_identity(...))` reproduces the pre-lift FORK-time
        // seed's owned-`Identity` shape at every corner. Binds the
        // exact composition the `phase_machine::handle_forking`
        // consumer performs: on the populated-slot corner the
        // reconciler-persisted `Identity` is returned verbatim (the
        // fallback never fires), and on both empty corners
        // (missing-status + empty-slot) the fallback fires
        // producing a fresh `derive_identity(&spec,
        // name_override)`. A regression that (a) swapped the
        // fallback direction, (b) made `.cloned()` re-derive
        // instead of clone, or (c) made the empty-slot corner
        // return a synthetic `Some(default_identity)` collides
        // with the fallback surfaces here rather than as silent
        // FORK-time PID allocator skew.
        let spec = empty_spec();
        let fallback_expected = crate::identity::derive_identity(&spec, None);
        // Populated-slot corner: the seed returns the persisted
        // identity, NOT the derive fallback.
        let persisted = sample_identity("seph");
        let p = process_with_identity(Some(persisted.clone()));
        let seed = p.observed_identity().cloned().unwrap_or_else(|| {
            crate::identity::derive_identity(&p.spec, p.declared_name_override())
        });
        assert_eq!(seed, persisted);
        assert_ne!(seed, fallback_expected);
        // Empty-slot corner: the seed fires the derive fallback.
        let p = process_with_identity(None);
        let seed = p.observed_identity().cloned().unwrap_or_else(|| {
            crate::identity::derive_identity(&p.spec, p.declared_name_override())
        });
        assert_eq!(seed, fallback_expected);
        // Missing-status corner: the seed fires the derive
        // fallback, byte-identical to the empty-slot corner.
        let mut p = Process::new("api-gateway", empty_spec());
        p.metadata.namespace = Some("prod".into());
        p.status = None;
        let seed = p.observed_identity().cloned().unwrap_or_else(|| {
            crate::identity::derive_identity(&p.spec, p.declared_name_override())
        });
        assert_eq!(seed, fallback_expected);
    }

    // ─── Process::observed_phase substrate pins ───────────────────────
    //
    // The copy-form status-projection primitive on the phase axis.
    // Collapses the paired 3-line `.status.as_ref().map(|s| s.phase)`
    // chain every consumer in `tatara-reconciler` restated by hand
    // pre-lift at FIVE sites. Peer to the borrow-form
    // `observed_pid_*` + `observed_flux_resources_*` +
    // `observed_attestation_*` pin families; all four compose the
    // same missing-`status` fallback skeleton on distinct
    // `ProcessStatus` slots, with the phase-axis form returning
    // `Option<ProcessPhase>` (copy of a `Copy` scalar) rather than
    // `Option<&T>` (borrow) because the underlying slot is a bare
    // `ProcessPhase` — no allocation to borrow past, and the enum
    // is one byte on the wire. Each pin fails-before-pass-after
    // granularity: `observed_phase` did not exist pre-lift, so any
    // test invoking it fails to compile pre-lift and passes
    // post-lift.

    fn process_with_phase(phase: Option<ProcessPhase>) -> Process {
        let mut p = Process::new("api-gateway", empty_spec());
        p.metadata.namespace = Some("prod".into());
        if let Some(ph) = phase {
            let mut status = ProcessStatus::default();
            status.phase = ph;
            p.status = Some(status);
        }
        p
    }

    #[test]
    fn observed_phase_returns_none_when_status_is_none() {
        // Missing-`status` corner pin: the primitive collapses the
        // no-status case to `None` so downstream `.unwrap_or(...)`
        // at every reconciler consumer chooses the default
        // deliberately (`Pending` for the top-level dispatch seed
        // + boundary evaluator + routing groupby; `Attested` for
        // the released-from annotation composer). Matches the
        // pre-lift `.map(|s| s.phase)` chain's `None`
        // byte-identically at every consumer's downstream shape.
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert!(p.observed_phase().is_none());
    }

    #[test]
    fn observed_phase_returns_some_default_when_status_is_populated_with_default_phase() {
        // Populated-status corner pin: the primitive returns
        // `Some(ProcessPhase::default())` — a `ProcessStatus`
        // constructed via `default()` carries `phase: Pending`
        // because the phase field is a bare `ProcessPhase` (not
        // `Option<ProcessPhase>`), so there is NO "empty slot"
        // corner peer to the borrow-form projections' empty-slot
        // pins. A regression that reshaped the return type to
        // filter out `Pending` (treating it as "unset") would
        // surface here and silently break the top-level
        // dispatcher's Pending → Forking transition on a Process
        // freshly written by the reconciler.
        let p = process_with_phase(Some(ProcessPhase::default()));
        assert_eq!(p.observed_phase(), Some(ProcessPhase::Pending));
        assert_eq!(p.observed_phase(), Some(ProcessPhase::default()));
    }

    #[test]
    fn observed_phase_returns_persisted_phase_when_status_is_populated() {
        // Happy-path pin: with a populated `status.phase` slot,
        // the primitive returns the persisted `ProcessPhase`.
        // A regression that filtered / reshaped / canonicalized
        // the phase would surface here rather than as silent
        // skew at the top-level dispatcher's phase handler
        // dispatch on the SAME Process.
        let p = process_with_phase(Some(ProcessPhase::Running));
        assert_eq!(p.observed_phase(), Some(ProcessPhase::Running));
    }

    #[test]
    fn observed_phase_is_a_pure_projection() {
        // Purity pin: two consecutive calls return byte-identical
        // `Option<ProcessPhase>` values (no lazy materialization,
        // no interior mutation of `self`). Peer to the sibling
        // `observed_pid_is_a_pure_projection` +
        // `observed_flux_resources_is_a_pure_projection` +
        // `observed_attestation_is_a_pure_projection` pins; all
        // four bind the pure-projection discipline on the ONE
        // substrate accessor per status slot.
        let p = process_with_phase(Some(ProcessPhase::Attested));
        let a = p.observed_phase();
        let b = p.observed_phase();
        assert_eq!(a, b);
        assert_eq!(a, Some(ProcessPhase::Attested));
    }

    #[test]
    fn observed_phase_matches_pre_lift_reconciler_chain_shape() {
        // Parity pin: sweeps the two corners every pre-lift
        // consumer plausibly encountered (missing status,
        // populated status with a particular phase) and compares
        // the substrate call against a hand-authored pre-lift
        // chain byte-identically. A regression that reshaped ANY
        // of the two corners would surface here rather than as
        // silent operator-facing skew between the top-level
        // dispatcher and any of the four other reconciler
        // consumers on the SAME `Process`.
        fn pre_lift(p: &Process) -> Option<ProcessPhase> {
            p.status.as_ref().map(|s| s.phase)
        }
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert_eq!(p.observed_phase(), pre_lift(&p));
        let p = process_with_phase(Some(ProcessPhase::Running));
        assert_eq!(p.observed_phase(), pre_lift(&p));
        let p = process_with_phase(Some(ProcessPhase::Attested));
        assert_eq!(p.observed_phase(), pre_lift(&p));
        let p = process_with_phase(Some(ProcessPhase::Failed));
        assert_eq!(p.observed_phase(), pre_lift(&p));
    }

    #[test]
    fn observed_phase_default_unwrap_matches_pre_lift_pending_default() {
        // Callsite-shape pin: three of the FIVE pre-lift consumers
        // (`controller::reconcile`, `boundary::evaluate_process_phase`,
        // `table_controller::stable_name_group_key`) closed the
        // 3-line chain with `.unwrap_or(ProcessPhase::Pending)`
        // (identical to `.unwrap_or_default()`). This pin binds
        // that call-site shape: `observed_phase().unwrap_or
        // (Pending)` returns `Pending` on missing status and the
        // persisted phase otherwise. A regression that swapped
        // the `None` sentinel's downstream default would surface
        // here rather than as silent skew at three of the five
        // consumer sites.
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert_eq!(
            p.observed_phase().unwrap_or(ProcessPhase::Pending),
            ProcessPhase::Pending
        );
        let p = process_with_phase(Some(ProcessPhase::Running));
        assert_eq!(
            p.observed_phase().unwrap_or(ProcessPhase::Pending),
            ProcessPhase::Running
        );
    }

    #[test]
    fn observed_phase_attested_unwrap_matches_pre_lift_released_from_default() {
        // Callsite-shape pin: the ONE pre-lift consumer
        // (`phase_machine::p_current_phase_str` — the
        // released-from annotation composer) closed the 3-line
        // chain with `.unwrap_or(ProcessPhase::Attested)` rather
        // than the `Default` (`Pending`). This pin binds that
        // call-site shape: `observed_phase().unwrap_or(Attested)`
        // returns `Attested` on missing status and the persisted
        // phase otherwise. A regression that folded the
        // `Attested`-default consumer into the `Pending`-default
        // majority would break the SIGSTOP/SIGCONT release gate's
        // "which annotation label to emit" branch — the pin binds
        // the primitive at the raw `Option<ProcessPhase>` form so
        // this default choice stays local at the callsite.
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert_eq!(
            p.observed_phase().unwrap_or(ProcessPhase::Attested),
            ProcessPhase::Attested
        );
        let p = process_with_phase(Some(ProcessPhase::Failed));
        assert_eq!(
            p.observed_phase().unwrap_or(ProcessPhase::Attested),
            ProcessPhase::Failed
        );
    }

    #[test]
    fn observed_phase_preserves_every_process_phase_variant() {
        // Round-trip pin: every `ProcessPhase` variant round-
        // trips through the primitive unchanged. Peer to the
        // sibling `observed_pid_preserves_hierarchical_pid_format`
        // pin's dotted-segment sweep; this pin sweeps the closed
        // set of `ProcessPhase` variants directly so a
        // canonicalization pass that dropped or reshaped one
        // (e.g. folded `Reconverging` back into `Execing`, or
        // remapped `Zombie` to `Reaped`) surfaces here rather
        // than as silent skew at the SIGSTOP/SIGCONT release
        // gate's phase-name annotation branch. Covers every
        // variant the `ProcessPhase::DeriveClosedSet` enumerates
        // so a future variant addition surfaces via the closed-
        // set macro rather than at a silent partial sweep.
        for phase in [
            ProcessPhase::Pending,
            ProcessPhase::Forking,
            ProcessPhase::Execing,
            ProcessPhase::Running,
            ProcessPhase::Attested,
            ProcessPhase::Reconverging,
            ProcessPhase::Releasing,
            ProcessPhase::Exiting,
            ProcessPhase::Failed,
            ProcessPhase::Zombie,
            ProcessPhase::Reaped,
        ] {
            let p = process_with_phase(Some(phase));
            assert_eq!(
                p.observed_phase(),
                Some(phase),
                "phase variant {phase:?} did not round-trip"
            );
        }
    }

    // ─── Process::observed_phase_or_pending substrate pins ─────────────
    //
    // Pins the copy-form status-projection primitive on the phase
    // axis with the `Pending` sink applied. Sibling to the raw
    // `observed_phase_*` pin family on the (return-form × fallback
    // shape) axis pair — the raw-`Option` corner stays with the
    // sibling family; this pin family opens the `Pending`-defaulted
    // corner that four of the five pre-lift `observed_phase`
    // consumers wrote by hand. Fail-before-pass-after granularity:
    // `observed_phase_or_pending` did not exist pre-lift, so any
    // test invoking it fails to compile pre-lift and passes
    // post-lift.

    #[test]
    fn observed_phase_or_pending_returns_pending_when_status_is_none() {
        // Missing-`status` corner pin: the primitive collapses the
        // no-status case to `Pending` — the sink four of the five
        // pre-lift `observed_phase` consumers wrote by hand
        // (`controller::reconcile` / `boundary::
        // evaluate_process_phase` / `table_controller::
        // stable_name_group_key` / `controller_pool::reconcile_pool`)
        // and the sentinel `ProcessPhase::default()` returns. A
        // regression that folded the `None` sink to any other phase
        // (e.g. `Forking` — treating "not yet observed" as "already
        // dispatched") would silently mis-seed the top-level
        // dispatcher's `Pending → Forking` transition and surface as
        // operator-visible reconcile-cycle skew on a freshly-forked
        // Process rather than at this pin.
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert_eq!(p.observed_phase_or_pending(), ProcessPhase::Pending);
    }

    #[test]
    fn observed_phase_or_pending_returns_persisted_phase_when_status_is_populated() {
        // Populated-status corner pin: the primitive passes through
        // the persisted `ProcessPhase` unchanged — the sink only
        // fires on missing `status`, not on a populated one carrying
        // a `Pending`-adjacent variant. Two variants pinned to
        // separate the "pass through the persisted phase" arm from
        // the "sink fires" arm: `Running` (mid-lifecycle) and
        // `Attested` (post-verify) both round-trip unchanged where
        // a regression that always returned `Pending` (dropped the
        // pass-through arm entirely) would surface here rather than
        // as silent skew at every reconciler's per-phase branch.
        let p = process_with_phase(Some(ProcessPhase::Running));
        assert_eq!(p.observed_phase_or_pending(), ProcessPhase::Running);
        let p = process_with_phase(Some(ProcessPhase::Attested));
        assert_eq!(p.observed_phase_or_pending(), ProcessPhase::Attested);
    }

    #[test]
    fn observed_phase_or_pending_matches_pre_lift_unwrap_or_pending_chain_shape() {
        // Byte-identical parity pin: the primitive's return equals
        // the pre-lift two-link `.observed_phase().unwrap_or
        // (ProcessPhase::Pending)` chain at every one of the four
        // corner values (missing `status` → `Pending`, populated
        // with `Pending` → `Pending`, populated with a mid-lifecycle
        // variant → pass-through, populated with a terminal variant
        // → pass-through). A regression that swapped the sink to
        // `ProcessPhase::default()` (currently equivalent to
        // `Pending`) would keep this pin green until the enum's
        // `Default` impl drifted — the explicit `Pending` spelling
        // in the pin binds the operator-visible label rather than
        // the derived `Default`, so a future rename or reordering
        // of `ProcessPhase` variants that shifted `Default` off
        // `Pending` would surface here rather than as silent skew
        // at the four downstream consumer sites.
        let pre_lift = |p: &Process| p.observed_phase().unwrap_or(ProcessPhase::Pending);
        let mut p = Process::new("api", empty_spec());
        p.status = None;
        assert_eq!(p.observed_phase_or_pending(), pre_lift(&p));
        let p = process_with_phase(Some(ProcessPhase::Pending));
        assert_eq!(p.observed_phase_or_pending(), pre_lift(&p));
        let p = process_with_phase(Some(ProcessPhase::Running));
        assert_eq!(p.observed_phase_or_pending(), pre_lift(&p));
        let p = process_with_phase(Some(ProcessPhase::Reaped));
        assert_eq!(p.observed_phase_or_pending(), pre_lift(&p));
    }

    #[test]
    fn observed_phase_or_pending_is_a_pure_projection() {
        // Purity pin: two back-to-back calls on the same `Process`
        // return the same `ProcessPhase` — the primitive stamps no
        // side effect (no clock read, no metadata write, no
        // `status` mutation) despite the sibling `observed_phase`
        // taking `&self` too. Peer to the sibling `observed_phase`
        // purity pin; a regression that folded a clock read (e.g.
        // "if the sink fired, stamp `phase_since = Utc::now()`")
        // into the primitive would surface here rather than at the
        // consumer sites' downstream reconcile-cycle behavior.
        let p = process_with_phase(Some(ProcessPhase::Running));
        let a = p.observed_phase_or_pending();
        let b = p.observed_phase_or_pending();
        assert_eq!(a, b);
    }

    #[test]
    fn observed_phase_or_pending_preserves_every_process_phase_variant() {
        // Round-trip pin: every `ProcessPhase` variant round-trips
        // through the primitive unchanged when the `status` slot is
        // populated. Peer to the sibling `observed_phase_preserves
        // _every_process_phase_variant` sweep; this pin sweeps the
        // closed set through the `Pending`-sinked accessor rather
        // than the raw-`Option` accessor so a canonicalization pass
        // that dropped or reshaped one variant (e.g. folded
        // `Reconverging` back into `Execing`, remapped `Zombie` to
        // `Reaped`) surfaces at BOTH primitives' pin sets rather
        // than as silent skew at a subset of the reconciler
        // consumers. Covers every variant the
        // `ProcessPhase::DeriveClosedSet` enumerates so a future
        // variant addition surfaces via the closed-set macro rather
        // than at a silent partial sweep.
        for phase in [
            ProcessPhase::Pending,
            ProcessPhase::Forking,
            ProcessPhase::Execing,
            ProcessPhase::Running,
            ProcessPhase::Attested,
            ProcessPhase::Reconverging,
            ProcessPhase::Releasing,
            ProcessPhase::Exiting,
            ProcessPhase::Failed,
            ProcessPhase::Zombie,
            ProcessPhase::Reaped,
        ] {
            let p = process_with_phase(Some(phase));
            assert_eq!(
                p.observed_phase_or_pending(),
                phase,
                "phase variant {phase:?} did not round-trip through observed_phase_or_pending"
            );
        }
    }

    // ─── Process::is_being_deleted substrate pins ───────────────────────
    //
    // Pins the copy-form metadata-projection primitive on the
    // deletion-tombstone axis. Peer to the borrow-form + copy-form
    // metadata-fallback family (`namespace_or_default`,
    // `name_or_placeholder`, `uid_or_empty`, `coordinates_or_defaults`,
    // `coordinates_or_none`, `owned_coordinates_or_err`, `annotation`);
    // this one opens the presence-probe corner for the tombstone slot.
    // Fail-before-pass-after granularity: `is_being_deleted` did not
    // exist pre-lift, so any test invoking it fails to compile pre-
    // lift and passes post-lift.

    fn tombstoned_process() -> Process {
        let mut p = Process::new("api-gateway", empty_spec());
        p.metadata.namespace = Some("prod".into());
        p.metadata.deletion_timestamp = Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(
            Utc::now(),
        ));
        p
    }

    #[test]
    fn is_being_deleted_returns_false_when_deletion_timestamp_is_absent() {
        // Missing-tombstone corner pin: the primitive collapses the
        // no-tombstone case to `false` so the SIGTERM preempt at
        // `controller::reconcile` skips the `→ Exiting` forcing
        // branch and the DELETE-skip at `handle_exiting`'s child
        // fan-out does NOT `continue` past a child that is still
        // healthy. Matches the pre-lift `.is_some()` chain's `false`
        // byte-identically at every consumer's downstream gate.
        let mut p = Process::new("api", empty_spec());
        p.metadata.deletion_timestamp = None;
        assert!(!p.is_being_deleted());
    }

    #[test]
    fn is_being_deleted_returns_true_when_deletion_timestamp_is_present() {
        // Present-tombstone corner pin: the primitive returns
        // `true` on any populated `metadata.deletionTimestamp`
        // slot regardless of the timestamp payload — the two
        // consumers only read the tombstone's PRESENCE, never
        // its RFC-3339 timestamp value. A regression that gated
        // the `true` return on the timestamp being non-epoch, or
        // parsed the timestamp before returning, would surface
        // here rather than as silent skew at the SIGTERM preempt
        // or child-fan-out DELETE-skip on the SAME `Process`.
        let p = tombstoned_process();
        assert!(p.is_being_deleted());
    }

    #[test]
    fn is_being_deleted_is_a_pure_projection() {
        // Purity pin: two consecutive calls return byte-identical
        // `bool` values (no lazy materialization, no interior
        // mutation of `self`). Peer to the sibling
        // `observed_phase_is_a_pure_projection` +
        // `observed_pid_is_a_pure_projection` +
        // `observed_flux_resources_is_a_pure_projection` +
        // `observed_attestation_is_a_pure_projection` pins; all
        // five bind the pure-projection discipline on the ONE
        // substrate accessor per metadata / status slot.
        let p = tombstoned_process();
        let a = p.is_being_deleted();
        let b = p.is_being_deleted();
        assert_eq!(a, b);
        assert!(a);
    }

    #[test]
    fn is_being_deleted_matches_pre_lift_reconciler_chain_shape() {
        // Parity pin: sweeps the two corners every pre-lift
        // consumer plausibly encountered (missing tombstone,
        // present tombstone) and compares the substrate call
        // against a hand-authored pre-lift chain byte-identically.
        // A regression that reshaped either corner would surface
        // here rather than as silent operator-facing skew between
        // the top-level dispatcher's SIGTERM preempt and the
        // SIGTERM cascade's child-fan-out DELETE-skip on the
        // SAME `Process` within one reconcile pass.
        fn pre_lift(p: &Process) -> bool {
            p.metadata.deletion_timestamp.is_some()
        }
        let mut p = Process::new("api", empty_spec());
        p.metadata.deletion_timestamp = None;
        assert_eq!(p.is_being_deleted(), pre_lift(&p));
        let p = tombstoned_process();
        assert_eq!(p.is_being_deleted(), pre_lift(&p));
    }

    #[test]
    fn is_being_deleted_composes_with_process_phase_is_alive_at_reconcile_preempt() {
        // Call-site-shape pin: the `controller::reconcile` SIGTERM
        // preempt composes `is_being_deleted() && current_phase
        // .is_alive()` — the tombstone-presence probe AND the
        // alive-phase gate must BOTH hold to force `→ Exiting`.
        // A dead-phase (`Zombie` / `Reaped` / `Failed`) Process
        // that carries a tombstone still runs its normal handler,
        // not the preempt. This pin binds that composition shape
        // at the primitive so a regression that flipped either
        // half of the `&&` (or that broadened the tombstone probe
        // to include the `is_alive` half implicitly) surfaces
        // here rather than as silent skew at the top-level
        // dispatch on the SAME `Process`.
        let mut p = tombstoned_process();
        // Alive + tombstoned → preempt fires.
        let mut alive = ProcessStatus::default();
        alive.phase = ProcessPhase::Running;
        p.status = Some(alive);
        assert!(p.is_being_deleted());
        assert!(p.observed_phase().unwrap_or_default().is_alive());
        // Dead + tombstoned → preempt does NOT fire (composition
        // with `is_alive` returns false).
        let mut dead = ProcessStatus::default();
        dead.phase = ProcessPhase::Reaped;
        p.status = Some(dead);
        assert!(p.is_being_deleted());
        assert!(!p.observed_phase().unwrap_or_default().is_alive());
    }

    // ─── Process::created_at substrate pins ─────────────────────────
    //
    // Pins the copy-form metadata-projection primitive on the
    // `metadata.creationTimestamp` axis that owns the
    // `.metadata.creation_timestamp.as_ref().map(|t| t.0)` chain the
    // three hand-authored sites (`lifetime_clock::evaluate`,
    // `lifetime_clock::requeue_with_ttl`,
    // `tatara-reconciler::table_controller`) restated by hand pre-lift.
    // Peer to the sibling `is_being_deleted_*` +
    // `observed_phase_*` pin families — all three primitives project a
    // wire-format `Option<T>` slot into a `Copy` inner value at ONE
    // owner. Fail-before-pass-after granularity: `created_at` did not
    // exist pre-lift, so any test invoking it fails to compile pre-lift
    // and passes post-lift.

    fn creation_stamped_process(t: DateTime<Utc>) -> Process {
        let mut p = Process::new("age-anchor", empty_spec());
        p.metadata.namespace = Some("prod".into());
        p.metadata.creation_timestamp =
            Some(k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(t));
        p
    }

    #[test]
    fn created_at_returns_none_when_creation_timestamp_is_absent() {
        // Missing-slot corner pin: the primitive collapses the
        // no-creation-timestamp case to `None` so the TTL-expiry gate
        // at `lifetime_clock::evaluate` short-circuits its inner
        // `if let Some(...)` branch (no elapsed computation), the
        // requeue-budget picker returns its default sleep, and the
        // stable-name arbiter's `.unwrap_or_else(Utc::now)` tail
        // synthesizes a "just created" anchor at its own site. Matches
        // the pre-lift `.as_ref().map(|t| t.0)` chain's `None`
        // byte-identically at every consumer's downstream tail.
        let mut p = Process::new("api", empty_spec());
        p.metadata.creation_timestamp = None;
        assert!(p.created_at().is_none());
    }

    #[test]
    fn created_at_returns_some_datetime_when_slot_is_populated() {
        // Populated-slot corner pin: with a populated
        // `metadata.creationTimestamp` slot, the primitive unwraps the
        // wire-format `Time` newtype to its inner `DateTime<Utc>` and
        // returns it as `Some(datetime)` — hiding the `.0` field-access
        // every pre-lift consumer restated to reach the underlying
        // instant.
        let anchor = Utc::now() - chrono::Duration::seconds(300);
        let p = creation_stamped_process(anchor);
        assert_eq!(p.created_at(), Some(anchor));
    }

    #[test]
    fn created_at_is_a_pure_projection() {
        // Purity pin: two consecutive calls return byte-identical
        // `Option<DateTime<Utc>>` values (no lazy materialization, no
        // interior mutation of `self`). Peer to the sibling
        // `is_being_deleted_is_a_pure_projection` +
        // `observed_phase_is_a_pure_projection` pins; all three bind
        // the pure-projection discipline on the ONE substrate accessor
        // per metadata / status slot.
        let anchor = Utc::now();
        let p = creation_stamped_process(anchor);
        let a = p.created_at();
        let b = p.created_at();
        assert_eq!(a, b);
        assert_eq!(a, Some(anchor));
    }

    #[test]
    fn created_at_matches_pre_lift_creation_timestamp_chain_shape() {
        // Parity pin: sweeps the two corners every pre-lift consumer
        // plausibly encountered (missing slot, populated slot) and
        // compares the substrate call against a hand-authored pre-lift
        // chain byte-identically. A regression that reshaped either
        // corner (returning `Some(Utc::now())` on the missing slot,
        // returning a rounded / truncated timestamp on the populated
        // slot) would surface here rather than as silent operator-
        // facing skew between the TTL-expiry gate, the requeue-budget
        // picker, and the stable-name claim-arbiter tie-break on the
        // SAME `Process` within one reconcile pass.
        fn pre_lift(p: &Process) -> Option<DateTime<Utc>> {
            p.metadata.creation_timestamp.as_ref().map(|t| t.0)
        }
        // Missing slot.
        let mut p = Process::new("x", empty_spec());
        p.metadata.creation_timestamp = None;
        assert_eq!(p.created_at(), pre_lift(&p));
        // Populated slot.
        let anchor = Utc::now() - chrono::Duration::seconds(42);
        let p = creation_stamped_process(anchor);
        assert_eq!(p.created_at(), pre_lift(&p));
    }

    #[test]
    fn created_at_composes_with_signed_duration_since_at_ttl_gate() {
        // Call-site-shape pin: the `lifetime_clock::evaluate` TTL-
        // expiry gate composes `now.signed_duration_since(creation)`
        // where `creation` is the `DateTime<Utc>` returned by this
        // primitive's `Some` corner. A regression that returned a
        // per-callsite `Local` timezone (or that stripped the timezone
        // marker) would break the arithmetic silently. This pin
        // computes the elapsed duration byte-identically against the
        // pre-lift `.map(|t| t.0)` chain so a timezone drift surfaces
        // here rather than as silent skew at the TTL-expiry decision
        // on the SAME `Process` within one reconcile pass.
        let now = Utc::now();
        let anchor = now - chrono::Duration::seconds(120);
        let p = creation_stamped_process(anchor);
        let via_primitive = p.created_at().expect("populated slot");
        let via_pre_lift = p
            .metadata
            .creation_timestamp
            .as_ref()
            .map(|t| t.0)
            .expect("populated slot");
        assert_eq!(
            now.signed_duration_since(via_primitive),
            now.signed_duration_since(via_pre_lift)
        );
    }
}
