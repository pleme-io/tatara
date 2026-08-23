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

/// Substrate-primitive builder for the standard tatara-reconciler
/// **ownership label pair** — the 2-slot
/// `{MANAGED_BY: FIELD_MANAGER, PROCESS: process_ref}` object shape
/// every emitted resource in [`crate::render`] and every routing edge
/// in [`crate::edges`] marks its `metadata.labels` block with. Sibling
/// on the `labels` axis to [`ownership_annotations`] on the
/// `annotations` axis; both carry the SAME 2-key ownership pair so
/// operators grepping resources by either axis (label selectors from
/// `kubectl get -l tatara.pleme.io/process=…`, annotation lookups from
/// the reconciler's own drift detector) land at the identical key
/// pair — the `assert_ownership_pair_matches_annotations` pin binds
/// the invariant at compile-adjacent test granularity.
///
/// Pre-lift the 2-slot label shape was hand-authored at THREE sites
/// past the PRIME-DIRECTIVE ≥ 2 duplication threshold:
/// * [`crate::edges::IngressEdge`] — the Ingress `metadata.labels`
///   block, literal `annotations::MANAGED_BY: "tatara-reconciler",
///   annotations::PROCESS: ctx.process_ref` inside a `json!({...})`
///   before the routing-form + app extension.
/// * [`crate::edges::DnsEndpointEdge`] — the DNSEndpoint
///   `metadata.labels` block, same 2-key literal (already delegating
///   through [`ownership_annotations`] on the annotations axis).
/// * [`crate::render::one_export_job`] — the export Job
///   `metadata.labels` block, same 2-key literal before the ROLE +
///   EXPORT_INDEX extension.
///
/// The three literal sites additionally hand-coded the
/// `"tatara-reconciler"` string on the `MANAGED_BY` slot, so they
/// bypassed [`FIELD_MANAGER`] and would drift silently if the field
/// manager string is ever renamed. Post-lift every one of these
/// sites reads the const, so the invariant "MANAGED_BY ==
/// FIELD_MANAGER across every emitted resource's labels axis" holds
/// by construction, matching the invariant the annotations-axis
/// primitive already enforced.
///
/// **Peer sites intentionally NOT collapsed:** the export Job's pod
/// template `metadata.labels` at [`crate::render::one_export_job`]
/// carries only `{PROCESS, ROLE, EXPORT_INDEX}` — no `MANAGED_BY` —
/// because pod-template labels feed the Job's pod-selector wiring,
/// not reconciler ownership discovery, and adding `MANAGED_BY` there
/// would inflate the selector unnecessarily. That site is a
/// deliberately different shape, not a lift candidate.
///
/// Returns a [`serde_json::Map`] rather than a [`Value`] so callers
/// can either drop it under a `"labels"` key with `Value::Object(map)`
/// inside a `json!` macro, or `extend` it with additional keys
/// (see [`crate::edges::IngressEdge`] / [`crate::edges::DnsEndpointEdge`]
/// which append `app` + `routing-form` labels to the same map, or
/// [`crate::render::one_export_job`] which appends `role` +
/// `export-index` labels).
///
/// A future addition (e.g. a `VERSION` slot naming the reconciler
/// build, a `LEASE_ID` slot for multi-instance leadership, a
/// `RECONCILE_GENERATION` counter for stall detection) lands at this
/// ONE substrate primitive and every downstream emit site inherits
/// the upgrade mechanically — no per-site hand-edit at edges.rs or
/// render.rs. And because the sibling annotations primitive carries
/// the same 2-slot shape, a future `VERSION`/`LEASE_ID` slot added
/// to both primitives keeps the labels-axis and annotations-axis
/// ownership tags in lockstep by construction.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the 2-slot labels shape recurred at three hand-authored sites
/// past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted
/// to ONE owner here). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — a regression that drifted the label key or
/// the field manager string at ONE site surfaces at
/// [`tests::ownership_labels_produces_field_manager_and_process_ref`]
/// / [`tests::ownership_labels_pair_matches_annotations_pair`]
/// rather than as silent drift at every downstream emit site).
pub fn ownership_labels(process_ref: &str) -> serde_json::Map<String, Value> {
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

/// Substrate-primitive builder for the standard tatara-reconciler
/// **namespace-qualified process reference** — the `<ns>/<name>`
/// string every consumer that grepped, keyed, or annotated a
/// Process by "which cluster location owns it" composed by hand.
/// Peer to [`ownership_annotations`], whose `process_ref` parameter
/// is the value this primitive returns.
///
/// Pre-lift the `format!("{ns}/{name}")` incantation was hand-authored
/// at SEVEN sites past the PRIME-DIRECTIVE ≥ 2 duplication threshold:
/// * [`crate::render::render_flux`] — the Kustomization
///   `metadata.annotations` seed (`ownership_annotations(&format!
///   ("{ns}/{name}"))`).
/// * [`crate::render::render_aplicacao`] × 2 — the OCIRepository +
///   HelmRelease `metadata.annotations` seeds, same shape.
/// * [`crate::render::render_export_jobs`] — the per-Process
///   `process_ref` binding threaded through every emitted export
///   Job's owner metadata.
/// * [`crate::ssapply::inject_annotations`] — the SSA-time
///   re-injection's `ownership_annotations(&format!("{ns}/{name}"))`
///   feed, seeding the standard 2-slot ownership tag.
/// * [`crate::phase_machine::process_holds_any_claim`] — the
///   claim-arbiter's `holder` comparator (matches ProcessTable
///   claim rows keyed by `<ns>/<name>`).
/// * [`crate::phase_machine::handle_releasing`] — the export-Job
///   label-selector `PROCESS=<ns>/<name>` filter used to enumerate
///   THIS Process's Jobs (not any sibling Process's).
///
/// Post-lift every callsite reads through this ONE primitive so a
/// future change to the reference shape — a `<ns>/<name>@<gen>`
/// multi-generation variant for attestation grepping, a
/// `<cluster>/<ns>/<name>` cross-cluster form, a normalization
/// (case-fold, unicode-safe collation) that must apply everywhere
/// — lands at ONE substrate method here and every downstream
/// composer (annotation seed, ProcessTable claim key, label
/// selector, owner metadata) inherits the upgrade mechanically.
///
/// The `&str` parameters accept both `&String` (which coerces via
/// deref) and `&str` literal / slice callers, matching every shape
/// currently authored: `render_flux` / `render_aplicacao` pass
/// their `ns: &str, name: &str` function params directly;
/// `render_export_jobs` / `handle_releasing` pass `&ns, &name`
/// from `String` locals; `inject_annotations` /
/// `process_holds_any_claim` pass `&str` slices from
/// `.as_deref().unwrap_or(...)`.
///
/// The 2-arg signature encodes the invariant "the qualified
/// reference is EXACTLY `<ns>/<name>`, in that order, joined by
/// a single `/` separator" at the type level — a caller cannot
/// accidentally swap the two axes (which would produce
/// `<name>/<ns>` and silently break every downstream grep) nor
/// omit either half, the way a pre-lift hand-authored
/// `format!("{name}/{ns}")` or `format!("{ns}-{name}")` typo
/// would.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition —
/// the `<ns>/<name>` shape recurred at seven hand-authored sites
/// well past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is
/// lifted to ONE owner here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — a regression that swapped the
/// two axes or the separator at ONE site surfaces at
/// [`tests::qualified_process_ref_joins_ns_and_name_with_slash`]
/// rather than as silent drift at every downstream annotation
/// seed / claim key / label selector).
pub fn qualified_process_ref(ns: &str, name: &str) -> String {
    format!("{ns}/{name}")
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

    // Route the two-slot metadata pull through the substrate primitive
    // on `Process` — the pre-lift hand-authored `.metadata.namespace
    // .as_deref().unwrap_or("default")` +
    // `.metadata.name.as_deref().unwrap_or("unnamed")` incantations
    // now share ONE fallback owner with the render owner-metadata
    // seed (render::render), claim-arbiter row builder
    // (table_controller::reconcile), and boundary-evaluator default-
    // namespace resolver (boundary::evaluate / check_depends_on).
    let (ns, name) = process.coordinates_or_defaults();
    // Seed the standard 2-slot ownership tag through the shared
    // substrate primitive so the SSA-time re-injection uses the exact
    // same key pair + FIELD_MANAGER value the render-time authoring
    // sites do — a rename of FIELD_MANAGER, or a new mandatory tag
    // added to `ownership_annotations`, propagates here mechanically.
    for (k, v) in ownership_annotations(&qualified_process_ref(ns, name)) {
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

    // ─── ownership_labels substrate pins ────────────────────────────────
    //
    // The 2-slot `{MANAGED_BY: FIELD_MANAGER, PROCESS: process_ref}`
    // labels shape recurred at three hand-authored sites
    // (edges::IngressEdge, edges::DnsEndpointEdge, render::one_export_job)
    // before this primitive existed, each hand-coding the
    // `"tatara-reconciler"` literal on the MANAGED_BY slot in addition
    // to the shape. These pins mirror the sibling annotations-axis
    // pins immediately above so a regression on either axis surfaces
    // at fail-before-pass-after granularity, and additionally cross-
    // check that the two axes carry the IDENTICAL 2-key pair — the
    // invariant every operator relies on when the same resource is
    // grepped via either axis.

    #[test]
    fn ownership_labels_produces_field_manager_and_process_ref() {
        let m = ownership_labels("demo-ns/demo-app");
        // Exactly two keys — no accidental extras (a regression that
        // seeded the pod-selector-only ROLE/EXPORT_INDEX keys into
        // the primitive would fail here rather than pollute every
        // edges/render callsite's labels block).
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
    fn ownership_labels_rides_arbitrary_process_ref_shapes() {
        // Peer to `ownership_annotations_rides_arbitrary_process_ref_shapes`:
        // the primitive treats the input as opaque, so any
        // caller-composed reference shape (`<ns>/<name>`,
        // `<ns>/<name>@<gen>`, bare `<name>`, empty) rides through
        // unchanged into the labels axis with no interpretation.
        for input in [
            "flux-system/observability-stack",
            "just-a-name",
            "",
            "ns/name@42",
            "with spaces and / slashes",
        ] {
            let m = ownership_labels(input);
            assert_eq!(
                m.get(annotations::PROCESS).and_then(Value::as_str),
                Some(input),
                "PROCESS slot must ride {input:?} verbatim"
            );
        }
    }

    #[test]
    fn ownership_labels_interpolates_cleanly_through_json_macro() {
        // The three shipped sites interpolate the primitive under a
        // `"labels"` key inside a `json!({...})` block via
        // `Value::Object(map)`. Pin the interop shape so a regression
        // that swapped the return type from `Map` to a `Value` variant
        // that stops interpolating as an object surfaces here rather
        // than as a broken `metadata.labels` on every emitted Ingress
        // / DNSEndpoint / export Job.
        let m = ownership_labels("demo/ephemeral-demo");
        let wrapped = json!({
            "metadata": {
                "name": "ephemeral-demo",
                "namespace": "demo",
                "labels": Value::Object(m.clone()),
            },
        });
        let labels = &wrapped["metadata"]["labels"];
        assert!(labels.is_object(), "labels must land as a JSON object");
        assert_eq!(labels[annotations::MANAGED_BY], FIELD_MANAGER);
        assert_eq!(labels[annotations::PROCESS], "demo/ephemeral-demo");
        assert_eq!(serde_json::Value::Object(m), *labels);
    }

    #[test]
    fn ownership_labels_pair_matches_annotations_pair() {
        // Cross-axis coherence: labels and annotations carry the SAME
        // 2-key ownership pair, byte-identical, for the same
        // `process_ref`. Operators grep resources by either axis
        // (`kubectl get -l tatara.pleme.io/process=…` on the labels
        // axis, annotation lookups on the annotations axis) and land
        // at the identical key pair. A future addition to one
        // primitive that isn't mirrored to the other (e.g. a
        // labels-only `VERSION` slot) would drift the two axes and
        // fail this pin — surfacing the desync at fail-before-pass-
        // after granularity rather than as a silent operator-facing
        // discrepancy between what `-l` selects and what the
        // annotations reader sees.
        let labels_map = ownership_labels("demo-ns/demo-app");
        let annotations_map = ownership_annotations("demo-ns/demo-app");
        assert_eq!(
            labels_map, annotations_map,
            "ownership_labels and ownership_annotations must return byte-identical maps",
        );
    }

    // ─── qualified_process_ref substrate pins ───────────────────────────
    //
    // The `format!("{ns}/{name}")` incantation was hand-authored at
    // SEVEN sites — three inside `render::render_flux`/
    // `render_aplicacao` metadata annotation seeds, one at
    // `render::render_export_jobs` binding `process_ref`, one here at
    // `inject_annotations`'s SSA-time re-injection seed, and two in
    // `phase_machine.rs` (`process_holds_any_claim`'s claim
    // comparator + `handle_releasing`'s label-selector composer).
    // These pins bind the primitive at fail-before-pass-after
    // granularity so a regression that swapped the two axes, changed
    // the separator, or renormalized the input surfaces here rather
    // than as silent operator-facing drift at every downstream
    // annotation seed / claim key / label selector.
    #[test]
    fn qualified_process_ref_joins_ns_and_name_with_slash() {
        // The invariant every downstream consumer composes against:
        // the qualified reference is EXACTLY `<ns>/<name>`, in that
        // order, joined by a single `/`. A regression that inserted
        // a colon, swapped the two axes, or dropped either half would
        // silently break every grep keyed on this shape (annotation
        // reader, claim-arbiter comparator, `PROCESS=<ref>` label
        // selector).
        assert_eq!(
            qualified_process_ref("demo-ns", "ephemeral-demo"),
            "demo-ns/ephemeral-demo",
        );
    }

    #[test]
    fn qualified_process_ref_accepts_string_deref_and_str_slice_shapes() {
        // The seven shipped callsites split across two shapes: the
        // render.rs `render_flux` / `render_aplicacao` sites + the
        // phase_machine.rs `process_holds_any_claim` site pass
        // `&str` slices directly from function params /
        // `.as_deref().unwrap_or(...)`; the render.rs
        // `render_export_jobs` site + the phase_machine.rs
        // `handle_releasing` site pass `&ns, &name` from `String`
        // locals (via deref coercion). Pin both shapes at the type
        // level — the `&str` parameters must accept both without
        // widening.
        let owned_ns = String::from("owned-ns");
        let owned_name = String::from("owned-app");
        let borrowed_ns: &str = "borrowed-ns";
        let borrowed_name: &str = "borrowed-app";
        assert_eq!(
            qualified_process_ref(&owned_ns, &owned_name),
            "owned-ns/owned-app",
        );
        assert_eq!(
            qualified_process_ref(borrowed_ns, borrowed_name),
            "borrowed-ns/borrowed-app",
        );
        // Mixed shapes (one owned, one borrowed) also ride cleanly —
        // matches the phase_machine.rs `process_holds_any_claim`
        // path where both slots come from the same `.as_deref()`
        // chain but future consumers may compose across owned +
        // borrowed provenance.
        assert_eq!(
            qualified_process_ref(&owned_ns, borrowed_name),
            "owned-ns/borrowed-app",
        );
    }

    #[test]
    fn qualified_process_ref_composes_cleanly_into_ownership_annotations() {
        // The primary consumer is `ownership_annotations` — four of
        // the seven pre-lift sites (three in render.rs + this one
        // in ssapply.rs) pass the result straight through as the
        // annotation seed's `process_ref` arg. Pin the composition:
        // building the standard 2-slot ownership tag through
        // `ownership_annotations(&qualified_process_ref(ns, name))`
        // produces the SAME map a pre-lift hand-authored
        // `ownership_annotations(&format!("{ns}/{name}"))` did —
        // no drift between the primitive-composed form and the
        // literal-composed form the tests assert against below.
        let composed = ownership_annotations(&qualified_process_ref("demo", "ephemeral-demo"));
        let hand_authored = ownership_annotations("demo/ephemeral-demo");
        assert_eq!(
            composed, hand_authored,
            "primitive-composed process_ref must produce the same annotation map as a pre-lift hand-authored literal"
        );
        // And the resulting PROCESS slot value is exactly the
        // `<ns>/<name>` shape every downstream `PROCESS=<ref>`
        // label-selector composes against.
        assert_eq!(
            composed.get(annotations::PROCESS).and_then(Value::as_str),
            Some("demo/ephemeral-demo"),
        );
    }

    #[test]
    fn qualified_process_ref_rides_edge_case_axis_shapes() {
        // The reconciler shapes the two axes as arbitrary strings —
        // no length/character validation happens at the composer, so
        // any shape a Process's `metadata.namespace` /
        // `metadata.name` can hold rides through unchanged. Pin the
        // empty-string cases (unnamed process pre-metadata,
        // cluster-scoped `namespace = ""` fallback), the
        // whitespace-and-slash-in-name pathological case (a
        // regression that URL-escaped the input at this primitive
        // would silently break every downstream grep), and the
        // shape `process_holds_any_claim` early-returns on
        // (`name.is_empty()`) so the composer's post-condition
        // matches the caller's pre-condition.
        assert_eq!(qualified_process_ref("", ""), "/");
        assert_eq!(qualified_process_ref("default", ""), "default/");
        assert_eq!(qualified_process_ref("", "orphan"), "/orphan");
        assert_eq!(
            qualified_process_ref("weird ns", "with/slash"),
            "weird ns/with/slash",
        );
    }
}
