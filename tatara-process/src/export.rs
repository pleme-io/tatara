//! `ExportSpec` — what an ephemeral Process is allowed to leave behind.
//!
//! The compounding move: ephemeral envs default to **leaving nothing
//! behind**. Anything that must survive teardown is named explicitly via
//! one or more `ExportSpec`s on `:lifetime (:ephemeral … :exports …)`.
//! Each export declares:
//!
//!   * what artifact to ship out (a [`ArtifactSource`] variant)
//!   * where to ship it through (a [`VectorChannel`] variant — Vector
//!     ingest endpoint, NATS JetStream subject, or stdout)
//!   * when to ship it (an [`ExportTrigger`] variant)
//!
//! The pleme-io convention is that **everything emitted from a
//! workload flows through the Vector + NATS layer**, never to ad-hoc
//! sinks. `VectorChannel` enforces that at the type level — there is
//! no `S3Bucket` or `RawFileSystem` variant. Vector's downstream
//! sink graph (file, VictoriaLogs, VictoriaMetrics, Loki, …) handles
//! durability + analytics; this primitive only names the *ingestion*
//! shape.
//!
//! The reconciler's `Releasing` phase (between `Attested`/`Failed` and
//! `Exiting`) reads `lifetime.ephemeral.exports`, filters by
//! [`ExportTrigger`] against the terminal phase, and emits one
//! tatara-export-worker Job per surviving spec. Each Job emits a
//! receipt of its own export action so the export itself participates
//! in the BLAKE3 attestation chain.
//!
//! Lisp authoring:
//! ```lisp
//! (defephemeral akeyless-closed-loop-attest
//!   :aplicacao  (…)
//!   :ttl        "1h"
//!   :teardown   OnAttested
//!   :exports
//!     (;; Receipts — tier-1 guaranteed delivery via NATS JetStream
//!      (:source  (:receipts)
//!       :channel (:nats-subject :subject "pleme.pleme-dev.ephemeral.{{run_id}}.receipt"
//!                               :stream  "EPHEMERAL_RECEIPTS")
//!       :when    OnAttested)
//!      ;; Test report — best-effort via Vector HTTP ingest
//!      (:source  (:test-report :configmap "akeyless-test-results"
//!                              :key       "junit.xml"
//!                              :format    Junit)
//!       :channel (:http-event :signal-type "test-report")
//!       :when    Always)
//!      ;; Run marker — small synthetic event for shinryu cohort math
//!      (:source  (:run-marker :labels (:run-id "{{run_id}}"
//!                                       :phase "end"))
//!       :channel (:http-event :signal-type "ephemeral-marker")
//!       :when    Always)))
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

use crate::phase::ProcessPhase;

// ─── ExportSpec ────────────────────────────────────────────────────

/// One declared export from an ephemeral Process.
///
/// Multiple `ExportSpec`s can be attached to a single ephemeral
/// lifetime — each fires independently during the `Releasing` phase
/// when its [`ExportTrigger`] matches the terminal phase reached.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExportSpec {
    /// What artifact to ship.
    pub source: ArtifactSource,

    /// Where to ship it. Always Vector-native — pleme-io routes every
    /// emission through one of the four canonical channels.
    pub channel: VectorChannel,

    /// When to ship. Defaults to `OnAttested`.
    #[serde(default)]
    pub when: ExportTrigger,

    /// Override the run-id label that templates into channel subjects
    /// / signal-type metadata. Defaults to the Process's PID-derived
    /// run id when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub experiment_id_override: Option<String>,
}

// ─── ArtifactSource ────────────────────────────────────────────────

/// What artifact this export ships out.
///
/// Exactly-one-Option pattern, matching the rest of the typescape
/// (`Intent`, `Lifetime`). Adding a new artifact kind is additive on
/// the wire — existing JSON keeps deserializing unchanged.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactSource {
    /// Every `ReceiptEnvelope` emitted by this Process during its
    /// lifetime — the BLAKE3-chained typed attestation stream.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipts: Option<ReceiptsSource>,

    /// A test report stored in a ConfigMap by an in-cluster test
    /// runner (Job, gator, closed-loop probe). Worker reads the
    /// ConfigMap, packages it per `format`, and forwards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_report: Option<TestReportSource>,

    /// The Process's own `ProcessSpec` + `ProcessStatus` snapshot at
    /// teardown time, as canonical JSON. Useful for post-mortems on
    /// failed ephemeral runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_snapshot: Option<ProcessSnapshotSource>,

    /// A small synthetic event — start/end of run markers, cohort
    /// tags, experiment correlation. Worker emits a single
    /// timestamped event with the declared `labels`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_marker: Option<RunMarkerSource>,
}

/// Resolved enum view used by the worker.
#[derive(Clone, Debug)]
pub enum ArtifactVariant<'a> {
    Receipts(&'a ReceiptsSource),
    TestReport(&'a TestReportSource),
    ProcessSnapshot(&'a ProcessSnapshotSource),
    RunMarker(&'a RunMarkerSource),
}

impl ArtifactVariant<'_> {
    /// Reverse projection — every borrowed variant knows its
    /// `ArtifactKind` discriminator. Pairs with `ArtifactKind::select`
    /// so `ArtifactKind::select(source).map(|v| v.kind())` round-trips
    /// the closed set on the populated side; pinned by
    /// `artifact_kind_round_trips_through_variant_kind`. Future
    /// kind-keyed consumers (metric labels like
    /// `tatara_exports_total{artifact="receipts"}`, status-condition
    /// reason strings, audit-trail classifiers, LSP completion) reach
    /// through this projection instead of pattern-matching the
    /// payload-carrying view.
    pub fn kind(&self) -> ArtifactKind {
        match self {
            Self::Receipts(_) => ArtifactKind::Receipts,
            Self::TestReport(_) => ArtifactKind::TestReport,
            Self::ProcessSnapshot(_) => ArtifactKind::ProcessSnapshot,
            Self::RunMarker(_) => ArtifactKind::RunMarker,
        }
    }
}

/// Closed-set discriminator over `ArtifactSource`'s four tagged-union
/// slots. Single source of truth that drives `ArtifactSource::variant`'s
/// ambiguity + emptiness resolver, the `ArtifactError::Empty` message,
/// and the reverse `ArtifactVariant::kind` projection. Adding a fifth
/// artifact variant lands at one `ALL` entry + one `as_str` arm + one
/// `select` arm + one `ArtifactVariant::kind` arm — exhaustively
/// checked by the compiler.
///
/// Sibling closed-set lifts on the same `ExportSpec` axis:
/// [`crate::intent::IntentKind::ALL`], [`crate::lifetime::LifetimeKind::ALL`],
/// [`ExportTrigger::ALL`], [`ReportFormat::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_lisp::DeriveClosedSet)]
#[closed_set(via = "as_str", display, generate_unknown)]
pub enum ArtifactKind {
    Receipts,
    TestReport,
    ProcessSnapshot,
    RunMarker,
}

impl ArtifactKind {
    /// The closed set of artifact kinds — single source of truth that
    /// drives `ArtifactSource::variant`'s sweep so a variant added
    /// without an `ALL` entry never reaches the resolver. The `[Self; 4]`
    /// array literal forces the arity at compile time.
    pub const ALL: [Self; 4] = [
        Self::Receipts,
        Self::TestReport,
        Self::ProcessSnapshot,
        Self::RunMarker,
    ];

    /// Canonical camelCase wire-format key — matches the serde
    /// `rename_all = "camelCase"` field name on `ArtifactSource`. The
    /// `ArtifactError::Empty` message composes the human-readable list
    /// from this projection so a new variant lands in the
    /// operator-facing diagnostic automatically via the `ALL` sweep,
    /// not via hand-maintained error-string drift. Pinned by
    /// `artifact_kind_as_str_matches_field_name`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Receipts => "receipts",
            Self::TestReport => "testReport",
            Self::ProcessSnapshot => "processSnapshot",
            Self::RunMarker => "runMarker",
        }
    }

    /// Project an `ArtifactSource` borrow into the optional typed variant
    /// view for this kind. Returns `None` iff the matching slot is
    /// `None`. Composes the closed-set sweep `ArtifactSource::variant`
    /// loops over. Mirrors [`crate::intent::IntentKind::select`].
    pub fn select<'a>(self, source: &'a ArtifactSource) -> Option<ArtifactVariant<'a>> {
        match self {
            Self::Receipts => source.receipts.as_ref().map(ArtifactVariant::Receipts),
            Self::TestReport => source.test_report.as_ref().map(ArtifactVariant::TestReport),
            Self::ProcessSnapshot => source
                .process_snapshot
                .as_ref()
                .map(ArtifactVariant::ProcessSnapshot),
            Self::RunMarker => source.run_marker.as_ref().map(ArtifactVariant::RunMarker),
        }
    }
}

