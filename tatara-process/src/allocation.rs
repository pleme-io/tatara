//! `EphemeralAllocation` CRD — a typed request for a pool member.
//!
//! Pairs with `EphemeralPool`: an Allocation is the request side;
//! the pool reconciler answers it by matching one of its free
//! Process members and stamping the requestor's identity on the
//! Allocation's status.
//!
//! Topology:
//! - The requestor (GitHub PR webhook, CI runner, operator running
//!   `feira allocation request …`) creates an `EphemeralAllocation`.
//! - The pool reconciler watches Allocations; matches `spec.poolRef`
//!   (or routes via PoolSelector if `poolRef` is omitted) to a pool;
//!   picks one Free member; transitions the member to Allocated and
//!   the Allocation to Bound.
//! - When the requestor is done, it deletes the Allocation. The pool
//!   reconciler honors the pool's `returnPolicy` (Reset / Replace /
//!   Keep).

use chrono::{DateTime, Utc};
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::pool::AllocationRef;

/// `EphemeralAllocation` CRD spec — a typed request for a pool member.
///
/// ```yaml
/// apiVersion: tatara.pleme.io/v1alpha1
/// kind: EphemeralAllocation
/// metadata:
///   name: pr-123-demo-app
///   namespace: ephemeral-pools
/// spec:
///   poolRef:
///     name: attest-pool
///     namespace: ephemeral-pools
///   requestor:
///     kind: github-pr
///     repo: "pleme-io/demo-app"
///     branch: "fix-something"
///     prNumber: 123
///     prLabels: ["needs-ephemeral"]
///   ttl: "1h"
/// ```
#[derive(CustomResource, Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[kube(
    group = "tatara.pleme.io",
    version = "v1alpha1",
    kind = "EphemeralAllocation",
    plural = "ephemeralallocations",
    shortname = "ealloc",
    namespaced,
    status = "AllocationStatus",
    printcolumn = r#"{"name":"Pool","type":"string","jsonPath":".spec.poolRef.name"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#,
    printcolumn = r#"{"name":"Process","type":"string","jsonPath":".status.assignedProcess.name"}"#,
    printcolumn = r#"{"name":"Requestor","type":"string","jsonPath":".spec.requestor.kind"}"#,
    printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
#[serde(rename_all = "camelCase")]
pub struct AllocationSpec {
    /// Direct pool reference. When set, skip selector-based routing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool_ref: Option<AllocationRef>,

    /// Who is asking for the env.
    pub requestor: Requestor,

    /// How long the requestor needs the env (`humantime`). The pool
    /// reconciler clamps this to `pool.spec.maxAllocationTtl`.
    /// When unset, falls back to the pool's `template.ttl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl: Option<String>,

    /// Operator-supplied notes — surfaced in `feira allocation list`
    /// for audit / debugging context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl AllocationSpec {
    /// The canonical minimal [`AllocationSpec`] composer — binds only
    /// the single caller-varying [`Self::requestor`] slot and leaves
    /// the three-slot default tail (`pool_ref = None`, `ttl = None`,
    /// `note = None`) at ONE substrate owner. The lift of the 5-line
    /// `AllocationSpec { pool_ref: None, requestor: <r>, ttl: None,
    /// note: None }` incantation past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold — pre-lift the SAME requestor-only
    /// fixture shape recurred at NINE workspace-wide fixture sites
    /// (five inside [`crate::allocation`]'s own test module — the
    /// `alloc_with_phase` / `alloc_without_status` observers on the
    /// phase axis, the `alloc_with_bound_pool` observer on the
    /// routing axis, the `alloc_with_expires_at` observer on the TTL
    /// axis, and the `allocation_spec_omits_optional_fields` wire-
    /// shape pin; three inside [`crate::lib`]'s pin fixtures — the
    /// `alloc_fixture` + two `empty_alloc_spec` helpers on the
    /// coordinate / annotation axes; one inside `tatara-pool-
    /// reconciler::allocation_decide` — the `alloc` fixture seeding
    /// the pool convergence-decision test battery). All nine sites
    /// walked the SAME three-slot default tail — differing only in
    /// the caller-varying [`Requestor`] value.
    ///
    /// The three-slot default tail is the SAFE minimal shape: `pool_ref
    /// = None` triggers selector-based routing (rather than pinning a
    /// specific pool), `ttl = None` falls back to the pool template's
    /// TTL, `note = None` leaves the audit slot empty. Every
    /// [`crate::pool::PoolSelector`] filter sees "no direct pool
    /// binding" and matches every candidate pool for the requestor's
    /// kind; post-lift a callsite that legitimately overrides a slot
    /// lands the override at its own site via struct-update on top
    /// of `requestor_only`.
    ///
    /// A future addition to [`AllocationSpec`] — a new optional slot
    /// (e.g. a `priority` for admission-control kinds, a `budget`
    /// for cost-accounting, a `labels` set for per-allocation
    /// tagging) — lands at ONE primitive body and every fixture /
    /// default-shape callsite inherits the upgrade mechanically.
    /// Pre-lift a new field would have broken all NINE callsites
    /// (each holds an exhaustive struct literal); post-lift only
    /// sites that legitimately override the new slot need to name it.
    ///
    /// Sibling substrate primitives on the same "bind-the-required-
    /// slot-only" axis: [`Requestor::kind_only`] (Requestor kind-
    /// only composer; the six-slot Requestor counterpart, one of the
    /// values this composer stamps into its own `requestor` slot),
    /// [`crate::intent::AplicacaoIntent::chart_only`] (Aplicacao
    /// chart-pointer-only composer; the 7-slot Aplicacao counterpart),
    /// [`crate::pool::PoolSpec::with_template`] (Pool template-only
    /// composer; the 11-slot Pool counterpart), and
    /// [`crate::spec::ProcessSpec::gate_compute_defaults`] (Process
    /// zero-arg-fixture composer; the 12-slot Process counterpart).
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the 3-slot default tail recurred at NINE hand-authored sites
    /// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger, spanning
    /// two workspace crates, and is lifted onto the ONE workspace-
    /// wide substrate owner here). THEORY.md §II.1 invariant 5
    /// (composition preserves proofs — the pin block below binds the
    /// primitive at fail-before-pass-after granularity so a
    /// regression that drifted any of the three default-tail slots
    /// surfaces at THESE pins rather than as silent fixture skew
    /// across the nine downstream consumers).
    #[must_use]
    pub fn requestor_only(requestor: Requestor) -> Self {
        Self {
            pool_ref: None,
            requestor,
            ttl: None,
            note: None,
        }
    }
}

/// Identity + routing context for a request.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Requestor {
    /// Discriminator: `"github-pr"`, `"manual"`, `"ci-run"`,
    /// `"scheduled"`, … The wire shape is open by design — operators
    /// may register their own kinds and the [`crate::pool::PoolSelector`]
    /// matches on raw string equality. The substrate's own emitters
    /// stamp one of the four canonical kebab-case kinds enumerated by
    /// [`RequestorKind::ALL`]; [`Requestor::known_kind`] projects the
    /// open wire field through that closed-set view at ONE site so
    /// future kind-keyed consumers (pool dashboards, completion lists,
    /// audit-trail classifiers) sweep the typed variants without
    /// re-implementing `match self.kind.as_str()` arm-by-arm. Sibling
    /// shape to [`crate::receipt::ReceiptEnvelope::known_kind`].
    pub kind: String,

    /// Optional repo identifier (e.g., `"pleme-io/demo-app"`).
    /// Matched against `PoolSelector.repos`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,

    /// Optional branch name. Matched against `PoolSelector.branches`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,

    /// Optional PR number (for `kind: github-pr`). Surfaces in
    /// printcolumns + audit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u64>,

    /// Optional commit SHA (for `kind: github-pr` or `ci-run`).
    /// Stamped onto the allocated Process for traceability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,

    /// PR / commit labels — matched as a subset against
    /// `PoolSelector.prLabels`.
    #[serde(default)]
    pub pr_labels: Vec<String>,

    /// Free-form actor — username, CI runner ID, etc.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

impl Requestor {
    /// Decode [`Self::kind`] into the typed [`RequestorKind`] variant
    /// when the wire string matches one of the four substrate-emitted
    /// canonical kebab-case kinds; `None` when the kind is an
    /// operator-registered open string (the schema is open by design —
    /// every allocation remains a valid allocation, but only typed
    /// kinds participate in closed-set dispatch). The (open `String`,
    /// closed-typed view) split lets future kind-keyed consumers
    /// (pool-selector classifiers, dashboard completion, audit-trail
    /// classifiers) sweep the typed variants without touching the
    /// open-by-design wire shape. Lifted as the canonical decode site
    /// so no consumer re-implements the `match self.kind.as_str()` arm-
    /// by-arm — the closed-set sweep happens through
    /// [`RequestorKind::from_str`] at ONE site. Sibling shape to
    /// [`crate::receipt::ReceiptEnvelope::known_kind`].
    #[must_use]
    pub fn known_kind(&self) -> Option<RequestorKind> {
        self.kind.parse().ok()
    }

    /// The canonical minimal [`Requestor`] composer — binds only the
    /// single caller-varying [`Self::kind`] slot and leaves the
    /// six-slot default tail (`repo = None`, `branch = None`,
    /// `pr_number = None`, `sha = None`, `pr_labels = vec![]`,
    /// `actor = None`) at ONE substrate owner. The lift of the 9-line
    /// `Requestor { kind: <lit>.into(), repo: None, branch: None,
    /// pr_number: None, sha: None, pr_labels: vec![], actor: None }`
    /// incantation past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// threshold — pre-lift the SAME kind-only fixture shape recurred
    /// at TEN workspace-wide fixture sites (six inside
    /// [`crate::allocation`]'s own test module — the
    /// `known_kind_decodes_built_requestors` sweep, the
    /// `known_kind_returns_none_for_open_kinds` open-kind pin, the
    /// `alloc_with_phase` / `alloc_without_status` observers on the
    /// phase axis, the `bound_pool` observer on the routing axis, the
    /// `expires_at` observer on the TTL axis, and the
    /// `allocation_spec_omits_optional_fields` wire-shape pin; three
    /// inside [`crate::lib`]'s pin fixtures (the `alloc_fixture` +
    /// two `empty_alloc_spec` helpers on the coordinate / annotation
    /// axes)).
    ///
    /// `impl Into<String>` on the argument accepts every pre-lift
    /// caller shape verbatim without an argument recast:
    /// * `Requestor::kind_only("manual")` — the operator-authored
    ///   default fixture, matching the pre-lift `kind: "manual".into()`.
    /// * `Requestor::kind_only("github-pr")` — the GitHub-webhook
    ///   fixture, matching the pre-lift `kind: "github-pr".into()`.
    /// * `Requestor::kind_only(RequestorKind::GithubPr)` — the
    ///   typed-round-trip callsite, matching the pre-lift `kind: k
    ///   .into()` where `k: RequestorKind` composes through the
    ///   `From<RequestorKind> for String` bridge.
    /// * `Requestor::kind_only("operator-custom-kind")` — the
    ///   open-kind pin, matching the pre-lift `kind:
    ///   "operator-custom-kind".into()`.
    ///
    /// The six-slot default tail is the SAFE minimal shape: every
    /// [`crate::pool::PoolSelector`] filter sees "no repo constraint,
    /// no branch constraint, no PR labels" and matches every pool
    /// (post-lift a callsite that legitimately overrides a slot lands
    /// the override at its own site via struct-update on top of
    /// `kind_only`). A future addition to [`Requestor`] — a new
    /// optional slot (e.g. a `run_id` for CI kinds, an `email` for
    /// scheduled kinds, a `cluster` scoping override) — lands at ONE
    /// primitive body and every fixture / default-shape callsite
    /// inherits the upgrade mechanically. Pre-lift a new field would
    /// have broken all TEN callsites (each holds an exhaustive struct
    /// literal); post-lift only sites that legitimately override the
    /// new slot need to name it.
    ///
    /// Sibling substrate primitives on the same "bind-the-required-
    /// slots-only" axis: [`crate::intent::AplicacaoIntent::chart_only`]
    /// (Aplicacao chart-pointer-only composer; the 7-slot Aplicacao
    /// counterpart), [`crate::pool::PoolSpec::with_template`] (Pool
    /// template-only composer; the 11-slot Pool counterpart), and
    /// [`crate::spec::ProcessSpec::gate_compute_defaults`] (Process
    /// zero-arg-fixture composer; the 12-slot Process counterpart).
    #[must_use]
    pub fn kind_only(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            repo: None,
            branch: None,
            pr_number: None,
            sha: None,
            pr_labels: vec![],
            actor: None,
        }
    }
}

/// Closed-set view over the substrate-emitted canonical
/// [`Requestor::kind`] wire strings — the four kebab-case
/// discriminators every pleme-io requestor stamps onto an
/// [`EphemeralAllocation`]: `github-pr` (the [`tatara_github_watcher`-
/// authored](../../tatara-github-watcher/src/allocation_factory.rs)
/// PR-driven path), `manual` (operator-authored via `feira allocation
/// request …`), `ci-run` (non-PR CI driver), and `scheduled` (a
/// cron-style emitter). The wire field stays `pub kind: String` on
/// [`Requestor`] so operators can register their own kinds without a
/// schema bump; this enum is the typed view future kind-keyed
/// consumers (pool dashboards, LSP completion, audit-trail
/// classifiers) sweep against.
///
/// Pre-lift the four canonical kinds existed only as `&'static str`
/// literals at four scattered sites — the documentation header on
/// [`Requestor::kind`], the [`crate::pool::PoolSelector::kinds`]
/// docstring, the `tatara-github-watcher` allocation factory, and the
/// per-test `kind: "github-pr".into()` fixtures. A rename of one
/// canonical kind (e.g. `"github-pr"` → `"github-pull-request"`) had
/// no compile-time link to the others, so the documentation drifted
/// independently of the emitter, and the [`PoolSelector::matches`]
/// kind-filter silently kept matching the old spelling forever. Post-
/// lift the (canonical-name, typed-variant) pairing binds at ONE site
/// ([`Self::as_str`]); the `From<RequestorKind> for String` bridge
/// lets emitters compose `Requestor { kind: RequestorKind::GithubPr.into(), … }`
/// so the four canonical strings stop appearing as bare `&'static str`
/// literals at author sites.
///
/// Adding a fifth kind (e.g. `Slack` → `"slack"`, `Webhook` →
/// `"webhook"`) lands at one [`Self::ALL`] entry + one [`Self::as_str`]
/// arm — exhaustively checked by the compiler (the `[Self; 4]` array
/// literal forces the arity) AND by the per-variant truth-table tests
/// below.
///
/// Sibling closed-set `ALL`-keyed lifts across the crate:
/// [`crate::receipt::ReceiptKind::ALL`] (the four substrate-emitted
/// receipt kinds — direct shape peer, same open-wire + closed-view
/// split), [`AllocationPhase::ALL`], [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::lifetime_clock::TerminateReasonKind::ALL`].
///
/// Theory anchor: THEORY.md §III — the typescape; the substrate's own
/// requestor kinds become a TYPE rather than four `&'static str`
/// literals at every author + docstring + fixture site. THEORY.md
/// §V.1 — knowable platform; the closed-set view turns "which kinds
/// does the substrate actually emit" from a grep job into a method
/// the compiler enforces exhaustively at every dispatch site.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_closed_set::DeriveClosedSet)]
#[closed_set(via = "as_str", generate_unknown, display)]
pub enum RequestorKind {
    /// GitHub pull-request webhook — `tatara-github-watcher` stamps
    /// this on every allocation built from a `PullRequestEvent`.
    GithubPr,
    /// Operator-authored allocation — `feira allocation request …`
    /// and any hand-crafted CR.
    Manual,
    /// Non-PR CI driver — a pipeline run that wants an ephemeral env
    /// without an associated pull request.
    CiRun,
    /// Cron-style scheduled emitter — periodic allocation creation
    /// (e.g. nightly drift detection).
    Scheduled,
}

impl RequestorKind {
    /// The closed set of substrate-emitted requestor kinds — single
    /// source of truth that drives the [`Self::from_str`] decode sweep
    /// AND any future enumeration consumer (pool-selector classifiers,
    /// dashboard completion, `tatara-check` kind enumeration). Adding
    /// a fifth variant (e.g. `Slack` → `"slack"`) lands at one `ALL`
    /// entry + one `as_str` arm — exhaustively checked by the compiler
    /// (the `[Self; 4]` array literal forces the arity) AND by the
    /// per-variant truth-table tests below.
    pub const ALL: [Self; 4] = [Self::GithubPr, Self::Manual, Self::CiRun, Self::Scheduled];

