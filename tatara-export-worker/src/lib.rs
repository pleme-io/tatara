//! Pure decision logic for `tatara-export-worker` — the binary that
//! ships one declared `ExportSpec` from an ephemeral Process to its
//! Vector-native channel.
//!
//! The compounding move: every function in this module is pure (no
//! HTTP, no NATS, no kube client, no clock), takes its inputs by
//! reference, and returns a typed value the I/O layer in `main.rs`
//! then consumes. That means the whole worker is unit-testable
//! without standing up infrastructure — and any new artifact source
//! or channel can be added by extending this module first, then the
//! I/O glue mechanically follows.
//!
//! Three substrate primitives live here:
//!
//! 1. [`prepare_event_payload`] — given an [`ArtifactVariant`] + raw
//!    artifact bytes + run id + signal_type, produces the JSON event
//!    the channel will ship. Encoded once, shared by all channels.
//!
//! 2. [`resolve_run_id`] / [`resolve_subject`] — string-template
//!    substitution for `{{run_id}}` in NATS subjects + event labels.
//!    Single source of truth so the worker, the reconciler, and any
//!    downstream cohort-correlation logic agree on what the run id
//!    means.
//!
//! 3. [`compose_export_receipt`] — builds a typed `ReceiptEnvelope`
//!    of the export action itself, with the three BLAKE3 pillars
//!    derived from the ExportSpec (intent), the shipped payload
//!    bytes (artifact), and the outcome (control). The receipt
//!    chains into the Process's attestation tree, so the act of
//!    exporting is itself attested.
//!
//! The I/O glue in `main.rs` is thin — argv → ExportSpec → call
//! these functions → ship to channel → write receipt.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use tatara_process::export::{
    ArtifactVariant, ExportSpec, NatsSubjectChannel, ReportFormat, ReportPayloadShape,
    RunMarkerSource,
};
use tatara_process::receipt::ReceiptEnvelope;

// ─── Run id resolution ─────────────────────────────────────────────

/// Resolve the run id used in event labels + subject templates.
///
/// Precedence:
/// 1. `spec.experiment_id_override` when set
/// 2. `{process_namespace}/{process_name}` otherwise
///
/// Single source of truth so every channel (HTTP, NATS, stdout) and
/// every downstream consumer (shinryu cohort math, Vector
/// transforms) agree on what "run id" means for a given export.
pub fn resolve_run_id(spec: &ExportSpec, namespace: &str, name: &str) -> String {
    if let Some(o) = &spec.experiment_id_override {
        if !o.is_empty() {
            return o.clone();
        }
    }
    format!("{namespace}/{name}")
}

/// Substitute `{{run_id}}` placeholders in a NATS subject template.
///
/// The chart's subject template (e.g.
/// `pleme.pleme-dev.ephemeral.{{run_id}}.receipt`) gets expanded
/// once, here, before the NATS publish call.
pub fn resolve_subject(channel: &NatsSubjectChannel, run_id: &str) -> String {
    channel.subject.replace("{{run_id}}", run_id)
}

// ─── Event payload preparation ─────────────────────────────────────

/// JSON event shape shipped through every `VectorChannel`. Stable
/// schema — shinryu's analytical SQL plane reads from it directly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportEvent {
    /// One of "receipt" / "test-report" / "process-snapshot" /
    /// "run-marker". Carried both in the body (for downstream SQL)
    /// and in the channel metadata (HttpEventChannel.signal_type or
    /// NATS subject path).
    pub signal_type: String,

    /// Resolved run id — `{process_namespace}/{process_name}` by
    /// default, or `spec.experiment_id_override` when set.
    pub run_id: String,

    /// Timestamp the export was prepared (RFC 3339 UTC).
    pub timestamp: DateTime<Utc>,

    /// Artifact-source-specific labels — empty for Receipts /
    /// ProcessSnapshot, ConfigMap reference for TestReport, free-form
    /// for RunMarker.
    pub labels: BTreeMap<String, String>,

    /// The shipped artifact bytes, embedded as the `payload` field.
    /// For JSON sources (receipts, snapshots) this is a JSON Value;
    /// for opaque bytes (TestReport with format=Raw) it's a base64
    /// string under the `raw` key.
    pub payload: serde_json::Value,

    /// Format hint copied from `TestReportSource.format` when
    /// applicable. Lets downstream parsers branch by JUnit / TAP /
    /// NDJSON / Raw without inspecting the bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<ReportFormat>,
}