// `impl fmt::Display for ArtifactKind` + `impl FromStr for
// ArtifactKind` + `impl tatara_lisp::ClosedSet for ArtifactKind` +
// `pub struct UnknownArtifactKind(pub String)` are generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(via =
// "as_str", display, generate_unknown)]` on the enum declaration above.
// The auto-derived label `"artifact kind"` matches the prior hand-
// rolled `#[error("unknown artifact kind: {0}")]` verbatim. The
// inherent `as_str` projection stays load-bearing — the camelCase
// wire-format that matches the serde rename + the `ArtifactSource`
// struct-field name + the `ArtifactError::Empty` diagnostic verbatim
// — while the trait method `label` gives generic consumers a STABLE
// name across the workspace-wide closed-set implementors.

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ArtifactError {
    #[error("artifact source has no variant set (one of {0} required)")]
    Empty(&'static str),
    #[error("artifact source has multiple variants set; exactly one required")]
    Ambiguous,
}

/// Slash-joined list of every `ArtifactKind::as_str()` — composed once
/// at compile time so `ArtifactError::Empty`'s diagnostic carries the
/// closed-set summary without per-variant string drift. Mirrors
/// [`crate::intent::INTENT_KIND_LIST`] in shape.
const ARTIFACT_KIND_LIST: &str = "receipts/testReport/processSnapshot/runMarker";

impl ArtifactSource {
    /// Resolve to exactly one variant. Errors on zero or many.
    /// Sweeps over `ArtifactKind::ALL` so a fifth variant added with an
    /// `ALL` entry is structurally honored at this site — no parallel
    /// `is_some()` count, no per-variant if-let chain, no
    /// `unreachable!()`. The Empty diagnostic carries the closed-set
    /// list via `ARTIFACT_KIND_LIST`.
    pub fn variant(&self) -> Result<ArtifactVariant<'_>, ArtifactError> {
        use crate::tagged_union::{resolve, ResolveError};
        resolve(ArtifactKind::ALL.into_iter().map(|k| k.select(self))).map_err(|e| match e {
            ResolveError::None => ArtifactError::Empty(ARTIFACT_KIND_LIST),
            ResolveError::Many => ArtifactError::Ambiguous,
        })
    }
}

/// Receipts source — no fields. The worker reads every
/// `ReceiptEnvelope` annotated with this Process's PID.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptsSource {}

/// Test report source — a ConfigMap key with optional format hint.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestReportSource {
    /// ConfigMap name (in the Process's namespace) the runner wrote to.
    pub configmap: String,
    /// Key inside the ConfigMap holding the report bytes.
    pub key: String,
    /// Report shape hint — downstream parsers in shinryu key off this.
    #[serde(default)]
    pub format: ReportFormat,
    /// Optional ConfigMap namespace override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

/// Process snapshot source — bundles spec + status as JSON.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSnapshotSource {
    /// When true, also bundle Process attestation history.
    #[serde(default)]
    pub include_attestation_chain: bool,
}

/// Run marker source — small synthetic event with labels.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RunMarkerSource {
    /// Labels emitted on the marker event. Free-form key/value;
    /// downstream consumers (shinryu cohort math, vector transforms)
    /// read by key name.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

/// Bytes-shape hint for `TestReportSource`. Tatara emits the bytes
/// untransformed and tags the Vector event with this so shinryu
/// can route to the right parser tier.
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
    Default,
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown)]
pub enum ReportFormat {
    /// xUnit / JUnit XML — the closed-loop probe + gator emit this.
    Junit,
    /// TAP v13 — Bash/Bats test suites.
    TapV13,
    /// Newline-delimited JSON — one event per line, native shinryu shape.
    NdJson,
    /// Opaque bytes — no parser hint, downstream stores as-is.
    #[default]
    Raw,
}

/// How the export worker should embed a `TestReportSource`'s bytes into
/// the shipped `ExportEvent.payload`. Lifted as a closed-set typed
/// projection from [`ReportFormat::payload_shape`] so the worker's
/// dispatch is exhaustive on `ReportPayloadShape`, not on `ReportFormat`
/// with a silent `_` arm. Adding a future `ReportFormat` variant forces
/// the author to pick its shape here (single edit site); adding a
/// future shape (e.g. compressed) forces every consumer to handle it.
///
/// The (shape, JSON-embed-field) pairing — `NdJsonLines` → `"ndjson"`,
/// `OpaqueBytes` → `"raw_b64"` — binds at ONE typed projection
/// ([`Self::payload_field`]) rather than at the future worker's
/// embed-site string literals; pre-lift the field names lived in this
/// enum's per-variant docstring prose AND would have lived at the
/// worker's `payload.insert("ndjson", …)` / `payload.insert("raw_b64",
/// …)` call sites, where a rename of `"raw_b64"` → `"raw"` at one site
/// drifts silently from the docstring and the operator-facing shinryu
/// schema.
///
/// Sibling typed-projection lift over a closed enum (rather than
/// `matches!` / `_` arm dispatch):
/// [`crate::lifetime::TeardownPolicy::should_teardown_on`],
/// [`ExportTrigger::fires_on`], [`crate::phase::ProcessPhase::as_str`].
///
/// Sibling closed-set [`Self::ALL`] lift in lockstep with every other
/// `ALL`-keyed enum on the same `ExportSpec` axis ([`ReportFormat::ALL`],
/// [`ExportTrigger::ALL`]) and across the crate
/// ([`crate::phase::ProcessPhase::ALL`],
/// [`crate::signal::ProcessSignal::ALL`],
/// [`crate::boundary::ConditionKind::ALL`],
/// [`crate::lifetime::TeardownPolicy::ALL`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReportPayloadShape {
    /// Newline-delimited JSON — split on `\n`, parse each non-empty
    /// line as JSON, embed under `payload.<payload_field>` (=
    /// `payload.ndjson`) as an array. Field name is the typed
    /// projection [`Self::payload_field`] so no embed-site literal
    /// can drift from this docstring.
    NdJsonLines,
    /// Opaque bytes — base64-encode the artifact verbatim, embed
    /// under `payload.<payload_field>` (= `payload.raw_b64`) as a
    /// string. The default shape for anything Vector / shinryu
    /// shouldn't pre-parse. Field name is the typed projection
    /// [`Self::payload_field`].
    OpaqueBytes,
}

impl ReportPayloadShape {
    /// The closed set of payload shapes — single source of truth
    /// that drives the [`Self::as_str`] / [`fmt::Display`] pair and
    /// the [`Self::payload_field`] projection. Adding a third shape
    /// (e.g. `Compressed` → `"gzip"`) lands at one `ALL` entry + one
    /// `as_str` arm + one `payload_field` arm + at least one new
    /// `ReportFormat::payload_shape` mapping — exhaustively checked
    /// by the compiler (the `[Self; 2]` array literal forces the
    /// arity).
    ///
    /// Sibling closed-set lifts on the same `ExportSpec` axis:
    /// [`ReportFormat::ALL`], [`ExportTrigger::ALL`],
    /// [`crate::lifetime::TeardownPolicy::ALL`],
    /// [`crate::boundary::ConditionKind::ALL`],
    /// [`crate::phase::ProcessPhase::ALL`],
    /// [`crate::signal::ProcessSignal::ALL`],
    /// [`crate::encapsulates::EncapsulationMode::ALL`].
    pub const ALL: [Self; 2] = [Self::NdJsonLines, Self::OpaqueBytes];

    /// Canonical PascalCase identifier — used by [`fmt::Display`] (so
    /// `format!("{shape}")` never reaches for `{:?}` Debug) and as
    /// the operator-facing reason-string projection. No serde wire
    /// shape today (the enum is worker-internal), but the identifier
    /// matches the sibling-aligned `as_str` shape that every other
    /// closed-set enum in this crate exposes
    /// ([`ReportFormat::as_str`], [`ExportTrigger::as_str`],
    /// [`crate::phase::ProcessPhase::as_str`]). Pinned by
    /// `report_payload_shape_as_str_unique_per_variant`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NdJsonLines => "NdJsonLines",
            Self::OpaqueBytes => "OpaqueBytes",
        }
    }

    /// The JSON field name within `ExportEvent.payload` where the
    /// worker embeds this shape's encoded value — `"ndjson"` for the
    /// newline-split array, `"raw_b64"` for the base64-encoded
    /// opaque string. Pre-lift these names lived ONLY in the
    /// per-variant docstring prose; once the export worker lands, a
    /// rename at the embed site (`payload.insert("ndjson", …)` →
    /// `payload.insert("events", …)`) silently drifts from the
    /// docstring and from the operator-facing shinryu schema with no
    /// compile or runtime signal. Post-lift the worker's embed site
    /// is `payload.insert(shape.payload_field().into(), …)` and a
    /// rename lands at ONE arm here. The per-variant uniqueness
    /// invariant (no two shapes alias to the same field name) is
    /// pinned by `report_payload_shape_payload_field_unique_per_
    /// variant` so the worker's embed site cannot have two shapes
    /// collide on the same destination key. Truth table pinned by
    /// `report_payload_shape_payload_field_truth_table` so a future
    /// rename lands here at one site, not in the worker.
    pub const fn payload_field(self) -> &'static str {
        match self {
            Self::NdJsonLines => "ndjson",
            Self::OpaqueBytes => "raw_b64",
        }
    }
}

