//! `closed-loop-probe` — verifies that a system's bundled identity issuer
//! authenticates its own bundled consumer, then emits a tatara-receipt/v1
//! envelope to a ConfigMap.
//!
//! Consumed by the closed-loop-probe Helm chart and any
//! future closed-loop-testable consumer (databases, identity providers,
//! message brokers — anything where the under-test instance can issue
//! credentials its own under-test client must accept).
//!
//! NO SHELL — every K8s interaction goes through `kube-rs`; every HTTP
//! call through `reqwest`. Three pillars composed by `tatara_process::
//! receipt::ReceiptEnvelope::build`.

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::{Api, Client};
use serde_json::json;
use std::collections::BTreeMap;
use tatara_process::receipt::{ReceiptEnvelope, ReceiptKind, RECEIPT_JSON_KEY, RECEIPT_YAML_KEY};
use tracing::{info, warn};

mod probe;

#[derive(Parser, Debug)]
#[command(name = "closed-loop-probe")]
#[command(about = "Closed-loop authentication probe — emits a tatara-receipt/v1 envelope")]
struct Args {
    /// Issuer Service name (in-namespace). The probe fetches a token here.
    #[arg(long, env = "ISSUER_SERVICE")]
    issuer_service: String,

    /// Issuer Service port.
    #[arg(long, env = "ISSUER_PORT", default_value_t = 8080)]
    issuer_port: u16,

    /// Path on the issuer that accepts ACCESS_ID / ACCESS_KEY and returns a token.
    #[arg(long, env = "ISSUER_AUTH_PATH", default_value = "/v2/auth")]
    issuer_auth_path: String,

    /// Issuer's JWKS endpoint — the probe fetches this to compute the
    /// `intent_hash` pillar.
    #[arg(
        long,
        env = "ISSUER_JWKS_PATH",
        default_value = "/.well-known/jwks.json"
    )]
    issuer_jwks_path: String,

    /// Consumer Service name (in-namespace).
    #[arg(long, env = "CONSUMER_SERVICE")]
    consumer_service: String,

    /// Consumer Service port.
    #[arg(long, env = "CONSUMER_PORT", default_value_t = 8000)]
    consumer_port: u16,

    /// Path on the consumer that accepts the issuer-issued token and
    /// returns a typed auth verdict.
    #[arg(long, env = "CONSUMER_AUTH_PATH", default_value = "/v2/whoami")]
    consumer_auth_path: String,

    /// Receipt ConfigMap name (in this namespace). Created if missing.
    #[arg(long, env = "RECEIPT_CONFIG_MAP")]
    receipt_config_map: String,

    /// Receipt ConfigMap namespace.
    #[arg(long, env = "RECEIPT_NAMESPACE", default_value = "default")]
    receipt_namespace: String,

    /// `kind` field on the emitted receipt. Defaults to the typed
    /// [`ReceiptKind::ClosedLoopAuth`] canonical wire string so the
    /// probe binary, the receipt envelope, and the reconciler verifier
    /// all bind to the same `ReceiptKind` projection — a rename of
    /// the canonical kebab-case kind lands at one [`ReceiptKind::as_str`]
    /// arm and propagates here through `default_value_t`.
    #[arg(long, env = "RECEIPT_KIND", default_value_t = String::from(ReceiptKind::ClosedLoopAuth))]
    receipt_kind: String,

    /// Optional Process reference (`<ns>/<name>`) stamped on the receipt
    /// so the reconciler can correlate.
    #[arg(long, env = "TATARA_PROCESS_REF")]
    process_ref: Option<String>,

    /// Probe HTTP timeout (per request).
    #[arg(long, default_value = "10s")]
    timeout: humantime::Duration,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let access_id =
        std::env::var("ACCESS_ID").context("ACCESS_ID env var (from auth Secret) required")?;
    let access_key =
        std::env::var("ACCESS_KEY").context("ACCESS_KEY env var (from auth Secret) required")?;

    info!(
        issuer = %args.issuer_service,
        consumer = %args.consumer_service,
        receipt_cm = %args.receipt_config_map,
        "starting closed-loop probe"
    );

    let probe_result = probe::run(probe::ProbeConfig {
        issuer: probe::ServiceEndpoint {
            service: args.issuer_service,
            port: args.issuer_port,
        },
        issuer_auth_path: args.issuer_auth_path,
        issuer_jwks_path: args.issuer_jwks_path,
        consumer: probe::ServiceEndpoint {
            service: args.consumer_service,
            port: args.consumer_port,
        },
        consumer_auth_path: args.consumer_auth_path,
        access_id,
        access_key,
        http_timeout: args.timeout.into(),
    })
    .await?;

    let mut envelope = ReceiptEnvelope::build(
        &args.receipt_kind,
        &probe_result.intent_hash,
        &probe_result.artifact_hash,
        &probe_result.control_hash,
        None,
    );
    envelope.process_ref = args.process_ref.clone();
    envelope.evidence = json!({
        "issuer_url": probe_result.issuer_url,
        "consumer_url": probe_result.consumer_url,
        "token_present": probe_result.token_present,
        "jwks_keys": probe_result.jwks_key_count,
        "whoami_status": probe_result.whoami_status,
    });

    info!(
        composed_root = %envelope.composed_root,
        kind = %envelope.kind,
        "writing receipt to ConfigMap"
    );
    write_receipt(&envelope, &args.receipt_config_map, &args.receipt_namespace).await?;
    info!("closed-loop probe succeeded");
    Ok(())
}

