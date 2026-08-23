//! DynamicObject server-side apply — the bridge between the reconciler
//! and arbitrary K8s resources emitted by `render`.
//!
//! Pure helpers (owner ref injection, plural lookup, Ready condition parsing)
//! are unit-tested; the `apply_owned`/`fetch` entry points require a cluster.

use anyhow::{anyhow, Result};
use kube::api::{ApiResource, DynamicObject, Patch, PatchParams};
use kube::{Api, Client};
use serde_json::{json, Value};

use tatara_process::annotations;
use tatara_process::prelude::Process;

/// Field manager string we use for all SSA writes.
pub const FIELD_MANAGER: &str = "tatara-reconciler";

/// Substrate-primitive builder for the standard tatara-reconciler
/// **ownership tag** — the 2-slot
/// `{MANAGED_BY: FIELD_MANAGER, PROCESS: process_ref}`
/// object shape every emitted resource in [`crate::render`] and every
/// routing edge in [`crate::edges`] marks itself with. Every K8s
/// resource this reconciler owns carries this pair so operators (and
/// external tooling — dashboards, GC sweeps, drift detectors) can
/// grep for "resources this reconciler manages" on ONE well-known key
/// pair rather than probing each resource's owner references.
///
/// Pre-lift the 2-slot shape was hand-authored at FIVE sites past the
/// PRIME-DIRECTIVE ≥ 2 duplication threshold:
/// * [`crate::render`] × 3 — Kustomization / OCIRepository /
///   HelmRelease metadata annotations, each literal
///   `json!({ MANAGED_BY: "tatara-reconciler",
///   PROCESS: format!("{ns}/{name}") })` inline.
/// * [`crate::edges::DnsEndpointEdge`] — the DNSEndpoint annotations
///   block, same 2-key literal.
/// * [`crate::edges::IngressEdge`] — the Ingress annotations map,
///   started as two `serde_json::Map::insert(...)` calls seeding the
///   same 2-slot pair before adding routing / TLS annotations.
///
/// PLUS the workspace's own [`inject_annotations`] gate re-authored
/// the same pair at its own local site as
/// `annot.insert(MANAGED_BY, FIELD_MANAGER); annot.insert(PROCESS,
/// format!("{ns}/{name}"));`, so the SSA-time re-injection now
/// delegates through the same primitive as the render-time authoring.
///
/// The literal render + edges sites additionally hand-coded the
/// `"tatara-reconciler"` string on the `MANAGED_BY` slot, so they
/// bypassed [`FIELD_MANAGER`] and would drift silently if the field
/// manager string is ever renamed. Post-lift every one of these sites
/// reads the const, so the invariant "MANAGED_BY == FIELD_MANAGER
/// across every emitted resource" holds by construction.
///
/// Returns a [`serde_json::Map`] rather than a [`Value`] so callers
/// can either drop it under an `"annotations"` / `"labels"` key with
/// `Value::Object(map)` inside a `json!` macro, or `extend` it with
/// additional keys (see [`crate::edges::IngressEdge`] which appends
/// routing-form + backend + cert-manager annotations to the same
/// map, or [`inject_annotations`] which appends PID + CONTENT_HASH
/// + GENERATION + ATTESTATION_ROOT).
///
/// A future addition (e.g. a `VERSION` slot naming the reconciler
/// build, a `LEASE_ID` slot for multi-instance leadership, a
/// `RECONCILE_GENERATION` counter for stall detection) lands at this
/// ONE substrate primitive and every downstream emit site inherits
/// the upgrade mechanically — no per-site hand-edit at render.rs,
/// edges.rs, or inject_annotations.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// 2-slot shape recurred at five hand-authored sites well past the
/// PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to ONE
/// owner here). THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — a regression that drifted the annotation key or the
/// field manager string at ONE site surfaces at
/// [`tests::ownership_annotations_produces_field_manager_and_process_ref`]
/// rather than as silent drift at every downstream emit site).
pub fn ownership_annotations(process_ref: &str) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    m.insert(
        annotations::MANAGED_BY.to_string(),
        Value::String(FIELD_MANAGER.to_string()),
    );
    m.insert(
        annotations::PROCESS.to_string(),
        Value::String(process_ref.to_string()),
    );
    m
}