impl fmt::Display for ReportPayloadShape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl ReportFormat {
    /// The closed set of report formats — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad and the typed
    /// `payload_shape` dispatch. Adding a fifth variant lands at one
    /// `ALL` entry + one `as_str` arm + one `payload_shape` arm —
    /// exhaustively checked by the compiler (the `[Self; 4]` array
    /// literal forces the arity).
    ///
    /// Sibling closed-set lifts on the same `ExportSpec` axis:
    /// [`ExportTrigger::ALL`], [`crate::lifetime::TeardownPolicy::ALL`],
    /// [`crate::boundary::ConditionKind::ALL`],
    /// [`crate::phase::ProcessPhase::ALL`],
    /// [`crate::signal::ProcessSignal::ALL`],
    /// [`crate::encapsulates::EncapsulationMode::ALL`].
    pub const ALL: [Self; 4] = [Self::Junit, Self::TapV13, Self::NdJson, Self::Raw];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim. Used by Display
    /// (single source of truth) and by `FromStr`'s sweep of `ALL` so
    /// the typed surface and the YAML wire format cannot drift. Pinned
    /// by `report_format_as_str_matches_serde`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Junit => "Junit",
            Self::TapV13 => "TapV13",
            Self::NdJson => "NdJson",
            Self::Raw => "Raw",
        }
    }

    /// Typed projection: which payload-embedding strategy the export
    /// worker should pick for this format. ONE typed dispatch that
    /// replaces the worker's `match tr.format { NdJson => …, _ => … }`
    /// silent-default arm. Adding a new `ReportFormat` variant forces
    /// the author to decide its shape here (the compiler exhaustively
    /// checks this match); the worker's dispatch on the returned
    /// `ReportPayloadShape` then remains a closed 2-arm match. Pinned
    /// by `report_format_payload_shape_truth_table`.
    pub const fn payload_shape(self) -> ReportPayloadShape {
        match self {
            Self::NdJson => ReportPayloadShape::NdJsonLines,
            Self::Junit | Self::TapV13 | Self::Raw => ReportPayloadShape::OpaqueBytes,
        }
    }
}

impl fmt::Display for ReportFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// `impl FromStr for ReportFormat` + `impl tatara_lisp::ClosedSet for
// ReportFormat` are generated by `#[derive(tatara_lisp_derive::ClosedSet)]`
// on the enum declaration above — the trait-impl plumbing collapses onto
// ONE derive line + the `#[closed_set(via = "as_str")]` attribute that
// names the inherent projection method. Per-implementor content stays
// at `pub const ALL` + `pub const fn as_str` + the `UnknownReportFormat`
// carrier below.

// `pub struct UnknownReportFormat(pub String)` is generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"report format"`
// matches the prior hand-rolled `#[error("unknown report format: {0}")]`
// verbatim. Symmetric to [`UnknownChannelKind`],
// [`UnknownExportTrigger`], [`crate::lifetime::UnknownTeardownPolicy`],
// [`crate::boundary::UnknownConditionKind`],
// [`crate::phase::UnknownPhase`].

// ─── VectorChannel ─────────────────────────────────────────────────

/// Where the export bytes flow.
///
/// All variants land in the pleme-io Vector + NATS layer — there is
/// no escape hatch for ad-hoc sinks. Vector's downstream sink graph
/// (file / Loki / VictoriaLogs / VictoriaMetrics) handles durability
/// + analytics. This primitive only names the *ingestion* shape.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VectorChannel {
    /// HTTP POST to Vector's `http_server` source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_event: Option<HttpEventChannel>,

    /// Publish to a NATS JetStream subject. Use for tier-1
    /// guaranteed-delivery events (receipts that MUST survive).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nats_subject: Option<NatsSubjectChannel>,

    /// Print to the export worker's stdout. Vector's
    /// `kubernetes_logs` source picks it up. Lowest-effort channel;
    /// fine for debug / one-off exports.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<StdoutChannel>,
}

#[derive(Clone, Debug)]
pub enum ChannelVariant<'a> {
    HttpEvent(&'a HttpEventChannel),
    NatsSubject(&'a NatsSubjectChannel),
    Stdout(&'a StdoutChannel),
}

impl ChannelVariant<'_> {
    /// Reverse projection — every borrowed channel variant knows its
    /// `ChannelKind` discriminator. Pairs with [`ChannelKind::select`]
    /// so `ChannelKind::select(channel).map(|v| v.kind())` round-trips
    /// the closed set on the populated side; pinned by
    /// `channel_kind_round_trips_through_variant_kind`. Future
    /// kind-keyed consumers (metric labels like
    /// `tatara_exports_total{channel="natsSubject"}`, status-condition
    /// reason strings, audit-trail classifiers, LSP completion) reach
    /// through this projection instead of pattern-matching the
    /// payload-carrying view. Mirrors
    /// [`ArtifactVariant::kind`] and [`crate::intent::IntentVariant::kind`].
    pub fn kind(&self) -> ChannelKind {
        match self {
            Self::HttpEvent(_) => ChannelKind::HttpEvent,
            Self::NatsSubject(_) => ChannelKind::NatsSubject,
            Self::Stdout(_) => ChannelKind::Stdout,
        }
    }
}

/// Closed-set discriminator over `VectorChannel`'s three tagged-union
/// slots. Single source of truth that drives `VectorChannel::variant`'s
/// ambiguity + emptiness resolver, the `ChannelError::Empty` message,
/// and the reverse `ChannelVariant::kind` projection. Adding a fourth
/// channel variant lands at one `ALL` entry + one `as_str` arm + one
/// `select` arm + one `ChannelVariant::kind` arm — exhaustively
/// checked by the compiler.
///
/// Sibling closed-set lifts on the same `ExportSpec` axis:
/// [`ArtifactKind::ALL`], [`ExportTrigger::ALL`], [`ReportFormat::ALL`],
/// [`ReportPayloadShape::ALL`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_lisp::DeriveClosedSet)]
#[closed_set(via = "as_str", generate_unknown)]
pub enum ChannelKind {
    HttpEvent,
    NatsSubject,
    Stdout,
}

impl ChannelKind {
    /// The closed set of channel kinds — single source of truth that
    /// drives `VectorChannel::variant`'s sweep so a variant added
    /// without an `ALL` entry never reaches the resolver. The
    /// `[Self; 3]` array literal forces the arity at compile time.
    pub const ALL: [Self; 3] = [Self::HttpEvent, Self::NatsSubject, Self::Stdout];

    /// Canonical camelCase wire-format key — matches the serde
    /// `rename_all = "camelCase"` field name on `VectorChannel`. The
    /// `ChannelError::Empty` message composes the human-readable list
    /// from this projection so a new variant lands in the
    /// operator-facing diagnostic automatically via the `ALL` sweep,
    /// not via hand-maintained error-string drift. Pinned by
    /// `channel_kind_as_str_matches_field_name`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HttpEvent => "httpEvent",
            Self::NatsSubject => "natsSubject",
            Self::Stdout => "stdout",
        }
    }

    /// Project a `VectorChannel` borrow into the optional typed variant
    /// view for this kind. Returns `None` iff the matching slot is
    /// `None`. Composes the closed-set sweep `VectorChannel::variant`
    /// loops over. Mirrors [`ArtifactKind::select`] +
    /// [`crate::intent::IntentKind::select`] +
    /// [`crate::lifetime::LifetimeKind::select`].
    pub fn select<'a>(self, channel: &'a VectorChannel) -> Option<ChannelVariant<'a>> {
        match self {
            Self::HttpEvent => channel.http_event.as_ref().map(ChannelVariant::HttpEvent),
            Self::NatsSubject => channel
                .nats_subject
                .as_ref()
                .map(ChannelVariant::NatsSubject),
            Self::Stdout => channel.stdout.as_ref().map(ChannelVariant::Stdout),
        }
    }
}

impl fmt::Display for ChannelKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// `impl FromStr for ChannelKind` + `impl tatara_lisp::ClosedSet for
// ChannelKind` are generated by `#[derive(tatara_lisp::DeriveClosedSet)]`
// on the enum declaration above.

// `pub struct UnknownChannelKind(pub String)` is generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"channel kind"`
// matches the prior hand-rolled `#[error("unknown channel kind: {0}")]`
// verbatim. Symmetric to [`UnknownArtifactKind`],
// [`UnknownReportFormat`], [`UnknownExportTrigger`],
// [`crate::lifetime::UnknownTeardownPolicy`],
// [`crate::boundary::UnknownConditionKind`],
// [`crate::phase::UnknownPhase`].

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChannelError {
    #[error("vector channel has no variant set (one of {0} required)")]
    Empty(&'static str),
    #[error("vector channel has multiple variants set; exactly one required")]
    Ambiguous,
}

/// Slash-joined list of every `ChannelKind::as_str()` — composed once
/// at compile time so `ChannelError::Empty`'s diagnostic carries the
/// closed-set summary without per-variant string drift. Mirrors
/// [`ARTIFACT_KIND_LIST`] in shape.
const CHANNEL_KIND_LIST: &str = "httpEvent/natsSubject/stdout";

impl VectorChannel {
    /// Resolve to exactly one channel variant. Errors on zero or many.
    /// Sweeps over `ChannelKind::ALL` so a fourth variant added with an
    /// `ALL` entry is structurally honored at this site — no parallel
    /// `is_some()` count, no per-variant if-let chain, no
    /// `unreachable!()`. The Empty diagnostic carries the closed-set
    /// list via `CHANNEL_KIND_LIST`.
    pub fn variant(&self) -> Result<ChannelVariant<'_>, ChannelError> {
        use crate::tagged_union::{resolve, ResolveError};
        resolve(ChannelKind::ALL.into_iter().map(|k| k.select(self))).map_err(|e| match e {
            ResolveError::None => ChannelError::Empty(CHANNEL_KIND_LIST),
            ResolveError::Many => ChannelError::Ambiguous,
        })
    }
}