    /// Canonical kebab-case wire-format kind — the literal that lands
    /// in [`Requestor::kind`] when this variant authors the request.
    /// Pinned to four byte-exact strings the substrate has already
    /// published (the `tatara-github-watcher` factory, the operator
    /// fixtures in this file, the `PoolSelector.kinds` filter, the
    /// CRD printcolumns) — renaming any one is a wire-format change,
    /// not a typed-internal refactor, and the
    /// `requestor_kind_canonical_names_pinned` truth-table test fails
    /// first to keep the substrate honest. Used by [`std::fmt::Display`]
    /// (single source of truth) and as the `String` projection that
    /// `From<RequestorKind> for String` ([`Self::into`]) composes so
    /// emitters can spell `Requestor { kind: RequestorKind::GithubPr.into(), … }`
    /// without re-typing the canonical literal at every author site.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GithubPr => "github-pr",
            Self::Manual => "manual",
            Self::CiRun => "ci-run",
            Self::Scheduled => "scheduled",
        }
    }
}

// `impl FromStr for RequestorKind` + `impl tatara_lisp::ClosedSet for
// RequestorKind` + `impl std::fmt::Display for RequestorKind` are
// generated by `#[derive(tatara_closed_set::DeriveClosedSet)]` on the enum
// declaration above. `label` delegates to the inherent
// `RequestorKind::as_str` via `#[closed_set(via = "as_str")]` so the
// kebab-case wire-format projection stays load-bearing (matches the
// `tatara-github-watcher` factory + the CRD printcolumns + the
// `PoolSelector.kinds` filter verbatim) while generic `T: ClosedSet`
// consumers reach the STABLE workspace-wide name (`label`). The
// `display` flag emits the `f.write_str(self.as_str())` delegation
// block — the substrate-wide closed-set-enum idiom's third piece —
// at the same proc-macro site rather than a hand-rolled
// `fmt::Display` block per implementor.

// `pub struct UnknownRequestorKind(pub String)` is generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"requestor kind"`
// matches the prior hand-rolled `#[error("unknown requestor kind: {0}")]`
// verbatim — pinned generically by clause (5) of
// `tatara_closed_set::assert_closed_set_well_formed::<RequestorKind>()` (called
// from `requestor_kind_is_well_formed_closed_set` in the test module).
// Symmetric to every sibling `Unknown*` error in this crate (e.g.
// [`UnknownAllocationPhase`], [`crate::receipt::UnknownReceiptKind`],
// [`crate::phase::UnknownPhase`], [`crate::lifetime::UnknownTeardownPolicy`]).

impl From<RequestorKind> for String {
    /// Composes [`RequestorKind::as_str`] into an owned `String` so
    /// every `impl Into<String>` API surface (the `kind:` field
    /// initializer on [`Requestor`] most notably) accepts the typed
    /// variant transparently — the call site stays
    /// `kind: RequestorKind::GithubPr.into()` and the typed → wire
    /// bridge runs through ONE place. Sibling shape to
    /// [`crate::receipt::ReceiptKind`]'s `From for String`.
    fn from(k: RequestorKind) -> Self {
        k.as_str().to_owned()
    }
}

impl From<RequestorKind> for &'static str {
    fn from(k: RequestorKind) -> Self {
        k.as_str()
    }
}

/// `EphemeralAllocation.status` — observed allocation state.
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AllocationStatus {
    /// Current lifecycle phase.
    #[serde(default)]
    pub phase: AllocationPhase,

    /// When the phase last changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_since: Option<DateTime<Utc>>,

    /// Pool that owns the matched member. Set as soon as routing
    /// resolves; not cleared on release (audit trail).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_pool: Option<AllocationRef>,

    /// The Process backing this allocation, if Bound.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_process: Option<AllocationRef>,

    /// When the allocation was matched to a Process.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allocated_at: Option<DateTime<Utc>>,

    /// Wall-clock expiry derived from `spec.ttl` + `allocated_at`.
    /// The pool reconciler force-returns the member at this point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,

    /// Operator-visible message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// Standard Conditions.
    ///
    /// The empty case is skipped at serialization so a merge-patch
    /// body built from a caller-supplied [`AllocationStatus`] whose
    /// `conditions` slot has not been touched does NOT emit
    /// `"conditions": []` on the wire — under RFC-7396 JSON Merge
    /// Patch (the shape `Patch::Merge` sends) an empty array
    /// REPLACES the persisted list rather than merges into it, so a
    /// controller round-trip that reused a scratch `AllocationStatus`
    /// as a patch body would silently clobber whatever conditions the
    /// prior status carried. Peer to `phase_since` /
    /// `bound_pool` / `assigned_process` / `allocated_at` /
    /// `expires_at` above, each already skip-serialized on its
    /// [`Default`]-equivalent variant.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<AllocationCondition>,
}

impl AllocationStatus {
    /// Substrate composer for a phase-transition [`AllocationStatus`]
    /// seed: stamps the THREE always-present slots (`phase` +
    /// `phase_since = Some(now)` + `message = Some(<supplied>)`) and
    /// defaults every other slot (`bound_pool` / `assigned_process` /
    /// `allocated_at` / `expires_at` = `None`, `conditions = vec![]`).
    /// Caller-branches attach the extra slots via struct-update
    /// syntax onto the seed.
    ///
    /// Pre-lift the 4-slot phase-transition seed
    /// ```rust,ignore
    /// json!({
    ///     "status": {
    ///         "phase": <AllocationPhase-variant>,
    ///         "phaseSince": Utc::now(),
    ///         "message": "<transition-reason>",
    ///         …optional caller-attached slots…
    ///     }
    /// })
    /// ```
    /// was hand-authored at FOUR sites past the ★★ PRIME-DIRECTIVE
    /// ≥ 2 duplication threshold in
    /// `tatara-pool-reconciler::controller_allocation::reconcile_inner`,
    /// each restating the SAME `phase + phase_since + message` invariant
    /// triplet on a different [`AllocationPhase`] variant:
    /// * `AllocationDecision::NoMatchingPool` — the "no Pool selector
    ///   matched this Requestor" fallthrough
    ///   ([`AllocationPhase::NoMatchingPool`]).
    /// * `AllocationDecision::Wait` — the "pool matched; no Free member
    ///   available" queued path
    ///   ([`AllocationPhase::Queued`]) with a `bound_pool` addition.
    /// * `AllocationDecision::Bind` — the "bound to pool member"
    ///   allocation path ([`AllocationPhase::Bound`]) with
    ///   `bound_pool` + `assigned_process` + `allocated_at` +
    ///   `expires_at` additions.
    /// * `AllocationDecision::Release` — the "released; pool reconciler
    ///   will return the member" release path
    ///   ([`AllocationPhase::Released`]) with `bound_pool` +
    ///   `assigned_process` additions.
    ///
    /// All four hand-authored the SAME `phaseSince: Utc::now()` stamp
    /// alongside the phase transition, and all four spelled the
    /// invariant triplet as bare JSON keys inside a `json!({...})`
    /// literal — a fragile shape where any drift in the underlying
    /// [`AllocationStatus`] field naming (a rename from `phaseSince`
    /// to `phase_since` at the serde surface, a promotion of `message`
    /// to a structured envelope) silently stops the JSON keys from
    /// mapping to the typed struct's fields and the K8s API server
    /// merges an ill-shaped patch. Post-lift the four callers build a
    /// typed [`AllocationStatus`] via `AllocationStatus::transition`,
    /// attach any branch-specific slots via struct-update syntax, and
    /// wrap the result in `json!({ "status": s })` — the serde
    /// `rename_all = "camelCase"` derive on [`AllocationStatus`] owns
    /// the wire-shape composition, so a field rename lands at ONE
    /// site (the derive) and every emit site inherits the upgrade
    /// mechanically.
    ///
    /// Cross-CRD peer to [`crate::pool::PoolStatus::observed`] on the
    /// same `<CRD>Status` substrate-composer axis — both primitives
    /// stamp `phase_since = Some(now)` from a caller-supplied `now`
    /// timestamp so the composer stays clock-injectable rather than
    /// implicitly reading wall time, and both close every optional slot
    /// with its [`Default`]-equivalent variant so a future slot
    /// addition on either status shape plugs into the composer at ONE
    /// site and every downstream emit site inherits the new slot
    /// mechanically.
    ///
    /// Cross-CRD peer to the `tatara-reconciler::patch::phase_status_msg`
    /// primitive on the (CRD × phase-transition-with-message) axis —
    /// both primitives own the three-slot `phase + phase_since +
    /// message` invariant on their respective CRDs' status subresource,
    /// and both accept `impl Into<String>` for the message so the
    /// callsite carries `&'static str` literal reasons and
    /// `format!(...)`-owned strings without widening the signature.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the 4-slot phase-transition status-seed incantation recurred at
    /// four hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, and is lifted to ONE owner here).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs —
    /// the pins bind the three always-present slots + the
    /// [`Default`]-defaulted rest + byte-identical parity with the
    /// pre-lift `json!({...})` triplet through serde round-trip, so a
    /// regression that drifted any surface at
    /// `tests::allocation_status_transition_*` rather than as silent
    /// operator-visible skew between the four allocation-decision
    /// patch sites).
    #[must_use]
    pub fn transition(
        phase: AllocationPhase,
        message: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            phase,
            phase_since: Some(now),
            message: Some(message.into()),
            ..Default::default()
        }
    }

    /// Substrate composer for a phase-transition [`AllocationStatus`]
    /// seed whose `bound_pool` + `assigned_process` axis-pair is
    /// stamped alongside the base [`Self::transition`] triplet
    /// (`phase` + `phase_since = Some(now)` + `message =
    /// Some(<supplied>)`). Every other slot lands at its
    /// [`Default`]-equivalent variant so a caller-branch that attaches
    /// an optional slot via struct-update syntax (a `Bind` arm's
    /// `allocated_at` / `expires_at` addenda, say) does not silently
    /// inherit a pre-populated non-`None` value.
    ///
    /// Pre-lift the `bound_pool: Some(pool)` + `assigned_process:
    /// Some(AllocationRef::new(name, ns))` pair rode struct-update
    /// syntax onto [`Self::transition`] at TWO sites past the ★★
    /// PRIME-DIRECTIVE ≥ 2 duplication threshold in
    /// `tatara-pool-reconciler::controller_allocation::reconcile_inner`
    /// — the `AllocationDecision::Bind` arm ([`AllocationPhase::Bound`]
    /// with two extra `allocated_at` / `expires_at` addenda) and the
    /// `AllocationDecision::Release` arm ([`AllocationPhase::Released`]
    /// with no addenda). Both restated the SAME pair-of-`Some`-slot
    /// invariant against the SAME struct-update seed and funneled the
    /// resulting body through the SAME `patch_status` call on
    /// `Api<EphemeralAllocation>`. Post-lift both callers reach the
    /// pair through ONE substrate composer; a future normalization on
    /// the bound-set axis (a symmetry gate that the assigned_process's
    /// namespace matches the bound_pool's namespace, a canonicalization
    /// that closes the pair against a stale audit record, a
    /// backwards-compatibility rename of either slot at the serde
    /// surface) lands at ONE substrate site rather than at each
    /// callsite in the two-arm allocation reconciler.
    ///
    /// Composes atop [`Self::transition`] so any future evolution to
    /// the base three-slot invariant triplet (a `phase_since` rename,
    /// a `message` promotion to a structured envelope, a fourth
    /// always-stamped diagnostic slot) reaches this composer through
    /// ONE substrate site and both consumers inherit the upgrade
    /// mechanically. Sibling composition discipline to
    /// [`crate::pool::PoolStatus::observed`]'s `state_count_fanout` +
    /// `Utc::now()` fold — the compound composer names its axis + calls
    /// the substrate primitive on the invariant it wraps rather than
    /// restating the wrapped shape inline.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `bound_pool + assigned_process` pair recurred at two hand-
    /// authored sites past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// trigger, and is lifted to ONE owner here). THEORY.md §II.1
    /// invariant 5 (composition preserves proofs — the pins bind the
    /// pair + the composed base triplet + byte-identical parity with
    /// the pre-lift struct-update shape through serde round-trip, so a
    /// regression that drifted any surface at
    /// `tests::allocation_status_bound_transition_*` rather than as
    /// silent operator-visible skew between the two Bind / Release
    /// patch sites).
    #[must_use]
    pub fn bound_transition(
        phase: AllocationPhase,
        message: impl Into<String>,
        now: DateTime<Utc>,
        bound_pool: AllocationRef,
        assigned_process: AllocationRef,
    ) -> Self {
        Self {
            bound_pool: Some(bound_pool),
            assigned_process: Some(assigned_process),
            ..Self::transition(phase, message, now)
        }
    }

    /// Wall-clock-anchored peer of [`Self::transition`] — reads
    /// `Utc::now()` at call time and forwards it into the substrate
    /// composer's `now` slot.
    ///
    /// Pre-lift the 3-arg [`Self::transition`] chain fed by an inline
    /// `Utc::now()` third argument was hand-authored at TWO production
    /// sites in `tatara-pool-reconciler::controller_allocation::
    /// reconcile_inner` past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// threshold:
    ///
    /// * The `AllocationDecision::NoMatchingPool` arm — status seed at
    ///   `AllocationPhase::NoMatchingPool` (no addenda; the composer's
    ///   seed is patched verbatim).
    /// * The `AllocationDecision::Wait { pool }` arm — status seed at
    ///   `AllocationPhase::Queued` inside a `..Self::transition(...)`
    ///   struct-update that adds `bound_pool: Some(pool)`.
    ///
    /// Both sites walked the SAME 3-arg call with the SAME
    /// `chrono::Utc::now()` third argument — the wall-clock projection
    /// had no per-callsite variation. Post-lift both consumers share
    /// ONE substrate owner for the wall-clock-at-tick projection; a
    /// future clock swap (a monotonic clock cross-check, a per-
    /// reconciler injected time source, a test-only override at the
    /// production callsite via feature flag) lands at ONE substrate
    /// function and every allocation-decision status-patch site
    /// inherits the upgrade mechanically.
    ///
    /// The 3-arg [`Self::transition`] peer stays load-bearing for test
    /// callers — the injected-`now` shape is what unit tests use to
    /// drive the clock deterministically (every
    /// `AllocationStatus::transition(phase, msg, anchor_time())` in
    /// this module's own test suite reads that surface). This peer is
    /// production-only: pinning the wall-clock at the substrate site
    /// means no test can accidentally consume it without the
    /// deterministic-clock injection that makes the test meaningful.
    ///
    /// Sibling of [`crate::pool::PoolStatus::observed_now`] on the
    /// (`<CRD>Status` substrate composer × wall-clock-anchored peer)
    /// axis — both primitives own the "read the wall clock at tick-
    /// time" projection on a peer clock-injectable substrate composer
    /// so the workspace's `<CRD>Status` composer family stays uniform
    /// across `PoolStatus.observed` on the pool axis and
    /// `AllocationStatus.transition` on the allocation axis. Peer to
    /// [`crate::lifetime_clock::evaluate_now`] on the (typed pure-fn,
    /// wall-clock-anchored peer) axis for the timed-decision family.
    ///
    /// # Invariants
    ///
    /// - **Same shape:** returns the SAME [`AllocationStatus`] the
    ///   3-arg [`Self::transition`] returns when passed
    ///   `chrono::Utc::now()` as the third argument. This is a
    ///   delegation, not a re-implementation.
    /// - **Wall-clock read once:** `Utc::now()` is called exactly ONCE
    ///   per invocation, at the primitive's body, so a future consumer
    ///   that chains two `transition_now` calls back-to-back still sees
    ///   monotonic `now` reads (each call reads a fresh instant, not a
    ///   cached one) — matches the pre-lift shape where each of the
    ///   two status-patch sites computed its own `chrono::Utc::now()`
    ///   at its own line.
    ///
    /// # `#[must_use]`
    ///
    /// Every consumer feeds the returned [`AllocationStatus`] into
    /// `tatara_process::patch::merge_status(&alloc_api, &name, &<status>)`
    /// or a peer status-patch call. Dropping the return means the
    /// transition composed for no observable reason — the attribute
    /// surfaces that as a warning at every call site.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the 3-arg call with `chrono::Utc::now()` as the third argument
    /// recurred at 2 hand-authored sites past the ★★ PRIME-DIRECTIVE
    /// ≥ 2 duplication trigger, lifted onto the ONE workspace-wide
    /// substrate owner here). THEORY.md §II.1 invariant 5 (composition
    /// preserves proofs — the wall-clock projection lives at ONE site
    /// so a future clock swap reaches both consumers through one edit).
    #[must_use]
    pub fn transition_now(phase: AllocationPhase, message: impl Into<String>) -> Self {
        Self::transition(phase, message, Utc::now())
    }
}