/// Build the JSON event the channel ships.
///
/// `artifact_bytes` is the raw artifact (ConfigMap value, snapshot
/// JSON, or empty for run-markers). The function dispatches on
/// `source` to embed the bytes the right way:
///
/// - **Receipts** — `artifact_bytes` is a JSON array of receipt
///   envelopes; embedded under `payload.receipts`.
/// - **TestReport** — `artifact_bytes` is the report file's bytes;
///   embedded as either a parsed JSON value (when `format=NdJson`)
///   or a base64 string (every other format).
/// - **ProcessSnapshot** — `artifact_bytes` is the Process JSON;
///   embedded under `payload.snapshot`.
/// - **RunMarker** — `artifact_bytes` is ignored; labels come from
///   `RunMarkerSource.labels`.
pub fn prepare_event_payload(
    source: ArtifactVariant<'_>,
    artifact_bytes: &[u8],
    run_id: &str,
    signal_type: &str,
    now: DateTime<Utc>,
) -> ExportEvent {
    let mut labels = BTreeMap::new();
    labels.insert("run_id".into(), run_id.to_string());

    let (payload, format) = match source {
        ArtifactVariant::Receipts(_) => {
            // Receipts are JSON; the worker pre-parses them into an array.
            let parsed: serde_json::Value =
                serde_json::from_slice(artifact_bytes).unwrap_or(serde_json::Value::Array(vec![]));
            (serde_json::json!({ "receipts": parsed }), None)
        }
        ArtifactVariant::TestReport(tr) => {
            labels.insert("configmap".into(), tr.configmap.clone());
            labels.insert("key".into(), tr.key.clone());
            // Closed-set dispatch via `ReportFormat::payload_shape` — the
            // 2-arm match over `ReportPayloadShape` is exhaustive, so
            // adding a future `ReportFormat` variant lands at one
            // `payload_shape` arm in tatara-process and never touches
            // the worker. Replaces the prior `_ => base64` silent
            // default that quietly swallowed new variants.
            let p = match tr.format.payload_shape() {
                ReportPayloadShape::NdJsonLines => {
                    let lines: Vec<serde_json::Value> = artifact_bytes
                        .split(|b| *b == b'\n')
                        .filter(|l| !l.is_empty())
                        .filter_map(|l| serde_json::from_slice(l).ok())
                        .collect();
                    serde_json::json!({ "ndjson": lines })
                }
                ReportPayloadShape::OpaqueBytes => {
                    use base64_inline as base64;
                    serde_json::json!({ "raw_b64": base64::encode(artifact_bytes) })
                }
            };
            (p, Some(tr.format))
        }
        ArtifactVariant::ProcessSnapshot(_) => {
            let parsed: serde_json::Value =
                serde_json::from_slice(artifact_bytes).unwrap_or(serde_json::Value::Null);
            (serde_json::json!({ "snapshot": parsed }), None)
        }
        ArtifactVariant::RunMarker(rm) => {
            merge_labels(&mut labels, &rm.labels);
            (serde_json::Value::Null, None)
        }
    };

    ExportEvent {
        signal_type: signal_type.to_string(),
        run_id: run_id.to_string(),
        timestamp: now,
        labels,
        payload,
        format,
    }
}

fn merge_labels(into: &mut BTreeMap<String, String>, from: &BTreeMap<String, String>) {
    for (k, v) in from {
        into.insert(k.clone(), v.clone());
    }
}

/// Convenience for the worker — calls the right run marker
/// preparation when no artifact bytes exist (e.g. start/end markers
/// the worker synthesizes itself).
pub fn run_marker_event(
    rm: &RunMarkerSource,
    run_id: &str,
    signal_type: &str,
    now: DateTime<Utc>,
) -> ExportEvent {
    prepare_event_payload(
        ArtifactVariant::RunMarker(rm),
        &[],
        run_id,
        signal_type,
        now,
    )
}

// ─── Outcome + receipt composition ─────────────────────────────────

/// Final state of the export action — feeds the `control_hash`
/// pillar of the typed receipt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportOutcome {
    /// The shipment succeeded — the destination acknowledged
    /// (HTTP 2xx, NATS publish ack, stdout written).
    Shipped,
    /// The destination explicitly rejected the shipment (HTTP 4xx,
    /// NATS no-stream-match). Worker emits a receipt of the failure;
    /// the Process advances to Zombie via Releasing.
    Rejected(String),
    /// The shipment timed out / connection refused / network error.
    /// Same Zombie path; the receipt records the error type.
    Failed(String),
}

