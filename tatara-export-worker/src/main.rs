//! `tatara-export-worker` — runs one declared `ExportSpec` from an
//! ephemeral Process to completion.
//!
//! Inputs (CLI + env):
//!   * `--spec-json <path>` OR `--spec '<json>'` — the [`ExportSpec`]
//!     to run, serialized as JSON. The reconciler mounts this via a
//!     ConfigMap or stamps it inline as a Job arg.
//!   * `--process-namespace` / `--process-name` — owning Process,
//!     for run-id resolution + receipt `process_ref` stamping.
//!   * `--previous-root <hex>` — chain into the Process attestation
//!     tree. Optional; new chains start with no previous root.
//!   * `--receipt-configmap <name>` — where the worker writes its
//!     typed [`ReceiptEnvelope`]. Required.
//!   * `--receipt-key <name>` — key inside the ConfigMap (default
//!     `receipt.yaml`). The reconciler's `JobAttested` evaluator
//!     reads from `<receipt-configmap>.data.<receipt-key>`.
//!
//! The binary is intentionally thin — the pure decision logic lives
//! in the library (`lib.rs`) and is tested without infrastructure.
//! Main is argv parsing + kube/HTTP/NATS plumbing.

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use k8s_openapi::api::core::v1::ConfigMap;
use k8s_openapi::ByteString;
use kube::api::{Patch, PatchParams};
use kube::{Api, Client};
use std::collections::BTreeMap;
use std::path::PathBuf;
use tracing::{info, warn};

use tatara_export_worker::{
    compose_export_receipt, prepare_event_payload, resolve_run_id, resolve_subject, ExportEvent,
    ExportOutcome,
};
use tatara_process::export::{ArtifactVariant, ChannelVariant, ExportSpec};

#[derive(Parser, Debug)]
#[command(
    name = "tatara-export-worker",
    about = "Run one declared ExportSpec from an ephemeral Process"
)]
struct Cli {
    /// Path to a JSON file containing the ExportSpec.
    #[arg(long, conflicts_with = "spec")]
    spec_json: Option<PathBuf>,

    /// Inline JSON ExportSpec.
    #[arg(long, conflicts_with = "spec_json")]
    spec: Option<String>,

    /// Owning Process namespace (kube downward API).
    #[arg(long, env = "TATARA_PROCESS_NAMESPACE")]
    process_namespace: String,

    /// Owning Process name.
    #[arg(long, env = "TATARA_PROCESS_NAME")]
    process_name: String,

    /// Optional previous BLAKE3 root to chain this receipt into.
    #[arg(long, env = "TATARA_PREVIOUS_ROOT")]
    previous_root: Option<String>,

    /// ConfigMap to write the receipt envelope to.
    #[arg(long, env = "TATARA_RECEIPT_CONFIGMAP")]
    receipt_configmap: String,

    /// Key inside the ConfigMap for the receipt YAML payload.
    #[arg(long, env = "TATARA_RECEIPT_KEY", default_value = "receipt.yaml")]
    receipt_key: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let spec_json = match (&cli.spec_json, &cli.spec) {
        (Some(p), None) => std::fs::read_to_string(p).context("read --spec-json")?,
        (None, Some(s)) => s.clone(),
        _ => return Err(anyhow!("exactly one of --spec-json or --spec is required")),
    };
    let spec: ExportSpec = serde_json::from_str(&spec_json).context("parse ExportSpec JSON")?;

    let run_id = resolve_run_id(&spec, &cli.process_namespace, &cli.process_name);
    let process_ref = format!("{}/{}", &cli.process_namespace, &cli.process_name);
    info!(
        run_id = %run_id,
        process_ref = %process_ref,
        "tatara-export-worker starting"
    );

    let kube = Client::try_default().await.context("kube client")?;

    // 1. Read the artifact bytes the source points to.
    let artifact_bytes = read_artifact(&spec, &kube, &cli.process_namespace, &cli.process_name)
        .await
        .context("read artifact")?;

    // 2. Build the event payload via the pure lib function.
    let signal_type = extract_signal_type(&spec);
    let event = prepare_event_payload(
        spec.source.variant().map_err(|e| anyhow!("source: {e}"))?,
        &artifact_bytes,
        &run_id,
        &signal_type,
        chrono::Utc::now(),
    );
    let event_bytes = serde_json::to_vec(&event).context("serialize event")?;

