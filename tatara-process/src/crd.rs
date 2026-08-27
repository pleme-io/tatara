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
}