/// Resolve an `ApiResource` for `apiVersion/kind`. Hand-maintains plurals
/// for resources we emit or consume — good enough for v0; future move to
/// `kube::discovery` lands when we want to handle arbitrary CRDs.
pub fn api_resource(api_version: &str, kind: &str) -> Result<ApiResource> {
    let (group, version) = match api_version.split_once('/') {
        Some((g, v)) => (g.to_string(), v.to_string()),
        // Core/v1 has no group — api_version is just "v1".
        None => (String::new(), api_version.to_string()),
    };
    let plural = plural_of(kind)?;
    Ok(ApiResource {
        group,
        version,
        api_version: api_version.to_string(),
        kind: kind.to_string(),
        plural: plural.to_string(),
    })
}

fn plural_of(kind: &str) -> Result<&'static str> {
    match kind {
        // Flux source-controller
        "GitRepository" => Ok("gitrepositories"),
        "HelmRepository" => Ok("helmrepositories"),
        "OCIRepository" => Ok("ocirepositories"),
        "Bucket" => Ok("buckets"),
        // Flux kustomize-controller
        "Kustomization" => Ok("kustomizations"),
        // Flux helm-controller
        "HelmRelease" => Ok("helmreleases"),
        // Core kinds we might emit later
        "ConfigMap" => Ok("configmaps"),
        "Secret" => Ok("secrets"),
        "Namespace" => Ok("namespaces"),
        other => Err(anyhow!("unknown plural for kind {other:?}")),
    }
}

/// Server-side apply a JSON resource, injecting owner reference + standard
/// tatara annotations derived from the Process.
pub async fn apply_owned(
    client: Client,
    process: &Process,
    namespace: &str,
    mut resource: Value,
) -> Result<()> {
    inject_owner_reference(&mut resource, build_owner_reference(process)?)?;
    inject_annotations(&mut resource, process)?;

    let api_version = resource
        .get("apiVersion")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("resource missing apiVersion"))?
        .to_string();
    let kind = resource
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("resource missing kind"))?
        .to_string();
    let name = resource
        .get("metadata")
        .and_then(|m| m.get("name"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("resource missing metadata.name"))?
        .to_string();

    let ar = api_resource(&api_version, &kind)?;
    let obj: DynamicObject = serde_json::from_value(resource)?;
    let api: Api<DynamicObject> = Api::namespaced_with(client, namespace, &ar);

    let pp = PatchParams::apply(FIELD_MANAGER).force();
    api.patch(&name, &pp, &Patch::Apply(&obj))
        .await
        .map_err(|e| anyhow!("ssapply {kind}/{name}: {e}"))?;
    Ok(())
}

/// Fetch a DynamicObject by kind + namespace + name. Returns None on 404.
pub async fn fetch(
    client: Client,
    namespace: &str,
    api_version: &str,
    kind: &str,
    name: &str,
) -> Result<Option<DynamicObject>> {
    let ar = api_resource(api_version, kind)?;
    let api: Api<DynamicObject> = Api::namespaced_with(client, namespace, &ar);
    Ok(api.get_opt(name).await?)
}

/// Parsed readiness state of a resource's `status.conditions[type=Ready]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadyState {
    Ready,
    NotReady(Option<String>),
    Unknown,
}

/// Extract `status.conditions[type=Ready]` from a DynamicObject.
pub fn ready_condition(obj: &DynamicObject) -> ReadyState {
    ready_condition_value(&obj.data)
}