impl ExportOutcome {
    /// One short token used in the receipt's `kind` field.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Shipped => "Shipped",
            Self::Rejected(_) => "Rejected",
            Self::Failed(_) => "Failed",
        }
    }

    /// True iff the outcome is a successful shipment.
    pub fn is_shipped(&self) -> bool {
        matches!(self, Self::Shipped)
    }
}

/// Build a typed `ReceiptEnvelope` of the export action via the
/// existing `ReceiptEnvelope::build()` constructor (single source of
/// truth for the three-pillar composition + BLAKE3 root).
///
/// Three BLAKE3 pillars (each fed to `build()` as a hex string):
/// - **intent_hash**  ← canonical JSON of the `ExportSpec`
/// - **artifact_hash** ← the shipped event bytes (post-`prepare_event_payload`)
/// - **control_hash**  ← canonical JSON of the `ExportOutcome`
///
/// Returned envelope has `kind = "tatara.export"`, `process_ref`
/// stamped as `{namespace}/{name}`, and structured `evidence`
/// carrying the run id, outcome kind, and any error string. The
/// composed root + version + generated_at are set by `build()`.
///
/// tatara-reconciler's `JobAttested` evaluator reads this envelope
/// from the worker's ConfigMap and verifies the root before
/// advancing the Process out of `Releasing`.
pub fn compose_export_receipt(
    spec: &ExportSpec,
    shipped_event_bytes: &[u8],
    outcome: &ExportOutcome,
    previous_root: Option<&str>,
    run_id: &str,
    process_ref: Option<&str>,
) -> anyhow::Result<ReceiptEnvelope> {
    let intent_hash = hex_blake3(&canonical_json(spec)?);
    let artifact_hash = hex_blake3(shipped_event_bytes);
    let control_hash = hex_blake3(&canonical_json(outcome)?);

    let mut env = ReceiptEnvelope::build(
        "tatara.export",
        intent_hash,
        artifact_hash,
        control_hash,
        previous_root,
    );
    env.process_ref = process_ref.map(String::from);

    let mut evidence = serde_json::Map::new();
    evidence.insert(
        "run_id".into(),
        serde_json::Value::String(run_id.to_string()),
    );
    evidence.insert(
        "outcome".into(),
        serde_json::Value::String(outcome.kind().to_string()),
    );
    if let ExportOutcome::Rejected(m) | ExportOutcome::Failed(m) = outcome {
        evidence.insert("error".into(), serde_json::Value::String(m.clone()));
    }
    evidence.insert(
        "shipped_bytes_len".into(),
        serde_json::Value::Number(shipped_event_bytes.len().into()),
    );
    env.evidence = serde_json::Value::Object(evidence);

    Ok(env)
}

fn canonical_json<T: Serialize>(value: &T) -> anyhow::Result<Vec<u8>> {
    // Canonical = serde_json through to_value then to_vec. Stable
    // across runs of the same struct because serde_json::Value
    // preserves field-emission order from the source serializer's
    // declaration order (Rust struct field order).
    let v = serde_json::to_value(value)?;
    Ok(serde_json::to_vec(&v)?)
}