/// PATCH the receipt into the ConfigMap. Creates the CM if absent
/// (the chart's RBAC grants create on this name + get/patch/update).
async fn write_receipt(envelope: &ReceiptEnvelope, cm_name: &str, ns: &str) -> Result<()> {
    let client = Client::try_default()
        .await
        .context("create in-cluster kube client")?;
    let api: Api<ConfigMap> = Api::namespaced(client, ns);
    let payload = serde_json::to_string(envelope)?;

    // Receipt-CM `data` key spellings ride through the substrate
    // primitives `tatara_process::receipt::{RECEIPT_JSON_KEY,
    // RECEIPT_YAML_KEY}` — pre-lift the two `&'static str` literals
    // were hand-authored inline here AND at the reader-side lookup
    // gate in `tatara-reconciler::boundary::verify_receipt_cm`, one of
    // FOUR workspace-wide restatements past the ★★ PRIME-DIRECTIVE ≥ 2
    // duplication threshold with NO shared owner binding the two
    // keys' spelling. Post-lift the two keys live at ONE substrate-
    // owned pair of constants and every writer insert AND every
    // reader lookup routes through ONE substrate owner — a rename at
    // ONE writer key or ONE reader key can no longer silently
    // desynchronize the twin (a probe writing `"receipt.jsonl"` while
    // the reader still gates on `"receipt.json"`), because both sides
    // now read the same const.
    let mut data = BTreeMap::new();
    data.insert(RECEIPT_JSON_KEY.to_string(), payload.clone());
    // YAML twin so operators can `kubectl get cm -o yaml` and read the receipt
    // without re-parsing the embedded JSON.
    data.insert(
        RECEIPT_YAML_KEY.to_string(),
        serde_yaml::to_string(envelope)?,
    );

    // Try create-or-patch — idempotent across re-runs.
    let cm = ConfigMap {
        metadata: kube::core::ObjectMeta {
            name: Some(cm_name.into()),
            namespace: Some(ns.into()),
            labels: Some(BTreeMap::from([(
                "tatara.pleme.io/receipt".into(),
                "tatara-receipt/v1".into(),
            )])),
            ..Default::default()
        },
        data: Some(data),
        ..Default::default()
    };

    // Create-verb dispatch rides the substrate primitive
    // `tatara_process::create::default` — pre-lift this was a hand-
    // authored `api.create(&PostParams::default(), &cm)` chain, one of
    // FIVE workspace-wide restatements past the ★★ PRIME-DIRECTIVE ≥ 2
    // duplication threshold. Post-lift the create-verb family lives at
    // ONE substrate owner and the compound "create-then-409-retry"
    // idiom composes THREE substrate primitives (`create::default` +
    // `kube_error::is_conflict` + `patch::merge`) at the callsite.
    match tatara_process::create::default(&api, &cm).await {
        Ok(_) => Ok(()),
        // 409 detection rides the substrate primitive
        // `tatara_process::kube_error::is_conflict` — pre-lift this
        // was a hand-authored `Err(kube::Error::Api(e)) if e.code ==
        // 409` match-arm guard, one of FIVE workspace-wide restatements
        // past the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold. Post-
        // lift the two-link matches-shape lives at ONE substrate owner
        // (peer of `is_not_found` for HTTP 404 on the same axis).
        Err(ref e) if tatara_process::kube_error::is_conflict(e) => {
            // Already exists — PATCH the data field.
            let patch = json!({ "data": cm.data });
            // Wire-side dispatch rides the substrate primitive
            // `tatara_process::patch::merge` — pre-lift this was a
            // hand-authored `api.patch(cm_name, &PatchParams::
            // default(), &Patch::Merge(&patch))` chain, one of SIX
            // workspace-wide restatements past the ★★ PRIME-DIRECTIVE
            // ≥ 2 duplication threshold. Post-lift the primary-
            // resource merge posture lives at ONE substrate owner
            // (peers of `merge_status` on the `/status` subresource
            // axis + `apply_patch_params` on the SSA wire-posture
            // axis, both already opened on the substrate side).
            tatara_process::patch::merge(&api, cm_name, &patch)
                .await
                .map_err(|e| anyhow!("patch ConfigMap {ns}/{cm_name}: {e}"))?;
            Ok(())
        }
        Err(e) => {
            warn!(error = %e, "create ConfigMap failed");
            Err(anyhow!("create ConfigMap {ns}/{cm_name}: {e}"))
        }
    }
}

// Composition contract (see `tatara_process::receipt`):
//
//   intent_hash   = BLAKE3(canonical(JWKS body))
//   artifact_hash = BLAKE3(token blob the consumer received)
//   control_hash  = BLAKE3(whoami response body || verdict)
//   composed_root = BLAKE3(
//       "tatara-process/v1alpha1\n"
//       ++ artifact_hash ++ "\n"
//       ++ control_hash  ++ "\n"
//       ++ intent_hash   ++ "\n"
//       ++ "")
//
// Used unchanged by `ProcessAttestation::compose` so the reconciler
// verifying the receipt + chaining into the Process attestation gets
// byte-exact equality between the probe-computed root and the
// reconciler-recomputed root.

#[cfg(test)]
mod tests {
    use super::Args;
    use clap::Parser;
    use tatara_process::receipt::ReceiptKind;

    #[test]
    fn args_parse_with_required_flags() {
        let args = Args::try_parse_from([
            "closed-loop-probe",
            "--issuer-service",
            "issuer",
            "--consumer-service",
            "gateway",
            "--receipt-config-map",
            "my-receipt",
        ]);
        assert!(args.is_ok(), "{:?}", args.err());
        let a = args.unwrap();
        assert_eq!(a.issuer_port, 8080);
        // Default kind binds through the typed projection — a rename of
        // the canonical kebab-case literal lands at ONE `as_str` arm,
        // not at this CLI default + every consumer assertion.
        assert_eq!(a.receipt_kind, ReceiptKind::ClosedLoopAuth.as_str());
    }
}