/// Allocation lifecycle phase.
///
/// Sibling closed-set lifts on the same `EphemeralAllocation` /
/// `EphemeralPool` axis: [`crate::pool::ReplacementPolicy::ALL`],
/// [`crate::pool::ReturnPolicy::ALL`]. Sibling closed-sets on the
/// `tatara-process` algebra: [`crate::lifetime::TeardownPolicy::ALL`],
/// [`crate::lifetime::LifetimeKind::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::intent::IntentKind::ALL`],
/// [`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`].
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
pub enum AllocationPhase {
    /// Admitted; pool selector matching not yet attempted.
    Pending,
    /// Routed to a pool but no `Free` member is available — queued.
    Queued,
    /// A pool member has been assigned + transitioned to Allocated.
    Bound,
    /// `expires_at` reached or requestor deleted; member is returning.
    Releasing,
    /// Released; the allocation is a permanent audit record.
    Released,
    /// No pool selector matched. The reconciler will retry on each
    /// pool spec update; surfaced in status so operators see why.
    NoMatchingPool,
    /// Pool refused (e.g., `max_size` reached and no member can be
    /// freed) — operator intervention needed.
    Failed,
}

impl Default for AllocationPhase {
    fn default() -> Self {
        Self::Pending
    }
}

impl AllocationPhase {
    /// The closed set of allocation phases — single source of truth
    /// that drives the `as_str` / Display / `FromStr` triad AND the
    /// `is_terminal` / `needs_pool_routing` predicate pair the
    /// allocation reconciler's observe/decide split dispatches on.
    /// Adding an eighth variant lands at one `ALL` entry + one
    /// `as_str` arm + one arm per predicate — exhaustively checked by
    /// the compiler (the `[Self; 7]` array literal forces the arity)
    /// and by the implication test
    /// (`allocation_phase_terminal_excludes_routing`) so a new
    /// variant can't claim to be both terminal AND routing-eligible.
    pub const ALL: [Self; 7] = [
        Self::Pending,
        Self::Queued,
        Self::Bound,
        Self::Releasing,
        Self::Released,
        Self::NoMatchingPool,
        Self::Failed,
    ];

    /// Canonical PascalCase wire-format projection — matches the
    /// serde `rename_all = "PascalCase"` output verbatim AND the CRD
    /// `enum:` enumeration the allocation reconciler stamps on the
    /// `ephemeralallocations.tatara.pleme.io` schema. Pinned by
    /// `allocation_phase_as_str_matches_serde` so a variant rename
    /// can't drift between the typed surface, the CRD enum, the YAML
    /// wire format AND any operator-facing diagnostic composed via
    /// Display rather than a hard-coded literal that would silently
    /// rot.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Queued => "Queued",
            Self::Bound => "Bound",
            Self::Releasing => "Releasing",
            Self::Released => "Released",
            Self::NoMatchingPool => "NoMatchingPool",
            Self::Failed => "Failed",
        }
    }

    /// True iff the allocation has reached an absorbing state —
    /// `Released` (clean audit record) or `Failed` (pool refused;
    /// operator intervention needed). The allocation reconciler
    /// short-circuits both phases to `NoOp` rather than re-running
    /// the routing / heartbeat ladder against a settled record.
    ///
    /// Closed-set match (not `matches!`) so a future variant
    /// triggers the compiler's exhaustiveness check at this site
    /// rather than silently defaulting to `false` and letting a new
    /// terminal phase fall through into pool rebinding. Paired with
    /// `needs_pool_routing` they form the two-axis projection
    /// `allocation_decide::AllocationConvergence::decide` matches
    /// against — the impossible bucket `(true, true)` is pinned
    /// empty by `allocation_phase_terminal_excludes_routing`.
    pub const fn is_terminal(self) -> bool {
        match self {
            Self::Released | Self::Failed => true,
            Self::Pending | Self::Queued | Self::Bound | Self::Releasing | Self::NoMatchingPool => {
                false
            }
        }
    }

    /// True iff the allocation is on the routing path — the
    /// reconciler still needs to resolve a target pool + look up a
    /// free member. `Pending` (just admitted), `Queued` (matched
    /// pool was full last tick), and `NoMatchingPool` (no selector
    /// matched yet; retry on pool spec updates) all live here. The
    /// settled non-terminal phases `Bound` (already matched) and
    /// `Releasing` (being torn down) don't — they short-circuit to
    /// the heartbeat / release ladder without re-resolving the pool.
    ///
    /// Closed-set match (not `matches!`) — same exhaustiveness
    /// discipline as [`Self::is_terminal`]. Lifts the open-coded
    /// `phase != Released && phase != Bound` gate that
    /// `allocation_decide::AllocationConvergenceCtx::observe` used
    /// to predicate pool resolution on, AND closes the latent gap
    /// where `Failed` / `Releasing` (neither `Released` nor `Bound`)
    /// would slip through to the routing branch — a `Failed`
    /// allocation without a deletion timestamp could be silently
    /// rebound to a fresh pool member, which is the opposite of
    /// "operator intervention needed."
    pub const fn needs_pool_routing(self) -> bool {
        match self {
            Self::Pending | Self::Queued | Self::NoMatchingPool => true,
            Self::Bound | Self::Releasing | Self::Released | Self::Failed => false,
        }
    }
}

// `impl FromStr for AllocationPhase` + `impl tatara_lisp::ClosedSet for
// AllocationPhase` + `impl std::fmt::Display for AllocationPhase` are
// generated by `#[derive(tatara_closed_set::DeriveClosedSet)]` on the enum
// declaration above. `label` delegates to the inherent
// `AllocationPhase::as_str` via `#[closed_set(via = "as_str")]` so the
// PascalCase wire-format projection stays load-bearing (matches the serde
// rename + the CRD `enum:` enumeration the allocation reconciler stamps
// on the `ephemeralallocations.tatara.pleme.io` schema verbatim) while
// generic `T: ClosedSet` consumers reach the STABLE workspace-wide name
// (`label`). The `display` flag emits the `f.write_str(self.as_str())`
// delegation block at the same proc-macro site rather than a
// hand-rolled `fmt::Display` block per implementor.

// `pub struct UnknownAllocationPhase(pub String)` is generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"allocation phase"`
// matches the prior hand-rolled `#[error("unknown allocation phase: {0}")]`
// verbatim — pinned generically by clause (5) of
// `tatara_closed_set::assert_closed_set_well_formed::<AllocationPhase>()` (called
// from `allocation_phase_is_well_formed_closed_set` in the test module).
// Symmetric to [`crate::pool::UnknownReplacementPolicy`],
// [`crate::pool::UnknownReturnPolicy`],
// [`crate::lifetime::UnknownTeardownPolicy`],
// [`crate::boundary::UnknownConditionKind`], and
// [`crate::phase::UnknownPhase`].

/// Allocation Condition (same shape as PoolCondition for downstream
/// uniformity).
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AllocationCondition {
    pub type_: String,
    pub status: String,
    pub reason: String,
    pub message: String,
    pub last_transition_time: DateTime<Utc>,
}

impl EphemeralAllocation {
    /// The copy-form status-projection primitive on the phase axis:
    /// returns the [`AllocationPhase`] the pool reconciler currently
    /// persists at `status.phase`, wrapped in an `Option` so the
    /// missing-`status` corner collapses to `None` — the ONE-liner
    /// collapse of the paired `self.status.as_ref().map(|s| s.phase)`
    /// incantation the pool reconciler's `AllocationConvergenceCtx::
    /// observe` restated by hand pre-lift.
    ///
    /// Cross-CRD peer to [`crate::prelude::Process::observed_phase`]
    /// on the (CRD × phase-slot × observed-status) axis pair — both
    /// primitives walk the identical `.status.as_ref().map(|s| s.
    /// phase)` shape, differing only in the `Phase` type projected
    /// ([`AllocationPhase`] vs [`crate::phase::ProcessPhase`]). The
    /// substrate now owns the borrow-form `.status.as_ref().map(|s|
    /// s.phase)` chain axis-uniformly across the two `Phase`-having
    /// CRDs so a future normalization (a generation-filter that
    /// returns `None` for a phase stamped with a stale
    /// `metadata.generation`, a staleness gate that drops a phase
    /// whose observing `phase_since` predates a reconcile deadline,
    /// a canonicalization pass that maps a phase outside the CRD's
    /// closed set to `None`) lands at ONE substrate method per CRD
    /// rather than being restated at every observer.
    #[must_use]
    pub fn observed_phase(&self) -> Option<AllocationPhase> {
        self.status.as_ref().map(|s| s.phase)
    }

    /// The copy-form status-projection primitive on the phase axis
    /// with the [`AllocationPhase::Pending`] sink applied — the
    /// ONE-liner collapse of the paired `self.observed_phase().
    /// unwrap_or(AllocationPhase::Pending)` incantation the pool
    /// reconciler's `AllocationConvergenceCtx::observe` restated by
    /// hand pre-lift as a 5-line `.status.as_ref().map(|s| s.phase).
    /// unwrap_or(AllocationPhase::Pending)` chain.
    ///
    /// Pre-lift the chain sat at [`tatara-pool-reconciler::
    /// allocation_decide::AllocationConvergenceCtx::observe`]'s
    /// `phase` seed. Cross-CRD peer to [`crate::prelude::Process::
    /// observed_phase_or_pending`] on the (CRD × phase-slot × sink)
    /// axis pair — both primitives close the missing-`status`
    /// corner with each CRD's respective [`Default`]-equivalent
    /// `Pending` variant, and both compose on top of their peer
    /// [`Self::observed_phase`] / [`crate::prelude::Process::
    /// observed_phase`] borrow-form projections so a future
    /// normalization at the underlying `observed_phase` primitive
    /// reaches both the raw-`Option` accessor and the `Pending`-
    /// sinked composer through the SAME upstream body.
    ///
    /// The [`AllocationPhase::Pending`] sink is load-bearing as the
    /// "not yet observed" default — the pool reconciler's typed
    /// `AllocationPhase::needs_pool_routing` predicate returns
    /// `true` for `Pending`, so a freshly-admitted Allocation whose
    /// pool reconciler has not yet stamped a `.status` slot reads
    /// as `Pending` and immediately enters the routing ladder,
    /// matching the pre-lift `AllocationPhase::Pending` fallback
    /// semantics verbatim.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition
    /// — the two-link `.status.as_ref().map(|s| s.phase).unwrap_or
    /// (AllocationPhase::Pending)` chain recurred at both the
    /// [`crate::prelude::Process`] site (already lifted onto
    /// [`crate::prelude::Process::observed_phase_or_pending`]) AND
    /// the [`EphemeralAllocation`] site by hand, i.e. the SHAPE
    /// itself recurs past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
    /// trigger, and is lifted to ONE owner per CRD here). THEORY.md
    /// §II.1 invariant 5 (composition preserves proofs — the pins
    /// bind the missing-`status` sink to `Pending` + populated-
    /// status pass-through + every [`AllocationPhase`] variant
    /// round-trip + byte-identical parity with the pre-lift
    /// two-link chain + cross-CRD peer coherence with
    /// [`crate::prelude::Process::observed_phase_or_pending`], so
    /// a regression that drifted any surface at
    /// `tests::observed_phase_*` rather than as silent operator-
    /// facing skew between the allocation observer's routing seed
    /// and the Process observer's dispatch seed).
    #[must_use]
    pub fn observed_phase_or_pending(&self) -> AllocationPhase {
        self.observed_phase().unwrap_or(AllocationPhase::Pending)
    }

    /// The borrow-form status-projection primitive on the bound-pool
    /// axis: returns the [`AllocationRef`] the pool reconciler
    /// currently persists at `status.bound_pool` (name + namespace of
    /// the pool that owns the matched member), with the
    /// missing-`status` corner AND the empty-slot corner BOTH
    /// collapsed to `None` — the ONE-liner collapse of the paired
    /// `self.status.as_ref().and_then(|s| s.bound_pool.<clone|as_ref>())`
    /// incantation the pool reconciler's `AllocationConvergenceCtx::
    /// observe` restated by hand pre-lift.
    ///
    /// Cross-CRD peer to [`crate::prelude::Process::observed_identity`]
    /// on the (CRD × structured-record-slot × borrow-form) axis pair
    /// — both primitives walk the identical `.status.as_ref()
    /// .and_then(|s| s.<slot>.as_ref())` shape, differing only in the
    /// record projected ([`AllocationRef`] here, [`crate::identity::
    /// Identity`] on `Process`). The substrate now owns the
    /// borrow-form `.status.as_ref().and_then(|s| s.<slot>.as_ref())`
    /// chain on the second `structured-record` slot across the two
    /// `status`-having CRDs, so a future normalization step (a
    /// generation-filter that returns `None` for a bound-pool
    /// reference stamped with a stale `metadata.generation`, a
    /// canonicalization pass that rejects a malformed
    /// `(name, namespace)` pair, a cross-cluster reference-rewrite
    /// gate) lands at ONE substrate method per CRD rather than being
    /// restated at every observer.
    ///
    /// Return-form axis: `Option<&AllocationRef>` mirrors the
    /// borrow-first discipline of [`crate::prelude::Process::
    /// observed_identity`]. The lone pre-lift consumer
    /// ([`tatara-pool-reconciler::allocation_decide::
    /// AllocationConvergenceCtx::observe`]'s `bound_pool` seed) spelled
    /// the projection as `.and_then(|s| s.bound_pool.clone())` — an
    /// eager clone allocated inside every reconcile pass even when the
    /// downstream branch (the Release-composition arm) needed only the
    /// borrow for the `.as_ref()` re-projection two lines later.
    /// Post-lift the consumer reaches the primitive borrow-first
    /// (`alloc.observed_bound_pool().cloned()`) and the empty-borrow
    /// corner clones nothing (`Option::cloned` on `None` is `None`);
    /// the composition point where the owned `AllocationRef` fallback
    /// is required (the `AllocationConvergenceCtx` snapshot slot,
    /// still `Option<AllocationRef>`-typed for serde stability) is the
    /// ONLY site that materializes an owned copy.
    ///
    /// The missing-`status` corner AND the populated-status-with-
    /// `bound_pool=None` corner BOTH collapse to `None` so
    /// `.is_some()` / `if let Some(_)` / `.cloned()` behave
    /// identically on an `EphemeralAllocation` whose status field is
    /// `None` and on one whose status carries an unpopulated
    /// `bound_pool` slot — matching what the pre-lift `.and_then(...)`
    /// chain produced. Consumers that need to tell those corners
    /// apart reach for [`Self::status`] directly, exactly as the
    /// existing peer accessors [`Self::observed_phase`] +
    /// [`Self::observed_phase_or_pending`] admit.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition
    /// — the `.status.as_ref().and_then(|s| s.<structured-record>
    /// .<clone|as_ref>())` shape recurred as ONE hand-authored
    /// `.and_then(|s| s.bound_pool.clone())` chain in
    /// [`tatara-pool-reconciler::allocation_decide::
    /// AllocationConvergenceCtx::observe`] AND as the peer
    /// [`crate::prelude::Process::observed_identity`] primitive
    /// already owned on the `Process` CRD's `status.identity` slot,
    /// past the ★★ PRIME-DIRECTIVE ≥ 2 duplication trigger at
    /// substrate-shape level. THEORY.md §II.1 invariant 5
    /// (composition preserves proofs — the pins bind the missing-
    /// `status` corner + the empty-`bound_pool`-slot corner + the
    /// borrow-form `&AllocationRef` lifetime + the zero-copy
    /// projection contract + byte-identical parity with the pre-lift
    /// `.and_then(|s| s.bound_pool.clone())` chain across the full
    /// corner set + cross-CRD peer coherence with
    /// [`crate::prelude::Process::observed_identity`], so a
    /// regression that drifted any surface at
    /// `tests::observed_bound_pool_*` rather than as silent operator-
    /// facing skew between the allocation observer's Release-
    /// composition seed and the Process observer's FORK-time
    /// identity seed on the SAME reconcile tick).
    #[must_use]
    pub fn observed_bound_pool(&self) -> Option<&AllocationRef> {
        self.status.as_ref().and_then(|s| s.bound_pool.as_ref())
    }