/// HTTP POST channel — Vector `http_server` source.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HttpEventChannel {
    /// Vector ingest endpoint. Defaults to the in-cluster Service
    /// `http://vector.observability.svc.cluster.local:8080` when
    /// unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,

    /// `signal_type` tag added to every emitted event. Vector
    /// transforms + shinryu's analytical schema route by this tag
    /// (`receipt`, `test-report`, `ephemeral-marker`, …).
    pub signal_type: String,
}

/// Default Vector ingest endpoint when `HttpEventChannel.endpoint`
/// is unset. Single source of truth for downstream tooling.
pub const DEFAULT_VECTOR_INGEST: &str = "http://vector.observability.svc.cluster.local:8080";

impl HttpEventChannel {
    /// Resolve the endpoint URL, falling back to the in-cluster default.
    pub fn resolved_endpoint(&self) -> &str {
        self.endpoint.as_deref().unwrap_or(DEFAULT_VECTOR_INGEST)
    }
}

/// NATS JetStream channel — guaranteed-delivery publish.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NatsSubjectChannel {
    /// Subject to publish to. May contain `{{run_id}}` template
    /// substitution — the worker substitutes the resolved run id at
    /// publish time.
    pub subject: String,

    /// JetStream stream the subject belongs to. The stream itself is
    /// declared by the consumer chart (e.g. tatara-pool-reconciler)
    /// via the pleme-nats broker-only design.
    pub stream: String,

    /// Optional NATS URL. Defaults to `nats://nats.observability.svc.cluster.local:4222`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Default NATS URL when `NatsSubjectChannel.url` is unset.
pub const DEFAULT_NATS_URL: &str = "nats://nats.observability.svc.cluster.local:4222";

impl NatsSubjectChannel {
    /// Resolve the NATS URL, falling back to the in-cluster default.
    pub fn resolved_url(&self) -> &str {
        self.url.as_deref().unwrap_or(DEFAULT_NATS_URL)
    }
}

/// Stdout channel — worker prints the event; Vector picks up via
/// `kubernetes_logs`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StdoutChannel {
    /// Pretty-print JSON (multi-line) instead of compact NDJSON.
    /// Defaults to false — compact NDJSON matches Vector's parser.
    #[serde(default)]
    pub pretty: bool,
}

// ─── ExportTrigger ─────────────────────────────────────────────────

/// When the export fires. Aligns with `ProcessPhase` so the
/// reconciler's `Releasing` phase can match against the terminal
/// phase reached directly.
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
    Default,
    tatara_lisp::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown)]
pub enum ExportTrigger {
    /// Fire when the Process reaches `Attested`. Default — matches
    /// the most common case (capture successful-run artifacts).
    #[default]
    OnAttested,
    /// Fire when the Process reaches `Failed`. Use for failure
    /// post-mortems (process snapshots, last receipts).
    OnFailed,
    /// Fire on every terminal phase (`Attested` or `Failed`). Use
    /// for run markers that need to surface regardless of outcome.
    Always,
}

impl ExportTrigger {
    /// The closed set of export triggers — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad and the typed
    /// `fires_on` dispatch over `ProcessPhase`. Adding a fourth variant
    /// lands at one `ALL` entry + one `as_str` arm + one `fires_on` arm
    /// — exhaustively checked by the compiler (the `[Self; 3]` array
    /// literal forces the arity).
    ///
    /// Sibling closed-set lifts on the same `ProcessSpec` axis:
    /// [`crate::lifetime::TeardownPolicy::ALL`],
    /// [`crate::intent::IntentKind::ALL`],
    /// [`crate::lifetime::LifetimeKind::ALL`],
    /// [`crate::boundary::ConditionKind::ALL`],
    /// [`crate::phase::ProcessPhase::ALL`],
    /// [`crate::signal::ProcessSignal::ALL`].
    pub const ALL: [Self; 3] = [Self::OnAttested, Self::OnFailed, Self::Always];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim. Used by Display
    /// (single source of truth), by `FromStr` to identify the variant
    /// from its annotation / status-field representation, and by
    /// operator-facing reason strings without reaching for `{:?}` Debug
    /// formatting. Pinned by `export_trigger_as_str_matches_serde`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OnAttested => "OnAttested",
            Self::OnFailed => "OnFailed",
            Self::Always => "Always",
        }
    }

    /// True iff, given a `ProcessPhase`, this trigger says "fire."
    /// ONE typed dispatch over the typed phase enum that replaces the
    /// four hand-rolled `match phase { Attested => fires_on_attested(),
    /// Failed => fires_on_failed(), _ => false }` sites the reconciler
    /// and `EphemeralLifetime` previously branched on. Every
    /// non-terminal phase always returns `false` — exports are a
    /// terminal-phase decision, now enforced by the closed-set match
    /// over `ProcessPhase`.
    ///
    /// The legacy [`Self::fires_on_attested`] / [`Self::fires_on_failed`]
    /// predicates remain as thin delegates so existing call sites keep
    /// their narrow signatures; the truth table is pinned by
    /// `export_trigger_legacy_predicates_delegate_to_phase_dispatch`.
    pub const fn fires_on(self, phase: ProcessPhase) -> bool {
        match phase {
            ProcessPhase::Attested => matches!(self, Self::OnAttested | Self::Always),
            ProcessPhase::Failed => matches!(self, Self::OnFailed | Self::Always),
            ProcessPhase::Pending
            | ProcessPhase::Forking
            | ProcessPhase::Execing
            | ProcessPhase::Running
            | ProcessPhase::Reconverging
            | ProcessPhase::Releasing
            | ProcessPhase::Exiting
            | ProcessPhase::Zombie
            | ProcessPhase::Reaped => false,
        }
    }

    /// Thin delegate to [`Self::fires_on`] for the `Attested` case —
    /// kept so existing call sites that already know the gate keep
    /// their narrow signature without reaching for the typed-phase
    /// variant.
    pub const fn fires_on_attested(self) -> bool {
        self.fires_on(ProcessPhase::Attested)
    }

    /// Symmetric delegate to [`Self::fires_on`] for the `Failed` case.
    pub const fn fires_on_failed(self) -> bool {
        self.fires_on(ProcessPhase::Failed)
    }
}

impl fmt::Display for ExportTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// `impl FromStr for ExportTrigger` + `impl tatara_lisp::ClosedSet for
// ExportTrigger` are generated by `#[derive(tatara_lisp::DeriveClosedSet)]`
// on the enum declaration above.