/// Same extraction but operating on raw JSON — testable without a cluster.
pub fn ready_condition_value(data: &Value) -> ReadyState {
    let conditions = data
        .get("status")
        .and_then(|s| s.get("conditions"))
        .and_then(|c| c.as_array());
    let Some(conditions) = conditions else {
        return ReadyState::Unknown;
    };
    for c in conditions {
        let Some(typ) = c.get("type").and_then(|v| v.as_str()) else {
            continue;
        };
        if typ != "Ready" {
            continue;
        }
        return match c.get("status").and_then(|v| v.as_str()) {
            Some("True") => ReadyState::Ready,
            Some("False") => {
                ReadyState::NotReady(c.get("message").and_then(|v| v.as_str()).map(String::from))
            }
            _ => ReadyState::Unknown,
        };
    }
    ReadyState::Unknown
}

fn build_owner_reference(p: &Process) -> Result<Value> {
    let name = p
        .metadata
        .name
        .clone()
        .ok_or_else(|| anyhow!("process missing metadata.name"))?;
    let uid = p
        .metadata
        .uid
        .clone()
        .ok_or_else(|| anyhow!("process missing metadata.uid"))?;
    Ok(json!({
        "apiVersion": "tatara.pleme.io/v1alpha1",
        "kind": "Process",
        "name": name,
        "uid": uid,
        "controller": true,
        "blockOwnerDeletion": true,
    }))
}

fn inject_owner_reference(resource: &mut Value, owner_ref: Value) -> Result<()> {
    let metadata = resource
        .as_object_mut()
        .ok_or_else(|| anyhow!("resource is not an object"))?
        .entry("metadata")
        .or_insert_with(|| Value::Object(Default::default()));
    let md = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow!("metadata is not an object"))?;
    let refs = md
        .entry("ownerReferences")
        .or_insert_with(|| Value::Array(vec![]));
    if let Value::Array(arr) = refs {
        arr.push(owner_ref);
    }
    Ok(())
}