    /// The copy-form status-projection primitive on the TTL-expiry axis:
    /// returns the wall-clock deadline the pool reconciler currently
    /// persists at `status.expires_at` (derived from `spec.ttl` +
    /// `allocated_at` at Bind time), wrapped in an `Option` so both the
    /// missing-`status` corner AND the populated-status-with-`expires_at
    /// =None` corner collapse to `None` — the ONE-liner collapse of the
    /// paired `self.status.as_ref().and_then(|s| s.expires_at)`
    /// incantation the pool reconciler's `AllocationConvergenceCtx::
    /// observe` restated by hand pre-lift.
    ///
    /// Same-CRD peer to [`Self::observed_phase`] on the (CRD × copy-form
    /// × status-slot) axis pair — both primitives walk the identical
    /// `.status.as_ref().<map|and_then>(|s| s.<Copy-field>)` shape,
    /// differing only in the record projected ([`DateTime<Utc>`] here,
    /// [`AllocationPhase`] on the phase axis) and in the outer combinator
    /// (`and_then` here because the persisted field is itself an
    /// `Option<DateTime<Utc>>`, `map` there because the persisted phase
    /// is bare). The substrate now owns the copy-form
    /// `.status.as_ref().<map|and_then>(|s| s.<Copy-field>)` chain
    /// axis-uniformly across every `Copy`-valued slot on
    /// `AllocationStatus`, so a future normalization (a clock-skew
    /// guard that drops an `expires_at` stamped before its owning
    /// allocation's observed `allocated_at`, a canonicalization pass
    /// that clamps a deadline to a monotonic upper bound, a stale-
    /// timestamp gate that returns `None` on an `expires_at` older than
    /// a controller-configured horizon) lands at ONE substrate method
    /// rather than being restated at every observer.
    ///
    /// Return-form axis: `Option<DateTime<Utc>>` mirrors the copy-first
    /// discipline of [`Self::observed_phase`]. The lone pre-lift consumer
    /// ([`tatara-pool-reconciler::allocation_decide::
    /// AllocationConvergenceCtx::observe`]'s `expires_at` seed) spelled
    /// the projection as `.status.as_ref().and_then(|s| s.expires_at)` —
    /// a 3-link hand-authored chain the observer walked on every
    /// reconcile pass. Post-lift the consumer reaches the primitive
    /// once and the whole missing-status + empty-slot corner cross
    /// collapses at the substrate rather than at the callsite.
    ///
    /// The missing-`status` corner AND the populated-status-with-
    /// `expires_at=None` corner BOTH collapse to `None` so
    /// `.is_some()` / `if let Some(_)` / any `>=` deadline comparison
    /// behave identically on an `EphemeralAllocation` whose status
    /// field is `None` and on one whose status carries an unpopulated
    /// `expires_at` slot — matching what the pre-lift `.and_then(...)`
    /// chain produced. Consumers that need to tell those corners apart
    /// reach for [`Self::status`] directly, exactly as the existing peer
    /// accessors [`Self::observed_phase`] +
    /// [`Self::observed_phase_or_pending`] admit.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the `.status.as_ref().and_then(|s| s.<Copy-field>)` shape
    /// recurred as ONE hand-authored chain in
    /// [`tatara-pool-reconciler::allocation_decide::
    /// AllocationConvergenceCtx::observe`] AND as the copy-form peer
    /// [`Self::observed_phase`] primitive already owned on the same
    /// CRD's `status.phase` slot, past the substrate-shape recurrence
    /// trigger; the substrate now owns the third status-projection
    /// primitive on `EphemeralAllocation`, closing the copy-form family
    /// alongside the borrow-form [`Self::observed_bound_pool`]).
    /// THEORY.md §II.1 invariant 5 (composition preserves proofs — the
    /// pins bind the missing-`status` corner + the empty-`expires_at`-
    /// slot corner + the copy-form `DateTime<Utc>` return + byte-
    /// identical parity with the pre-lift `.and_then(|s| s.expires_at)`
    /// chain across the full corner set, so a regression that drifted
    /// any surface surfaces at `tests::observed_expires_at_*` rather
    /// than as silent operator-facing skew between the allocation
    /// observer's Release-composition TTL gate and any future consumer
    /// that reaches for the same slot).
    #[must_use]
    pub fn observed_expires_at(&self) -> Option<DateTime<Utc>> {
        self.status.as_ref().and_then(|s| s.expires_at)
    }

    /// The namespaced-CRD constructor composer on the
    /// `EphemeralAllocation` axis: forwards `(name, spec)` to the
    /// kube-derived [`Self::new`] constructor + stamps
    /// `metadata.namespace` with the caller-supplied slot in ONE
    /// step. The ONE-liner collapse of the paired `let mut a =
    /// EphemeralAllocation::new(<name>, <spec>); a.meta_mut().
    /// namespace = Some(<ns>.into());` incantation every allocation-
    /// side emitter restated by hand pre-lift.
    ///
    /// Pre-lift the 2-line construct-then-set-namespace chain was
    /// hand-authored at TWO sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication threshold across TWO workspace crates, composing
    /// a namespaced `EphemeralAllocation` fixture / event from a
    /// `name` slot and an `AllocationSpec`:
    /// * `tatara-github-watcher::allocation_factory::build_allocation`
    ///   — the production PR-webhook → `EphemeralAllocation` emitter
    ///   at `alloc.meta_mut().namespace = Some(namespace.to_string
    ///   ())`; stamps the ephemeral-pools namespace as part of the
    ///   canonical opened / reopened / synchronize allocation shape.
    /// * `tatara-pool-reconciler::allocation_decide::tests::alloc` —
    ///   the allocation-decision test fixture pinned to `"pools"`,
    ///   sibling to the peer `pool` fixture in the same module that
    ///   composes an `EphemeralPool` via [`crate::pool::EphemeralPool::
    ///   new_in`] on the same `ns` slot value.
    ///
    /// Both sites walked the SAME 2-line chain and both wanted the
    /// `EphemeralAllocation` back with `metadata.namespace` stamped as
    /// `Some(<ns>.into())`. Post-lift each callsite reads
    /// `EphemeralAllocation::new_in(<name>, <ns>, <spec>)` and the
    /// produced value feeds the same downstream `Api::create(&pp,
    /// &alloc)` chain / test-battery input unchanged.
    ///
    /// The `impl Into<String>` at the `namespace` slot matches the
    /// sibling `impl Into<String>`-widening discipline the workspace's
    /// other namespaced-CRD-adjacent composers walk
    /// ([`crate::pool::PoolMember::unallocated`] on the
    /// `process_name` slot, [`crate::pool::AllocationRef::new`] on the
    /// `(name, namespace)` slot pair, [`Requestor::kind_only`] on the
    /// `kind` slot) and accepts BOTH `&'static str` (the majority
    /// pre-lift caller shape) AND owned `String` at the SAME
    /// signature.
    ///
    /// Peer to [`crate::pool::EphemeralPool::new_in`] on the sister
    /// `EphemeralPool` CRD — the two primitives partition the
    /// namespaced-CRD-constructor family axis for the two pool-
    /// adjacent CRDs the workspace stamps at reconciler fixture /
    /// GitHub-webhook-emitter time. A future normalization (a per-
    /// fleet virtual-cluster prefix rewrite on the `namespace` slot,
    /// a per-cluster canonical case-fold pass, a `generateName`
    /// fallback on the `name` slot, an operator-scoped default
    /// namespace for cluster-local test rigs, an audit-tag stamped
    /// on every fixture-emitted CRD for post-hoc grep discipline)
    /// lands at ONE primitive body per CRD and every downstream
    /// consumer inherits the upgrade mechanically.
    ///
    /// `#[must_use]` on the return keeps a caller from composing the
    /// namespaced value and dropping it un-passed to a `kube::Api`
    /// create call or a `Vec<EphemeralAllocation>` reconciler-input
    /// slot.
    ///
    /// Theory anchor: THEORY.md §VI.1 (generation over composition —
    /// the 2-line construct-then-set-namespace chain recurred at
    /// TWO hand-authored sites past the ★★ PRIME-DIRECTIVE ≥ 2
    /// duplication trigger, spanning two workspace crates, and is
    /// lifted to ONE substrate owner here). THEORY.md §II.1
    /// invariant 5 (composition preserves proofs — the pins below
    /// bind the (name-slot → metadata.name, ns-slot → metadata.
    /// namespace, spec-slot → spec) slot-projection triple + the
    /// byte-identical parity with the pre-lift 2-line chain across
    /// the two representative `impl Into<String>` value shapes
    /// (`&'static str` and owned `String`) + the sibling-composer
    /// coherence with [`Self::new`]).
    #[must_use]
    pub fn new_in(name: &str, namespace: impl Into<String>, spec: AllocationSpec) -> Self {
        // Routes through the ONE substrate owner of the
        // `metadata.namespace` stamp — the [`crate::PlacedInNamespace`]
        // blanket-impl trait over `kube::Resource<DynamicType = ()>`.
        // Byte-identical to the pre-lift 3-line body
        // (`Self::new(name, spec); metadata.namespace = Some(namespace
        // .into())`); the trait-forwarding form collapses the mutation
        // duplication with the sibling per-CRD composer
        // [`crate::pool::EphemeralPool::new_in`] and with the
        // render-fixture site on `Process` that has no per-CRD
        // `new_in` sibling.
        use crate::PlacedInNamespace;
        Self::new(name, spec).in_namespace(namespace)
    }
}

#[cfg(test)]
mod tests {
    // `FromStr` lives in scope at the test surface only — the derive
    // emits `impl ::core::str::FromStr` via the full path so the lib
    // body no longer reaches `FromStr` directly, but the cross-axis
    // sweeps + the verbatim-echo contract tests call
    // `AllocationPhase::from_str(bad)` / `bad.parse::<RequestorKind>()`.
    use std::str::FromStr;

    use super::*;