// `pub struct UnknownExportTrigger(pub String)` is generated by
// `#[derive(tatara_lisp::DeriveClosedSet)]` + `#[closed_set(generate_unknown)]`
// on the enum declaration above. The auto-derived label `"export trigger"`
// matches the prior hand-rolled `#[error("unknown export trigger: {0}")]`
// verbatim. Symmetric to [`UnknownChannelKind`],
// [`UnknownReportFormat`], [`crate::lifetime::UnknownTeardownPolicy`],
// [`crate::boundary::UnknownConditionKind`], and
// [`crate::phase::UnknownPhase`].

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_source_empty_errors() {
        let s = ArtifactSource::default();
        match s.variant().unwrap_err() {
            ArtifactError::Empty(list) => assert_eq!(list, ARTIFACT_KIND_LIST),
            other => panic!("expected Empty, got {other:?}"),
        }
    }

    #[test]
    fn artifact_source_receipts_resolves() {
        let s = ArtifactSource {
            receipts: Some(ReceiptsSource::default()),
            ..ArtifactSource::default()
        };
        assert!(matches!(s.variant().unwrap(), ArtifactVariant::Receipts(_)));
    }

    #[test]
    fn artifact_source_two_variants_ambiguous() {
        let s = ArtifactSource {
            receipts: Some(ReceiptsSource::default()),
            test_report: Some(TestReportSource {
                configmap: "x".into(),
                key: "y".into(),
                format: ReportFormat::Junit,
                namespace: None,
            }),
            ..ArtifactSource::default()
        };
        assert_eq!(s.variant().unwrap_err(), ArtifactError::Ambiguous);
    }

    #[test]
    fn vector_channel_empty_errors() {
        let c = VectorChannel::default();
        match c.variant().unwrap_err() {
            ChannelError::Empty(list) => assert_eq!(list, CHANNEL_KIND_LIST),
            other => panic!("expected Empty, got {other:?}"),
        }
    }

    #[test]
    fn vector_channel_resolves_http_event() {
        let c = VectorChannel {
            http_event: Some(HttpEventChannel {
                endpoint: None,
                signal_type: "test-report".into(),
            }),
            ..VectorChannel::default()
        };
        match c.variant().unwrap() {
            ChannelVariant::HttpEvent(h) => {
                assert_eq!(h.signal_type, "test-report");
                assert_eq!(h.resolved_endpoint(), DEFAULT_VECTOR_INGEST);
            }
            other => panic!("expected HttpEvent, got {other:?}"),
        }
    }

    #[test]
    fn vector_channel_resolves_nats_subject() {
        let c = VectorChannel {
            nats_subject: Some(NatsSubjectChannel {
                subject: "pleme.pleme-dev.ephemeral.{{run_id}}.receipt".into(),
                stream: "EPHEMERAL_RECEIPTS".into(),
                url: None,
            }),
            ..VectorChannel::default()
        };
        match c.variant().unwrap() {
            ChannelVariant::NatsSubject(n) => {
                assert_eq!(n.stream, "EPHEMERAL_RECEIPTS");
                assert_eq!(n.resolved_url(), DEFAULT_NATS_URL);
            }
            other => panic!("expected NatsSubject, got {other:?}"),
        }
    }

    #[test]
    fn export_trigger_fire_logic() {
        assert!(ExportTrigger::OnAttested.fires_on_attested());
        assert!(!ExportTrigger::OnAttested.fires_on_failed());
        assert!(ExportTrigger::OnFailed.fires_on_failed());
        assert!(!ExportTrigger::OnFailed.fires_on_attested());
        assert!(ExportTrigger::Always.fires_on_attested());
        assert!(ExportTrigger::Always.fires_on_failed());
    }

    // ── closed-set algebra for ExportTrigger (ALL × as_str × FromStr ×
    //    fires_on(phase)) ─

    /// `ALL` is the source of truth for the resolver / `FromStr` sweep
    /// Structural well-formedness of [`ExportTrigger`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `export_trigger_all_is_unique_and_complete` +
    /// `export_trigger_roundtrip_via_as_str` + the empty-input arm of
    /// `unknown_export_trigger_errors`. `FromStr` delegates to
    /// `<Self as tatara_lisp::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the reconciler hits when parsing
    /// a CRD `enum:`-validated value back to the typed trigger.
    #[test]
    fn export_trigger_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<ExportTrigger>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface and the YAML wire format
    /// the reconciler / operator both read.
    #[test]
    fn export_trigger_as_str_matches_serde() {
        for trigger in ExportTrigger::ALL {
            let serialized = serde_json::to_string(&trigger).expect("serialize");
            // serde_json wraps strings in quotes; strip them for compare.
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                trigger.as_str(),
                "as_str drift for {trigger:?}: as_str={} serde={unquoted}",
                trigger.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. If a reviewer
    /// accidentally re-introduces an inline match in Display, this
    /// test would fail the moment a variant rename touches one site
    /// but not the other.
    #[test]
    fn export_trigger_display_matches_as_str() {
        for trigger in ExportTrigger::ALL {
            assert_eq!(trigger.to_string(), trigger.as_str());
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / unrelated — and the error
    /// echoes the input verbatim so the operator-facing diagnostic
    /// carries the offending value, not a normalized form. The
    /// empty-input arm is pinned by
    /// [`export_trigger_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownExportTrigger`] newtype,
    /// which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_export_trigger_errors() {
        use std::str::FromStr;
        for bad in ["onAttested", "ALWAYS", "Never", "OnSuccess"] {
            let err = ExportTrigger::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    // `unknown_export_trigger_message_matches_substrate_convention`
    // removed — clause (5) of
    // `tatara_lisp::assert_closed_set_well_formed::<ExportTrigger>()`
    // verifies the substrate-wide `"unknown {SET_LABEL}: {input}"`
    // shape generically (called from `trigger_is_well_formed_closed_set`
    // above); the `SET_LABEL` projection is pinned by
    // `tatara_lisp_derive::pascal_to_spaced_lowercase_tests`.

    /// TRUTH-TABLE CONTRACT: `fires_on(phase)` agrees with the
    /// documented (trigger, phase) -> bool table for every (3 × 11)
    /// combination. A new variant in either `ExportTrigger` or
    /// `ProcessPhase` reaches this test by iteration — adding a phase
    /// without extending `fires_on`'s match would be caught by the
    /// compiler (the closed-set match over `ProcessPhase` enforces it);
    /// adding a trigger without extending its truth row is caught
    /// here.
    #[test]
    fn export_trigger_fires_on_truth_table() {
        // ProcessPhase imports are local to the test to keep the
        // module's top-level surface minimal.
        use crate::phase::ProcessPhase::{
            Attested, Execing, Exiting, Failed, Forking, Pending, Reaped, Reconverging, Releasing,
            Running, Zombie,
        };
        let table: &[(ExportTrigger, &[(crate::phase::ProcessPhase, bool)])] = &[
            (
                ExportTrigger::OnAttested,
                &[
                    (Attested, true),
                    (Failed, false),
                    (Pending, false),
                    (Forking, false),
                    (Execing, false),
                    (Running, false),
                    (Reconverging, false),
                    (Releasing, false),
                    (Exiting, false),
                    (Zombie, false),
                    (Reaped, false),
                ],
            ),
            (
                ExportTrigger::OnFailed,
                &[
                    (Attested, false),
                    (Failed, true),
                    (Pending, false),
                    (Forking, false),
                    (Execing, false),
                    (Running, false),
                    (Reconverging, false),
                    (Releasing, false),
                    (Exiting, false),
                    (Zombie, false),
                    (Reaped, false),
                ],
            ),
            (
                ExportTrigger::Always,
                &[
                    (Attested, true),
                    (Failed, true),
                    (Pending, false),
                    (Forking, false),
                    (Execing, false),
                    (Running, false),
                    (Reconverging, false),
                    (Releasing, false),
                    (Exiting, false),
                    (Zombie, false),
                    (Reaped, false),
                ],
            ),
        ];
        // The truth table must cover every (trigger, phase) pair.
        assert_eq!(table.len(), ExportTrigger::ALL.len());
        for (_, row) in table {
            assert_eq!(row.len(), crate::phase::ProcessPhase::ALL.len());
        }
        for (trigger, row) in table {
            for (phase, expected) in *row {
                assert_eq!(
                    trigger.fires_on(*phase),
                    *expected,
                    "fires_on({trigger:?}, {phase:?}) drift"
                );
            }
        }
    }

    /// DELEGATION CONTRACT: the legacy `fires_on_attested` /
    /// `fires_on_failed` predicates agree with the typed
    /// `fires_on(phase)` dispatch they delegate to, for every variant
    /// in `ALL`. A regression that re-introduces an inline `matches!`
    /// in either legacy predicate fails here. `fires_on` is the
    /// source of truth.
    #[test]
    fn export_trigger_legacy_predicates_delegate_to_phase_dispatch() {
        for trigger in ExportTrigger::ALL {
            assert_eq!(
                trigger.fires_on_attested(),
                trigger.fires_on(crate::phase::ProcessPhase::Attested),
                "legacy fires_on_attested drift for {trigger:?}"
            );
            assert_eq!(
                trigger.fires_on_failed(),
                trigger.fires_on(crate::phase::ProcessPhase::Failed),
                "legacy fires_on_failed drift for {trigger:?}"
            );
        }
    }

    // ── closed-set algebra for ReportFormat (ALL × as_str × FromStr ×
    //    payload_shape) ─

    /// Structural well-formedness of [`ReportFormat`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `report_format_all_is_unique_and_complete` +
    /// `report_format_roundtrip_via_as_str` + the empty-input arm of
    /// `unknown_report_format_errors`. `FromStr` delegates to
    /// `<Self as tatara_lisp::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the export worker hits when
    /// parsing a CRD `enum:`-validated value back to the typed format.
    #[test]
    fn report_format_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<ReportFormat>();
    }

    /// CANONICAL-KEY CONTRACT: `as_str` matches serde's PascalCase
    /// output verbatim for every variant. A future variant rename
    /// (or an `as_str` arm typo) lands here at one site, instead of
    /// drifting between the typed surface and the YAML wire format
    /// the reconciler / operator both read.
    #[test]
    fn report_format_as_str_matches_serde() {
        for format in ReportFormat::ALL {
            let serialized = serde_json::to_string(&format).expect("serialize");
            let unquoted = serialized
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            assert_eq!(
                unquoted,
                format.as_str(),
                "as_str drift for {format:?}: as_str={} serde={unquoted}",
                format.as_str()
            );
        }
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift.
    #[test]
    fn report_format_display_matches_as_str() {
        for format in ReportFormat::ALL {
            assert_eq!(format.to_string(), format.as_str());
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / unrelated — and the error
    /// echoes the input verbatim so the operator-facing diagnostic
    /// carries the offending value, not a normalized form. The
    /// empty-input arm is pinned by
    /// [`report_format_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownReportFormat`] newtype,
    /// which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_report_format_errors() {
        use std::str::FromStr;
        for bad in ["junit", "JUNIT", "tap", "Yaml", "TomlV1"] {
            let err = ReportFormat::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    // `unknown_report_format_message_matches_substrate_convention`
    // removed — clause (5) of
    // `tatara_lisp::assert_closed_set_well_formed::<ReportFormat>()`
    // verifies the substrate-wide `"unknown {SET_LABEL}: {input}"`
    // shape generically (called from `report_format_is_well_formed_closed_set`
    // above); the `SET_LABEL` projection is pinned by
    // `tatara_lisp_derive::pascal_to_spaced_lowercase_tests`.

    /// TRUTH-TABLE CONTRACT: `payload_shape` agrees with the documented
    /// shape table for every variant in `ALL`. A new variant whose
    /// shape the author forgets to add to `payload_shape`'s match is
    /// caught by the compiler at the match site; a regression that
    /// reshuffles existing variants (e.g. routing `NdJson` to opaque
    /// bytes) is caught here. `payload_shape` is the worker's only
    /// dispatch — once this passes, the worker's `match shape { … }`
    /// is exhaustive on the 2-variant `ReportPayloadShape` instead of
    /// the 4-variant `ReportFormat`, so adding a future format never
    /// touches the worker.
    #[test]
    fn report_format_payload_shape_truth_table() {
        let table: &[(ReportFormat, ReportPayloadShape)] = &[
            (ReportFormat::Junit, ReportPayloadShape::OpaqueBytes),
            (ReportFormat::TapV13, ReportPayloadShape::OpaqueBytes),
            (ReportFormat::NdJson, ReportPayloadShape::NdJsonLines),
            (ReportFormat::Raw, ReportPayloadShape::OpaqueBytes),
        ];
        assert_eq!(table.len(), ReportFormat::ALL.len());
        for (format, expected) in table {
            assert_eq!(
                format.payload_shape(),
                *expected,
                "payload_shape({format:?}) drift"
            );
        }
    }

    /// CLOSURE-OF-PROJECTION CONTRACT: every `ReportPayloadShape`
    /// variant is the image of at least one `ReportFormat` variant —
    /// no shape is stranded. A `Compressed` shape added to
    /// `ReportPayloadShape::ALL` without an `ALL → payload_shape`
    /// mapping at any `ReportFormat` arm makes the worker's
    /// 3-variant dispatch reachable from no input, which would
    /// silently dead-code one arm. Caught here.
    #[test]
    fn report_payload_shape_reachable_from_some_report_format() {
        for shape in ReportPayloadShape::ALL {
            let reachable = ReportFormat::ALL.iter().any(|f| f.payload_shape() == shape);
            assert!(
                reachable,
                "{shape:?} is in ReportPayloadShape::ALL but no ReportFormat projects to it"
            );
        }
    }

    /// CLOSED-SET CONTRACT: `ReportPayloadShape::ALL` enumerates each
    /// variant exactly once. The `[Self; 2]` array literal forces
    /// the arity at compile time; this test pins per-variant
    /// reachability so adding a third shape (`Compressed`) without
    /// extending `ALL` fails here rather than silently dropping the
    /// new variant from every sweep through `Self::ALL`.
    #[test]
    fn report_payload_shape_all_enumerates_each_variant_exactly_once() {
        let mut seen = std::collections::HashSet::new();
        for shape in ReportPayloadShape::ALL {
            assert!(seen.insert(shape), "duplicate variant in ALL: {shape:?}");
        }
        assert_eq!(seen.len(), ReportPayloadShape::ALL.len());
        for shape in [
            ReportPayloadShape::NdJsonLines,
            ReportPayloadShape::OpaqueBytes,
        ] {
            assert!(
                ReportPayloadShape::ALL.contains(&shape),
                "{shape:?} declared but not in ALL"
            );
        }
    }

    /// CANONICAL-KEY UNIQUENESS: no two shapes alias the same
    /// `as_str` identifier. A future rename of one variant to a name
    /// that collides with another (e.g. both → `"Lines"`) breaks the
    /// shape's identity in operator-facing reason strings and would
    /// silently make Display non-injective. Caught here.
    #[test]
    fn report_payload_shape_as_str_unique_per_variant() {
        let mut seen = std::collections::HashSet::new();
        for shape in ReportPayloadShape::ALL {
            assert!(
                seen.insert(shape.as_str()),
                "as_str collision: {shape:?} → {:?}",
                shape.as_str()
            );
        }
        assert_eq!(seen.len(), ReportPayloadShape::ALL.len());
    }

    /// DISPLAY-IS-AS_STR: the Display impl IS `as_str` — pinning
    /// this lets callers reach for either projection without drift.
    /// Sibling to `report_format_display_matches_as_str` and
    /// `export_trigger_display_matches_as_str`.
    #[test]
    fn report_payload_shape_display_matches_as_str() {
        for shape in ReportPayloadShape::ALL {
            assert_eq!(shape.to_string(), shape.as_str());
        }
    }

    /// EMBED-FIELD UNIQUENESS: no two shapes write into the same
    /// `payload.<field>` key. The worker's embed site is
    /// `payload.insert(shape.payload_field().into(), …)`; if two
    /// shapes aliased to the same field name, two different report
    /// sources arriving in the same export envelope would silently
    /// overwrite each other's bytes. Caught here.
    #[test]
    fn report_payload_shape_payload_field_unique_per_variant() {
        let mut seen = std::collections::HashSet::new();
        for shape in ReportPayloadShape::ALL {
            assert!(
                seen.insert(shape.payload_field()),
                "payload_field collision: {shape:?} → {:?}",
                shape.payload_field()
            );
        }
        assert_eq!(seen.len(), ReportPayloadShape::ALL.len());
    }

    /// TRUTH-TABLE: `payload_field` matches the documented
    /// `payload.ndjson` / `payload.raw_b64` shinryu schema. A future
    /// rename (e.g. `"raw_b64"` → `"raw"`) lands here at one arm
    /// rather than drifting between the docstring prose and the
    /// worker's embed-site literal. Adding a third shape forces the
    /// author to add a row here (driven by `ALL`), so the table
    /// width tracks the closed set.
    #[test]
    fn report_payload_shape_payload_field_truth_table() {
        let table: &[(ReportPayloadShape, &str)] = &[
            (ReportPayloadShape::NdJsonLines, "ndjson"),
            (ReportPayloadShape::OpaqueBytes, "raw_b64"),
        ];
        assert_eq!(table.len(), ReportPayloadShape::ALL.len());
        for (shape, expected) in table {
            assert_eq!(
                shape.payload_field(),
                *expected,
                "payload_field({shape:?}) drift"
            );
        }
    }

    /// Every variant's `payload_field` is non-empty and contains no
    /// JSON-path-separator (`.`) — the worker concatenates
    /// `payload.<payload_field>` so an embedded `.` would alias into
    /// the parent map and silently flatten the embed. Structural
    /// guard for the field-name shape.
    #[test]
    fn report_payload_shape_payload_field_is_a_single_segment() {
        for shape in ReportPayloadShape::ALL {
            let field = shape.payload_field();
            assert!(
                !field.is_empty(),
                "payload_field({shape:?}) is empty — embed site has no destination"
            );
            assert!(
                !field.contains('.'),
                "payload_field({shape:?}) contains '.' ({field:?}) — would flatten the embed into payload's parent map"
            );
        }
    }

    #[test]
    fn export_spec_serde_round_trip() {
        let spec = ExportSpec {
            source: ArtifactSource {
                test_report: Some(TestReportSource {
                    configmap: "akeyless-test-results".into(),
                    key: "junit.xml".into(),
                    format: ReportFormat::Junit,
                    namespace: None,
                }),
                ..ArtifactSource::default()
            },
            channel: VectorChannel {
                http_event: Some(HttpEventChannel {
                    endpoint: None,
                    signal_type: "test-report".into(),
                }),
                ..VectorChannel::default()
            },
            when: ExportTrigger::Always,
            experiment_id_override: Some("akeyless-run-2026-05-20".into()),
        };

        let yaml = serde_yaml::to_string(&spec).unwrap();
        // camelCase wire format — what FluxCD / kubectl users see.
        assert!(yaml.contains("source:"));
        assert!(yaml.contains("testReport:"));
        assert!(yaml.contains("configmap: akeyless-test-results"));
        assert!(yaml.contains("format: Junit"));
        assert!(yaml.contains("channel:"));
        assert!(yaml.contains("httpEvent:"));
        assert!(yaml.contains("signalType: test-report"));
        assert!(yaml.contains("when: Always"));
        assert!(yaml.contains("experimentIdOverride: akeyless-run-2026-05-20"));

        let back: ExportSpec = serde_yaml::from_str(&yaml).unwrap();
        assert!(back.source.test_report.is_some());
        assert!(back.channel.http_event.is_some());
        assert_eq!(back.when, ExportTrigger::Always);
    }

    #[test]
    fn run_marker_labels_round_trip() {
        let mut labels = BTreeMap::new();
        labels.insert("run-id".into(), "akeyless-run-2026-05-20".into());
        labels.insert("phase".into(), "end".into());
        let spec = ExportSpec {
            source: ArtifactSource {
                run_marker: Some(RunMarkerSource { labels }),
                ..ArtifactSource::default()
            },
            channel: VectorChannel {
                http_event: Some(HttpEventChannel {
                    endpoint: None,
                    signal_type: "ephemeral-marker".into(),
                }),
                ..VectorChannel::default()
            },
            when: ExportTrigger::Always,
            experiment_id_override: None,
        };
        let yaml = serde_yaml::to_string(&spec).unwrap();
        assert!(yaml.contains("runMarker:"));
        assert!(yaml.contains("run-id: akeyless-run-2026-05-20"));
        let back: ExportSpec = serde_yaml::from_str(&yaml).unwrap();
        let rm = back.source.run_marker.unwrap();
        assert_eq!(rm.labels["phase"], "end");
    }

    /// Default endpoints resolve to the canonical in-cluster Service
    /// DNS — a single source of truth other tatara crates can
    /// re-export instead of duplicating literals.
    #[test]
    fn default_endpoints_are_stable_constants() {
        assert_eq!(
            DEFAULT_VECTOR_INGEST,
            "http://vector.observability.svc.cluster.local:8080"
        );
        assert_eq!(
            DEFAULT_NATS_URL,
            "nats://nats.observability.svc.cluster.local:4222"
        );
    }

    // ── closed-set algebra for ArtifactKind (ALL × as_str × Display ×
    //    FromStr × select × ArtifactVariant::kind) ─────────────────────

    /// Structural well-formedness of [`ArtifactKind`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `artifact_kind_all_is_unique_and_complete` +
    /// `artifact_kind_roundtrip_via_as_str` + the empty-input arm of
    /// `unknown_artifact_kind_errors`. `FromStr` delegates to
    /// `<Self as tatara_lisp::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the export worker hits when
    /// parsing a CRD `enum:`-validated value back to the typed kind.
    #[test]
    fn artifact_kind_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<ArtifactKind>();
    }

    /// CANONICAL-KEY CONTRACT: every `ArtifactKind::as_str()` matches
    /// the serde `rename_all = "camelCase"` field name on the
    /// corresponding `Option<…>` slot of `ArtifactSource`. A future
    /// rename of either the struct field OR the `as_str` arm lands
    /// here at one site, instead of drifting between the typed
    /// surface, the YAML wire format, and the `ArtifactError::Empty`
    /// diagnostic. The mapping is the table the serde derive produces
    /// against the struct field declarations above; reading the YAML
    /// output pins it without re-deriving by hand.
    #[test]
    fn artifact_kind_as_str_matches_field_name() {
        // Round-trip a populated source through serde_yaml and confirm
        // the emitted key name agrees with `ArtifactKind::as_str()` for
        // each variant. Drives the closed set via `ALL` so a fifth
        // variant lands here automatically once construction is added.
        for kind in ArtifactKind::ALL {
            let s = single_slot_source(kind);
            let yaml = serde_yaml::to_string(&s).expect("serialize");
            let key = kind.as_str();
            assert!(
                yaml.contains(&format!("{key}:")),
                "as_str(={key:?}) for {kind:?} not present in serialized YAML:\n{yaml}"
            );
        }
    }

    /// CANONICAL-NAMES PIN: byte-exact camelCase wire-format pin —
    /// renaming any of these strings IS a wire-format break that fails
    /// this test FIRST so the rename stays a deliberate decision, not
    /// a typo. Locks the (variant → operator-facing key) table.
    #[test]
    fn artifact_kind_canonical_names_pinned() {
        assert_eq!(ArtifactKind::Receipts.as_str(), "receipts");
        assert_eq!(ArtifactKind::TestReport.as_str(), "testReport");
        assert_eq!(ArtifactKind::ProcessSnapshot.as_str(), "processSnapshot");
        assert_eq!(ArtifactKind::RunMarker.as_str(), "runMarker");
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. If a reviewer
    /// accidentally re-introduces an inline match in Display, this
    /// test would fail the moment a variant rename touches one site
    /// but not the other.
    #[test]
    fn artifact_kind_display_matches_as_str() {
        for kind in ArtifactKind::ALL {
            assert_eq!(kind.to_string(), kind.as_str());
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — PascalCased / typo / cross-axis-leaked inputs
    /// from sibling closed-set enums on the same `ExportSpec` axis
    /// (`Junit`, `OnAttested`, …) — and the error echoes the input
    /// verbatim so the operator-facing diagnostic carries the
    /// offending value, not a normalized form. `ArtifactKind` is its
    /// own axis, NOT a transparent reflection of any sibling. The
    /// empty-input arm is pinned by
    /// [`artifact_kind_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownArtifactKind`] newtype,
    /// which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_artifact_kind_errors() {
        use std::str::FromStr;
        for bad in [
            "Receipts",
            "test_report",
            "RECEIPTS",
            "snapshot",
            "marker",
            "Junit",
            "OnAttested",
            "NdJsonLines",
        ] {
            let err = ArtifactKind::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    /// ROUND-TRIP CONTRACT: every kind reaches its borrowed-variant
    /// view via `select`, and that variant projects back to the same
    /// kind via `ArtifactVariant::kind`. A regression that misroutes a
    /// select arm (e.g. `Self::Receipts => source.test_report.as_ref()
    /// ...`) fails loudly here.
    #[test]
    fn artifact_kind_round_trips_through_variant_kind() {
        for kind in ArtifactKind::ALL {
            let s = single_slot_source(kind);
            let v = kind.select(&s).expect("populated slot must select");
            assert_eq!(v.kind(), kind, "round-trip failed for {kind:?}");
            // And the resolver lands on the same variant.
            assert_eq!(
                s.variant().expect("exactly-one variant").kind(),
                kind,
                "variant() resolver disagreed on {kind:?}"
            );
        }
    }

    /// SELECT-EMPTY CONTRACT: an unpopulated slot returns `None` from
    /// `select`, for every kind. Pairs with the resolver's `Empty`
    /// path so a future kind's slot defaulting wrong (e.g. accidentally
    /// `Some(Default::default())` instead of `None`) is caught here.
    #[test]
    fn artifact_kind_select_returns_none_for_unset_slot() {
        let empty = ArtifactSource::default();
        for kind in ArtifactKind::ALL {
            assert!(
                kind.select(&empty).is_none(),
                "{kind:?} reported populated on a default ArtifactSource"
            );
        }
    }

    /// EMPTY-DIAGNOSTIC CONTRACT: the closed-set kind list embedded
    /// in `ArtifactError::Empty` echoes the canonical join of every
    /// `ArtifactKind::as_str()` projection. A variant added without
    /// updating `ARTIFACT_KIND_LIST` (or a renamed variant) shows up
    /// here as a mismatch. Mirrors
    /// `intent_error_empty_lists_every_kind_in_canonical_order`.
    #[test]
    fn artifact_error_empty_lists_every_kind_in_canonical_order() {
        let parts: Vec<&'static str> = ArtifactKind::ALL.iter().map(|k| k.as_str()).collect();
        let joined = parts.join("/");
        assert_eq!(joined, ARTIFACT_KIND_LIST);
    }

    /// AMBIGUOUS-PATH CONTRACT: when two slots are populated the
    /// resolver yields `Ambiguous`, exhaustively across every pair in
    /// `ALL × ALL` (excluding the diagonal). A future asymmetry where
    /// one slot would silently shadow another (e.g. an `if-let` chain
    /// re-introducing first-wins ordering) is caught here.
    #[test]
    fn artifact_source_two_slots_is_ambiguous_across_every_pair() {
        for a in ArtifactKind::ALL {
            for b in ArtifactKind::ALL {
                if a == b {
                    continue;
                }
                let s = two_slot_source(a, b);
                assert_eq!(
                    s.variant().unwrap_err(),
                    ArtifactError::Ambiguous,
                    "({a:?}, {b:?}) should resolve Ambiguous"
                );
            }
        }
    }

    // ── closed-set algebra for ChannelKind (ALL × as_str × Display ×
    //    FromStr × select × ChannelVariant::kind) ─────────────────────

    /// Structural well-formedness of [`ChannelKind`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through
    /// `label ↔ parse_label`, labels are pairwise distinct, `""` is
    /// outside the closed set) at ONE call site. Replaces the hand-
    /// derived `channel_kind_all_is_unique_and_complete` +
    /// `channel_kind_roundtrip_via_as_str` + the empty-input arm of
    /// `unknown_channel_kind_errors`. `FromStr` delegates to
    /// `<Self as tatara_lisp::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the export worker hits when
    /// parsing a CRD `enum:`-validated value back to the typed kind.
    #[test]
    fn channel_kind_is_well_formed_closed_set() {
        tatara_lisp::assert_closed_set_well_formed::<ChannelKind>();
    }

    /// CANONICAL-KEY CONTRACT: every `ChannelKind::as_str()` matches
    /// the serde `rename_all = "camelCase"` field name on the
    /// corresponding `Option<…>` slot of `VectorChannel`. A future
    /// rename of either the struct field OR the `as_str` arm lands
    /// here at one site, instead of drifting between the typed
    /// surface, the YAML wire format, and the `ChannelError::Empty`
    /// diagnostic.
    #[test]
    fn channel_kind_as_str_matches_field_name() {
        for kind in ChannelKind::ALL {
            let c = single_slot_channel(kind);
            let yaml = serde_yaml::to_string(&c).expect("serialize");
            let key = kind.as_str();
            assert!(
                yaml.contains(&format!("{key}:")),
                "as_str(={key:?}) for {kind:?} not present in serialized YAML:\n{yaml}"
            );
        }
    }

    /// CANONICAL-NAMES PIN: byte-exact camelCase wire-format pin —
    /// renaming any of these strings IS a wire-format break that fails
    /// this test FIRST so the rename stays a deliberate decision, not
    /// a typo. Locks the (variant → operator-facing key) table.
    #[test]
    fn channel_kind_canonical_names_pinned() {
        assert_eq!(ChannelKind::HttpEvent.as_str(), "httpEvent");
        assert_eq!(ChannelKind::NatsSubject.as_str(), "natsSubject");
        assert_eq!(ChannelKind::Stdout.as_str(), "stdout");
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift.
    #[test]
    fn channel_kind_display_matches_as_str() {
        for kind in ChannelKind::ALL {
            assert_eq!(kind.to_string(), kind.as_str());
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — PascalCased / typo / cross-axis-leaked inputs
    /// from sibling closed-set enums on the same `ExportSpec` axis
    /// (`Receipts`, `OnAttested`, `Junit`, …) — and the error echoes
    /// the input verbatim so the operator-facing diagnostic carries
    /// the offending value, not a normalized form. `ChannelKind` is
    /// its own axis, NOT a transparent reflection of any sibling. The
    /// empty-input arm is pinned by
    /// [`channel_kind_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownChannelKind`] newtype,
    /// which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_channel_kind_errors() {
        use std::str::FromStr;
        for bad in [
            "HttpEvent",
            "http_event",
            "HTTPEVENT",
            "nats",
            "STDOUT",
            "Receipts",
            "OnAttested",
            "Junit",
            "NdJsonLines",
        ] {
            let err = ChannelKind::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    // `unknown_channel_kind_message_matches_substrate_convention`
    // removed — clause (5) of
    // `tatara_lisp::assert_closed_set_well_formed::<ChannelKind>()`
    // verifies the substrate-wide `"unknown {SET_LABEL}: {input}"`
    // shape generically (called from `channel_kind_is_well_formed_closed_set`
    // above); the `SET_LABEL` projection is pinned by
    // `tatara_lisp_derive::pascal_to_spaced_lowercase_tests`.

    /// ROUND-TRIP CONTRACT: every kind reaches its borrowed-variant
    /// view via `select`, and that variant projects back to the same
    /// kind via `ChannelVariant::kind`. A regression that misroutes a
    /// select arm (e.g. `Self::HttpEvent => channel.nats_subject ...`)
    /// fails loudly here.
    #[test]
    fn channel_kind_round_trips_through_variant_kind() {
        for kind in ChannelKind::ALL {
            let c = single_slot_channel(kind);
            let v = kind.select(&c).expect("populated slot must select");
            assert_eq!(v.kind(), kind, "round-trip failed for {kind:?}");
            // And the resolver lands on the same variant.
            assert_eq!(
                c.variant().expect("exactly-one variant").kind(),
                kind,
                "variant() resolver disagreed on {kind:?}"
            );
        }
    }

    /// SELECT-EMPTY CONTRACT: an unpopulated slot returns `None` from
    /// `select`, for every kind. Pairs with the resolver's `Empty`
    /// path so a future kind's slot defaulting wrong (e.g. accidentally
    /// `Some(Default::default())` instead of `None`) is caught here.
    #[test]
    fn channel_kind_select_returns_none_for_unset_slot() {
        let empty = VectorChannel::default();
        for kind in ChannelKind::ALL {
            assert!(
                kind.select(&empty).is_none(),
                "{kind:?} reported populated on a default VectorChannel"
            );
        }
    }

    /// EMPTY-DIAGNOSTIC CONTRACT: the closed-set kind list embedded
    /// in `ChannelError::Empty` echoes the canonical join of every
    /// `ChannelKind::as_str()` projection. A variant added without
    /// updating `CHANNEL_KIND_LIST` (or a renamed variant) shows up
    /// here as a mismatch. Mirrors
    /// `artifact_error_empty_lists_every_kind_in_canonical_order`.
    #[test]
    fn channel_error_empty_lists_every_kind_in_canonical_order() {
        let parts: Vec<&'static str> = ChannelKind::ALL.iter().map(|k| k.as_str()).collect();
        let joined = parts.join("/");
        assert_eq!(joined, CHANNEL_KIND_LIST);
    }

    /// AMBIGUOUS-PATH CONTRACT: when two slots are populated the
    /// resolver yields `Ambiguous`, exhaustively across every pair in
    /// `ALL × ALL` (excluding the diagonal). A future asymmetry where
    /// one slot would silently shadow another (e.g. an `if-let` chain
    /// re-introducing first-wins ordering) is caught here.
    #[test]
    fn vector_channel_two_slots_is_ambiguous_across_every_pair() {
        for a in ChannelKind::ALL {
            for b in ChannelKind::ALL {
                if a == b {
                    continue;
                }
                let c = two_slot_channel(a, b);
                assert_eq!(
                    c.variant().unwrap_err(),
                    ChannelError::Ambiguous,
                    "({a:?}, {b:?}) should resolve Ambiguous"
                );
            }
        }
    }

    /// Construct a `VectorChannel` with exactly the given kind's slot
    /// populated by a minimal valid inner channel. Shared across the
    /// closed-set property tests so they each cover every variant
    /// without restating the construction table. Mirrors
    /// `single_slot_source` in shape.
    fn single_slot_channel(kind: ChannelKind) -> VectorChannel {
        match kind {
            ChannelKind::HttpEvent => VectorChannel {
                http_event: Some(HttpEventChannel {
                    endpoint: None,
                    signal_type: "x".into(),
                }),
                ..VectorChannel::default()
            },
            ChannelKind::NatsSubject => VectorChannel {
                nats_subject: Some(NatsSubjectChannel {
                    subject: "s".into(),
                    stream: "S".into(),
                    url: None,
                }),
                ..VectorChannel::default()
            },
            ChannelKind::Stdout => VectorChannel {
                stdout: Some(StdoutChannel::default()),
                ..VectorChannel::default()
            },
        }
    }

    /// Construct a `VectorChannel` with two slots populated — drives
    /// the pairwise `Ambiguous` sweep. Composes the single-slot
    /// constructor on top of itself to keep one source of truth for
    /// per-variant inner payloads.
    fn two_slot_channel(a: ChannelKind, b: ChannelKind) -> VectorChannel {
        let ca = single_slot_channel(a);
        let cb = single_slot_channel(b);
        VectorChannel {
            http_event: ca.http_event.or(cb.http_event),
            nats_subject: ca.nats_subject.or(cb.nats_subject),
            stdout: ca.stdout.or(cb.stdout),
        }
    }

    /// Construct an `ArtifactSource` with exactly the given kind's
    /// slot populated by a minimal valid inner source. Shared across
    /// the closed-set property tests so they each cover every variant
    /// without restating the construction table. Mirrors
    /// `single_slot_intent` in shape.
    fn single_slot_source(kind: ArtifactKind) -> ArtifactSource {
        match kind {
            ArtifactKind::Receipts => ArtifactSource {
                receipts: Some(ReceiptsSource::default()),
                ..ArtifactSource::default()
            },
            ArtifactKind::TestReport => ArtifactSource {
                test_report: Some(TestReportSource {
                    configmap: "cm".into(),
                    key: "k".into(),
                    format: ReportFormat::Junit,
                    namespace: None,
                }),
                ..ArtifactSource::default()
            },
            ArtifactKind::ProcessSnapshot => ArtifactSource {
                process_snapshot: Some(ProcessSnapshotSource::default()),
                ..ArtifactSource::default()
            },
            ArtifactKind::RunMarker => ArtifactSource {
                run_marker: Some(RunMarkerSource::default()),
                ..ArtifactSource::default()
            },
        }
    }

    /// Construct an `ArtifactSource` with two slots populated — drives
    /// the pairwise `Ambiguous` sweep. Composes the single-slot
    /// constructor on top of itself to keep one source of truth for
    /// per-variant inner payloads.
    fn two_slot_source(a: ArtifactKind, b: ArtifactKind) -> ArtifactSource {
        // Merge by populating each kind's slot from its single-slot view.
        let sa = single_slot_source(a);
        let sb = single_slot_source(b);
        ArtifactSource {
            receipts: sa.receipts.or(sb.receipts),
            test_report: sa.test_report.or(sb.test_report),
            process_snapshot: sa.process_snapshot.or(sb.process_snapshot),
            run_marker: sa.run_marker.or(sb.run_marker),
        }
    }
}