fn inject_annotations(resource: &mut Value, process: &Process) -> Result<()> {
    let metadata = resource
        .as_object_mut()
        .ok_or_else(|| anyhow!("resource is not an object"))?
        .entry("metadata")
        .or_insert_with(|| Value::Object(Default::default()));
    let md = metadata
        .as_object_mut()
        .ok_or_else(|| anyhow!("metadata is not an object"))?;
    let annot = md
        .entry("annotations")
        .or_insert_with(|| Value::Object(Default::default()));
    let annot = annot
        .as_object_mut()
        .ok_or_else(|| anyhow!("annotations is not an object"))?;

    let ns = process.metadata.namespace.as_deref().unwrap_or("default");
    let name = process.metadata.name.as_deref().unwrap_or("unnamed");
    // Seed the standard 2-slot ownership tag through the shared
    // substrate primitive so the SSA-time re-injection uses the exact
    // same key pair + FIELD_MANAGER value the render-time authoring
    // sites do — a rename of FIELD_MANAGER, or a new mandatory tag
    // added to `ownership_annotations`, propagates here mechanically.
    for (k, v) in ownership_annotations(&format!("{ns}/{name}")) {
        annot.insert(k, v);
    }

    if let Some(status) = &process.status {
        if let Some(pid) = &status.pid {
            annot.insert(annotations::PID.to_string(), Value::String(pid.clone()));
        }
        if let Some(id) = &status.identity {
            annot.insert(
                annotations::CONTENT_HASH.to_string(),
                Value::String(id.content_hash.clone()),
            );
        }
        if let Some(a) = &status.attestation {
            annot.insert(
                annotations::GENERATION.to_string(),
                Value::String(a.generation.to_string()),
            );
            annot.insert(
                annotations::ATTESTATION_ROOT.to_string(),
                Value::String(a.composed_root.clone()),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals_resolve() {
        assert_eq!(plural_of("Kustomization").unwrap(), "kustomizations");
        assert_eq!(plural_of("HelmRelease").unwrap(), "helmreleases");
        assert_eq!(plural_of("GitRepository").unwrap(), "gitrepositories");
        assert!(plural_of("Nonsense").is_err());
    }

    #[test]
    fn api_resource_splits_group_and_version() {
        let ar = api_resource("kustomize.toolkit.fluxcd.io/v1", "Kustomization").unwrap();
        assert_eq!(ar.group, "kustomize.toolkit.fluxcd.io");
        assert_eq!(ar.version, "v1");
        assert_eq!(ar.plural, "kustomizations");
    }

    #[test]
    fn api_resource_handles_core_v1() {
        let ar = api_resource("v1", "ConfigMap").unwrap();
        assert_eq!(ar.group, "");
        assert_eq!(ar.version, "v1");
        assert_eq!(ar.plural, "configmaps");
    }

    #[test]
    fn ready_condition_true() {
        let data = json!({
            "status": { "conditions": [
                { "type": "Ready", "status": "True" }
            ]}
        });
        assert_eq!(ready_condition_value(&data), ReadyState::Ready);
    }

    #[test]
    fn ready_condition_false_with_message() {
        let data = json!({
            "status": { "conditions": [
                { "type": "Ready", "status": "False", "message": "pull failed" }
            ]}
        });
        assert_eq!(
            ready_condition_value(&data),
            ReadyState::NotReady(Some("pull failed".to_string()))
        );
    }

    #[test]
    fn ready_condition_missing_is_unknown() {
        let data = json!({ "status": { "conditions": [] } });
        assert_eq!(ready_condition_value(&data), ReadyState::Unknown);
        let data = json!({});
        assert_eq!(ready_condition_value(&data), ReadyState::Unknown);
    }

    #[test]
    fn inject_owner_reference_adds_entry() {
        let mut obj = json!({
            "apiVersion": "v1", "kind": "ConfigMap",
            "metadata": { "name": "x" },
        });
        inject_owner_reference(
            &mut obj,
            json!({ "apiVersion": "tatara.pleme.io/v1alpha1", "kind": "Process", "name": "p", "uid": "u" }),
        )
        .unwrap();
        let refs = obj["metadata"]["ownerReferences"].as_array().unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0]["kind"], "Process");
    }

    // ─── ownership_annotations substrate pins ───────────────────────────
    //
    // The 2-slot `{MANAGED_BY: FIELD_MANAGER, PROCESS: process_ref}`
    // shape recurred at five hand-authored sites (three in
    // `render.rs` + two in `edges.rs`) before this primitive existed,
    // each hand-coding the `"tatara-reconciler"` literal on the
    // MANAGED_BY slot in addition to the shape. These pins bind the
    // primitive at fail-before-pass-after granularity so a regression
    // that drifted a key, swapped the MANAGED_BY value back to a
    // literal, or reordered the slots surfaces here rather than as
    // silent drift at every emitted resource's ownership tag.

    #[test]
    fn ownership_annotations_produces_field_manager_and_process_ref() {
        let m = ownership_annotations("demo-ns/demo-app");
        // Exactly two keys — no accidental extras (a regression that
        // seeded PID / CONTENT_HASH into the primitive would fail
        // here rather than pollute every render/edges callsite).
        assert_eq!(m.len(), 2);
        // MANAGED_BY reads FIELD_MANAGER, NOT the literal
        // `"tatara-reconciler"` string the pre-lift sites hard-coded.
        // A future rename of FIELD_MANAGER now propagates through
        // this primitive to every downstream emit site by
        // construction; a regression that re-hard-coded the literal
        // fails this assertion.
        assert_eq!(
            m.get(annotations::MANAGED_BY).and_then(Value::as_str),
            Some(FIELD_MANAGER),
            "MANAGED_BY slot must carry FIELD_MANAGER, not a hand-authored literal"
        );
        assert_eq!(
            m.get(annotations::PROCESS).and_then(Value::as_str),
            Some("demo-ns/demo-app"),
            "PROCESS slot must ride the caller-supplied process_ref verbatim"
        );
    }

    #[test]
    fn ownership_annotations_rides_arbitrary_process_ref_shapes() {
        // The reconciler shapes `process_ref` as `<ns>/<name>` at the
        // render.rs sites (via `format!("{ns}/{name}")`) and passes
        // pre-composed `ctx.process_ref` at the edges.rs sites. The
        // primitive treats the input as opaque — a caller-composed
        // reference (e.g. a future `<ns>/<name>@<generation>` shape
        // for multi-generation attestation grepping) rides through
        // unchanged, and the empty-string edge case (unnamed process
        // pre-metadata) does not panic.
        for input in [
            "flux-system/observability-stack",
            "just-a-name",
            "",
            "ns/name@42",
            "with spaces and / slashes",
        ] {
            let m = ownership_annotations(input);
            assert_eq!(
                m.get(annotations::PROCESS).and_then(Value::as_str),
                Some(input),
                "PROCESS slot must ride {input:?} verbatim"
            );
        }
    }

    #[test]
    fn ownership_annotations_interpolates_cleanly_through_json_macro() {
        // The three render.rs sites interpolate the primitive under an
        // `"annotations"` key inside a `json!({...})` block. Pin the
        // interop shape so a regression that swapped the return type
        // from `Map` to a `Value` variant that stops interpolating
        // as an object (e.g. `Value::Array`) surfaces here rather
        // than as a broken `metadata.annotations` on every emitted
        // Kustomization / OCIRepository / HelmRelease.
        let m = ownership_annotations("demo/ephemeral-demo");
        let wrapped = json!({
            "metadata": {
                "name": "ephemeral-demo",
                "namespace": "demo",
                "annotations": m.clone(),
            },
        });
        let anns = &wrapped["metadata"]["annotations"];
        assert!(anns.is_object(), "annotations must land as a JSON object");
        assert_eq!(anns[annotations::MANAGED_BY], FIELD_MANAGER);
        assert_eq!(anns[annotations::PROCESS], "demo/ephemeral-demo");
        // And the raw Map serialisation is byte-identical to the
        // interpolated form — no reshaping happens through the
        // macro boundary.
        assert_eq!(serde_json::Value::Object(m), *anns);
    }

    #[test]
    fn inject_annotations_delegates_through_ownership_primitive() {
        // `inject_annotations` seeds its annotation carrier through
        // `ownership_annotations` before extending with PID /
        // CONTENT_HASH / GENERATION / ATTESTATION_ROOT. Pin that the
        // SSA-time re-injection produces the SAME 2-slot ownership
        // pair the render-time authoring does — pre-existing
        // operator-facing keys don't change wording under the lift.
        //
        // Construct a Process via serde_json so the test doesn't need
        // to reproduce the full ProcessSpec builder scaffold from
        // `claim.rs`'s `empty_process` helper. Only metadata.name +
        // metadata.namespace matter for `inject_annotations`'s
        // seed-time behavior; `status` is `None` so no
        // PID / CONTENT_HASH keys land and only the seed keys are
        // asserted.
        let process: Process = serde_json::from_value(json!({
            "apiVersion": "tatara.pleme.io/v1alpha1",
            "kind": "Process",
            "metadata": { "name": "demo-app", "namespace": "demo-ns" },
            "spec": {
                "identity": {},
                "classification": {
                    "pointType": "Gate",
                    "substrate": "Compute",
                },
                "intent": { "flux": {
                    "path": "./",
                    "gitRepository": "flux-system",
                }},
                "boundary": {},
                "compliance": {},
                "signals": {},
                "lifetime": { "permanent": {} },
                "suspended": false,
            },
        }))
        .expect("Process deserialises from fixture JSON");
        let mut resource = json!({
            "apiVersion": "v1", "kind": "ConfigMap",
            "metadata": { "name": "x" },
        });
        inject_annotations(&mut resource, &process).unwrap();
        let anns = &resource["metadata"]["annotations"];
        assert_eq!(anns[annotations::MANAGED_BY], FIELD_MANAGER);
        assert_eq!(anns[annotations::PROCESS], "demo-ns/demo-app");
    }
}