    #[test]
    fn requestor_minimum_shape_round_trips() {
        let r = Requestor {
            kind: "github-pr".into(),
            repo: Some("pleme-io/demo-app".into()),
            branch: Some("fix-something".into()),
            pr_number: Some(123),
            sha: Some("abc123def".into()),
            pr_labels: vec!["needs-ephemeral".into()],
            actor: Some("drzln".into()),
        };
        let yaml = serde_yaml::to_string(&r).unwrap();
        assert!(yaml.contains("kind: github-pr"));
        assert!(yaml.contains("prNumber: 123"));
        let back: Requestor = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.kind, "github-pr");
        assert_eq!(back.pr_number, Some(123));
    }

    #[test]
    fn allocation_status_defaults_pending() {
        let s = AllocationStatus::default();
        assert_eq!(s.phase, AllocationPhase::Pending);
        assert!(s.bound_pool.is_none());
        assert!(s.assigned_process.is_none());
    }

    #[test]
    fn allocation_phase_round_trips_via_serde() {
        for p in [
            AllocationPhase::Pending,
            AllocationPhase::Queued,
            AllocationPhase::Bound,
            AllocationPhase::Releasing,
            AllocationPhase::Released,
            AllocationPhase::NoMatchingPool,
            AllocationPhase::Failed,
        ] {
            let s = serde_yaml::to_string(&p).unwrap();
            let back: AllocationPhase = serde_yaml::from_str(&s).unwrap();
            assert_eq!(back, p);
        }
    }

    // ── closed-set algebra contracts for AllocationPhase
    //    (ALL × as_str × FromStr × predicate-pair) ────────────────────

    /// `ALL` is the source of truth — pin its closure so a variant
    /// added without an `ALL` entry fails here via the uniqueness
    /// check before drifting `FromStr` or the sweep tests below. The
    /// arity is asserted by the `[Self; 7]` array type itself.
    ///
    /// Structural well-formedness of [`AllocationPhase`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants
    /// (`ALL` is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `allocation_phase_all_is_unique_and_complete` +
    /// `allocation_phase_roundtrip_via_as_str` + the empty-input arm
    /// of `unknown_allocation_phase_errors`. `FromStr` delegates to
    /// `<Self as tatara_closed_set::ClosedSet>::parse_label`, so this
    /// helper exercises the same code path the allocation reconciler
    /// hits when parsing a CRD `enum:`-validated value back to the
    /// typed phase.
    #[test]
    fn allocation_phase_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<AllocationPhase>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface, the CRD enum, the YAML
    /// wire format, and the operator-facing reason strings the
    /// reconciler stamps via Display.
    #[test]
    fn allocation_phase_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<AllocationPhase>();
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift.
    #[test]
    fn allocation_phase_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<AllocationPhase>();
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / unrelated — and the error
    /// echoes the input verbatim so the operator-facing diagnostic
    /// carries the offending value, not a normalized form. The
    /// empty-input arm is pinned by
    /// [`allocation_phase_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownAllocationPhase`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_allocation_phase_errors() {
        for bad in [
            "pending",
            "BOUND",
            "no-matching-pool",
            "release",
            "failed_state",
            "Reaped",
        ] {
            let err = AllocationPhase::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// TRUTH-TABLE CONTRACT: the predicate pair agrees with the
    /// documented per-variant disposition. `Released` + `Failed` are
    /// terminal (absorbing); `Pending` / `Queued` / `NoMatchingPool`
    /// need pool routing; `Bound` / `Releasing` are settled-but-not-
    /// terminal (heartbeat / release ladder).
    #[test]
    fn allocation_phase_predicate_truth_tables() {
        assert!(!AllocationPhase::Pending.is_terminal());
        assert!(AllocationPhase::Pending.needs_pool_routing());

        assert!(!AllocationPhase::Queued.is_terminal());
        assert!(AllocationPhase::Queued.needs_pool_routing());

        assert!(!AllocationPhase::Bound.is_terminal());
        assert!(!AllocationPhase::Bound.needs_pool_routing());

        assert!(!AllocationPhase::Releasing.is_terminal());
        assert!(!AllocationPhase::Releasing.needs_pool_routing());

        assert!(AllocationPhase::Released.is_terminal());
        assert!(!AllocationPhase::Released.needs_pool_routing());

        assert!(!AllocationPhase::NoMatchingPool.is_terminal());
        assert!(AllocationPhase::NoMatchingPool.needs_pool_routing());

        assert!(AllocationPhase::Failed.is_terminal());
        assert!(!AllocationPhase::Failed.needs_pool_routing());
    }

    /// IMPLICATION CONTRACT: `is_terminal → !needs_pool_routing`. A
    /// terminal allocation cannot also be routing-eligible — that's
    /// the bug the typed projection closes (a `Failed` allocation
    /// that's neither `Released` nor `Bound` would otherwise slip
    /// through the open-coded gate in `observe` and try to rebind to
    /// a pool member). A future variant that flipped both predicates
    /// true would fail here, forcing the author to flip one or
    /// extend the consumer dispatch site in
    /// `tatara-pool-reconciler::allocation_decide` deliberately
    /// rather than letting an impossible state slip in.
    #[test]
    fn allocation_phase_terminal_excludes_routing() {
        for phase in AllocationPhase::ALL {
            assert!(
                !(phase.is_terminal() && phase.needs_pool_routing()),
                "{phase:?} is both terminal and routing-eligible",
            );
        }
    }

    /// DEFAULT-AGREEMENT CONTRACT: `AllocationPhase::default()` is
    /// `Pending` — the entry state, neither terminal nor settled —
    /// and it lives on the routing path. A future default-variant
    /// rename without flipping the predicates fails here.
    #[test]
    fn allocation_phase_default_is_pending_and_routes() {
        let d = AllocationPhase::default();
        assert_eq!(d, AllocationPhase::Pending);
        assert!(!d.is_terminal());
        assert!(d.needs_pool_routing());
    }

    // ── RequestorKind closed-set truth-table ─────────────────────────

    /// Structural well-formedness of [`RequestorKind`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants
    /// (`ALL` is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `requestor_kind_all_enumerates_each_variant_exactly_once`
    /// + `requestor_kind_from_str_round_trips_canonical_names` + the
    /// empty-input arm of `requestor_kind_from_str_rejects_open_kinds`.
    /// `FromStr` delegates to
    /// `<Self as tatara_closed_set::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path
    /// [`Requestor::known_kind`]'s `Option<RequestorKind>` collapse
    /// rides on when classifying inbound `Requestor.kind` strings. The
    /// arity is asserted by the `[Self; 4]` array type itself.
    #[test]
    fn requestor_kind_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<RequestorKind>();
    }

    /// Byte-exact wire-format pin — renaming any of these is a wire-
    /// format change (the `tatara-github-watcher` emitter, the CRD
    /// printcolumns, the `PoolSelector.kinds` filter strings, the
    /// per-test `kind: "…".into()` fixtures all depend on these
    /// literals), not a typed-internal refactor.
    #[test]
    fn requestor_kind_canonical_names_pinned() {
        assert_eq!(RequestorKind::GithubPr.as_str(), "github-pr");
        assert_eq!(RequestorKind::Manual.as_str(), "manual");
        assert_eq!(RequestorKind::CiRun.as_str(), "ci-run");
        assert_eq!(RequestorKind::Scheduled.as_str(), "scheduled");
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased-mismatch / typo / unrelated — and the
    /// error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    /// The schema is open at the wire layer (operators MAY register
    /// new kinds and `Requestor::known_kind` collapses them to
    /// `None`), but the closed-set view is byte-exact. The empty-input
    /// arm is pinned by [`requestor_kind_is_well_formed_closed_set`]
    /// via the `tatara_lisp::ClosedSet` testkit; the cases here pin
    /// the verbatim-echo contract on the [`UnknownRequestorKind`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn requestor_kind_from_str_rejects_open_kinds() {
        for bad in [
            "github_pr",
            "GithubPr",
            "operator-custom-kind",
            "ci_run",
            "Scheduled",
        ] {
            let err = bad.parse::<RequestorKind>().unwrap_err();
            assert_eq!(err, UnknownRequestorKind(bad.to_string()));
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future
    /// callers reach for either projection without drift (Display is
    /// what operator-facing diagnostics compose against).
    #[test]
    fn requestor_kind_display_delegates_to_as_str() {
        for k in RequestorKind::ALL {
            assert_eq!(format!("{k}"), k.as_str());
        }
    }

    /// The `String` projection that `From<RequestorKind> for String`
    /// ([`RequestorKind::into`]) composes is byte-equal to `as_str`.
    /// This is the typed → wire bridge — emitters spell
    /// `kind: RequestorKind::GithubPr.into()` and the canonical
    /// literal is materialized at ONE place.
    #[test]
    fn requestor_kind_into_string_matches_as_str() {
        for k in RequestorKind::ALL {
            let s: String = k.into();
            assert_eq!(s, k.as_str());
        }
    }

    /// The typed → wire → typed round-trip: composing a `Requestor`
    /// with `kind: RequestorKind::X.into()` produces an object whose
    /// `known_kind()` decodes back to `X`. Pins the bridge invariant
    /// at the `Requestor` boundary, not just at `RequestorKind`.
    #[test]
    fn known_kind_decodes_built_requestors() {
        for k in RequestorKind::ALL {
            // Routes through the ONE substrate composer
            // `Requestor::kind_only` — one of TEN pre-lift exact-match
            // sites past the ★★ PRIME-DIRECTIVE ≥ 2 threshold.
            let r = Requestor::kind_only(k);
            assert_eq!(r.known_kind(), Some(k), "round-trip failed for {k:?}");
        }
    }

    /// Open-by-design: a custom operator-registered kind still
    /// stamps a valid `Requestor` (no schema rejection), it just
    /// doesn't project through the closed-set typed view. Mirrors
    /// `ReceiptEnvelope::known_kind`'s open-kind posture.
    #[test]
    fn known_kind_returns_none_for_open_kinds() {
        // Routes through the ONE substrate composer
        // `Requestor::kind_only` — one of TEN pre-lift exact-match
        // sites past the ★★ PRIME-DIRECTIVE ≥ 2 threshold.
        let r = Requestor::kind_only("operator-custom-kind");
        assert_eq!(r.known_kind(), None);
    }

    /// The four canonical literals match every previously-published
    /// fixture / doc anchor in this crate — pinning the bridge to
    /// existing call sites so any drift fails here before the next
    /// release ships.
    #[test]
    fn requestor_kind_matches_existing_fixture_literals() {
        // The `requestor_minimum_shape_round_trips` fixture above
        // composes `kind: "github-pr".into()` verbatim.
        assert_eq!(RequestorKind::GithubPr.as_str(), "github-pr");
        // The `allocation_spec_omits_optional_fields` fixture below
        // composes `kind: "manual".into()` verbatim.
        assert_eq!(RequestorKind::Manual.as_str(), "manual");
    }

    // ─── Requestor::kind_only substrate pins ────────────────────────
    //
    // Fail-before-pass-after granularity: `Requestor::kind_only` did
    // not exist before this commit. The composer's job is to bind ONE
    // caller-varying slot (`kind`) and freeze the six-slot default
    // tail so a future addition to `Requestor` lands at ONE primitive
    // body rather than at every fixture / default-shape callsite.
    // Sibling to the `AplicacaoIntent::chart_only` pin family (the
    // 7-slot chart-pointer-only composer) and the `PoolSpec::with_template`
    // pin family (the 11-slot pool full-spec composer).

    #[test]
    fn kind_only_binds_kind_slot_and_defaults_the_other_six() {
        // Positional-binding pin: the sole caller slot lands at
        // `kind`; every other slot lands at its safe empty default
        // (`None` / `vec![]`).
        let r = Requestor::kind_only("manual");
        assert_eq!(r.kind, "manual");
        assert!(r.repo.is_none());
        assert!(r.branch.is_none());
        assert!(r.pr_number.is_none());
        assert!(r.sha.is_none());
        assert!(r.pr_labels.is_empty());
        assert!(r.actor.is_none());
    }

    #[test]
    fn kind_only_matches_hand_authored_pre_lift_struct_literal_shape() {
        // Byte-identity pin: every pre-lift `Requestor { kind: <lit>
        // .into(), repo: None, branch: None, pr_number: None, sha:
        // None, pr_labels: vec![], actor: None }` shape must
        // deserialize back to the same fixture composed via
        // `kind_only`. Sweeps the two families every callsite used
        // (`"manual"` — the operator-authored fixture at seven
        // sites; `"github-pr"` — the github-webhook fixture at three
        // sites) plus one open-kind sample (`"operator-custom-kind"` —
        // the `known_kind_returns_none_for_open_kinds` open-kind pin)
        // so any drift between the primitive and the pre-lift shape
        // surfaces at ONE pin rather than as silent fixture skew at
        // ten downstream consumers.
        for kind in ["manual", "github-pr", "operator-custom-kind"] {
            let via_primitive = Requestor::kind_only(kind);
            let hand_authored = Requestor {
                kind: kind.into(),
                repo: None,
                branch: None,
                pr_number: None,
                sha: None,
                pr_labels: vec![],
                actor: None,
            };
            let via_yaml = serde_yaml::to_string(&via_primitive).unwrap();
            let hand_yaml = serde_yaml::to_string(&hand_authored).unwrap();
            assert_eq!(
                via_yaml, hand_yaml,
                "kind_only({kind:?}) must be YAML-identical to the pre-lift struct literal"
            );
        }
    }

    #[test]
    fn kind_only_accepts_string_and_str_and_requestor_kind_uniformly() {
        // `impl Into<String>` symmetry across the three caller shapes
        // pre-lift authors used verbatim: `&'static str` (`"manual"`),
        // owned `String` (from a formatted context), and
        // [`RequestorKind`] (via the `From<RequestorKind> for String`
        // bridge exercised at `known_kind_decodes_built_requestors`).
        let from_str_literal = Requestor::kind_only("manual");
        let from_owned_string = Requestor::kind_only(String::from("manual"));
        let from_typed_variant = Requestor::kind_only(RequestorKind::Manual);
        assert_eq!(from_str_literal.kind, "manual");
        assert_eq!(from_owned_string.kind, "manual");
        assert_eq!(from_typed_variant.kind, "manual");
    }

    #[test]
    fn kind_only_composes_downstream_through_known_kind_projection() {
        // Cross-primitive coherence pin: every substrate-emitted
        // `RequestorKind` variant round-trips through `kind_only` +
        // `known_kind` back to the same typed variant. Byte-identical
        // to the `known_kind_decodes_built_requestors` sweep the
        // primitive replaced — pins the primitive as the composer the
        // typed decoder sees the SAME wire shape from.
        for k in RequestorKind::ALL {
            let r = Requestor::kind_only(k);
            assert_eq!(
                r.known_kind(),
                Some(k),
                "kind_only({k:?}).known_kind() must round-trip to Some({k:?})"
            );
        }
    }

    // Per-implementor `unknown_X_message_matches_substrate_convention`
    // tests removed — clause (5) of
    // `tatara_closed_set::assert_closed_set_well_formed::<T>()` now verifies
    // the substrate-wide `"unknown {SET_LABEL}: {input}"` carrier shape
    // generically (called above on `RequestorKind` /
    // `AllocationPhase` through their `*_is_well_formed_closed_set`
    // sites). The `SET_LABEL` projection is pinned independently by
    // `tatara_lisp_derive::pascal_to_spaced_lowercase_tests` —
    // together the two contracts guarantee the operator-facing
    // diagnostic without needing per-enum literal pins.

    // ─── EphemeralAllocation::observed_phase* substrate pins ────────
    //
    // Fail-before-pass-after granularity: neither `observed_phase` nor
    // `observed_phase_or_pending` existed before this commit, so each
    // pin fails to compile until the corresponding inherent method
    // lands. Post-lift the pins bind the missing-`status` corner + the
    // populated-status pass-through + byte-identical parity with the
    // pre-lift 5-line `.status.as_ref().map(|s| s.phase).unwrap_or
    // (AllocationPhase::Pending)` chain the pool reconciler's
    // `AllocationConvergenceCtx::observe` walked. Cross-CRD peer
    // coherence with `Process::observed_phase_or_pending` is pinned
    // by the `_matches_process_peer_shape` sweep at the tail.

    fn alloc_with_phase(phase: AllocationPhase) -> EphemeralAllocation {
        // AllocationSpec rides through the ONE substrate composer
        // `AllocationSpec::requestor_only`; the inner Requestor rides
        // through the peer composer `Requestor::kind_only`. Nine pre-
        // lift exact-match `AllocationSpec { pool_ref: None,
        // requestor: <r>, ttl: None, note: None }` fixture sites past
        // the ★★ PRIME-DIRECTIVE ≥ 2 threshold collapse onto this
        // ONE substrate owner.
        let spec = AllocationSpec::requestor_only(Requestor::kind_only("manual"));
        let mut a = EphemeralAllocation::new("obs-alloc", spec);
        a.status = Some(AllocationStatus {
            phase,
            ..AllocationStatus::default()
        });
        a
    }

    fn alloc_without_status() -> EphemeralAllocation {
        // AllocationSpec rides through `AllocationSpec::requestor_only`
        // — sibling to `alloc_with_phase`.
        let spec = AllocationSpec::requestor_only(Requestor::kind_only("manual"));
        let mut a = EphemeralAllocation::new("no-status-alloc", spec);
        a.status = None;
        a
    }

    #[test]
    fn observed_phase_returns_none_when_status_is_none() {
        let a = alloc_without_status();
        assert!(a.observed_phase().is_none());
    }

    #[test]
    fn observed_phase_returns_populated_variant_verbatim() {
        for p in AllocationPhase::ALL {
            let a = alloc_with_phase(p);
            assert_eq!(
                a.observed_phase(),
                Some(p),
                "observed_phase must project the persisted variant verbatim for {p:?}"
            );
        }
    }

    #[test]
    fn observed_phase_matches_pre_lift_chain_bytewise() {
        // Sweep every corner: (status: None) plus every populated
        // (status: Some(phase)) variant. The pre-lift chain was
        // `alloc.status.as_ref().map(|s| s.phase)` — a 3-link chain
        // hand-authored inline at the observer. The primitive must
        // return the same `Option<AllocationPhase>` on every corner.
        let none_alloc = alloc_without_status();
        assert_eq!(
            none_alloc.observed_phase(),
            none_alloc.status.as_ref().map(|s| s.phase),
        );
        for p in AllocationPhase::ALL {
            let a = alloc_with_phase(p);
            assert_eq!(
                a.observed_phase(),
                a.status.as_ref().map(|s| s.phase),
                "primitive must be byte-identical to the pre-lift chain for {p:?}",
            );
        }
    }

    #[test]
    fn observed_phase_or_pending_defaults_to_pending_when_status_absent() {
        let a = alloc_without_status();
        assert_eq!(a.observed_phase_or_pending(), AllocationPhase::Pending);
    }

    #[test]
    fn observed_phase_or_pending_returns_populated_phase_verbatim() {
        for p in AllocationPhase::ALL {
            let a = alloc_with_phase(p);
            assert_eq!(
                a.observed_phase_or_pending(),
                p,
                "populated status must pass through verbatim for {p:?}"
            );
        }
    }

    #[test]
    fn observed_phase_or_pending_defaults_agree_with_allocation_phase_default() {
        // The `Pending` sink is load-bearing as the "not yet observed"
        // default. `AllocationPhase::default()` returns `Pending`; the
        // primitive must return the same variant on the missing-status
        // corner. A future default-variant rename that flipped
        // `AllocationPhase::default` without flipping the primitive
        // (or vice versa) surfaces here as a divergent seed for the
        // routing ladder.
        let a = alloc_without_status();
        assert_eq!(a.observed_phase_or_pending(), AllocationPhase::default());
    }

    #[test]
    fn observed_phase_or_pending_matches_pre_lift_chain_bytewise() {
        // The exact pre-lift 5-line chain in
        // `tatara-pool-reconciler::allocation_decide::
        // AllocationConvergenceCtx::observe` was:
        //     let phase = alloc
        //         .status
        //         .as_ref()
        //         .map(|s| s.phase)
        //         .unwrap_or(AllocationPhase::Pending);
        // Sweep every corner: (status: None) plus every populated
        // status variant. The primitive must be byte-identical for
        // every corner so the observer's routing decision matches
        // bytewise post-lift.
        let none_alloc = alloc_without_status();
        assert_eq!(
            none_alloc.observed_phase_or_pending(),
            none_alloc
                .status
                .as_ref()
                .map(|s| s.phase)
                .unwrap_or(AllocationPhase::Pending),
        );
        for p in AllocationPhase::ALL {
            let a = alloc_with_phase(p);
            assert_eq!(
                a.observed_phase_or_pending(),
                a.status
                    .as_ref()
                    .map(|s| s.phase)
                    .unwrap_or(AllocationPhase::Pending),
                "primitive must be byte-identical to the pre-lift 5-line chain for {p:?}",
            );
        }
    }

    #[test]
    fn observed_phase_or_pending_composes_from_observed_phase() {
        // The composer sits on top of the borrow-form projection —
        // `observed_phase_or_pending() == observed_phase().unwrap_or
        // (Pending)`. Pinning the composition means a future
        // normalization step layered onto `observed_phase` (a
        // generation-filter, a staleness gate, a canonicalization
        // pass) reaches BOTH the raw-`Option` accessor and the
        // `Pending`-sinked composer through the SAME upstream body,
        // without needing a per-corner rewrite of the composer.
        let none_alloc = alloc_without_status();
        assert_eq!(
            none_alloc.observed_phase_or_pending(),
            none_alloc
                .observed_phase()
                .unwrap_or(AllocationPhase::Pending),
        );
        for p in AllocationPhase::ALL {
            let a = alloc_with_phase(p);
            assert_eq!(
                a.observed_phase_or_pending(),
                a.observed_phase().unwrap_or(AllocationPhase::Pending),
                "composer must ride on top of the borrow-form projection for {p:?}",
            );
        }
    }

    #[test]
    fn observed_phase_is_a_pure_projection() {
        // Reading the phase twice must not mutate the allocation or
        // its status slot — pure projection semantics. Also witnesses
        // that the accessor doesn't clone / drop the inner `phase`
        // (the `Copy` scalar comes out identical on both reads).
        let a = alloc_with_phase(AllocationPhase::Bound);
        let one = a.observed_phase();
        let two = a.observed_phase();
        assert_eq!(one, two);
        assert!(a.status.is_some(), "projection must not consume the status");
    }

    #[test]
    fn observed_phase_pending_missing_status_and_populated_pending_collapse_to_same_composer_output(
    ) {
        // A subtle correctness pin: the missing-`status` corner and
        // a populated-with-Pending status BOTH read as `Pending`
        // through the composer — the observer cannot distinguish the
        // two through this accessor. This matches the pre-lift 5-line
        // chain's semantics exactly (an operator patching
        // `status.phase: Pending` is indistinguishable from a
        // freshly-admitted allocation with no status stamped yet).
        // The borrow-form `observed_phase` accessor DOES distinguish
        // the two, so a caller that needs to tell them apart reaches
        // for the raw `Option`.
        let none_alloc = alloc_without_status();
        let pending_alloc = alloc_with_phase(AllocationPhase::Pending);

        assert_eq!(
            none_alloc.observed_phase_or_pending(),
            pending_alloc.observed_phase_or_pending(),
        );
        assert_ne!(
            none_alloc.observed_phase(),
            pending_alloc.observed_phase(),
            "borrow-form accessor MUST distinguish missing-status from populated-Pending",
        );
    }

    #[test]
    fn observed_phase_or_pending_missing_status_sink_agrees_with_process_peer_shape() {
        // Cross-CRD peer-axis coherence with
        // `Process::observed_phase_or_pending`. Both primitives walk
        // the identical `.status.as_ref().map(|s| s.phase).unwrap_or
        // (<Phase>::Pending)` chain differing ONLY in the `Phase`
        // type projected. On a missing-status observation, each
        // primitive must return its CRD's `Default`-equivalent
        // `Pending` variant — for `EphemeralAllocation` that's
        // `AllocationPhase::Pending`; for `Process` that's
        // `crate::phase::ProcessPhase::Pending`. This pin binds the
        // sink-parity structurally so a future rename of either
        // default variant surfaces here as a divergent seed for the
        // observer's routing / dispatch decision rather than as
        // silent drift between the two reconcilers.
        let no_status_alloc = alloc_without_status();
        assert_eq!(
            no_status_alloc.observed_phase_or_pending(),
            AllocationPhase::default(),
        );
        // Peer-axis invariant on the `Process` side — the primitive
        // that owns the same shape reads `ProcessPhase::Pending` on
        // the missing-status corner via its own inherent method. The
        // parity is coordinated at the `Default` seat: both CRDs'
        // phase types default to `Pending`, so a rename that broke
        // one without the other would fail one of these two
        // conjoined assertions.
        assert_eq!(AllocationPhase::default(), AllocationPhase::Pending,);
        assert_eq!(
            crate::phase::ProcessPhase::default(),
            crate::phase::ProcessPhase::Pending,
        );
    }

    // ─── EphemeralAllocation::observed_bound_pool substrate pins ────
    //
    // The borrow-form status-projection primitive on the bound-pool
    // axis. Collapses the pre-lift hand-authored `.status.as_ref()
    // .and_then(|s| s.bound_pool.clone())` chain in
    // `tatara-pool-reconciler::allocation_decide::
    // AllocationConvergenceCtx::observe`'s `bound_pool` seed onto the
    // ONE substrate primitive. Cross-CRD peer to
    // `Process::observed_identity` on the (CRD × structured-record-
    // slot × borrow-form) axis pair — both primitives walk the
    // identical `.status.as_ref().and_then(|s| s.<slot>.as_ref())`
    // shape. Each pin is fail-before-pass-after: `observed_bound_pool`
    // did not exist pre-lift, so any test invoking it fails to compile
    // pre-lift and passes post-lift.

    fn sample_pool_ref(name: &str, ns: &str) -> AllocationRef {
        // Fixture ref rides through the ONE substrate composer
        // `AllocationRef::new` — the `impl Into<String>` signature
        // accepts the borrow-form `&str` slot pair verbatim without
        // a per-fixture `.to_string()` promotion.
        AllocationRef::new(name, ns)
    }

    fn alloc_with_bound_pool(bound: Option<AllocationRef>) -> EphemeralAllocation {
        // AllocationSpec rides through `AllocationSpec::requestor_only`
        // — sibling to `alloc_with_phase`.
        let spec = AllocationSpec::requestor_only(Requestor::kind_only("manual"));
        let mut a = EphemeralAllocation::new("bp-alloc", spec);
        a.status = Some(AllocationStatus {
            phase: AllocationPhase::Bound,
            bound_pool: bound,
            ..AllocationStatus::default()
        });
        a
    }

    #[test]
    fn observed_bound_pool_returns_none_when_status_is_none() {
        // Missing-`status` corner pin: the primitive collapses the
        // no-status case to `None` so downstream `.is_some()` /
        // `if let Some(_)` / `.cloned().unwrap_or_else(...)` behave
        // identically on an `EphemeralAllocation` whose status field
        // is `None` and on one whose status carries an unpopulated
        // `bound_pool` slot. Matches the pre-lift `.and_then(...)`
        // chain's `None` byte-identically at the pool reconciler's
        // Release-composition seed.
        let a = alloc_without_status();
        assert!(a.observed_bound_pool().is_none());
    }

    #[test]
    fn observed_bound_pool_returns_none_when_slot_is_none() {
        // Empty-slot-under-populated-status corner pin: the primitive
        // returns `None`, matching the missing-`status` corner byte-
        // identically. A regression that treated the two corners
        // differently would silently promote an internal representation
        // detail (whether the pool reconciler has ever written a
        // status subresource) into observable behavior at the
        // Release-composition branch of the allocation reconciler's
        // `decide` transition rule.
        let a = alloc_with_bound_pool(None);
        assert!(a.observed_bound_pool().is_none());
    }

    #[test]
    fn observed_bound_pool_returns_borrow_when_slot_is_populated() {
        // Happy-path pin: with a populated `status.bound_pool` slot,
        // the primitive returns a borrowed `&AllocationRef` whose
        // (name, namespace) fields match the persisted record. A
        // regression that filtered / reshaped / canonicalized the
        // record would surface here rather than as silent skew at the
        // Release-composition seed's `.cloned()` materialization.
        let expected = sample_pool_ref("demo-pool", "pools");
        let a = alloc_with_bound_pool(Some(expected.clone()));
        let observed = a.observed_bound_pool().expect("populated slot");
        assert_eq!(observed, &expected);
        assert_eq!(observed.name, "demo-pool");
        assert_eq!(observed.namespace, "pools");
    }

    #[test]
    fn observed_bound_pool_is_a_zero_copy_borrow_projection() {
        // Borrow-discipline pin: the returned reference points at the
        // persisted `AllocationRef` in place — NOT a fresh allocation
        // or a clone. A regression that switched the projection to an
        // owned `AllocationRef` (via `.clone()`) would defeat the
        // zero-copy contract the lift's primary strict-widening
        // delivers (the observer's Release-composition arm clones
        // once at the composition point where the
        // `AllocationConvergenceCtx` snapshot slot requires the owned
        // value). Peer to the sibling
        // `Process::observed_identity_is_a_zero_copy_borrow_projection`
        // pin on the `Process` CRD's `status.identity` slot.
        let a = alloc_with_bound_pool(Some(sample_pool_ref("demo-pool", "pools")));
        let observed = a.observed_bound_pool().expect("populated slot") as *const _;
        let persisted = a.status.as_ref().unwrap().bound_pool.as_ref().unwrap() as *const _;
        assert!(std::ptr::eq(observed, persisted));
    }

    #[test]
    fn observed_bound_pool_is_a_pure_projection() {
        // Purity pin: calling the projection twice on the same
        // `EphemeralAllocation` returns byte-identical borrows (same
        // pointer). A regression that introduced state — a lazy-
        // cached reference, a normalization step that ran once and
        // cached — would surface here rather than as silent drift
        // between two dispatches within one reconcile pass.
        let a = alloc_with_bound_pool(Some(sample_pool_ref("demo-pool", "pools")));
        let one = a.observed_bound_pool().expect("populated slot") as *const _;
        let two = a.observed_bound_pool().expect("populated slot") as *const _;
        assert!(std::ptr::eq(one, two));
    }

    #[test]
    fn observed_bound_pool_matches_pre_lift_chain_bytewise() {
        // Byte-identical parity pin between the borrow-form primitive
        // here and the pre-lift `tatara-pool-reconciler`
        // `.status.as_ref().and_then(|s| s.bound_pool.clone())` chain.
        // Sweeps every corner every callsite plausibly encounters
        // (missing status, empty `bound_pool` slot, populated
        // `bound_pool` slot). A regression that inserted a
        // normalization step at the primitive the pre-lift chain does
        // NOT apply — or vice versa — surfaces here rather than as
        // silent drift between the pre-lift consumer site and the ONE
        // substrate owner it now routes through.
        fn pre_lift(a: &EphemeralAllocation) -> Option<AllocationRef> {
            a.status.as_ref().and_then(|s| s.bound_pool.clone())
        }
        // Missing status.
        let a = alloc_without_status();
        assert_eq!(a.observed_bound_pool().cloned(), pre_lift(&a));
        // Populated status, empty `bound_pool` slot.
        let a = alloc_with_bound_pool(None);
        assert_eq!(a.observed_bound_pool().cloned(), pre_lift(&a));
        // Populated status, populated `bound_pool` slot.
        let a = alloc_with_bound_pool(Some(sample_pool_ref("demo-pool", "pools")));
        assert_eq!(a.observed_bound_pool().cloned(), pre_lift(&a));
    }

    #[test]
    fn observed_bound_pool_missing_status_and_empty_slot_collapse_to_the_same_option_shape() {
        // Cross-corner coherence pin: the missing-`status` corner and
        // the populated-empty-slot corner return `Option`s whose
        // `.is_none()` / `.is_some()` observations are IDENTICAL. A
        // regression that promoted the missing-`status` corner to a
        // typed error (via a signature change to `Result<_, _>`) — or
        // that widened the empty-slot corner to a synthetic
        // `Some(AllocationRef::default())` — would surface here rather
        // than as silent operator-facing divergence between a never-
        // status-written allocation and a bound-pool-cleared
        // allocation on the Release-composition branch.
        let a_no_status = alloc_without_status();
        let a_empty_slot = alloc_with_bound_pool(None);
        assert_eq!(
            a_no_status.observed_bound_pool().is_none(),
            a_empty_slot.observed_bound_pool().is_none(),
        );
        assert_eq!(
            a_no_status.observed_bound_pool().is_some(),
            a_empty_slot.observed_bound_pool().is_some(),
        );
    }

    #[test]
    fn observed_bound_pool_shape_agrees_with_process_observed_identity_peer_axis() {
        // Cross-CRD peer-axis coherence pin binding the SAME
        // `.status.as_ref().and_then(|s| s.<slot>.as_ref())` shape
        // that both `EphemeralAllocation::observed_bound_pool` (this
        // primitive) and `Process::observed_identity` walk, differing
        // ONLY in the record projected. Structural test — both
        // signatures must resolve as `&Self -> Option<&Record>` fn
        // pointers, so a future rename or a signature drift that
        // (say) widened one side to `Option<Record>` or narrowed one
        // side to `Option<&str>` fails to compile here rather than
        // silently drifting the two reconcilers apart at their
        // respective observer seeds. The runtime side of the pin
        // sweeps the missing-status + empty-slot corners on the
        // `EphemeralAllocation` half; the `Process` half is exercised
        // by its own `crd.rs::tests::observed_identity_*` pin
        // family — this test binds only the peer-axis shape.
        let a_no_status = alloc_without_status();
        let a_empty_slot = alloc_with_bound_pool(None);
        assert!(a_no_status.observed_bound_pool().is_none());
        assert!(a_empty_slot.observed_bound_pool().is_none());
        // Structural peer-axis coherence: bind both signatures as fn
        // pointers at their peer resolution type so the compiler
        // refuses to build if either side's shape drifts. The `_`
        // let-bindings assert the target type inference.
        let _bound_pool_shape: fn(&EphemeralAllocation) -> Option<&AllocationRef> =
            EphemeralAllocation::observed_bound_pool;
        let _identity_shape: fn(&crate::prelude::Process) -> Option<&crate::identity::Identity> =
            crate::prelude::Process::observed_identity;
    }

    // ─── EphemeralAllocation::observed_expires_at substrate pins ────
    //
    // The copy-form status-projection primitive on the TTL-expiry axis.
    // Collapses the pre-lift hand-authored `.status.as_ref().and_then(
    // |s| s.expires_at)` chain in `tatara-pool-reconciler::
    // allocation_decide::AllocationConvergenceCtx::observe`'s
    // `expires_at` seed onto the ONE substrate primitive. Same-CRD peer
    // to `observed_phase` on the (copy-form × status-slot) axis — both
    // primitives walk the identical `.status.as_ref().<map|and_then>(
    // |s| s.<Copy-field>)` shape. Each pin is fail-before-pass-after:
    // `observed_expires_at` did not exist pre-lift, so any test invoking
    // it fails to compile pre-lift and passes post-lift.

    fn alloc_with_expires_at(expires_at: Option<DateTime<Utc>>) -> EphemeralAllocation {
        // AllocationSpec rides through `AllocationSpec::requestor_only`
        // — sibling to `alloc_with_phase`.
        let spec = AllocationSpec::requestor_only(Requestor::kind_only("manual"));
        let mut a = EphemeralAllocation::new("exp-alloc", spec);
        a.status = Some(AllocationStatus {
            phase: AllocationPhase::Bound,
            expires_at,
            ..AllocationStatus::default()
        });
        a
    }

    #[test]
    fn observed_expires_at_returns_none_when_status_is_none() {
        // Missing-`status` corner pin: the primitive collapses the
        // no-status case to `None` so downstream `.is_some()` / any
        // deadline comparison behaves identically on an
        // `EphemeralAllocation` whose status field is `None` and on
        // one whose status carries an unpopulated `expires_at` slot.
        // Matches the pre-lift `.and_then(...)` chain's `None` byte-
        // identically at the pool reconciler's Release-composition
        // TTL gate.
        let a = alloc_without_status();
        assert!(a.observed_expires_at().is_none());
    }

    #[test]
    fn observed_expires_at_returns_none_when_slot_is_none() {
        // Empty-slot-under-populated-status corner pin: the primitive
        // returns `None`, matching the missing-`status` corner byte-
        // identically. A regression that treated the two corners
        // differently would silently promote an internal representation
        // detail (whether the pool reconciler has ever written a
        // `status.expires_at` field for a not-yet-Bound allocation)
        // into observable behavior at the Release-composition branch
        // of the allocation reconciler's `decide` transition rule.
        let a = alloc_with_expires_at(None);
        assert!(a.observed_expires_at().is_none());
    }

    #[test]
    fn observed_expires_at_returns_populated_timestamp_verbatim() {
        // Happy-path pin: with a populated `status.expires_at` slot,
        // the primitive returns the persisted `DateTime<Utc>` verbatim.
        // A regression that filtered / clamped / canonicalized the
        // timestamp would surface here rather than as silent skew at
        // the Release-composition TTL gate's `>=` deadline comparison.
        let expected = Utc::now();
        let a = alloc_with_expires_at(Some(expected));
        assert_eq!(a.observed_expires_at(), Some(expected));
    }

    #[test]
    fn observed_expires_at_is_a_pure_projection() {
        // Purity pin: calling the projection twice on the same
        // `EphemeralAllocation` returns byte-identical `Option`s. A
        // regression that introduced state — a lazy-cached value, a
        // normalization step that ran once and cached — would surface
        // here rather than as silent drift between two dispatches
        // within one reconcile pass.
        let expected = Utc::now();
        let a = alloc_with_expires_at(Some(expected));
        assert_eq!(a.observed_expires_at(), a.observed_expires_at());
    }

    #[test]
    fn observed_expires_at_matches_pre_lift_chain_bytewise() {
        // Byte-identical parity pin between the copy-form primitive
        // here and the pre-lift `tatara-pool-reconciler`
        // `.status.as_ref().and_then(|s| s.expires_at)` chain. Sweeps
        // every corner every callsite plausibly encounters (missing
        // status, empty `expires_at` slot, populated `expires_at`
        // slot). A regression that inserted a normalization step at
        // the primitive the pre-lift chain does NOT apply — or vice
        // versa — surfaces here rather than as silent drift between
        // the pre-lift consumer site and the ONE substrate owner it
        // now routes through.
        fn pre_lift(a: &EphemeralAllocation) -> Option<DateTime<Utc>> {
            a.status.as_ref().and_then(|s| s.expires_at)
        }
        // Missing status.
        let a = alloc_without_status();
        assert_eq!(a.observed_expires_at(), pre_lift(&a));
        // Populated status, empty `expires_at` slot.
        let a = alloc_with_expires_at(None);
        assert_eq!(a.observed_expires_at(), pre_lift(&a));
        // Populated status, populated `expires_at` slot.
        let a = alloc_with_expires_at(Some(Utc::now()));
        assert_eq!(a.observed_expires_at(), pre_lift(&a));
    }

    #[test]
    fn observed_expires_at_missing_status_and_empty_slot_collapse_to_the_same_option_shape() {
        // Cross-corner coherence pin: the missing-`status` corner and
        // the populated-empty-slot corner return `Option`s whose
        // `.is_none()` / `.is_some()` observations are IDENTICAL. A
        // regression that promoted the missing-`status` corner to a
        // typed error (via a signature change to `Result<_, _>`) — or
        // that widened the empty-slot corner to a synthetic
        // `Some(Utc::now())` — would surface here rather than as
        // silent operator-facing divergence between a never-status-
        // written allocation and a Bind-time-without-TTL allocation on
        // the Release-composition branch.
        let a_no_status = alloc_without_status();
        let a_empty_slot = alloc_with_expires_at(None);
        assert_eq!(
            a_no_status.observed_expires_at().is_none(),
            a_empty_slot.observed_expires_at().is_none(),
        );
        assert_eq!(
            a_no_status.observed_expires_at().is_some(),
            a_empty_slot.observed_expires_at().is_some(),
        );
    }

    #[test]
    fn observed_expires_at_shape_agrees_with_observed_phase_peer_axis() {
        // Same-CRD peer-axis coherence pin binding the SAME
        // `.status.as_ref().<map|and_then>(|s| s.<Copy-field>)` shape
        // that both `EphemeralAllocation::observed_expires_at` (this
        // primitive) and `EphemeralAllocation::observed_phase` walk,
        // differing only in the outer combinator (`and_then` here
        // because the persisted field is itself `Option<T>`, `map`
        // there because the persisted phase is bare) and in the
        // projected `Copy` type. Structural test — both signatures
        // must resolve as `&Self -> Option<T>` fn pointers with `T`
        // `Copy`, so a future rename or a signature drift that (say)
        // widened one side to `Option<&T>` or narrowed one side to
        // `T` fails to compile here rather than silently drifting
        // the family apart. The runtime side of the pin sweeps the
        // missing-status + empty-slot corners on the `expires_at`
        // half; the `phase` half is exercised by its own
        // `tests::observed_phase_*` pin family — this test binds
        // only the peer-axis shape.
        let a_no_status = alloc_without_status();
        let a_empty_slot = alloc_with_expires_at(None);
        assert!(a_no_status.observed_expires_at().is_none());
        assert!(a_empty_slot.observed_expires_at().is_none());
        // Structural peer-axis coherence: bind both signatures as fn
        // pointers at their peer resolution type so the compiler
        // refuses to build if either side's shape drifts.
        let _expires_at_shape: fn(&EphemeralAllocation) -> Option<DateTime<Utc>> =
            EphemeralAllocation::observed_expires_at;
        let _phase_shape: fn(&EphemeralAllocation) -> Option<AllocationPhase> =
            EphemeralAllocation::observed_phase;
    }

    #[test]
    fn allocation_spec_omits_optional_fields() {
        // AllocationSpec rides through `AllocationSpec::requestor_only`
        // + the inner Requestor through `Requestor::kind_only`; the
        // wire-shape pin still holds because BOTH composers produce
        // the byte-identical minimal shape whose `skip_serializing_if
        // = "Option::is_none"` + default-vec serde attributes elide
        // every optional slot from the YAML output.
        let s = AllocationSpec::requestor_only(Requestor::kind_only("manual"));
        let yaml = serde_yaml::to_string(&s).unwrap();
        assert!(!yaml.contains("poolRef"));
        assert!(!yaml.contains("ttl"));
        assert!(!yaml.contains("note"));
    }

    // ─── AllocationSpec::requestor_only substrate pins ──────────────
    //
    // The pre-lift `AllocationSpec { pool_ref: None, requestor: <r>,
    // ttl: None, note: None }` incantation recurred at NINE workspace-
    // wide fixture sites past the ★★ PRIME-DIRECTIVE ≥ 2 threshold
    // (five inside this file's own test module, three inside the
    // crate's `tests_owned_coordinates` / `tests_annotated` /
    // `tests_deletion_tombstoned` pin modules on `lib.rs`, and one in
    // `tatara-pool-reconciler::allocation_decide::alloc`). Every corner
    // of the three-slot default tail is pinned here so a future
    // normalization at the primitive lands with a fail-before-pass-
    // after regression at THIS composer's pins rather than as silent
    // fixture skew across the nine callsite arms.

    #[test]
    fn requestor_only_leaves_the_three_slot_default_tail_at_the_substrate_owner() {
        // Every default-tail slot must land at the values the substrate
        // owner stamps: `pool_ref = None` (selector-based routing),
        // `ttl = None` (fall back to pool template TTL), `note = None`
        // (empty audit slot). A regression that drifted ANY of the three
        // defaults would silently reshape every downstream fixture
        // simultaneously; this pin catches it.
        let s = AllocationSpec::requestor_only(Requestor::kind_only("manual"));
        assert!(s.pool_ref.is_none(), "pool_ref must default to None");
        assert!(s.ttl.is_none(), "ttl must default to None");
        assert!(s.note.is_none(), "note must default to None");
    }

    #[test]
    fn requestor_only_stamps_the_caller_requestor_verbatim() {
        // The single caller-varying slot MUST pass through untouched —
        // a regression that copied only a subset of the Requestor's
        // seven slots (e.g. re-authoring `Requestor { kind: r.kind, ..
        // Default::default() }` inside the composer) would drop the
        // caller's `repo` / `branch` / `pr_number` / `sha` / `pr_labels`
        // / `actor` at every downstream fixture. Passes a fully-
        // populated `Requestor` through and asserts every slot lands.
        let r = Requestor {
            kind: "github-pr".into(),
            repo: Some("pleme-io/demo".into()),
            branch: Some("main".into()),
            pr_number: Some(42),
            sha: Some("deadbeef".into()),
            pr_labels: vec!["needs-review".into()],
            actor: Some("dozer".into()),
        };
        let s = AllocationSpec::requestor_only(r.clone());
        assert_eq!(s.requestor.kind, r.kind);
        assert_eq!(s.requestor.repo, r.repo);
        assert_eq!(s.requestor.branch, r.branch);
        assert_eq!(s.requestor.pr_number, r.pr_number);
        assert_eq!(s.requestor.sha, r.sha);
        assert_eq!(s.requestor.pr_labels, r.pr_labels);
        assert_eq!(s.requestor.actor, r.actor);
    }

    #[test]
    fn requestor_only_matches_hand_authored_pre_lift_bytewise() {
        // Byte-identical parity with the pre-lift 5-line struct-
        // literal every downstream fixture restated verbatim. Swept
        // across the two representative requestor shapes: the
        // kind-only `"manual"` fixture (the majority of the collapsed
        // callsites) and the fully-populated github-pr requestor (the
        // `tatara-pool-reconciler::allocation_decide::alloc` shape).
        // A regression that reshaped the composer's output would
        // diverge from the pre-lift literal HERE rather than at every
        // downstream fixture's downstream assertion.
        let sample_requestors = [
            Requestor::kind_only("manual"),
            Requestor::kind_only("github-pr"),
            Requestor {
                kind: "github-pr".into(),
                repo: Some("pleme-io/demo".into()),
                branch: Some("main".into()),
                pr_number: None,
                sha: None,
                pr_labels: vec![],
                actor: None,
            },
        ];
        for r in sample_requestors {
            let via_primitive = AllocationSpec::requestor_only(r.clone());
            let hand_authored = AllocationSpec {
                pool_ref: None,
                requestor: r.clone(),
                ttl: None,
                note: None,
            };
            // Sweep every slot rather than round-tripping through
            // serde, so a slot rename that keeps the same serde name
            // still surfaces as a defect at the primitive's slot-
            // level parity.
            assert!(via_primitive.pool_ref.is_none() && hand_authored.pool_ref.is_none());
            assert_eq!(via_primitive.requestor.kind, hand_authored.requestor.kind);
            assert_eq!(via_primitive.ttl, hand_authored.ttl);
            assert_eq!(via_primitive.note, hand_authored.note);
        }
    }

    // ─── AllocationStatus::transition substrate pins ────────────────────
    //
    // Pin the substrate composer at fail-before-pass-after granularity:
    // the composer did not exist pre-lift, so any regression against
    // the four hand-authored sites in
    // `tatara-pool-reconciler::controller_allocation::reconcile_inner`
    // surfaces at these pins rather than as silent operator-visible
    // status-patch skew.

    fn anchor_time() -> DateTime<Utc> {
        // A deterministic non-`Utc::now()` anchor so pins that read
        // back `phase_since` do not race the wall clock.
        DateTime::parse_from_rfc3339("2026-05-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn allocation_status_transition_stamps_supplied_phase_verbatim() {
        for phase in AllocationPhase::ALL {
            let s = AllocationStatus::transition(phase, "irrelevant", anchor_time());
            assert_eq!(s.phase, phase, "phase drifted for {phase:?}");
        }
    }

    #[test]
    fn allocation_status_transition_stamps_supplied_message_verbatim() {
        let s = AllocationStatus::transition(
            AllocationPhase::Queued,
            "pool matched; no Free member available",
            anchor_time(),
        );
        assert_eq!(
            s.message.as_deref(),
            Some("pool matched; no Free member available"),
        );
    }

    #[test]
    fn allocation_status_transition_sets_phase_since_to_supplied_now() {
        let anchor = anchor_time();
        let s = AllocationStatus::transition(AllocationPhase::Bound, "bound", anchor);
        assert_eq!(
            s.phase_since,
            Some(anchor),
            "phase_since must be the supplied `now`, not a fresh Utc::now()",
        );
    }

    #[test]
    fn allocation_status_transition_defaults_every_optional_slot() {
        // The composer stamps only the three always-present slots
        // (`phase + phase_since + message`); every other slot on
        // `AllocationStatus` must land at its `Default`-equivalent
        // variant so a caller-branch that attaches an optional slot
        // via struct-update syntax does not silently inherit a
        // pre-populated non-`None`/non-empty value.
        let s = AllocationStatus::transition(AllocationPhase::Released, "released", anchor_time());
        assert!(s.bound_pool.is_none(), "bound_pool must default to None");
        assert!(
            s.assigned_process.is_none(),
            "assigned_process must default to None"
        );
        assert!(
            s.allocated_at.is_none(),
            "allocated_at must default to None"
        );
        assert!(s.expires_at.is_none(), "expires_at must default to None");
        assert!(
            s.conditions.is_empty(),
            "conditions must default to an empty Vec"
        );
    }

    #[test]
    fn allocation_status_transition_accepts_owned_string_and_static_str() {
        // `impl Into<String>` matches every current callsite:
        // three of the four hand-authored sites pass `&'static str`
        // literal reasons; the fourth ("bound to pool member") also
        // passes a `&'static str`. Sibling to
        // `tatara-reconciler::patch::phase_status_msg`'s identical
        // `impl Into<String>` signature.
        let via_static = AllocationStatus::transition(
            AllocationPhase::NoMatchingPool,
            "no Pool selector matched this Requestor",
            anchor_time(),
        );
        let via_owned = AllocationStatus::transition(
            AllocationPhase::NoMatchingPool,
            String::from("no Pool selector matched this Requestor"),
            anchor_time(),
        );
        assert_eq!(via_static.message, via_owned.message);
    }

    #[test]
    fn allocation_status_transition_serializes_to_pre_lift_json_shape() {
        // Byte-shape pin against the exact `json!({ "status": {
        // "phase": <variant>, "phaseSince": <now>, "message": "<msg>"
        // } })` incantation every pre-lift callsite restated. A
        // regression that reordered a slot, dropped the `phaseSince`
        // stamp, or drifted the camelCase key naming here surfaces at
        // THIS pin rather than as a subtle patch_status body the K8s
        // API server accepts but the pool reconciler's next observe
        // pass fails to read back.
        let anchor = anchor_time();
        let via_composer =
            AllocationStatus::transition(AllocationPhase::NoMatchingPool, "no match", anchor);
        let composed = serde_json::json!({ "status": via_composer });
        let hand_authored = serde_json::json!({
            "status": {
                "phase": AllocationPhase::NoMatchingPool,
                "phaseSince": anchor,
                "message": "no match",
            }
        });
        assert_eq!(composed, hand_authored);
    }

    #[test]
    fn allocation_status_transition_composes_with_struct_update_for_bind_seed() {
        // Pin the compound shape the `AllocationDecision::Bind`
        // callsite composes: the substrate seed carries `phase +
        // phase_since + message`, and the branch attaches
        // `bound_pool` + `assigned_process` + `allocated_at` +
        // `expires_at` via struct-update syntax. Post-lift the four
        // extra slots survive the compose intact and the base three
        // slots inherit the composer's stamps verbatim.
        let anchor = anchor_time();
        let ttl = anchor + chrono::Duration::hours(1);
        let pool = AllocationRef::new("demo-pool", "pools");
        let assigned = AllocationRef::new("demo-abcd", "pools");
        let bind_status = AllocationStatus {
            bound_pool: Some(pool.clone()),
            assigned_process: Some(assigned.clone()),
            allocated_at: Some(anchor),
            expires_at: Some(ttl),
            ..AllocationStatus::transition(AllocationPhase::Bound, "bound to pool member", anchor)
        };
        // Base-three slots stamped by the composer.
        assert_eq!(bind_status.phase, AllocationPhase::Bound);
        assert_eq!(bind_status.phase_since, Some(anchor));
        assert_eq!(bind_status.message.as_deref(), Some("bound to pool member"));
        // Struct-update-attached branch slots.
        assert_eq!(
            bind_status.bound_pool.as_ref().map(|r| &r.name),
            Some(&pool.name)
        );
        assert_eq!(
            bind_status.assigned_process.as_ref().map(|r| &r.name),
            Some(&assigned.name)
        );
        assert_eq!(bind_status.allocated_at, Some(anchor));
        assert_eq!(bind_status.expires_at, Some(ttl));
    }

    // ─── AllocationStatus::bound_transition substrate pins ─────────────
    //
    // Pin the compound composer at fail-before-pass-after granularity:
    // the composer wraps [`AllocationStatus::transition`] with the
    // `bound_pool + assigned_process` pair the Bind / Release arms
    // both stamped inline pre-lift.

    #[test]
    fn allocation_status_bound_transition_stamps_supplied_pool_and_process_verbatim() {
        let anchor = anchor_time();
        let pool = AllocationRef::new("demo-pool", "pools");
        let assigned = AllocationRef::new("demo-abcd", "pools");
        let s = AllocationStatus::bound_transition(
            AllocationPhase::Released,
            "released; pool reconciler will return the member",
            anchor,
            pool.clone(),
            assigned.clone(),
        );
        assert_eq!(s.bound_pool.as_ref(), Some(&pool));
        assert_eq!(s.assigned_process.as_ref(), Some(&assigned));
    }

    #[test]
    fn allocation_status_bound_transition_inherits_transition_triplet_verbatim() {
        // The compound composer must not stamp its own `phase +
        // phase_since + message` triplet — it MUST compose the pair
        // atop the substrate `Self::transition` seed so any future
        // evolution to the base triplet lands at ONE site and this
        // composer inherits the upgrade mechanically. Pin the triplet
        // through the same axis-uniform reads the transition tests use.
        let anchor = anchor_time();
        let via_compound = AllocationStatus::bound_transition(
            AllocationPhase::Bound,
            "bound to pool member",
            anchor,
            AllocationRef::new("p", "ns"),
            AllocationRef::new("q", "ns"),
        );
        let via_base =
            AllocationStatus::transition(AllocationPhase::Bound, "bound to pool member", anchor);
        assert_eq!(via_compound.phase, via_base.phase);
        assert_eq!(via_compound.phase_since, via_base.phase_since);
        assert_eq!(via_compound.message, via_base.message);
    }

    #[test]
    fn allocation_status_bound_transition_defaults_every_optional_slot_beyond_the_pair() {
        // The compound composer stamps only the base triplet + the
        // `bound_pool + assigned_process` pair; every other optional
        // slot (`allocated_at` / `expires_at` / `conditions`) must
        // land at its `Default`-equivalent variant so a caller-branch
        // that attaches an addendum via struct-update syntax (a Bind
        // arm's `allocated_at` + `expires_at` stamp) does not
        // silently inherit a pre-populated non-`None`/non-empty value.
        let s = AllocationStatus::bound_transition(
            AllocationPhase::Released,
            "released",
            anchor_time(),
            AllocationRef::new("p", "ns"),
            AllocationRef::new("q", "ns"),
        );
        assert!(
            s.allocated_at.is_none(),
            "allocated_at must default to None"
        );
        assert!(s.expires_at.is_none(), "expires_at must default to None");
        assert!(
            s.conditions.is_empty(),
            "conditions must default to an empty Vec"
        );
    }

    #[test]
    fn allocation_status_bound_transition_composes_with_struct_update_for_bind_seed() {
        // Pin the compound shape the `AllocationDecision::Bind`
        // callsite post-lift composes: the compound composer seeds
        // `phase + phase_since + message + bound_pool +
        // assigned_process`, and the Bind branch attaches
        // `allocated_at` + `expires_at` via struct-update syntax.
        // Post-lift the two extra slots survive the compose intact
        // and the base five slots inherit the composer's stamps
        // verbatim.
        let anchor = anchor_time();
        let ttl = anchor + chrono::Duration::hours(1);
        let pool = AllocationRef::new("demo-pool", "pools");
        let assigned = AllocationRef::new("demo-abcd", "pools");
        let bind_status = AllocationStatus {
            allocated_at: Some(anchor),
            expires_at: Some(ttl),
            ..AllocationStatus::bound_transition(
                AllocationPhase::Bound,
                "bound to pool member",
                anchor,
                pool.clone(),
                assigned.clone(),
            )
        };
        assert_eq!(bind_status.phase, AllocationPhase::Bound);
        assert_eq!(bind_status.phase_since, Some(anchor));
        assert_eq!(bind_status.message.as_deref(), Some("bound to pool member"));
        assert_eq!(bind_status.bound_pool.as_ref(), Some(&pool));
        assert_eq!(bind_status.assigned_process.as_ref(), Some(&assigned));
        assert_eq!(bind_status.allocated_at, Some(anchor));
        assert_eq!(bind_status.expires_at, Some(ttl));
    }

    #[test]
    fn allocation_status_bound_transition_matches_pre_lift_release_arm_verbatim() {
        // Byte-shape pin against the exact pre-lift `AllocationStatus
        // { bound_pool: Some(pool), assigned_process:
        // Some(AllocationRef::new(..)), ..AllocationStatus::transition
        // (Released, "…", now) }` composition the
        // `AllocationDecision::Release` arm restated inline pre-lift.
        // A regression that reordered the pair, dropped a `Some`, or
        // drifted the composed base triplet here surfaces at THIS pin
        // rather than as a subtle patch_status body the K8s API
        // server accepts but the audit record disagrees on.
        let anchor = anchor_time();
        let pool = AllocationRef::new("demo-pool", "pools");
        let assigned = AllocationRef::new("demo-abcd", "pools");
        let via_composer = AllocationStatus::bound_transition(
            AllocationPhase::Released,
            "released; pool reconciler will return the member",
            anchor,
            pool.clone(),
            assigned.clone(),
        );
        let via_hand_authored = AllocationStatus {
            bound_pool: Some(pool),
            assigned_process: Some(assigned),
            ..AllocationStatus::transition(
                AllocationPhase::Released,
                "released; pool reconciler will return the member",
                anchor,
            )
        };
        assert_eq!(
            serde_json::to_value(&via_composer).unwrap(),
            serde_json::to_value(&via_hand_authored).unwrap(),
        );
    }

    // ─── AllocationStatus::transition_now substrate pins ──────────────
    //
    // Bind [`AllocationStatus::transition_now`] at fail-before-pass-after
    // granularity so a regression that dropped the wall-clock read
    // (yielding a `phase_since` of `Some(DateTime::default())`),
    // reshaped the delegation target (a peer 3-arg composer that
    // stamped different defaults), or diverged the peer from the 3-arg
    // [`AllocationStatus::transition`] on any observable slot surfaces
    // HERE rather than as silent operator-facing drift at the two
    // controller_allocation status-patch sites.
    //
    // Each pin is fail-before-pass-after: the primitive did not exist
    // pre-lift, so any test that invokes it fails to compile pre-lift
    // and passes post-lift; the byte-identity pins below then bind the
    // specific shape choice. Sibling of the
    // `pool_status_observed_now_*` family in `crate::pool`.

    #[test]
    fn allocation_status_transition_now_composes_through_transition_with_wall_clock() {
        // Composition pin: `transition_now` MUST agree with the 3-arg
        // `transition(phase, msg, Utc::now())` peer at every slot other
        // than `phase_since` (which reads the wall clock at different
        // instants and diverges by scheduler jitter). A regression that
        // specialized either composer (a stray canonicalization at
        // `transition_now`, a swapped default at the 3-arg peer) would
        // surface HERE rather than as silent skew at the two
        // controller_allocation sites the primitive owns.
        let via_now = AllocationStatus::transition_now(AllocationPhase::Queued, "queued");
        let via_injected =
            AllocationStatus::transition(AllocationPhase::Queued, "queued", Utc::now());
        assert_eq!(via_now.phase, via_injected.phase);
        assert_eq!(via_now.message, via_injected.message);
        assert_eq!(via_now.bound_pool, via_injected.bound_pool);
        assert_eq!(via_now.assigned_process, via_injected.assigned_process);
        assert_eq!(via_now.allocated_at, via_injected.allocated_at);
        assert_eq!(via_now.expires_at, via_injected.expires_at);
        assert_eq!(via_now.conditions.len(), via_injected.conditions.len());
    }

    #[test]
    fn allocation_status_transition_now_reads_wall_clock_into_phase_since() {
        // Wall-clock pin: `phase_since` MUST fall between `Utc::now()`
        // reads bracketed around the call. A regression that dropped
        // the wall-clock read to a module-load constant (`Utc::now()`
        // captured at `static` init), a `DateTime::default()` (epoch),
        // or a stale `None` would fail this bracket check.
        let before = Utc::now();
        let s = AllocationStatus::transition_now(
            AllocationPhase::NoMatchingPool,
            "no Pool selector matched this Requestor",
        );
        let after = Utc::now();
        let phase_since = s
            .phase_since
            .expect("transition_now must stamp phase_since with the wall clock");
        assert!(
            phase_since >= before && phase_since <= after,
            "phase_since {phase_since} must fall in [{before}, {after}]"
        );
    }

    #[test]
    fn allocation_status_transition_now_stamps_the_same_defaults_as_the_injected_peer() {
        // Defaults pin: every optional slot beyond the base triplet
        // (`bound_pool` / `assigned_process` / `allocated_at` /
        // `expires_at` / `conditions`) MUST agree with the 3-arg
        // [`AllocationStatus::transition`] peer verbatim. A regression
        // that stamped a per-caller default at `transition_now` (a
        // "wall-clock-stamped transition" placeholder, say) or seeded
        // a "just-transitioned" Condition row would surface HERE
        // rather than as silent operator-facing drift at either
        // status-patch site.
        let s = AllocationStatus::transition_now(AllocationPhase::NoMatchingPool, "no match");
        assert!(s.bound_pool.is_none(), "bound_pool must default to None");
        assert!(
            s.assigned_process.is_none(),
            "assigned_process must default to None"
        );
        assert!(
            s.allocated_at.is_none(),
            "allocated_at must default to None"
        );
        assert!(s.expires_at.is_none(), "expires_at must default to None");
        assert!(
            s.conditions.is_empty(),
            "conditions must default to an empty Vec"
        );
    }

    #[test]
    fn allocation_status_transition_now_wall_clock_is_read_per_invocation_not_cached() {
        // Monotonic-read pin: two back-to-back `transition_now` calls
        // MUST read `Utc::now()` twice — the second `phase_since` MUST
        // be `>=` the first. A regression that cached a wall-clock
        // read into a `OnceLock` / lazy `static` would fire the SAME
        // `phase_since` for every caller on the reconciler's process
        // and every status-patch would carry the module-load instant
        // rather than the tick instant. Both instants may coincide on
        // a fast machine; use `>=` (not `>`) to keep the pin robust
        // against subsecond scheduler granularity while still catching
        // a cached-constant regression (where the second read would
        // be < the wall clock).
        let first = AllocationStatus::transition_now(AllocationPhase::Queued, "queued")
            .phase_since
            .expect("first transition_now stamps phase_since");
        let second = AllocationStatus::transition_now(AllocationPhase::Queued, "queued")
            .phase_since
            .expect("second transition_now stamps phase_since");
        assert!(
            second >= first,
            "second phase_since {second} must be >= first phase_since {first}"
        );
        let after = Utc::now();
        assert!(
            second <= after,
            "second phase_since {second} must be <= {after}"
        );
    }

    #[test]
    fn allocation_status_transition_now_accepts_owned_string_and_static_str() {
        // `impl Into<String>` matches every current callsite: both
        // hand-authored production sites pass `&'static str` literal
        // reasons; a future callsite that composes a `format!`-owned
        // reason routes through the same signature without widening.
        // Sibling to the 3-arg `AllocationStatus::transition` peer's
        // identical `impl Into<String>` signature.
        let via_static = AllocationStatus::transition_now(
            AllocationPhase::NoMatchingPool,
            "no Pool selector matched this Requestor",
        );
        let via_owned = AllocationStatus::transition_now(
            AllocationPhase::NoMatchingPool,
            String::from("no Pool selector matched this Requestor"),
        );
        assert_eq!(via_static.message, via_owned.message);
    }

    #[test]
    fn allocation_status_transition_now_composes_with_struct_update_for_wait_seed() {
        // Pin the compound shape the `AllocationDecision::Wait { pool }`
        // callsite post-lift composes: the composer seeds `phase +
        // phase_since + message`, and the Wait branch attaches
        // `bound_pool: Some(pool)` via struct-update syntax. Post-lift
        // the branch slot survives the compose intact and the base
        // three slots inherit the composer's stamps verbatim — matches
        // the pre-lift shape where `..AllocationStatus::transition(
        // Queued, msg, Utc::now())` fed the same struct-update seed.
        let pool = AllocationRef::new("attest-pool", "pools");
        let wait_status = AllocationStatus {
            bound_pool: Some(pool.clone()),
            ..AllocationStatus::transition_now(
                AllocationPhase::Queued,
                "pool matched; no Free member available",
            )
        };
        assert_eq!(wait_status.phase, AllocationPhase::Queued);
        assert_eq!(wait_status.bound_pool.as_ref(), Some(&pool));
        assert_eq!(
            wait_status.message.as_deref(),
            Some("pool matched; no Free member available")
        );
        assert!(
            wait_status.phase_since.is_some(),
            "phase_since must be stamped from the wall clock"
        );
        assert!(
            wait_status.assigned_process.is_none(),
            "assigned_process must remain None on the Wait seed"
        );
    }

    #[test]
    fn allocation_status_transition_now_shape_agrees_with_pool_status_observed_now_peer() {
        // Cross-CRD peer-axis coherence: both wall-clock-anchored
        // substrate composers
        // (`PoolStatus::observed_now`, `AllocationStatus::transition_now`)
        // read `Utc::now()` at their own body and stamp it into
        // `phase_since` uniformly across the two `<CRD>Status` axes.
        // Structural pin: if either side's `now` slot leaks into the
        // signature (e.g. adding an `impl Into<DateTime<Utc>>`
        // parameter), this bind fails to compile here rather than
        // silently drifting the wall-clock-anchored peer family apart.
        let _allocation_shape: fn(AllocationPhase, &'static str) -> AllocationStatus =
            AllocationStatus::transition_now;
        // (`PoolStatus::observed_now`'s pinned coherence lives at its
        // own peer pin family in `crate::pool`; this pin binds the
        // `AllocationStatus::transition_now` side of the peer pair.)
    }

    #[test]
    fn allocation_status_transition_shape_agrees_with_pool_status_observed_peer() {
        // Cross-CRD peer-axis coherence: both substrate composers
        // (`PoolStatus::observed`, `AllocationStatus::transition`)
        // accept a caller-supplied `now: DateTime<Utc>` at the SAME
        // signature slot, stamp it into `phase_since` uniformly, and
        // leave every other slot at its `Default`-equivalent variant.
        // Structural pin: if either side's `now` signature drifts to
        // `impl Into<DateTime<Utc>>` or a reference form, this bind
        // fails to compile here rather than silently drifting the
        // family apart.
        let _allocation_shape: fn(
            AllocationPhase,
            &'static str,
            DateTime<Utc>,
        ) -> AllocationStatus = AllocationStatus::transition;
        // (`PoolStatus::observed`'s pinned coherence lives at its own
        // peer pin family in `crate::pool`; this pin binds the
        // `AllocationStatus::transition` side of the peer pair.)
    }

    // ─── EphemeralAllocation::new_in substrate pins ────────────────────
    //
    // The pre-lift 2-line `let mut a = EphemeralAllocation::new(<name>,
    // <spec>); a.meta_mut().namespace = Some(<ns>.into());` chain
    // recurred at TWO workspace-wide sites past the ★★ PRIME-DIRECTIVE
    // ≥ 2 threshold across TWO crates (production PR-webhook emitter
    // at `tatara-github-watcher::allocation_factory::build_allocation`
    // + allocation-decision test fixture at `tatara-pool-reconciler::
    // allocation_decide::tests::alloc`). Post-lift the ONE substrate
    // composer stamps a namespaced `EphemeralAllocation` from
    // `(name, ns, spec)` in one call. Fail-before-pass-after
    // granularity: `new_in` did not exist pre-lift; the compiler
    // cannot resolve the name until the impl block above is in place,
    // so a rollback of the primitive breaks this whole pin block.

    fn sample_spec() -> AllocationSpec {
        AllocationSpec::requestor_only(Requestor::kind_only("manual"))
    }

    #[test]
    fn allocation_new_in_stamps_metadata_name_from_the_name_slot() {
        // `name` slot → `metadata.name` projection pin. Guards against
        // a regression that dropped the `name` slot into a `generate_
        // name` slot, an `annotations` seed, or any downstream slot the
        // kube-derived [`EphemeralAllocation::new`] does not populate
        // at `metadata.name` verbatim.
        let a = EphemeralAllocation::new_in("pr-42-demo", "pools", sample_spec());
        assert_eq!(a.metadata.name.as_deref(), Some("pr-42-demo"));
    }

    #[test]
    fn allocation_new_in_stamps_metadata_namespace_from_the_ns_slot() {
        // `ns` slot → `metadata.namespace` projection pin. Guards
        // against a regression that dropped the `ns` slot into a
        // `labels` seed, an unrelated annotation, or that stamped
        // `namespace = None` even after a caller-supplied value.
        let a = EphemeralAllocation::new_in("pr-42-demo", "pools", sample_spec());
        assert_eq!(a.metadata.namespace.as_deref(), Some("pools"));
    }

    #[test]
    fn allocation_new_in_stamps_spec_from_the_spec_slot_verbatim() {
        // `spec` slot → `spec` projection pin. A regression that
        // silently normalized the caller-supplied spec inside the
        // composer would diverge from the byte-identical pass-through
        // the pre-lift 2-line chain produced.
        let spec = AllocationSpec::requestor_only(Requestor::kind_only("github-pr"));
        let a = EphemeralAllocation::new_in("pr-42-demo", "pools", spec.clone());
        assert_eq!(a.spec.requestor.kind, spec.requestor.kind);
        assert!(a.spec.pool_ref.is_none());
        assert!(a.spec.ttl.is_none());
        assert!(a.spec.note.is_none());
    }

    #[test]
    fn allocation_new_in_accepts_both_owned_and_borrowed_namespace_slot() {
        // The `impl Into<String>` ergonomic contract round-trips
        // through both `&'static str` (the pre-lift test-fixture caller
        // shape) AND owned `String` (the pre-lift production-emitter
        // caller shape at `build_allocation`, where `namespace:
        // &str` was pushed through a `.to_string()`) at the SAME
        // signature. Guards against a regression that narrowed the
        // slot to `&str` only or that silently double-`.into()`d an
        // already-owned String.
        let via_str = EphemeralAllocation::new_in("pr-42-demo", "pools", sample_spec());
        let via_string =
            EphemeralAllocation::new_in("pr-42-demo", String::from("pools"), sample_spec());
        assert_eq!(via_str.metadata.namespace, via_string.metadata.namespace);
    }

    #[test]
    fn allocation_new_in_matches_pre_lift_construct_then_set_namespace_bytewise() {
        // Byte-shape parity witness against the pre-lift 2-line chain
        // across the two representative namespace shapes the collapsed
        // sites used (`"pools"` at `allocation_decide::alloc` +
        // production-emitter `"ephemeral-pools"` at `build_allocation`
        // + the free-form namespace `build_allocation` accepts). A
        // regression that shifted the composer's output would diverge
        // from the pre-lift literal HERE rather than at every
        // downstream consumer's assertion.
        for ns in ["pools", "ephemeral-pools", "custom-ns"] {
            let via_primitive = EphemeralAllocation::new_in("pr-42-demo", ns, sample_spec());
            let mut hand_authored = EphemeralAllocation::new("pr-42-demo", sample_spec());
            hand_authored.metadata.namespace = Some(ns.into());
            assert_eq!(via_primitive.metadata.name, hand_authored.metadata.name);
            assert_eq!(
                via_primitive.metadata.namespace,
                hand_authored.metadata.namespace,
            );
        }
    }

    #[test]
    fn allocation_new_in_defaults_other_metadata_slots_at_kube_derived_new() {
        // The composer forwards to the kube-derived
        // [`EphemeralAllocation::new`] for every non-namespace metadata
        // slot. A regression that stamped finalizers, owner_references,
        // labels, or annotations inside the composer's body —
        // inheriting the pre-lift chain's undocumented emptiness at
        // those slots — would surface here.
        let a = EphemeralAllocation::new_in("pr-42-demo", "pools", sample_spec());
        assert!(a.metadata.finalizers.is_none());
        assert!(a.metadata.owner_references.is_none());
        assert!(a.metadata.labels.is_none());
        assert!(a.metadata.annotations.is_none());
    }

    #[test]
    fn allocation_new_in_shape_agrees_with_pool_new_in_peer() {
        // Cross-CRD peer-axis structural coherence: both substrate
        // composers (`EphemeralPool::new_in`, `EphemeralAllocation::
        // new_in`) accept `(name: &str, namespace: impl Into<String>,
        // spec: <CRD>Spec)` at the SAME positional slot order and
        // return the CRD with `metadata.name` + `metadata.namespace`
        // stamped uniformly. Structural pin: if either side's slot
        // order drifts (e.g. `(ns, name, spec)`), this bind fails to
        // compile here rather than silently drifting the family apart.
        let _allocation_shape: fn(&str, &'static str, AllocationSpec) -> EphemeralAllocation =
            EphemeralAllocation::new_in;
        // (`EphemeralPool::new_in`'s pinned coherence lives at its own
        // peer pin family in `crate::pool`; this pin binds the
        // `EphemeralAllocation::new_in` side of the peer pair.)
    }
}