    // 3. Ship through the chosen channel.
    let outcome = ship(&spec, &event, &event_bytes, &run_id).await;
    match &outcome {
        ExportOutcome::Shipped => info!(bytes = event_bytes.len(), "shipped"),
        ExportOutcome::Rejected(m) => warn!(reason = %m, "rejected"),
        ExportOutcome::Failed(m) => warn!(reason = %m, "failed"),
    }

    // 4. Compose typed receipt + persist to the ConfigMap.
    let receipt = compose_export_receipt(
        &spec,
        &event_bytes,
        &outcome,
        cli.previous_root.as_deref(),
        &run_id,
        Some(&process_ref),
    )
    .context("compose receipt")?;
    let receipt_yaml = serde_yaml::to_string(&receipt).context("serialize receipt")?;

    write_receipt(
        &kube,
        &cli.process_namespace,
        &cli.receipt_configmap,
        &cli.receipt_key,
        &receipt_yaml,
    )
    .await
    .context("write receipt ConfigMap")?;

    info!(
        composed_root = %receipt.composed_root,
        configmap = %cli.receipt_configmap,
        "receipt persisted"
    );

    // 5. Exit non-zero on terminal failure so the Job phase reflects
    //    it; tatara-reconciler routes Failed Jobs to Releasing →
    //    Zombie. The receipt is already persisted either way.
    if !outcome.is_shipped() {
        std::process::exit(1);
    }
    Ok(())
}

/// Pull the signal_type tag from whichever channel variant is set.
/// Used both for `ExportEvent.signal_type` and for HTTP request
/// headers.
fn extract_signal_type(spec: &ExportSpec) -> String {
    if let Some(h) = &spec.channel.http_event {
        return h.signal_type.clone();
    }
    if let Some(n) = &spec.channel.nats_subject {
        // NATS doesn't carry a separate signal_type; the subject's
        // last segment is the convention.
        return n
            .subject
            .rsplit('.')
            .next()
            .unwrap_or("event")
            .to_string();
    }
    "event".into()
}

// ─── Artifact readers ──────────────────────────────────────────────

async fn read_artifact(
    spec: &ExportSpec,
    kube: &Client,
    ns: &str,
    name: &str,
) -> Result<Vec<u8>> {
    let v = spec.source.variant().map_err(|e| anyhow!("source: {e}"))?;
    match v {
        ArtifactVariant::RunMarker(_) => Ok(Vec::new()),
        ArtifactVariant::TestReport(tr) => {
            let cm_ns = tr.namespace.as_deref().unwrap_or(ns);
            let api: Api<ConfigMap> = Api::namespaced(kube.clone(), cm_ns);
            let cm = api
                .get(&tr.configmap)
                .await
                .with_context(|| format!("get configmap {cm_ns}/{}", tr.configmap))?;
            if let Some(s) = cm.data.as_ref().and_then(|d| d.get(&tr.key)) {
                return Ok(s.as_bytes().to_vec());
            }
            if let Some(b) = cm.binary_data.as_ref().and_then(|d| d.get(&tr.key)) {
                return Ok(b.0.clone());
            }
            Err(anyhow!(
                "ConfigMap {cm_ns}/{} has no key {:?}",
                tr.configmap,
                tr.key
            ))
        }
        ArtifactVariant::ProcessSnapshot(_) => {
            // Read the owning Process as a typed CR; serialize to JSON.
            use tatara_process::crd::Process;
            let api: Api<Process> = Api::namespaced(kube.clone(), ns);
            let p = api
                .get(name)
                .await
                .with_context(|| format!("get process {ns}/{name}"))?;
            Ok(serde_json::to_vec(&p).context("serialize process")?)
        }
        ArtifactVariant::Receipts(_) => {
            // Receipts collection — list ConfigMaps in the Process's
            // namespace carrying our process annotation, parse each
            // as a YAML ReceiptEnvelope, return the JSON array.
            let api: Api<ConfigMap> = Api::namespaced(kube.clone(), ns);
            let cms = api
                .list(&Default::default())
                .await
                .context("list configmaps")?;
            let want = format!("{ns}/{name}");
            let mut envelopes = Vec::new();
            for cm in cms.items {
                let ann = cm.metadata.annotations.as_ref();
                let owned = ann
                    .and_then(|m| m.get("tatara.pleme.io/process"))
                    .map(|p| p == &want)
                    .unwrap_or(false);
                if !owned {
                    continue;
                }
                if let Some(d) = &cm.data {
                    for (_k, v) in d {
                        if let Ok(env) =
                            tatara_process::receipt::ReceiptEnvelope::parse_either(v)
                        {
                            envelopes.push(env);
                        }
                    }
                }
            }
            Ok(serde_json::to_vec(&envelopes)?)
        }
    }
}