fn hex_blake3(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

// ─── Minimal inline base64 (no extra dep) ──────────────────────────

mod base64_inline {
    const ALPHA: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    pub fn encode(input: &[u8]) -> String {
        let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
        let mut chunks = input.chunks_exact(3);
        for chunk in chunks.by_ref() {
            let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
            out.push(ALPHA[((n >> 18) & 0x3F) as usize] as char);
            out.push(ALPHA[((n >> 12) & 0x3F) as usize] as char);
            out.push(ALPHA[((n >> 6) & 0x3F) as usize] as char);
            out.push(ALPHA[(n & 0x3F) as usize] as char);
        }
        let rem = chunks.remainder();
        match rem.len() {
            1 => {
                let n = (rem[0] as u32) << 16;
                out.push(ALPHA[((n >> 18) & 0x3F) as usize] as char);
                out.push(ALPHA[((n >> 12) & 0x3F) as usize] as char);
                out.push('=');
                out.push('=');
            }
            2 => {
                let n = ((rem[0] as u32) << 16) | ((rem[1] as u32) << 8);
                out.push(ALPHA[((n >> 18) & 0x3F) as usize] as char);
                out.push(ALPHA[((n >> 12) & 0x3F) as usize] as char);
                out.push(ALPHA[((n >> 6) & 0x3F) as usize] as char);
                out.push('=');
            }
            _ => {}
        }
        out
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tatara_process::export::{
        ArtifactSource, HttpEventChannel, ProcessSnapshotSource, ReceiptsSource, RunMarkerSource,
        TestReportSource, VectorChannel,
    };

    fn http_spec(signal_type: &str) -> ExportSpec {
        ExportSpec {
            source: ArtifactSource {
                run_marker: Some(RunMarkerSource::default()),
                ..ArtifactSource::default()
            },
            channel: VectorChannel {
                http_event: Some(HttpEventChannel {
                    endpoint: None,
                    signal_type: signal_type.to_string(),
                }),
                ..VectorChannel::default()
            },
            when: Default::default(),
            experiment_id_override: None,
        }
    }

    #[test]
    fn run_id_falls_back_to_ns_slash_name() {
        let s = http_spec("x");
        assert_eq!(resolve_run_id(&s, "akeyless-test", "r1"), "akeyless-test/r1");
    }

    #[test]
    fn run_id_uses_override_when_set() {
        let mut s = http_spec("x");
        s.experiment_id_override = Some("akeyless-run-2026-05-20".into());
        assert_eq!(resolve_run_id(&s, "ns", "n"), "akeyless-run-2026-05-20");
    }

    #[test]
    fn run_id_ignores_empty_override() {
        let mut s = http_spec("x");
        s.experiment_id_override = Some(String::new());
        assert_eq!(resolve_run_id(&s, "ns", "n"), "ns/n");
    }

    #[test]
    fn subject_substitutes_run_id_template() {
        let ch = NatsSubjectChannel {
            subject: "pleme.pleme-dev.ephemeral.{{run_id}}.receipt".into(),
            stream: "EPHEMERAL_RECEIPTS".into(),
            url: None,
        };
        assert_eq!(
            resolve_subject(&ch, "ns/n"),
            "pleme.pleme-dev.ephemeral.ns/n.receipt"
        );
    }

    #[test]
    fn subject_passthrough_when_no_template() {
        let ch = NatsSubjectChannel {
            subject: "pleme.fixed.subject".into(),
            stream: "S".into(),
            url: None,
        };
        assert_eq!(resolve_subject(&ch, "ignored"), "pleme.fixed.subject");
    }

    #[test]
    fn run_marker_event_has_labels_and_run_id() {
        let mut labels = BTreeMap::new();
        labels.insert("phase".into(), "end".into());
        let rm = RunMarkerSource { labels };
        let now = chrono::Utc::now();
        let ev = run_marker_event(&rm, "ns/n", "ephemeral-marker", now);
        assert_eq!(ev.signal_type, "ephemeral-marker");
        assert_eq!(ev.run_id, "ns/n");
        assert_eq!(ev.labels["phase"], "end");
        assert_eq!(ev.labels["run_id"], "ns/n");
        assert_eq!(ev.payload, serde_json::Value::Null);
    }

    #[test]
    fn test_report_ndjson_parses_into_array() {
        let tr = TestReportSource {
            configmap: "cm".into(),
            key: "out.ndjson".into(),
            format: ReportFormat::NdJson,
            namespace: None,
        };
        let bytes = b"{\"a\":1}\n{\"b\":2}\n\n{\"c\":3}\n";
        let now = chrono::Utc::now();
        let ev = prepare_event_payload(
            ArtifactVariant::TestReport(&tr),
            bytes,
            "ns/n",
            "test-report",
            now,
        );
        let arr = ev.payload["ndjson"].as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["a"], 1);
        assert_eq!(arr[2]["c"], 3);
        assert_eq!(ev.labels["configmap"], "cm");
        assert_eq!(ev.format, Some(ReportFormat::NdJson));
    }

    #[test]
    fn test_report_raw_format_base64_encodes() {
        let tr = TestReportSource {
            configmap: "cm".into(),
            key: "report.bin".into(),
            format: ReportFormat::Raw,
            namespace: None,
        };
        let bytes = b"<<binary>>";
        let now = chrono::Utc::now();
        let ev = prepare_event_payload(
            ArtifactVariant::TestReport(&tr),
            bytes,
            "ns/n",
            "test-report",
            now,
        );
        let b64 = ev.payload["raw_b64"].as_str().unwrap();
        // sanity — base64 length is ceil(N/3)*4
        assert_eq!(b64.len(), ((bytes.len() + 2) / 3) * 4);
        assert_eq!(ev.format, Some(ReportFormat::Raw));
    }

    #[test]
    fn receipts_source_embeds_parsed_json() {
        let r = ReceiptsSource::default();
        let raw = serde_json::to_vec(&serde_json::json!([
            { "kind": "tatara.processed.run", "composed_root": "abc" },
            { "kind": "tatara.processed.run", "composed_root": "def" },
        ]))
        .unwrap();
        let now = chrono::Utc::now();
        let ev = prepare_event_payload(
            ArtifactVariant::Receipts(&r),
            &raw,
            "ns/n",
            "receipt",
            now,
        );
        let arr = ev.payload["receipts"].as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[1]["composed_root"], "def");
    }

    #[test]
    fn process_snapshot_embeds_parsed_json() {
        let p = ProcessSnapshotSource::default();
        let raw = serde_json::to_vec(&serde_json::json!({ "phase": "Attested" })).unwrap();
        let now = chrono::Utc::now();
        let ev = prepare_event_payload(
            ArtifactVariant::ProcessSnapshot(&p),
            &raw,
            "ns/n",
            "process-snapshot",
            now,
        );
        assert_eq!(ev.payload["snapshot"]["phase"], "Attested");
    }

    // ─── Receipt composition ───────────────────────────────────────

    #[test]
    fn outcome_kind_is_stable() {
        assert_eq!(ExportOutcome::Shipped.kind(), "Shipped");
        assert_eq!(ExportOutcome::Rejected("x".into()).kind(), "Rejected");
        assert_eq!(ExportOutcome::Failed("y".into()).kind(), "Failed");
    }

    #[test]
    fn export_receipt_chains_three_pillars() {
        use tatara_process::receipt::RECEIPT_VERSION;
        let s = http_spec("test-report");
        let event_bytes = b"{\"signalType\":\"test-report\"}";
        let r = compose_export_receipt(
            &s,
            event_bytes,
            &ExportOutcome::Shipped,
            None,
            "ns/n",
            Some("akeyless-test/r1"),
        )
        .expect("receipt");
        assert_eq!(r.version, RECEIPT_VERSION);
        assert_eq!(r.kind, "tatara.export");
        // Each pillar is a 64-char BLAKE3 hex digest.
        assert_eq!(r.intent_hash.len(), 64);
        assert_eq!(r.artifact_hash.len(), 64);
        assert_eq!(r.control_hash.len(), 64);
        assert_eq!(r.composed_root.len(), 64);
        // Process ref + evidence stamped through.
        assert_eq!(r.process_ref.as_deref(), Some("akeyless-test/r1"));
        assert_eq!(r.evidence["run_id"], "ns/n");
        assert_eq!(r.evidence["outcome"], "Shipped");
        // verify_root() agrees the composed_root was built correctly
        // — same guarantee tatara-reconciler's evaluator checks.
        assert!(r.verify_root(None));
    }

    #[test]
    fn export_receipt_chains_prev_root() {
        let s = http_spec("test-report");
        let ev = b"x";
        let r1 =
            compose_export_receipt(&s, ev, &ExportOutcome::Shipped, None, "r", None).unwrap();
        let r2 = compose_export_receipt(
            &s,
            ev,
            &ExportOutcome::Shipped,
            Some(&r1.composed_root),
            "r",
            None,
        )
        .unwrap();
        // Same inputs but chained prev_root → different composed_root.
        assert_ne!(r1.composed_root, r2.composed_root);
        // verify_root checks the chain.
        assert!(r2.verify_root(Some(&r1.composed_root)));
    }

    #[test]
    fn export_receipt_failure_carries_error_text() {
        let s = http_spec("x");
        let r = compose_export_receipt(
            &s,
            b"",
            &ExportOutcome::Failed("connection refused".into()),
            None,
            "ns/n",
            None,
        )
        .unwrap();
        assert_eq!(r.evidence["error"], "connection refused");
        assert_eq!(r.evidence["outcome"], "Failed");
    }

    #[test]
    fn export_receipt_intent_hash_changes_with_spec() {
        let s1 = http_spec("a");
        let s2 = http_spec("b"); // different signal_type → different intent
        let r1 =
            compose_export_receipt(&s1, b"", &ExportOutcome::Shipped, None, "ns/n", None).unwrap();
        let r2 =
            compose_export_receipt(&s2, b"", &ExportOutcome::Shipped, None, "ns/n", None).unwrap();
        assert_ne!(r1.intent_hash, r2.intent_hash);
    }

    #[test]
    fn export_receipt_artifact_hash_changes_with_payload() {
        let s = http_spec("x");
        let r1 =
            compose_export_receipt(&s, b"payload-1", &ExportOutcome::Shipped, None, "ns/n", None)
                .unwrap();
        let r2 =
            compose_export_receipt(&s, b"payload-2", &ExportOutcome::Shipped, None, "ns/n", None)
                .unwrap();
        assert_ne!(r1.artifact_hash, r2.artifact_hash);
    }
}
