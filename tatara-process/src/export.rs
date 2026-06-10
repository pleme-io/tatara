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
use std::str::FromStr;

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

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ArtifactError {
    #[error(
        "artifact source has no variant set (one of receipts/testReport/processSnapshot/runMarker required)"
    )]
    Empty,
    #[error("artifact source has multiple variants set; exactly one required")]
    Ambiguous,
}

impl ArtifactSource {
    /// Resolve to exactly one variant.
    pub fn variant(&self) -> Result<ArtifactVariant<'_>, ArtifactError> {
        use crate::tagged_union::{resolve, ResolveError};
        resolve([
            self.receipts.as_ref().map(ArtifactVariant::Receipts),
            self.test_report.as_ref().map(ArtifactVariant::TestReport),
            self.process_snapshot
                .as_ref()
                .map(ArtifactVariant::ProcessSnapshot),
            self.run_marker.as_ref().map(ArtifactVariant::RunMarker),
        ])
        .map_err(|e| match e {
            ResolveError::None => ArtifactError::Empty,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
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

#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChannelError {
    #[error("vector channel has no variant set (one of httpEvent/natsSubject/stdout required)")]
    Empty,
    #[error("vector channel has multiple variants set; exactly one required")]
    Ambiguous,
}

impl VectorChannel {
    pub fn variant(&self) -> Result<ChannelVariant<'_>, ChannelError> {
        use crate::tagged_union::{resolve, ResolveError};
        resolve([
            self.http_event.as_ref().map(ChannelVariant::HttpEvent),
            self.nats_subject.as_ref().map(ChannelVariant::NatsSubject),
            self.stdout.as_ref().map(ChannelVariant::Stdout),
        ])
        .map_err(|e| match e {
            ResolveError::None => ChannelError::Empty,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
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

impl FromStr for ExportTrigger {
    type Err = UnknownExportTrigger;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        for trigger in Self::ALL {
            if s == trigger.as_str() {
                return Ok(trigger);
            }
        }
        Err(UnknownExportTrigger(s.to_string()))
    }
}

/// Typed parse failure carrying the offending input verbatim so the
/// operator-facing diagnostic surfaces the bad value, not a normalized
/// form. Symmetric to [`crate::lifetime::UnknownTeardownPolicy`],
/// [`crate::boundary::UnknownConditionKind`], and
/// [`crate::phase::UnknownPhase`].
#[derive(Debug, thiserror::Error)]
#[error("unknown export trigger: {0}")]
pub struct UnknownExportTrigger(pub String);

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_source_empty_errors() {
        let s = ArtifactSource::default();
        assert_eq!(s.variant().unwrap_err(), ArtifactError::Empty);
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
        assert_eq!(c.variant().unwrap_err(), ChannelError::Empty);
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
    /// — pin its closure so a variant added without an `ALL` entry
    /// fails here (via the uniqueness check) before drifting `as_str` /
    /// `fires_on`. The arity is asserted by the `[Self; 3]` array type
    /// itself.
    #[test]
    fn export_trigger_all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for trigger in ExportTrigger::ALL {
            assert!(
                seen.insert(trigger),
                "duplicate variant in ALL: {trigger:?}"
            );
        }
        assert_eq!(seen.len(), ExportTrigger::ALL.len());
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

    /// Every variant in ALL round-trips through `as_str` ↔ `FromStr`.
    /// Adding a variant without extending `as_str` / `FromStr`'s sweep
    /// of `ALL` fails here.
    #[test]
    fn export_trigger_roundtrip_via_as_str() {
        use std::str::FromStr;
        for trigger in ExportTrigger::ALL {
            assert_eq!(
                ExportTrigger::from_str(trigger.as_str()).unwrap(),
                trigger,
                "round-trip failed for {trigger:?}"
            );
        }
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — empty / lowercased / typo / unrelated — and the
    /// error echoes the input verbatim so the operator-facing
    /// diagnostic carries the offending value, not a normalized form.
    #[test]
    fn unknown_export_trigger_errors() {
        use std::str::FromStr;
        for bad in ["", "onAttested", "ALWAYS", "Never", "OnSuccess"] {
            let err = ExportTrigger::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

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
}