// ─── Channel shippers ──────────────────────────────────────────────

async fn ship(
    spec: &ExportSpec,
    event: &ExportEvent,
    event_bytes: &[u8],
    run_id: &str,
) -> ExportOutcome {
    let variant = match spec.channel.variant() {
        Ok(v) => v,
        Err(e) => return ExportOutcome::Failed(format!("channel: {e}")),
    };
    match variant {
        ChannelVariant::HttpEvent(h) => ship_http(h, event_bytes).await,
        ChannelVariant::NatsSubject(n) => ship_nats(n, event_bytes, run_id).await,
        ChannelVariant::Stdout(s) => ship_stdout(s, event),
    }
}

async fn ship_http(
    channel: &tatara_process::export::HttpEventChannel,
    event_bytes: &[u8],
) -> ExportOutcome {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => return ExportOutcome::Failed(format!("build client: {e}")),
    };
    let resp = client
        .post(channel.resolved_endpoint())
        .header("Content-Type", "application/json")
        .header("X-Tatara-Signal-Type", channel.signal_type.clone())
        .body(event_bytes.to_vec())
        .send()
        .await;
    match resp {
        Ok(r) if r.status().is_success() => ExportOutcome::Shipped,
        Ok(r) => ExportOutcome::Rejected(format!("HTTP {}", r.status())),
        Err(e) => ExportOutcome::Failed(format!("HTTP error: {e}")),
    }
}

async fn ship_nats(
    channel: &tatara_process::export::NatsSubjectChannel,
    event_bytes: &[u8],
    run_id: &str,
) -> ExportOutcome {
    let url = channel.resolved_url();
    let subject = resolve_subject(channel, run_id);
    let client = match async_nats::connect(url).await {
        Ok(c) => c,
        Err(e) => return ExportOutcome::Failed(format!("NATS connect: {e}")),
    };
    let js = async_nats::jetstream::new(client);
    let ack = js.publish(subject.clone(), event_bytes.to_vec().into()).await;
    match ack {
        Ok(fut) => match fut.await {
            Ok(_) => ExportOutcome::Shipped,
            Err(e) => ExportOutcome::Failed(format!("NATS ack: {e}")),
        },
        Err(e) => ExportOutcome::Rejected(format!("NATS publish: {e}")),
    }
}

fn ship_stdout(
    channel: &tatara_process::export::StdoutChannel,
    event: &ExportEvent,
) -> ExportOutcome {
    let result = if channel.pretty {
        serde_json::to_string_pretty(event)
    } else {
        serde_json::to_string(event)
    };
    match result {
        Ok(s) => {
            println!("{s}");
            ExportOutcome::Shipped
        }
        Err(e) => ExportOutcome::Failed(format!("serialize: {e}")),
    }
}

// ─── Receipt writer ────────────────────────────────────────────────

async fn write_receipt(
    kube: &Client,
    namespace: &str,
    configmap: &str,
    key: &str,
    payload: &str,
) -> Result<()> {
    let api: Api<ConfigMap> = Api::namespaced(kube.clone(), namespace);
    let mut data = BTreeMap::new();
    data.insert(key.to_string(), payload.to_string());
    let cm = ConfigMap {
        metadata: kube::api::ObjectMeta {
            name: Some(configmap.to_string()),
            namespace: Some(namespace.to_string()),
            ..Default::default()
        },
        data: Some(data),
        binary_data: Option::<BTreeMap<String, ByteString>>::None,
        ..Default::default()
    };
    let pp = PatchParams::apply("tatara-export-worker").force();
    api.patch(configmap, &pp, &Patch::Apply(&cm))
        .await
        .map(|_| ())
        .with_context(|| format!("apply configmap {namespace}/{configmap}"))?;
    Ok(())
}
