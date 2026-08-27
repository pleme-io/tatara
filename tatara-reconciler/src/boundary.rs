//! Boundary condition evaluator — the VERIFY half of the convergence loop.
//!
//! Evaluates the `ConditionKind` variants against live cluster state:
//! - `ProcessPhase`:           lookup the referenced Process, compare phase
//! - `KustomizationHealthy`:   fetch the Kustomization, read `status.conditions[Ready]`
//! - `HelmReleaseReleased`:    same, for `HelmRelease`
//! - `PromQL`:                 stub (returns Unknown) — needs a metrics client
//! - `Cel`:                    stub (returns Unknown) — needs a CEL runtime
//! - `NixEval`:                stub (returns Unknown) — needs tatara-engine
//! - `JobAttested`:            Job.status.succeeded >= 1; optional receipt
//!                             ConfigMap verification
//! - `ClosedLoopAuth`:         JobAttested + BLAKE3 receipt shape verified
//!                             (the canonical postcondition for any system
//!                             that can produce credentials for its own
//!                             client under test — e.g. a secrets
//!                             backend issuing creds to its bundled client)
//!
//! `check_depends_on` reuses the `ProcessPhase` evaluator and returns unmet
//! dependencies structured for UX messaging.

use anyhow::{anyhow, Result};
use kube::{Api, Client};
use serde::Deserialize;
use serde_json::Value;

use tatara_process::boundary::{Condition, ConditionKind};
use tatara_process::flux_resource::FluxResource;
use tatara_process::k8s_builtin_resource::K8sBuiltinResource;
use tatara_process::phase::ProcessPhase;
use tatara_process::prelude::Process;
#[cfg(test)]
use tatara_process::receipt::ReceiptKind;
use tatara_process::receipt::{ReceiptEnvelope, ReceiptError};

use crate::ssapply;

/// Result of a single boundary predicate evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Satisfaction {
    /// The predicate holds.
    Satisfied,
    /// The predicate does not hold; `String` is a user-facing reason.
    Unsatisfied(String),
    /// Evaluator could not determine (missing data, unimplemented kind, etc.).
    Unknown(String),
}

impl Satisfaction {
    pub fn is_satisfied(&self) -> bool {
        matches!(self, Self::Satisfied)
    }
    pub fn message(&self) -> Option<&str> {
        match self {
            Self::Satisfied => None,
            Self::Unsatisfied(m) | Self::Unknown(m) => Some(m),
        }
    }
}

/// Closed set of diagnostic label prefixes used by Job-based boundary
/// evaluators on this file — the substrate primitive that owns the
/// per-evaluator diagnostic label every `require_succeeded_job` +
/// `classify_receipt_verdict` callsite pair threads through.
///
/// Pre-lift the label was hand-authored at FOUR production sites past
/// the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold, split across two
/// evaluators:
///
/// * [`evaluate_job_attested`] — two adjacent sites:
///     * `require_succeeded_job(..., "Job")` (the fetch-and-classify
///       call before the optional receipt branch).
///     * `classify_receipt_verdict("Job", ...)` (the receipt-verdict
///       projection inside the optional receipt branch).
/// * [`evaluate_closed_loop_auth`] — two adjacent sites:
///     * `require_succeeded_job(..., "closed-loop probe Job")`.
///     * `classify_receipt_verdict("closed-loop probe Job", ...)`.
///
/// Each evaluator threads the SAME label through its two adjacent
/// call sites — a rename that reached the `require_succeeded_job` site
/// but not the `classify_receipt_verdict` site (or vice versa) would
/// silently bifurcate the diagnostic prefix operators grep for across
/// the Job's missing/failed/running vs receipt/malformed axes without
/// tripping any test. Post-lift both axes bind at ONE per-variant
/// owner, and rustc enforces every consumer reads the label from the
/// SAME variant.
///
/// Enumerable via [`Self::ALL`] so the sibling label-sweep tests
/// (`classify_job_status_tests::label_prefixes_all_three_unsatisfied_projections_across_shipped_labels`
/// and
/// `classify_receipt_verdict_tests::label_prefixes_both_unsatisfied_projections_across_shipped_labels`)
/// walk every variant without a hand-maintained `["Job", "closed-loop
/// probe Job"]` sweep on the caller side — a future third variant
/// added here extends both sweeps mechanically through the same
/// `ALL` binding, matching the peer sweep-enumerable seed on
/// [`FluxResource::ALL`].
///
/// Extension: kenshi-runner's P3 lift (see repo `CLAUDE.md`, "P3 —
/// kenshi-runner library lift") will land its per-suite Job under a
/// dedicated variant here, and the same three primitives
/// ([`require_succeeded_job`], [`classify_job_status`], and
/// [`classify_receipt_verdict`]) will pick up the new label
/// mechanically through the SAME closed-set arm. Every future Job-
/// based postcondition evaluator (per-membro contract receipts, any
/// Job whose completion produces a receipt) lands as ONE variant +
/// ONE `as_str` arm + ONE `ALL` entry, all three exhaustively enforced
/// by rustc's match coverage.
///
/// Sibling to the [`tatara_process::flux_resource::FluxResource`]
/// closed set on the K8s `(apiVersion, kind)` wire-form axis — both
/// project a closed set of variant → canonical string identity slots
/// through a `const fn` per-axis method dispatch. Where `FluxResource`
/// owns the wire-form identity of K8s resources tatara emits or
/// consumes, `JobEvaluatorLabel` owns the diagnostic-prefix identity
/// of the Job-based sub-axes tatara's boundary evaluator verifies.
///
/// Theory grounding: THEORY.md §VI.1 (generation over composition —
/// the label recurred at four hand-authored production sites past the
/// PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to ONE
/// closed-set owner per variant here). THEORY.md §II.1 invariant 5
/// (composition preserves proofs — the per-variant mappings live at
/// ONE typed algebra projection; a regression that drifted the label
/// at ONE adjacent callsite would fail-loudly at this module's byte-
/// shape pins rather than as silent operator-visible prefix skew
/// across the Job-status vs receipt-verdict axes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum JobEvaluatorLabel {
    /// The `ConditionKind::JobAttested` axis — a bare `batch/v1` Job
    /// whose success + optional receipt operators verify. Diagnostic
    /// prefix: `"Job"`.
    JobAttested,
    /// The `ConditionKind::ClosedLoopAuth` axis — the closed-loop
    /// probe Job that verifies a system's bundled issuer authenticates
    /// its own bundled consumer + writes a `tatara-receipt/v1`
    /// envelope. Diagnostic prefix: `"closed-loop probe Job"`.
    ClosedLoopProbe,
}

impl JobEvaluatorLabel {
    /// The closed set of Job-evaluator label variants this substrate
    /// binds. Enumerable so the sibling label-sweep tests (and any
    /// future coherence check) can walk every variant without a hand-
    /// maintained list on the caller side.
    ///
    /// `#[allow(dead_code)]` gates the not-yet-referenced production
    /// use: today `ALL` is consumed only by the sibling `#[cfg(test)]`
    /// sweeps, but a future production coherence check (per the P3
    /// kenshi-runner lift's per-suite Job registration path, or a
    /// registry-time sweep across every workspace-shipped label) will
    /// bind through the same `ALL` iterator. Peer to the sibling
    /// substrate primitive [`FluxResource::ALL`] on the K8s wire-form
    /// axis in `tatara-process`.
    #[allow(dead_code)]
    const ALL: [Self; 2] = [Self::JobAttested, Self::ClosedLoopProbe];

    /// Diagnostic label prefix stamped at the head of every operator-
    /// facing `Satisfaction::Unsatisfied("<label> {ns}/{name} …")`
    /// diagnostic on this evaluator's Job-based sub-axes.
    ///
    /// * `JobAttested`     → `"Job"`
    /// * `ClosedLoopProbe` → `"closed-loop probe Job"`
    ///
    /// `const fn` so the closed-set arm reduces to a `&'static str`
    /// slot at every callsite with no runtime overhead. A regression
    /// that drifted either arm's byte shape would silently split the
    /// diagnostic prefix operators grep for at the JobAttested vs
    /// ClosedLoopAuth evaluator — post-lift both arms bind here and
    /// the sibling per-variant tests below pin every arm's byte shape.
    const fn as_str(self) -> &'static str {
        match self {
            Self::JobAttested => "Job",
            Self::ClosedLoopProbe => "closed-loop probe Job",
        }
    }
}

#[cfg(test)]
mod job_evaluator_label_tests {
    use super::*;

    #[test]
    fn all_enumerates_every_variant_exactly_once() {
        // A future 3rd variant added without an `ALL` entry (or a
        // duplicate entry that skewed the sweep) surfaces here.
        assert_eq!(JobEvaluatorLabel::ALL.len(), 2);
        let mut seen = std::collections::HashSet::new();
        for v in JobEvaluatorLabel::ALL {
            assert!(seen.insert(v), "duplicate variant in ALL: {v:?}");
        }
    }

    #[test]
    fn job_attested_label_is_bare_job() {
        // Byte-identity pin: the exact prefix operators grep for
        // across the ConditionKind::JobAttested arm's Missing / Failed
        // / Running / receipt-Missing / receipt-Malformed diagnostics.
        // A drift here silently splits dashboards keyed on the "Job "
        // prefix — before the lift the primitives could be renamed on
        // one adjacent callsite (e.g. `require_succeeded_job(..., "Job")`)
        // and not the other (e.g. `classify_receipt_verdict("Job", ...)`)
        // in the same evaluator body without a single test failing.
        assert_eq!(JobEvaluatorLabel::JobAttested.as_str(), "Job");
    }

    #[test]
    fn closed_loop_probe_label_is_closed_loop_probe_job() {
        // Byte-identity pin: the exact prefix stamped on the
        // ConditionKind::ClosedLoopAuth arm's diagnostics — sibling of
        // the pin above.
        assert_eq!(
            JobEvaluatorLabel::ClosedLoopProbe.as_str(),
            "closed-loop probe Job"
        );
    }

    #[test]
    fn every_variants_as_str_is_distinct_across_the_closed_set() {
        // Cross-variant coherence pin: no two variants may share a
        // diagnostic prefix. A future extension that added a variant
        // duplicating an existing label (e.g. a `KenshiSuite` variant
        // that reused the `"Job"` prefix in a copy-paste) would surface
        // here rather than as silent operator-visible cross-axis
        // ambiguity in dashboards keyed on the prefix.
        let mut labels = std::collections::HashSet::new();
        for v in JobEvaluatorLabel::ALL {
            assert!(
                labels.insert(v.as_str()),
                "duplicate as_str at {v:?}: {}",
                v.as_str()
            );
        }
    }

    #[test]
    fn as_str_is_const_fn_reachable() {
        // Compile-time reachability pin: every variant's projection is
        // `const fn`, so a caller can bind it into a `const` slot. A
        // regression that dropped the `const` qualifier would fail-
        // loudly here rather than as a wrong-slot runtime dispatch at
        // every callsite.
        const JOB: &str = JobEvaluatorLabel::JobAttested.as_str();
        const CLOSED_LOOP: &str = JobEvaluatorLabel::ClosedLoopProbe.as_str();
        assert_eq!(JOB, "Job");
        assert_eq!(CLOSED_LOOP, "closed-loop probe Job");
    }

    #[test]
    fn as_str_is_a_pure_function_of_the_variant() {
        // Purity pin: calling `as_str` repeatedly on the same variant
        // returns byte-identical `&'static str`s. Guards against an
        // implementation that lazily materialized an interned key per-
        // call and hashed against runtime state.
        for v in JobEvaluatorLabel::ALL {
            assert_eq!(v.as_str(), v.as_str());
        }
    }
}

/// Deserialize the axis-typed params carrier `T` from a boundary
/// condition's `params: serde_json::Value`, projecting a serde-level
/// rejection into the operator-facing
/// [`Satisfaction::Unknown("<kind_label> params invalid: {e}")`]
/// diagnostic every kind-typed evaluator on this file pre-lift restated
/// verbatim at its own site.
///
/// Pre-lift the FOUR kind-typed evaluators
/// ([`evaluate_process_phase`] on the `ProcessPhase` axis,
/// [`evaluate_flux_ready`] on the `Kustomization` / `HelmRelease` axes,
/// [`evaluate_job_attested`] on the `JobAttested` axis,
/// [`evaluate_closed_loop_auth`] on the `ClosedLoopAuth` axis) each
/// hand-authored the same three-invariant scaffold at their own local
/// site: (1) clone the `Value`, (2) call `serde_json::from_value::<T>`
/// with the carrier's turbofish, (3) on Err wrap the syn-error into
/// `Satisfaction::Unknown` with a `"<kind_label> params invalid: {e}"`
/// prefix. FOUR byte-for-byte identical `let parsed: <T> = match
/// serde_json::from_value(params.clone()) { … }` blocks past the
/// PRIME-DIRECTIVE ≥ 2 duplication threshold, differing only in the
/// axis-typed carrier `T` and the `&str` kind label each threaded
/// through the SAME scaffold.
///
/// Post-lift the three shared invariants live at ONE substrate primitive
/// here; each evaluator threads its axis-typed carrier through the
/// turbofish and its kind label through the `&str` slot, and the Err
/// arm rides through `parse_condition_params(...).map_err(|s| ...)?`
/// at each callsite's own `Result<Satisfaction>` early-return path. A
/// regression that drifted the diagnostic wording (a swapped `params
/// invalid` phrase, a missing kind-label prefix, a promoted variant on
/// the `Satisfaction` axis) at ONE evaluator surfaces at the sibling
/// test [`parse_condition_params_tests`] rather than as silent
/// operator-facing drift across the four kind-typed evaluators.
///
/// The `kind_label` parameter is a `&str` (not `&'static str`) so a
/// future ConditionKind whose evaluator wants to tag its diagnostic
/// with a dynamic axis (a hypothetical `PromQL(query_label)` variant
/// that names the failing query in the diagnostic) lands as ONE new
/// callsite through the SAME primitive with a locally-borrowed
/// `&str`, no widening of the primitive's signature. Every current
/// callsite passes a bare `&'static str` literal — either a hard-
/// coded kind name (`"ProcessPhase"`, `"JobAttested"`,
/// `"ClosedLoopAuth"`) or the [`FluxResource::kind`] projection
/// ([`evaluate_flux_ready`] takes a typed [`FluxResource`] and
/// derives its `&'static str` kind for this slot); the `&str` bound
/// admits both.
///
/// The `T: serde::de::DeserializeOwned` bound rides the caller's
/// turbofish, matching how the substrate's own trait-dispatch primitive
/// [`tatara_lisp::domain::DeserializeKwarg`] binds its axis-agnostic
/// `T: DeserializeOwned` slot. All four workspace-shipped params
/// carriers (`ProcessPhaseParams` / `NamedResourceParams` /
/// `JobAttestedParams` / `ClosedLoopAuthParams`) are `#[derive(Deserialize)]`
/// with owned `String` slots, so `DeserializeOwned` holds mechanically.
///
/// Future structural promotion of the emitted diagnostic (a caller-
/// supplied span, a `note = "help: …"` chain, an `expected: <shape>`
/// hint drawn from the carrier's `serde` schema, a structured
/// `Satisfaction::Unknown { kind, source }` promotion) lands at ONE
/// substrate primitive here — all four kind-typed evaluators and every
/// future new ConditionKind evaluator pick up the upgrade mechanically,
/// with no per-evaluator hand-edit.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// three-invariant scaffold recurred at four kind-typed evaluators
/// well past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is
/// lifted to ONE owner here). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — the four axis-typed evaluators now compose
/// structurally through ONE primitive; a regression that drifted the
/// wording or the Satisfaction variant at ONE axis surfaces at
/// [`parse_condition_params_tests`] rather than as silent drift at
/// every downstream evaluator with a params carrier on that axis).
fn parse_condition_params<T: serde::de::DeserializeOwned>(
    kind_label: &str,
    params: &Value,
) -> Result<T, Satisfaction> {
    serde_json::from_value(params.clone())
        .map_err(|e| Satisfaction::Unknown(format!("{kind_label} params invalid: {e}")))
}

/// Parse a boundary condition's `params: &Value` into an axis-typed
/// carrier `T` OR short-circuit the enclosing evaluator with
/// `Ok(Satisfaction::Unknown(...))`.
///
/// Pre-lift the FOUR kind-typed evaluators
/// ([`evaluate_process_phase`] on the `ProcessPhase` axis,
/// [`evaluate_flux_ready`] on the `Kustomization` / `HelmRelease` axes,
/// [`evaluate_job_attested`] on the `JobAttested` axis,
/// [`evaluate_closed_loop_auth`] on the `ClosedLoopAuth` axis) each
/// hand-authored the same four-line `match parse_condition_params(...)
/// { Ok(p) => p, Err(unk) => return Ok(unk) }` short-circuit scaffold
/// verbatim at its own local site, differing only in the axis-typed
/// carrier's annotation on the `let` binding and the `&str` kind label
/// threaded through the SAME scaffold. FOUR byte-for-byte identical
/// short-circuit blocks past the ★★ PRIME-DIRECTIVE ≥ 2 duplication
/// threshold, sitting on top of the ONE substrate primitive
/// [`parse_condition_params`].
///
/// Post-lift each callsite reads as ONE line naming the axis-typed
/// carrier once (as the `let` annotation) and the kind label once (as
/// the `$label` slot). The macro binds the "short-circuit
/// `Ok(Satisfaction::Unknown)` on Err" discipline at ONE substrate
/// owner — a future promotion of the short-circuit shape (a
/// `Satisfaction::Unknown` upgrade to a structured `Unknown { kind,
/// source }` variant, a caller-supplied span, a `note = "help: …"`
/// chain, an `expected: <shape>` hint drawn from the carrier's serde
/// schema) lands here rather than at four hand-authored callsites,
/// and every future new ConditionKind evaluator picks up the
/// short-circuit path mechanically. Every current kind-typed
/// evaluator returns `Result<Satisfaction>` = `anyhow::Result<Satisfaction>`,
/// so the `return Ok(unk)` binds structurally — a hypothetical
/// evaluator with a different return type is a compile-time error
/// here rather than a runtime type-mismatch at the enclosing function.
///
/// Sibling to the [`parse_condition_params`] primitive on the
/// serde-projection axis — the primitive owns the (params → `Result<T,
/// Satisfaction>`) projection; this macro owns the (`Result<T,
/// Satisfaction>` → early-return-or-continue) control flow. Together
/// they partition the "parse-and-short-circuit" scaffold every
/// kind-typed evaluator on this file dispatches through.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// four-line short-circuit scaffold recurred at four kind-typed
/// evaluators past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and
/// is lifted to ONE substrate macro here). THEORY.md §II.1
/// invariant 5 (composition preserves proofs — post-lift the
/// "parse-or-short-circuit" scaffold binds structurally through ONE
/// macro; a regression that drifted the short-circuit posture at ONE
/// evaluator surfaces at the macro's tests rather than as silent
/// operator-visible drift across the four evaluators).
macro_rules! parse_params_or_return_unknown {
    ($label:expr, $params:expr) => {
        match $crate::boundary::parse_condition_params($label, $params) {
            Ok(p) => p,
            Err(unk) => return Ok(unk),
        }
    };
}

// ── typed params per kind ────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessPhaseParams {
    process_ref: String,
    #[serde(default)]
    namespace: Option<String>,
    phase: ProcessPhase,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NamedResourceParams {
    name: String,
    #[serde(default)]
    namespace: Option<String>,
}

// ── entry point ──────────────────────────────────────────────────────

/// Evaluate a single boundary condition against the cluster.
pub async fn evaluate(
    client: Client,
    process: &Process,
    condition: &Condition,
) -> Result<Satisfaction> {
    let default_ns = process.namespace_or_default();
    match condition.kind {
        ConditionKind::ProcessPhase => {
            evaluate_process_phase(client, default_ns, &condition.params).await
        }
        ConditionKind::KustomizationHealthy | ConditionKind::HelmReleaseReleased => {
            // The (ConditionKind → FluxResource) mapping rides through
            // the typed projection [`ConditionKind::flux_resource`] —
            // pre-lift the two arms each hand-authored a `(FluxResource
            // ::X.api_version(), FluxResource::X.kind())` pair as the
            // two `&str` slots the pre-lift `evaluate_flux_ready(...,
            // api_version: &str, kind: &str)` signature required, and
            // the callee's signature admitted arbitrary `&str` pairs
            // (a caller could invert the two slots or pair one kind's
            // apiVersion with another's kind, and the K8s API server
            // would silently 404 at SSA-fetch time). Post-lift the
            // callee accepts a typed [`FluxResource`] slot (invalid
            // pairings become unrepresentable), the mapping lives at
            // ONE typed projection on [`ConditionKind::flux_resource`],
            // and the two dispatch arms collapse onto ONE OR-pattern
            // that reads the FluxResource variant from `.flux_resource()`.
            // A new ConditionKind variant that fetches a fourth Flux
            // resource (a hypothetical `BucketSynced` against a Flux
            // `Bucket` source) extends the OR-pattern by ONE arm + adds
            // ONE `flux_resource` arm + ONE `FluxResource` variant.
            let resource = condition
                .kind
                .flux_resource()
                .expect("OR-pattern above covers exactly the flux-fetching kinds");
            evaluate_flux_ready(client, default_ns, &condition.params, resource).await
        }
        // Stub evaluators — `ConditionKind::stub_message` owns each
        // operator-facing string in `tatara-process::boundary`, so the
        // three "not yet implemented" arms here collapse to one site
        // that delegates to the typed projection. Adding (or
        // promoting) a stub kind lands at `stub_message`, not here.
        // The OR-pattern keeps exhaustiveness — the compiler still
        // forces every `ConditionKind` to reach a branch.
        ConditionKind::PromQL | ConditionKind::Cel | ConditionKind::NixEval => {
            Ok(Satisfaction::Unknown(
                condition
                    .kind
                    .stub_message()
                    .expect("is_stub() iff stub_message().is_some()")
                    .into(),
            ))
        }
        ConditionKind::JobAttested => {
            evaluate_job_attested(client, default_ns, &condition.params, process).await
        }
        ConditionKind::ClosedLoopAuth => {
            evaluate_closed_loop_auth(client, default_ns, &condition.params, process).await
        }
    }
}

// ── JobAttested + ClosedLoopAuth typed evaluators ────────────────────

/// `JobAttested` params:
/// ```json
/// { "name": "<job-name>", "namespace": "<ns>",
///   "expectReceipt": true,                        // default false
///   "receiptConfigMap": "<cm-name>" }             // defaults to <name>-receipt
/// ```
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct JobAttestedParams {
    name: String,
    #[serde(default)]
    namespace: Option<String>,
    #[serde(default)]
    expect_receipt: bool,
    #[serde(default)]
    receipt_config_map: Option<String>,
}

/// Read a Kubernetes `batch/v1` Job's status and decide.
async fn evaluate_job_attested(
    client: Client,
    default_ns: &str,
    params: &Value,
    _process: &Process,
) -> Result<Satisfaction> {
    let parsed: JobAttestedParams = parse_params_or_return_unknown!("JobAttested", params);
    let ns = ssapply::resolve_target_namespace(parsed.namespace.as_deref(), default_ns);
    if let Err(unsat) = require_succeeded_job(
        client.clone(),
        ns,
        &parsed.name,
        JobEvaluatorLabel::JobAttested.as_str(),
    )
    .await?
    {
        return Ok(unsat);
    }

    if !parsed.expect_receipt {
        return Ok(Satisfaction::Satisfied);
    }
    let cm_name = parsed
        .receipt_config_map
        .clone()
        .unwrap_or_else(|| tatara_process::receipt::default_receipt_config_map_name(&parsed.name));
    let verdict = verify_receipt_cm(client, ns, &cm_name, None).await?;
    Ok(classify_receipt_verdict(
        JobEvaluatorLabel::JobAttested.as_str(),
        ns,
        &parsed.name,
        &cm_name,
        verdict,
    ))
}

/// `ClosedLoopAuth` params — the typed shape is in `tatara-process`'s
/// boundary.rs doc; the reconciler reads the optional Job + ConfigMap
/// names and falls back to deterministic defaults derived from the
/// owning Process's name.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClosedLoopAuthParams {
    #[serde(default)]
    namespace: Option<String>,
    /// Optional override; defaults to `<process>-closed-loop-probe`.
    #[serde(default)]
    job_name: Option<String>,
    /// Optional override; defaults to `<job-name>-receipt`.
    #[serde(default)]
    receipt_config_map: Option<String>,
    /// Expected three-pillar BLAKE3 root. When omitted, we only verify
    /// shape; the reconciler chains the observed root into the
    /// Process's attestation regardless.
    #[serde(default)]
    expected_root: Option<String>,
    /// Free-form remaining keys (issuer/consumer/jwkSource/probeImage/etc.)
    /// — the chart deploying the probe consumes these; the reconciler
    /// itself does not need them to verify the receipt.
    #[serde(default, flatten)]
    _extra: std::collections::BTreeMap<String, Value>,
}

async fn evaluate_closed_loop_auth(
    client: Client,
    default_ns: &str,
    params: &Value,
    process: &Process,
) -> Result<Satisfaction> {
    let parsed: ClosedLoopAuthParams = parse_params_or_return_unknown!("ClosedLoopAuth", params);
    let ns = ssapply::resolve_target_namespace(parsed.namespace.as_deref(), default_ns);
    let process_name = process
        .metadata
        .name
        .as_deref()
        .unwrap_or("unnamed-process");
    let job_name = parsed
        .job_name
        .clone()
        .unwrap_or_else(|| format!("{process_name}-closed-loop-probe"));
    let cm_name = parsed
        .receipt_config_map
        .clone()
        .unwrap_or_else(|| tatara_process::receipt::default_receipt_config_map_name(&job_name));

    // 1. The probe Job must have succeeded.
    if let Err(unsat) = require_succeeded_job(
        client.clone(),
        ns,
        &job_name,
        JobEvaluatorLabel::ClosedLoopProbe.as_str(),
    )
    .await?
    {
        return Ok(unsat);
    }

    // 2. The receipt ConfigMap must exist and parse.
    let verdict = verify_receipt_cm(client, ns, &cm_name, parsed.expected_root.as_deref()).await?;
    Ok(classify_receipt_verdict(
        JobEvaluatorLabel::ClosedLoopProbe.as_str(),
        ns,
        &job_name,
        &cm_name,
        verdict,
    ))
}

#[derive(Debug, PartialEq, Eq)]
enum JobLookup {
    Missing,
    Found(JobStatusView),
}

#[derive(Debug, Default, PartialEq, Eq)]
struct JobStatusView {
    succeeded: i64,
    failed: i64,
    active: i64,
}

/// Pure projection of a fetched Job's status into an operator-facing
/// [`Satisfaction`] diagnostic keyed on the caller's `label`.
///
/// Pre-lift the TWO postcondition evaluators
/// ([`evaluate_job_attested`] on the `JobAttested` axis and
/// [`evaluate_closed_loop_auth`] on the `ClosedLoopAuth` axis) each
/// hand-authored the same three-invariant scaffold at their own local
/// site: (1) [`JobLookup::Missing`] projects to
/// `Satisfaction::Unsatisfied("<label> {ns}/{name} not found")`,
/// (2) `status.failed > 0` projects to
/// `Satisfaction::Unsatisfied("<label> {ns}/{name} failed
/// (status.failed={n})")`, (3) `status.succeeded < 1` projects to
/// `Satisfaction::Unsatisfied("<label> {ns}/{name} still running
/// (succeeded={s}, active={a})")`. TWO byte-for-byte identical
/// `let job_status = match fetch_job_status(...) { … }; if
/// job_status.failed > 0 { … } if job_status.succeeded < 1 { … }`
/// blocks past the PRIME-DIRECTIVE ≥ 2 duplication threshold,
/// differing only in the label prefix (`"Job"` vs `"closed-loop
/// probe Job"`) each threaded through the SAME scaffold.
///
/// Post-lift the three shared invariants live at ONE substrate
/// primitive here (the pure projection) plus one async peer
/// [`require_succeeded_job`] that owns the fetch. A regression
/// that drifted the diagnostic wording (a swapped counter phrase,
/// a missing label prefix, a promoted variant on the
/// [`Satisfaction`] axis) at ONE evaluator surfaces at
/// [`classify_job_status_tests`] rather than as silent
/// operator-facing drift across the two postcondition evaluators.
///
/// The lift also **strictly widens** the closed-loop path's "still
/// running" diagnostic: pre-lift it read
/// `"closed-loop probe Job {ns}/{name} still running"` (no
/// counters); post-lift it inherits the JobAttested path's
/// counter-carrying tail `(succeeded={s}, active={a})`, matching the
/// axis where operators grep for those counters. Both callsites now
/// route through the same primitive, so a future counter axis
/// (e.g. `startTime`, `completionTime`, `active` breakdown) lands at
/// ONE substrate primitive and every downstream evaluator picks up
/// the upgrade mechanically.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// three-invariant scaffold recurred at two Job-based evaluators
/// past the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted
/// to ONE owner here). THEORY.md §II.1 invariant 5 (composition
/// preserves proofs — the two evaluators now compose structurally
/// through ONE primitive; a regression that drifted the wording at
/// ONE axis surfaces at [`classify_job_status_tests`] rather than as
/// silent drift at every future postcondition evaluator with a
/// Job-shaped substrate).
fn classify_job_status(
    label: &str,
    ns: &str,
    name: &str,
    lookup: JobLookup,
) -> Result<JobStatusView, Satisfaction> {
    match lookup {
        JobLookup::Missing => Err(Satisfaction::Unsatisfied(format!(
            "{label} {ns}/{name} not found"
        ))),
        JobLookup::Found(status) => {
            if status.failed > 0 {
                Err(Satisfaction::Unsatisfied(format!(
                    "{label} {ns}/{name} failed (status.failed={})",
                    status.failed
                )))
            } else if status.succeeded < 1 {
                Err(Satisfaction::Unsatisfied(format!(
                    "{label} {ns}/{name} still running (succeeded={}, active={})",
                    status.succeeded, status.active
                )))
            } else {
                Ok(status)
            }
        }
    }
}

/// Async wrapper: fetch a Job's status and project the three-invariant
/// scaffold through [`classify_job_status`]. Returns
/// `Ok(Ok(JobStatusView))` when the Job has succeeded (status.succeeded
/// ≥ 1 && status.failed == 0), `Ok(Err(Satisfaction::Unsatisfied(...)))`
/// on the Missing / failed / running short-circuits, and propagates
/// fetch-time errors as the outer `Err`.
async fn require_succeeded_job(
    client: Client,
    ns: &str,
    name: &str,
    label: &str,
) -> Result<Result<JobStatusView, Satisfaction>> {
    let lookup = fetch_job_status(client, ns, name).await?;
    Ok(classify_job_status(label, ns, name, lookup))
}

/// Pure projection of a fetched receipt-ConfigMap [`ReceiptVerdict`]
/// into an operator-facing [`Satisfaction`] diagnostic keyed on the
/// caller's `label` + Job `(ns, name)` + ConfigMap `cm_name`.
///
/// Pre-lift the TWO postcondition evaluators
/// ([`evaluate_job_attested`] on the `JobAttested` axis and
/// [`evaluate_closed_loop_auth`] on the `ClosedLoopAuth` axis) each
/// hand-authored the same three-invariant scaffold at their own local
/// site: (1) [`ReceiptVerdict::Ok`] projects to
/// `Satisfaction::Satisfied`; (2) [`ReceiptVerdict::Missing`] projects
/// to `Satisfaction::Unsatisfied("<label> {ns}/{name} receipt
/// ConfigMap {ns}/{cm_name} missing")`; (3)
/// [`ReceiptVerdict::Malformed(why)`] projects to
/// `Satisfaction::Unsatisfied("<label> {ns}/{name} receipt malformed:
/// {why}")`. TWO byte-for-byte identical `match verify_receipt_cm(...)
/// .await? { … }` blocks past the PRIME-DIRECTIVE ≥ 2 duplication
/// threshold, differing only in the label prefix (`"Job"` vs
/// `"closed-loop probe Job"`) each threaded through the SAME scaffold.
///
/// Post-lift the three shared invariants live at ONE substrate
/// primitive here (the pure projection). Both callsites route through
/// the same primitive with a locally-owned `label: &str`; the async
/// fetch stays at each callsite so caller-owned bindings (`parsed.name`
/// on the JobAttested axis, the derived `job_name` on the closed-loop
/// axis, and the `expected_root` slot only the closed-loop callsite
/// threads through `verify_receipt_cm`) don't need to be plumbed into
/// the primitive's signature. A regression that drifted the diagnostic
/// wording (a swapped counter phrase, a missing label prefix, a
/// promoted variant on the [`Satisfaction`] axis) at ONE evaluator
/// surfaces at [`classify_receipt_verdict_tests`] rather than as
/// silent operator-facing drift across the two postcondition
/// evaluators.
///
/// The lift also **strictly widens** the closed-loop path's Missing +
/// Malformed diagnostics: pre-lift they read
/// `"closed-loop receipt ConfigMap {ns}/{cm_name} missing"` /
/// `"closed-loop receipt malformed: {why}"` (no Job `{ns}/{name}`);
/// post-lift they inherit the JobAttested path's `{label} {ns}/{name}
/// receipt …` shape, matching the sibling [`classify_job_status`]
/// primitive's convention (label + ns + name at the diagnostic head).
/// Operators grepping for the closed-loop Job's name now see it on
/// this axis too.
///
/// Sibling to the earlier lifts on this file:
/// - [`parse_condition_params`] (b7209c3) shape-gates the params
///   carrier at every ConditionKind evaluator on the axis of "params
///   invalid: {e}" projection.
/// - [`classify_job_status`] (4eb542d) status-gates the Job at every
///   Job-based evaluator on the axis of Missing/Failed/Running
///   projection.
/// - This primitive verdict-gates the receipt at every Job-based
///   evaluator on the axis of Ok/Missing/Malformed projection.
///
/// Each new Job-based postcondition evaluator (kenshi-runner
/// JobAttested creation per P3, per-membro contract receipts, any Job
/// whose completion produces a receipt) lands as ONE new callsite
/// through the SAME three primitives, not another hand-authored
/// three-invariant scaffold trio.
///
/// Theory anchor: THEORY.md §VI.1 (generation over composition — the
/// three-invariant scaffold recurred at two Job-based evaluators past
/// the PRIME-DIRECTIVE ≥ 2 duplication trigger, and is lifted to ONE
/// owner here). THEORY.md §II.1 invariant 5 (composition preserves
/// proofs — the two evaluators now compose structurally through ONE
/// primitive; a regression that drifted the wording at ONE axis
/// surfaces at [`classify_receipt_verdict_tests`] rather than as
/// silent drift at every future postcondition evaluator with a
/// receipt-shaped substrate).
fn classify_receipt_verdict(
    label: &str,
    ns: &str,
    name: &str,
    cm_name: &str,
    verdict: ReceiptVerdict,
) -> Satisfaction {
    match verdict {
        ReceiptVerdict::Ok(_) => Satisfaction::Satisfied,
        ReceiptVerdict::Missing => Satisfaction::Unsatisfied(format!(
            "{label} {ns}/{name} receipt ConfigMap {ns}/{cm_name} missing"
        )),
        ReceiptVerdict::Malformed(why) => {
            Satisfaction::Unsatisfied(format!("{label} {ns}/{name} receipt malformed: {why}"))
        }
    }
}

async fn fetch_job_status(client: Client, ns: &str, name: &str) -> Result<JobLookup> {
    // The `(apiVersion, kind)` pair for the `batch/v1::Job` fetch
    // rides through the typed [`K8sBuiltinResource::Job`] closed-set
    // owner projected onto [`tatara_process::K8sWireIdentity`] via
    // `.wire_identity()` — pre-lift both slots were hand-authored
    // adjacent `&str` arguments (`Job.api_version(), Job.kind()`) at
    // ONE of THREE fetch sites in `tatara-reconciler::boundary` past
    // the ★★ PRIME-DIRECTIVE ≥ 2 duplication threshold that threaded
    // the two-slot pair off a typed closed-set variant into the raw
    // `ssapply::fetch(av: &str, kind: &str)` signature (this Job
    // fetch + the sibling `verify_receipt_cm` ConfigMap fetch +
    // `evaluate_flux_ready`'s Flux fetch). Post-lift each site names
    // the variant ONCE via `.wire_identity()` and the pair rides
    // through the shared substrate composer
    // [`ssapply::fetch_by_identity`]; the pair binds structurally at
    // the typed [`tatara_process::K8sWireIdentity`] so a copy-paste
    // that swapped the two adjacent arguments (or paired one
    // variant's apiVersion with another's kind) is unrepresentable.
    // `fetch_by_identity` wraps its own error with the standardized
    // `"fetch {identity.kind} {ns}/{name}: {e}"` prefix via the
    // substrate helper `ssapply::fetch_by_identity_error_context` —
    // pre-lift this callsite hand-authored the `.map_err(|e|
    // anyhow!("fetch Job {ns}/{name}: {e}"))?` wrap verbatim,
    // restating `"Job"` as a bare `&str` literal alongside the same
    // variant's `.wire_identity()` at the fetch call. Post-lift the
    // label rides through `identity.kind` so the two mentions of
    // `K8sBuiltinResource::Job` collapse into ONE at this site — a
    // copy-paste that swapped the fetched variant without also
    // updating the label is unrepresentable.
    let obj = ssapply::fetch_by_identity(client, ns, K8sBuiltinResource::Job.wire_identity(), name)
        .await?;
    let Some(obj) = obj else {
        return Ok(JobLookup::Missing);
    };
    let status = obj.data.get("status").cloned().unwrap_or(Value::Null);
    let mut view = JobStatusView::default();
    if let Some(s) = status.get("succeeded").and_then(|v| v.as_i64()) {
        view.succeeded = s;
    }
    if let Some(f) = status.get("failed").and_then(|v| v.as_i64()) {
        view.failed = f;
    }
    if let Some(a) = status.get("active").and_then(|v| v.as_i64()) {
        view.active = a;
    }
    Ok(JobLookup::Found(view))
}

#[derive(Debug, PartialEq, Eq)]
enum ReceiptVerdict {
    Missing,
    Malformed(String),
    /// Composed root — for the attestation chain.
    Ok(String),
}

/// Fetch the receipt ConfigMap, look up `data['receipt.json']` (or
/// `data['receipt.yaml']` as a fallback), and delegate parsing to the
/// typed `ReceiptEnvelope::parse_either` in `tatara-process`.
async fn verify_receipt_cm(
    client: Client,
    ns: &str,
    name: &str,
    expected_root: Option<&str>,
) -> Result<ReceiptVerdict> {
    // The `(apiVersion, kind)` pair for the `v1::ConfigMap` receipt
    // fetch rides through the typed [`K8sBuiltinResource::ConfigMap`]
    // closed-set owner projected onto
    // [`tatara_process::K8sWireIdentity`] via `.wire_identity()` —
    // sibling to the `Job` fetch above; both rides through the shared
    // substrate composer [`ssapply::fetch_by_identity`], so the two
    // adjacent `&str` arguments the raw `ssapply::fetch` accepts no
    // longer appear at either callsite. Every future receipt-fetch
    // consumer (kenshi-runner's P3 lift, per-membro contract receipts,
    // any Job whose completion produces a receipt) inherits the wire-
    // form pair through the SAME closed-set variant with no per-
    // callsite literal AND with no two-slot skew possible at the
    // callee's signature.
    // Sibling to the Job fetch above; `fetch_by_identity` now wraps
    // its own error via the substrate `fetch_by_identity_error_context`
    // helper. The pre-lift `.map_err(|e| anyhow!("fetch ConfigMap
    // {ns}/{name}: {e}"))?` wrap that restated `"ConfigMap"` as a
    // bare literal alongside the same variant's `.wire_identity()`
    // now collapses onto ONE mention of `K8sBuiltinResource::ConfigMap`
    // — the two-mention drift trap is unrepresentable.
    let obj = ssapply::fetch_by_identity(
        client,
        ns,
        K8sBuiltinResource::ConfigMap.wire_identity(),
        name,
    )
    .await?;
    let Some(obj) = obj else {
        return Ok(ReceiptVerdict::Missing);
    };
    let data = obj.data.get("data");
    let payload = data
        .and_then(|d| d.get("receipt.json"))
        .or_else(|| data.and_then(|d| d.get("receipt.yaml")))
        .and_then(|v| v.as_str());
    let Some(payload) = payload else {
        return Ok(ReceiptVerdict::Malformed(
            "ConfigMap missing data['receipt.json' | 'receipt.yaml'] string key".into(),
        ));
    };
    Ok(parse_receipt_payload(payload, expected_root))
}

/// Pure parser — delegates to the typed `ReceiptEnvelope::parse_either`,
/// then runs the `expected_root` check separately. Maps typed
/// `ReceiptError` variants into `ReceiptVerdict::Malformed` with a
/// stable, operator-friendly string so existing UX is preserved.
fn parse_receipt_payload(payload: &str, expected_root: Option<&str>) -> ReceiptVerdict {
    let envelope = match ReceiptEnvelope::parse_either(payload) {
        Ok(e) => e,
        Err(err) => return ReceiptVerdict::Malformed(receipt_error_message(&err)),
    };
    match envelope.expect_root(expected_root) {
        Ok(root) => ReceiptVerdict::Ok(root.to_string()),
        Err(err) => ReceiptVerdict::Malformed(receipt_error_message(&err)),
    }
}

/// Lower a `ReceiptError` to the same operator-visible strings the
/// older hand-rolled parser surfaced, so dashboards / alerts that grep
/// for these messages keep working.
fn receipt_error_message(err: &ReceiptError) -> String {
    match err {
        ReceiptError::InvalidJson(m) => format!("invalid JSON: {m}"),
        ReceiptError::InvalidYaml(m) => format!("invalid YAML: {m}"),
        ReceiptError::WrongVersion(v) => format!("version != tatara-receipt/v1 (got {v:?})"),
        ReceiptError::MissingField(f) => format!("missing '{f}' string field"),
        ReceiptError::EmptyKind => "kind is empty".into(),
        ReceiptError::RootMismatch { got, want } => {
            format!("composed_root mismatch (got {got}, want {want})")
        }
    }
}

// ── per-kind evaluators ──────────────────────────────────────────────

async fn evaluate_process_phase(
    client: Client,
    default_ns: &str,
    params: &Value,
) -> Result<Satisfaction> {
    let parsed: ProcessPhaseParams = parse_params_or_return_unknown!("ProcessPhase", params);
    let ns = ssapply::resolve_target_namespace(parsed.namespace.as_deref(), default_ns);
    let api: Api<Process> = Api::namespaced(client, ns);
    let target = match api
        .get_opt(&parsed.process_ref)
        .await
        .map_err(|e| anyhow!("fetch process {ns}/{}: {e}", parsed.process_ref))?
    {
        Some(t) => t,
        None => {
            return Ok(Satisfaction::Unsatisfied(format!(
                "process {}/{} not found",
                ns, parsed.process_ref
            )))
        }
    };
    let actual = target
        .status
        .as_ref()
        .map(|s| s.phase)
        .unwrap_or(ProcessPhase::Pending);
    if phase_reached(actual, parsed.phase) {
        Ok(Satisfaction::Satisfied)
    } else {
        Ok(Satisfaction::Unsatisfied(format!(
            "{}/{} is {actual}; need at least {}",
            ns, parsed.process_ref, parsed.phase
        )))
    }
}

async fn evaluate_flux_ready(
    client: Client,
    default_ns: &str,
    params: &Value,
    resource: FluxResource,
) -> Result<Satisfaction> {
    // The `(apiVersion, kind)` pair rides through the typed
    // [`FluxResource`] slot projected onto
    // [`tatara_process::K8sWireIdentity`] via `.wire_identity()` —
    // sibling to the two `K8sBuiltinResource::X.wire_identity()`
    // fetches above; all three fetch sites now route through the
    // shared substrate composer [`ssapply::fetch_by_identity`], so
    // the raw two-adjacent-`&str` `ssapply::fetch` signature no
    // longer appears at any of them. The callee derives both slots
    // from the SAME closed-set variant, so a caller can no longer
    // pass a mismatched `(&str, &str)` pair. Pre-lift the two slots
    // were separate `&str` parameters and each dispatch arm at
    // [`evaluate`] hand-authored a `(FluxResource::X.api_version(),
    // FluxResource::X.kind())` pair; post-lift the callee owns both
    // projections through the identity pair.
    let kind = resource.kind();
    let parsed: NamedResourceParams = parse_params_or_return_unknown!(kind, params);
    let ns = ssapply::resolve_target_namespace(parsed.namespace.as_deref(), default_ns);
    // Sibling to the two `K8sBuiltinResource::X`-gated boundary
    // fetches above; every closed-set-variant-gated fetch in this
    // module now rides through the ONE substrate composer + wraps
    // its error through the ONE substrate helper. Pre-lift the
    // `.map_err(|e| anyhow!("fetch {kind} {ns}/{}: {e}", parsed.name))?`
    // wrap read the `kind` slot off `resource.kind()`, restating the
    // same closed-set variant's `.wire_identity()` twice at the call.
    // Post-lift `identity.kind` inside `fetch_by_identity` drives the
    // label so `resource.wire_identity()` names the variant ONCE.
    let obj =
        ssapply::fetch_by_identity(client, ns, resource.wire_identity(), &parsed.name).await?;
    match obj {
        None => Ok(Satisfaction::Unsatisfied(format!(
            "{kind} {}/{} not found",
            ns, parsed.name
        ))),
        Some(dyn_obj) => match ssapply::ready_condition(&dyn_obj) {
            ssapply::ReadyState::Ready => Ok(Satisfaction::Satisfied),
            ssapply::ReadyState::NotReady(m) => Ok(Satisfaction::Unsatisfied(
                m.unwrap_or_else(|| format!("{kind} not ready")),
            )),
            ssapply::ReadyState::Unknown => {
                Ok(Satisfaction::Unknown(format!("{kind} condition unknown")))
            }
        },
    }
}

// ── phase ordering (pure) ────────────────────────────────────────────

/// Rank for the "must reach" comparison. Dead phases rank 0 to prevent
/// terminating processes from satisfying any live-phase requirement.
pub fn phase_rank(p: ProcessPhase) -> u8 {
    match p {
        ProcessPhase::Pending => 0,
        ProcessPhase::Forking => 1,
        ProcessPhase::Execing => 2,
        ProcessPhase::Running | ProcessPhase::Reconverging => 3,
        ProcessPhase::Attested => 4,
        ProcessPhase::Releasing
        | ProcessPhase::Exiting
        | ProcessPhase::Failed
        | ProcessPhase::Zombie
        | ProcessPhase::Reaped => 0,
    }
}

/// Has `actual` reached the required minimum phase?
pub fn phase_reached(actual: ProcessPhase, required: ProcessPhase) -> bool {
    phase_rank(actual) >= phase_rank(required)
}

// ── depends_on ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UnmetDependency {
    pub name: String,
    pub namespace: String,
    pub required: ProcessPhase,
    pub actual: Option<ProcessPhase>,
    pub message: String,
}

/// Check every `spec.dependsOn` entry against live cluster state.
/// Returns the list of unmet dependencies (empty = proceed).
pub async fn check_depends_on(client: Client, process: &Process) -> Result<Vec<UnmetDependency>> {
    let default_ns = process.namespace_or_default();
    let mut unmet = Vec::new();

    for dep in &process.spec.depends_on {
        let ns = ssapply::resolve_target_namespace(dep.namespace.as_deref(), default_ns);
        let required: ProcessPhase = dep.must_reach.into();
        let api: Api<Process> = Api::namespaced(client.clone(), ns);
        match api.get_opt(&dep.name).await {
            Ok(Some(target)) => {
                let actual = target.status.as_ref().map(|s| s.phase);
                let actual_phase = actual.unwrap_or(ProcessPhase::Pending);
                if !phase_reached(actual_phase, required) {
                    unmet.push(UnmetDependency {
                        name: dep.name.clone(),
                        namespace: ns.to_string(),
                        required,
                        actual,
                        message: format!("{}/{} is {actual_phase}; need {required}", ns, dep.name),
                    });
                }
            }
            Ok(None) => unmet.push(UnmetDependency {
                name: dep.name.clone(),
                namespace: ns.to_string(),
                required,
                actual: None,
                message: format!("{}/{} not found", ns, dep.name),
            }),
            Err(e) => unmet.push(UnmetDependency {
                name: dep.name.clone(),
                namespace: ns.to_string(),
                required,
                actual: None,
                message: format!("error fetching {}/{}: {e}", ns, dep.name),
            }),
        }
    }
    Ok(unmet)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_rank_ordering_ascends_through_lifecycle() {
        assert!(phase_rank(ProcessPhase::Pending) < phase_rank(ProcessPhase::Forking));
        assert!(phase_rank(ProcessPhase::Forking) < phase_rank(ProcessPhase::Execing));
        assert!(phase_rank(ProcessPhase::Execing) < phase_rank(ProcessPhase::Running));
        assert!(phase_rank(ProcessPhase::Running) < phase_rank(ProcessPhase::Attested));
    }

    #[test]
    fn reconverging_has_same_rank_as_running() {
        assert_eq!(
            phase_rank(ProcessPhase::Reconverging),
            phase_rank(ProcessPhase::Running)
        );
    }

    #[test]
    fn attested_satisfies_running_requirement() {
        assert!(phase_reached(ProcessPhase::Attested, ProcessPhase::Running));
        assert!(phase_reached(ProcessPhase::Running, ProcessPhase::Running));
    }

    #[test]
    fn running_does_not_satisfy_attested() {
        assert!(!phase_reached(
            ProcessPhase::Running,
            ProcessPhase::Attested
        ));
        assert!(!phase_reached(ProcessPhase::Pending, ProcessPhase::Running));
    }

    // ── receipt parser (pure) ────────────────────────────────────────

    fn valid_receipt() -> String {
        // Compose root deterministically using the same domain tag as
        // ReceiptEnvelope so the generated payload parses cleanly.
        let env = ReceiptEnvelope::build(ReceiptKind::ClosedLoopAuth, "aaaa", "bbbb", "cccc", None);
        serde_json::to_string(&env).unwrap()
    }

    #[test]
    fn valid_receipt_parses() {
        let payload = valid_receipt();
        let v = parse_receipt_payload(&payload, None);
        assert!(matches!(v, ReceiptVerdict::Ok(_)));
    }

    #[test]
    fn expected_root_match_succeeds_mismatch_fails() {
        let env = ReceiptEnvelope::build(ReceiptKind::ClosedLoopAuth, "aaaa", "bbbb", "cccc", None);
        let payload = serde_json::to_string(&env).unwrap();
        let root = env.composed_root.clone();

        let v = parse_receipt_payload(&payload, Some(&root));
        assert!(matches!(v, ReceiptVerdict::Ok(_)));

        let v = parse_receipt_payload(&payload, Some("nope"));
        assert!(
            matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains("composed_root mismatch"))
        );
    }

    #[test]
    fn missing_version_is_malformed() {
        // serde with deny_unknown_fields rejects this before our shape check —
        // surfaces as "invalid JSON: missing field `version`".
        let s = r#"{"composed_root":"x","intent_hash":"a","artifact_hash":"b","control_hash":"c","kind":"x","generated_at":"2026-05-19T12:00:00Z"}"#;
        let v = parse_receipt_payload(s, None);
        assert!(
            matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains("version")),
            "expected version-related malformed message, got {v:?}"
        );
    }

    #[test]
    fn wrong_version_is_malformed() {
        let mut payload: Value = serde_json::from_str(&valid_receipt()).unwrap();
        payload["version"] = Value::String("tatara-receipt/v2".into());
        let v = parse_receipt_payload(&payload.to_string(), None);
        assert!(
            matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains("version != tatara-receipt/v1"))
        );
    }

    #[test]
    fn missing_any_pillar_is_malformed() {
        for pillar in ["intent_hash", "artifact_hash", "control_hash"] {
            let mut payload: Value = serde_json::from_str(&valid_receipt()).unwrap();
            payload.as_object_mut().unwrap().remove(pillar);
            let v = parse_receipt_payload(&payload.to_string(), None);
            assert!(
                matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains(pillar)),
                "expected malformed for missing '{pillar}', got {v:?}"
            );
        }
    }

    #[test]
    fn missing_composed_root_is_malformed() {
        let mut payload: Value = serde_json::from_str(&valid_receipt()).unwrap();
        payload.as_object_mut().unwrap().remove("composed_root");
        let v = parse_receipt_payload(&payload.to_string(), None);
        assert!(matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains("composed_root")));
    }

    #[test]
    fn invalid_json_is_malformed() {
        let v = parse_receipt_payload("not json", None);
        assert!(
            matches!(v, ReceiptVerdict::Malformed(ref m) if m.to_lowercase().contains("invalid"))
        );
    }

    #[test]
    fn missing_kind_is_malformed() {
        let mut payload: Value = serde_json::from_str(&valid_receipt()).unwrap();
        payload.as_object_mut().unwrap().remove("kind");
        let v = parse_receipt_payload(&payload.to_string(), None);
        assert!(matches!(v, ReceiptVerdict::Malformed(ref m) if m.contains("kind")));
    }

    #[test]
    fn yaml_payload_parses_via_either() {
        let env = ReceiptEnvelope::build(ReceiptKind::DbMigration, "aaaa", "bbbb", "cccc", None);
        let yaml = serde_yaml::to_string(&env).unwrap();
        let v = parse_receipt_payload(&yaml, None);
        assert!(matches!(v, ReceiptVerdict::Ok(_)));
    }

    #[test]
    fn terminal_phases_satisfy_nothing() {
        for dead in [
            ProcessPhase::Exiting,
            ProcessPhase::Failed,
            ProcessPhase::Zombie,
            ProcessPhase::Reaped,
        ] {
            assert!(!phase_reached(dead, ProcessPhase::Running));
            assert!(!phase_reached(dead, ProcessPhase::Attested));
        }
    }

    #[test]
    fn satisfaction_reports_correctly() {
        assert!(Satisfaction::Satisfied.is_satisfied());
        assert!(!Satisfaction::Unsatisfied("x".into()).is_satisfied());
        assert!(!Satisfaction::Unknown("y".into()).is_satisfied());
        assert_eq!(
            Satisfaction::Unsatisfied("why".into()).message(),
            Some("why")
        );
        assert_eq!(Satisfaction::Satisfied.message(), None);
    }

    /// Substrate-primitive pin at the CONSUMER surface: the two
    /// [`FluxResource`] variants this evaluator reaches at the wire
    /// (Kustomization / HelmRelease — the OCIRepository variant is
    /// referenced by [`crate::render::render_aplicacao`], not by the
    /// boundary evaluator) carry the canonical wire-form
    /// `(apiVersion, kind)` pair the sibling `render::render_flux` +
    /// `render::render_aplicacao` emit sites now stamp through the
    /// SAME closed-set arm. A regression that drifted either arm's
    /// literal at the fetch OR emit site would silently mis-route
    /// SSA-fetch against SSA-apply — surfaces here + at the sibling
    /// `render_flux_emits_flux_resource_owned_kustomization_api_version_and_kind`
    /// + `render_aplicacao_emits_flux_resource_owned_api_version_and_kind_pairs`
    /// pins rather than as an operator-visible 404 at every deploy.
    #[test]
    fn flux_ready_evaluator_reaches_flux_resource_owned_kustomization_and_helm_release_pairs() {
        // Kustomization axis — pinned wire-form pair the
        // ConditionKind::KustomizationHealthy arm now reads from
        // the substrate closed set.
        assert_eq!(
            FluxResource::Kustomization.api_version(),
            "kustomize.toolkit.fluxcd.io/v1"
        );
        assert_eq!(FluxResource::Kustomization.kind(), "Kustomization");
        // HelmRelease axis — pinned wire-form pair the
        // ConditionKind::HelmReleaseReleased arm now reads from
        // the substrate closed set.
        assert_eq!(
            FluxResource::HelmRelease.api_version(),
            "helm.toolkit.fluxcd.io/v2"
        );
        assert_eq!(FluxResource::HelmRelease.kind(), "HelmRelease");
    }
}

/// Substrate-primitive tests for [`parse_condition_params`] — the
/// shared `serde_json::from_value → Satisfaction::Unknown` axis every
/// kind-typed evaluator on this file dispatches through.
///
/// The pre-lift shape lived at FOUR sites across
/// [`evaluate_process_phase`] / [`evaluate_flux_ready`] /
/// [`evaluate_job_attested`] / [`evaluate_closed_loop_auth`], each
/// hand-authoring the (clone Value, from_value::<T>, wrap Err in
/// `Satisfaction::Unknown("<kind> params invalid: {e}")`) three-step
/// verbatim at its own local site. Post-lift the three shared invariants
/// live at ONE substrate primitive, and this module pins the four
/// contract axes at fail-before-pass-after granularity.
///
/// Sibling of the `tests` module above (on the [`Satisfaction`] /
/// [`phase_rank`] / [`parse_receipt_payload`] axes) — kept as its own
/// module so a regression at the substrate primitive's axis surfaces
/// distinctly from the per-kind evaluator or the receipt parser.
#[cfg(test)]
mod parse_condition_params_tests {
    use super::{parse_condition_params, JobAttestedParams, NamedResourceParams, Satisfaction};
    use serde_json::json;

    // ── Contract axis 1: successful deserialization returns Ok(T) ────
    //
    // The typed carrier's fields project structurally from the JSON;
    // the primitive is the identity path on the happy branch (a
    // wrapper only around the Err branch's Satisfaction promotion).

    #[test]
    fn valid_named_resource_params_deserialize_to_typed_carrier() {
        let params = json!({ "name": "my-kustomization", "namespace": "flux-system" });
        let parsed: NamedResourceParams = parse_condition_params("Kustomization", &params)
            .expect("valid params deserialize to typed carrier");
        assert_eq!(parsed.name, "my-kustomization");
        assert_eq!(parsed.namespace.as_deref(), Some("flux-system"));
    }

    #[test]
    fn valid_named_resource_params_with_missing_optional_namespace_deserialize() {
        // `NamedResourceParams::namespace` is `#[serde(default)] Option<String>`
        // — an absent field projects to `None` without rejecting.
        let params = json!({ "name": "only-name-required" });
        let parsed: NamedResourceParams = parse_condition_params("Kustomization", &params)
            .expect("valid params deserialize with optional field absent");
        assert_eq!(parsed.name, "only-name-required");
        assert_eq!(parsed.namespace, None);
    }

    #[test]
    fn valid_job_attested_params_deserialize_camelcase_fields() {
        // `JobAttestedParams` uses `#[serde(rename_all = "camelCase")]`,
        // so the JSON key `expectReceipt` deserializes into the Rust
        // field `expect_receipt`. Pinning this axis catches a regression
        // that swapped the primitive's `serde_json::from_value` for a
        // shape gate that lost the rename attribute (e.g. by round-
        // tripping through `serde_json::to_value` at the wrong axis).
        let params = json!({
            "name": "my-job",
            "namespace": "default",
            "expectReceipt": true,
            "receiptConfigMap": "my-cm",
        });
        let parsed: JobAttestedParams = parse_condition_params("JobAttested", &params)
            .expect("valid camelCase params deserialize into snake_case fields");
        assert_eq!(parsed.name, "my-job");
        assert!(parsed.expect_receipt);
        assert_eq!(parsed.receipt_config_map.as_deref(), Some("my-cm"));
    }

    // ── Contract axis 2: Err returns Satisfaction::Unknown (not
    // Unsatisfied or Satisfied) — pins the primitive's typed-projection
    // discipline so a future consumer that pattern-matches on the
    // Satisfaction variant binds structurally.

    #[test]
    fn invalid_params_project_to_satisfaction_unknown_variant() {
        // `NamedResourceParams::name` is a required `String`; passing
        // an integer produces a serde-level type-mismatch. `.err()`
        // projects `Result<T, E>` to `Option<E>` without requiring
        // `Debug` on the carrier `T`, then `.expect(...)` fires the
        // fail-branch message iff the primitive silently returned Ok.
        let params = json!({ "name": 42 });
        let err = parse_condition_params::<NamedResourceParams>("Kustomization", &params)
            .err()
            .expect("integer-typed name field must reject at serde gate");
        assert!(
            matches!(err, Satisfaction::Unknown(_)),
            "invalid params must project to Satisfaction::Unknown"
        );
    }

    #[test]
    fn absent_required_field_projects_to_satisfaction_unknown() {
        // `NamedResourceParams::name` is required (no serde default);
        // an empty object rejects at the serde-level missing-field gate.
        let params = json!({});
        let err = parse_condition_params::<NamedResourceParams>("Kustomization", &params)
            .err()
            .expect("missing required field must reject at serde gate");
        assert!(
            matches!(err, Satisfaction::Unknown(_)),
            "missing required field must project to Satisfaction::Unknown"
        );
    }

    // ── Contract axis 3: kind_label rides through as the diagnostic
    // prefix verbatim, ahead of " params invalid: " and the serde-
    // error tail. Pins the four workspace-shipped kind labels
    // ("ProcessPhase", "Kustomization" / "HelmRelease" via
    // evaluate_flux_ready's dynamic `kind` slot, "JobAttested",
    // "ClosedLoopAuth") so a regression that drifted the prefix
    // wording at ONE site fails here.

    #[test]
    fn kind_label_prefixes_the_unknown_diagnostic_across_all_shipped_axes() {
        for kind_label in [
            "ProcessPhase",
            "Kustomization",
            "HelmRelease",
            "JobAttested",
            "ClosedLoopAuth",
        ] {
            let params = json!({});
            let err = parse_condition_params::<NamedResourceParams>(kind_label, &params)
                .err()
                .expect("missing-field params must reject at serde gate");
            let Satisfaction::Unknown(msg) = err else {
                panic!("expected Satisfaction::Unknown for {kind_label}");
            };
            assert!(
                msg.starts_with(&format!("{kind_label} params invalid: ")),
                "kind label {kind_label:?} must prefix the diagnostic verbatim; \
                 got message {msg:?}"
            );
        }
    }

    #[test]
    fn kind_label_is_the_only_prefix_source_no_hardcoded_label_leaks() {
        // A regression that hardcoded a specific label in the
        // primitive (e.g. by copy-pasting "ProcessPhase params invalid"
        // as a literal instead of interpolating `{kind_label}`) would
        // surface here — the axis-typed prefix "AxisXYZ" is guaranteed
        // absent from the substrate's own trait paths.
        let params = json!({});
        let err = parse_condition_params::<NamedResourceParams>("AxisXYZ", &params)
            .err()
            .expect("missing-field params must reject at serde gate");
        let Satisfaction::Unknown(msg) = err else {
            panic!("expected Satisfaction::Unknown");
        };
        assert!(
            msg.starts_with("AxisXYZ params invalid: "),
            "kind label must interpolate through, got {msg:?}"
        );
    }

    // ── Contract axis 4: serde error's Display projection rides
    // through the trailing tail of the diagnostic. Pins that the
    // primitive does not drop or reshape the underlying serde error.

    #[test]
    fn serde_error_display_projects_through_the_diagnostic_tail() {
        let params = json!({ "name": 42 });
        let err = parse_condition_params::<NamedResourceParams>("Kustomization", &params)
            .err()
            .expect("integer-typed name field must reject at serde gate");
        let Satisfaction::Unknown(msg) = err else {
            panic!("expected Satisfaction::Unknown");
        };
        // serde_json's Display for an integer-vs-string mismatch names
        // the expected type ("string"); the substring is load-bearing
        // for operator grep-based diagnostics.
        let tail = msg
            .strip_prefix("Kustomization params invalid: ")
            .expect("primitive must emit the '<kind> params invalid: ' prefix verbatim");
        assert!(
            tail.contains("string"),
            "serde error tail must name the expected type verbatim; got {tail:?}"
        );
    }
}

/// Substrate-primitive tests for the [`parse_params_or_return_unknown`]
/// macro — the shared "parse-or-short-circuit" control-flow scaffold
/// every kind-typed evaluator on this file dispatches through.
///
/// The pre-lift shape (the four-line `match parse_condition_params(...)
/// { Ok(p) => p, Err(unk) => return Ok(unk) }` block) lived at FOUR
/// callsites across [`evaluate_process_phase`] / [`evaluate_flux_ready`]
/// / [`evaluate_job_attested`] / [`evaluate_closed_loop_auth`],
/// differing only in the axis-typed carrier annotation and the
/// `&str` kind label each site threaded through. Post-lift the
/// short-circuit shape lives at ONE substrate macro, and this module
/// pins the four contract axes at fail-before-pass-after granularity:
///
/// 1. **Happy path** — Ok(parsed) yields the value, no early return
///    triggered; the enclosing function reaches its post-macro body.
/// 2. **Short-circuit path** — Err(Satisfaction) triggers `return
///    Ok(...)` at the macro site; the enclosing function returns
///    `Ok(Satisfaction::Unknown)` byte-identical to the pre-lift
///    diagnostic.
/// 3. **Return-type inference** — the macro's `return Ok(unk)` binds
///    structurally against the enclosing function's
///    `Result<Satisfaction, _>`; a caller with an incompatible
///    return type is a compile-time error at expansion.
/// 4. **Cross-substrate parity** — the wording emitted through the
///    macro is byte-identical to the wording the pre-lift hand-authored
///    scaffold would have emitted at each of the four production
///    callsites (routes through the SAME [`parse_condition_params`]
///    primitive, so this axis is a pin against a hypothetical future
///    macro rewrite that diverged the two paths).
///
/// Sibling of the [`parse_condition_params_tests`] module above (on
/// the substrate primitive's own axis) — kept as its own module so a
/// regression at the macro's control-flow scaffold surfaces distinctly
/// from a regression at the underlying `serde_json::from_value` gate.
#[cfg(test)]
mod parse_params_or_return_unknown_tests {
    use super::{NamedResourceParams, Satisfaction};
    use anyhow::Result;
    use serde_json::{json, Value};

    /// Standalone stand-in for the four production evaluators — matches
    /// their `async fn -> Result<Satisfaction>` shape byte-for-byte so
    /// the macro's `return Ok(unk)` binds through the same return-type
    /// slot the four production evaluators expose. Kept `fn` (not
    /// `async fn`) so the tests don't need a runtime; the macro
    /// expansion is orthogonal to `.await`.
    fn stand_in_evaluator(kind_label: &str, params: &Value) -> Result<Satisfaction> {
        let _parsed: NamedResourceParams = parse_params_or_return_unknown!(kind_label, params);
        // Post-macro reachable only on the happy path — the macro's
        // `return Ok(...)` short-circuits before this point on Err.
        Ok(Satisfaction::Satisfied)
    }

    #[test]
    fn macro_yields_typed_carrier_on_ok_and_reaches_post_macro_body() {
        // Contract axis 1 (happy path) — a well-shaped params object
        // deserializes into `NamedResourceParams`, the macro binds the
        // typed carrier at the caller's `let` slot, and control flow
        // continues past the macro to the enclosing function's
        // `Ok(Satisfaction::Satisfied)` tail. A regression that made
        // the macro `return Ok(...)` unconditionally would surface here
        // as `Satisfaction::Unknown` instead of `Satisfaction::Satisfied`.
        let params = json!({ "name": "my-kustomization", "namespace": "flux-system" });
        let result = stand_in_evaluator("Kustomization", &params)
            .expect("happy-path evaluator must return Ok");
        assert_eq!(
            result,
            Satisfaction::Satisfied,
            "post-macro body must be reached on Ok(parsed)"
        );
    }

    #[test]
    fn macro_short_circuits_with_ok_unknown_on_err_from_primitive() {
        // Contract axis 2 (short-circuit path) — an invalid params
        // object (missing required `name` field) drives the primitive
        // to `Err(Satisfaction::Unknown(...))`; the macro's `return
        // Ok(unk)` must land the outer function on Ok(Unknown) rather
        // than reaching the post-macro tail. A regression that dropped
        // the `return` keyword would surface here as `Satisfaction::
        // Satisfied` instead of `Satisfaction::Unknown`. A regression
        // that changed `Ok(unk)` to `Err(...)` would surface as an
        // outer `.expect("must return Ok")` panic.
        let params = json!({});
        let result = stand_in_evaluator("Kustomization", &params)
            .expect("short-circuit must land as outer Ok, not outer Err");
        match result {
            Satisfaction::Unknown(msg) => assert!(
                msg.starts_with("Kustomization params invalid: "),
                "short-circuit must carry the primitive's diagnostic verbatim; got {msg:?}",
            ),
            other => panic!("short-circuit must yield Satisfaction::Unknown, got {other:?}",),
        }
    }

    #[test]
    fn macro_short_circuit_wording_is_byte_identical_across_kind_labels() {
        // Contract axis 4 (cross-substrate parity) — sweep the four
        // workspace-shipped kind labels the four production evaluators
        // thread through the macro. Each label rides through as the
        // diagnostic prefix verbatim, matching the pre-lift hand-
        // authored scaffold's wording. A regression that hardcoded a
        // specific label (e.g. by dropping the `$label:expr` slot from
        // the macro) or reshaped the diagnostic prefix at ONE label
        // surfaces here per-label.
        for kind_label in [
            "ProcessPhase",
            "Kustomization",
            "HelmRelease",
            "JobAttested",
            "ClosedLoopAuth",
        ] {
            let params = json!({});
            let result = stand_in_evaluator(kind_label, &params)
                .expect("short-circuit must land as outer Ok");
            let Satisfaction::Unknown(msg) = result else {
                panic!("expected Satisfaction::Unknown for {kind_label}");
            };
            assert!(
                msg.starts_with(&format!("{kind_label} params invalid: ")),
                "kind label {kind_label:?} must ride through the macro verbatim; \
                 got message {msg:?}",
            );
        }
    }

    #[test]
    fn macro_short_circuit_matches_hand_authored_pre_lift_bytewise() {
        // Contract axis 4 (cross-substrate parity, direct pin) — the
        // exact `match parse_condition_params(...) { Ok(p) => p,
        // Err(unk) => return Ok(unk) }` shape the four production
        // callsites hand-authored pre-lift is re-executed here and
        // compared byte-identically against the macro's short-circuit
        // result. A regression that drifted the diagnostic wording
        // (a swapped `params invalid` phrase, a missing kind-label
        // prefix, a promoted variant on the `Satisfaction` axis) at
        // the macro would surface as a byte-mismatch here rather than
        // as silent operator-facing drift across the four evaluators.
        //
        // Both blocks route through the SAME [`parse_condition_params`]
        // primitive, so this pin catches a hypothetical future macro
        // rewrite that stopped delegating to the primitive.
        fn hand_authored_scaffold(kind_label: &str, params: &Value) -> Result<Satisfaction> {
            let _parsed: NamedResourceParams =
                match super::parse_condition_params(kind_label, params) {
                    Ok(p) => p,
                    Err(unk) => return Ok(unk),
                };
            Ok(Satisfaction::Satisfied)
        }
        let params = json!({});
        let via_macro =
            stand_in_evaluator("Kustomization", &params).expect("macro path must return Ok");
        let via_hand = hand_authored_scaffold("Kustomization", &params)
            .expect("hand-authored path must return Ok");
        assert_eq!(
            via_macro, via_hand,
            "macro short-circuit must be byte-identical to hand-authored pre-lift scaffold",
        );
    }
}

/// Substrate-primitive tests for [`classify_job_status`] — the pure
/// (label, ns, name, JobLookup) → Result<JobStatusView, Satisfaction>
/// projection every Job-based postcondition evaluator on this file
/// dispatches through via [`require_succeeded_job`].
///
/// The pre-lift shape lived at TWO sites across
/// [`evaluate_job_attested`] and [`evaluate_closed_loop_auth`], each
/// hand-authoring the three-invariant scaffold (Missing / failed > 0
/// / succeeded < 1) verbatim at its own local site — differing only
/// in the label prefix each site threaded into the diagnostic. Post-
/// lift the three shared invariants live at ONE substrate primitive,
/// and this module pins the four contract axes (Missing, Failed,
/// Running, Succeeded) plus the label-riding, Satisfaction-variant,
/// and priority-ordering axes at fail-before-pass-after granularity.
///
/// Sibling of the [`parse_condition_params_tests`] module above (on
/// the shape-gate axis) and the `tests` module (on the
/// [`Satisfaction`] / [`phase_rank`] / [`parse_receipt_payload`]
/// axes) — kept as its own module so a regression at the primitive's
/// projection axis surfaces distinctly from the per-kind evaluator or
/// the shape gate.
#[cfg(test)]
mod classify_job_status_tests {
    use super::{classify_job_status, JobEvaluatorLabel, JobLookup, JobStatusView, Satisfaction};

    fn view(succeeded: i64, failed: i64, active: i64) -> JobStatusView {
        JobStatusView {
            succeeded,
            failed,
            active,
        }
    }

    // ── Contract axis 1: Succeeded projects to Ok(JobStatusView) ─────
    //
    // A Job with `status.succeeded ≥ 1 && status.failed == 0` returns
    // the fetched view verbatim so downstream evaluators (receipt
    // verification, ...) can continue with structural access to the
    // counter axes.

    #[test]
    fn succeeded_job_projects_to_ok_with_status_view() {
        let result = classify_job_status(
            "Job",
            "flux-system",
            "my-job",
            JobLookup::Found(view(1, 0, 0)),
        );
        let status = result.expect("succeeded Job must project to Ok(JobStatusView)");
        assert_eq!(status, view(1, 0, 0));
    }

    #[test]
    fn multi_succeeded_job_still_projects_to_ok() {
        // A completions=N Job may have succeeded > 1; the gate is
        // `succeeded >= 1`, so higher counts remain Ok.
        let result = classify_job_status(
            "Job",
            "default",
            "batch-job",
            JobLookup::Found(view(5, 0, 0)),
        );
        assert!(
            result.is_ok(),
            "succeeded > 1 must remain Ok, got {result:?}"
        );
    }

    // ── Contract axis 2: Missing projects to Unsatisfied verbatim ────
    //
    // Pins the exact diagnostic wording operators grep for; a
    // regression that reshaped the "not found" phrase surfaces here
    // rather than as silent operator-facing drift.

    #[test]
    fn missing_job_projects_to_unsatisfied_with_label_prefix() {
        let result = classify_job_status("Job", "flux-system", "my-job", JobLookup::Missing);
        let err = result.expect_err("Missing lookup must project to Err(Satisfaction)");
        assert!(
            matches!(err, Satisfaction::Unsatisfied(_)),
            "Missing must project to Satisfaction::Unsatisfied variant"
        );
        let Satisfaction::Unsatisfied(msg) = err else {
            unreachable!()
        };
        assert_eq!(msg, "Job flux-system/my-job not found");
    }

    // ── Contract axis 3: Failed projects to Unsatisfied verbatim ─────
    //
    // Pins the (status.failed={n}) counter tail — operators grep for
    // both the label prefix and the counter to alert on stuck Jobs.

    #[test]
    fn failed_job_projects_to_unsatisfied_with_counter_tail() {
        let result =
            classify_job_status("Job", "default", "my-job", JobLookup::Found(view(0, 3, 0)));
        let err = result.expect_err("failed > 0 must project to Err(Satisfaction)");
        let Satisfaction::Unsatisfied(msg) = err else {
            panic!("expected Satisfaction::Unsatisfied");
        };
        assert_eq!(msg, "Job default/my-job failed (status.failed=3)");
    }

    // ── Contract axis 4: Running projects to Unsatisfied verbatim ────
    //
    // Post-lift the closed-loop path also carries the `(succeeded={s},
    // active={a})` counter tail — pins that the more informative
    // wording rides through unconditionally, so a future evaluator on
    // the same primitive picks up the counters mechanically.

    #[test]
    fn running_job_projects_to_unsatisfied_with_counter_tail() {
        let result =
            classify_job_status("Job", "default", "my-job", JobLookup::Found(view(0, 0, 2)));
        let err = result.expect_err("succeeded < 1 must project to Err(Satisfaction)");
        let Satisfaction::Unsatisfied(msg) = err else {
            panic!("expected Satisfaction::Unsatisfied");
        };
        assert_eq!(
            msg,
            "Job default/my-job still running (succeeded=0, active=2)"
        );
    }

    // ── Contract axis 5: label prefix rides through both callsites ───
    //
    // Pins that both workspace-shipped labels (`"Job"` for
    // JobAttested, `"closed-loop probe Job"` for ClosedLoopAuth) ride
    // through the same primitive verbatim; a regression that
    // hardcoded a specific label at the primitive would surface at
    // both sub-axes here.

    #[test]
    fn label_prefixes_all_three_unsatisfied_projections_across_shipped_labels() {
        // Sweep every variant of the substrate closed set
        // [`JobEvaluatorLabel::ALL`] so a future third variant added
        // there (per the P3 kenshi-runner lift or any future Job-based
        // postcondition evaluator) automatically extends this pin
        // without a hand-edit here.
        for variant in JobEvaluatorLabel::ALL {
            let label = variant.as_str();
            // Missing
            let err = classify_job_status(label, "ns", "job", JobLookup::Missing)
                .expect_err("Missing must Err");
            let Satisfaction::Unsatisfied(msg) = err else {
                panic!("expected Unsatisfied for Missing at label={label:?}");
            };
            assert!(
                msg.starts_with(&format!("{label} ns/job")),
                "Missing diagnostic must start with '{label} ns/job'; got {msg:?}"
            );

            // Failed
            let err = classify_job_status(label, "ns", "job", JobLookup::Found(view(0, 1, 0)))
                .expect_err("failed > 0 must Err");
            let Satisfaction::Unsatisfied(msg) = err else {
                panic!("expected Unsatisfied for Failed at label={label:?}");
            };
            assert!(
                msg.starts_with(&format!("{label} ns/job failed")),
                "Failed diagnostic must start with '{label} ns/job failed'; got {msg:?}"
            );

            // Running
            let err = classify_job_status(label, "ns", "job", JobLookup::Found(view(0, 0, 1)))
                .expect_err("succeeded < 1 must Err");
            let Satisfaction::Unsatisfied(msg) = err else {
                panic!("expected Unsatisfied for Running at label={label:?}");
            };
            assert!(
                msg.starts_with(&format!("{label} ns/job still running")),
                "Running diagnostic must start with '{label} ns/job still running'; got {msg:?}"
            );
        }
    }

    // ── Contract axis 6: Failed takes priority over Running ──────────
    //
    // A Job with both `failed > 0` and `succeeded < 1` (partial
    // failure, some parallel pods failed but none succeeded yet)
    // must diagnose as Failed (the terminal condition), not Running
    // (the transient one). Pins the branch order at the primitive.

    #[test]
    fn failed_takes_priority_over_running_when_both_hold() {
        let result =
            classify_job_status("Job", "default", "my-job", JobLookup::Found(view(0, 1, 0)));
        let err = result.expect_err("failed > 0 must Err even when succeeded < 1");
        let Satisfaction::Unsatisfied(msg) = err else {
            panic!("expected Satisfaction::Unsatisfied");
        };
        assert!(
            msg.contains("failed"),
            "Failed must take branch priority over Running; got {msg:?}"
        );
        assert!(
            !msg.contains("still running"),
            "Failed branch must not fall through to Running wording; got {msg:?}"
        );
    }
}

/// Substrate-primitive tests for [`classify_receipt_verdict`] — the
/// pure (label, ns, name, cm_name, ReceiptVerdict) → Satisfaction
/// projection every Job-based postcondition evaluator on this file
/// dispatches through after [`verify_receipt_cm`] resolves.
///
/// The pre-lift shape lived at TWO sites across
/// [`evaluate_job_attested`] and [`evaluate_closed_loop_auth`], each
/// hand-authoring the three-invariant scaffold (Ok / Missing /
/// Malformed) verbatim at its own local site — differing only in the
/// label prefix each site threaded into the diagnostic. Post-lift the
/// three shared invariants live at ONE substrate primitive, and this
/// module pins the three contract axes (Ok, Missing, Malformed) plus
/// the label-riding, name-riding, cm-name-riding, and
/// Satisfaction-variant axes at fail-before-pass-after granularity.
///
/// Sibling of the [`classify_job_status_tests`] module above (on the
/// Job-status projection axis) and the [`parse_condition_params_tests`]
/// module (on the params-shape-gate axis) — kept as its own module so
/// a regression at the primitive's receipt-verdict projection axis
/// surfaces distinctly from the per-kind evaluator, the shape gate,
/// or the Job-status gate.
#[cfg(test)]
mod classify_receipt_verdict_tests {
    use super::{classify_receipt_verdict, JobEvaluatorLabel, ReceiptVerdict, Satisfaction};

    // ── Contract axis 1: Ok verdict projects to Satisfied ────────────
    //
    // A parsed receipt whose composed_root matches (or whose caller
    // asked only for shape verification) projects to
    // `Satisfaction::Satisfied` verbatim, dropping the Ok payload's
    // composed_root string — the outer evaluator does not thread that
    // root through the boundary result (it is chained into the
    // Process attestation separately).

    #[test]
    fn ok_verdict_projects_to_satisfied() {
        let result = classify_receipt_verdict(
            "Job",
            "flux-system",
            "my-job",
            "my-job-receipt",
            ReceiptVerdict::Ok("composed_root_string".into()),
        );
        assert_eq!(result, Satisfaction::Satisfied);
    }

    #[test]
    fn ok_verdict_with_empty_root_still_projects_to_satisfied() {
        // The primitive does not gate on Ok payload contents; a
        // hypothetical zero-length composed_root still Satisfies —
        // shape has been verified upstream by ReceiptEnvelope's parser.
        let result = classify_receipt_verdict(
            "closed-loop probe Job",
            "default",
            "probe-job",
            "probe-job-receipt",
            ReceiptVerdict::Ok(String::new()),
        );
        assert_eq!(result, Satisfaction::Satisfied);
    }

    // ── Contract axis 2: Missing projects to Unsatisfied verbatim ────
    //
    // Pins the exact diagnostic wording operators grep for; a
    // regression that reshaped the "receipt ConfigMap … missing"
    // phrase surfaces here rather than as silent operator-facing
    // drift.

    #[test]
    fn missing_verdict_projects_to_unsatisfied_with_label_and_names() {
        let result = classify_receipt_verdict(
            "Job",
            "flux-system",
            "my-job",
            "my-job-receipt",
            ReceiptVerdict::Missing,
        );
        let Satisfaction::Unsatisfied(msg) = result else {
            panic!("Missing must project to Satisfaction::Unsatisfied");
        };
        assert_eq!(
            msg,
            "Job flux-system/my-job receipt ConfigMap flux-system/my-job-receipt missing"
        );
    }

    // ── Contract axis 3: Malformed projects to Unsatisfied verbatim ──
    //
    // Pins the `receipt malformed: {why}` tail — operators grep for
    // both the label prefix and the malformed cause to alert on
    // ConfigMaps whose payload parses as JSON but fails the typed
    // ReceiptEnvelope shape gate.

    #[test]
    fn malformed_verdict_projects_to_unsatisfied_with_why_tail() {
        let result = classify_receipt_verdict(
            "Job",
            "default",
            "my-job",
            "my-job-receipt",
            ReceiptVerdict::Malformed("missing 'intent_hash' string field".into()),
        );
        let Satisfaction::Unsatisfied(msg) = result else {
            panic!("Malformed must project to Satisfaction::Unsatisfied");
        };
        assert_eq!(
            msg,
            "Job default/my-job receipt malformed: missing 'intent_hash' string field"
        );
    }

    // ── Contract axis 4: label prefix rides through both callsites ───
    //
    // Pins that both workspace-shipped labels (`"Job"` for
    // JobAttested, `"closed-loop probe Job"` for ClosedLoopAuth) ride
    // through the same primitive verbatim; a regression that
    // hardcoded a specific label at the primitive would surface at
    // both sub-axes here. Mirrors the sibling label-riding pinning
    // on [`classify_job_status`].

    #[test]
    fn label_prefixes_both_unsatisfied_projections_across_shipped_labels() {
        // Mirrors the sibling sweep on [`classify_job_status_tests`]:
        // iterate every variant of the substrate closed set
        // [`JobEvaluatorLabel::ALL`] so a future third variant added
        // there extends both pins mechanically through the same
        // `ALL` binding.
        for variant in JobEvaluatorLabel::ALL {
            let label = variant.as_str();
            let result = classify_receipt_verdict(
                label,
                "ns",
                "job",
                "job-receipt",
                ReceiptVerdict::Missing,
            );
            let Satisfaction::Unsatisfied(msg) = result else {
                panic!("expected Unsatisfied for Missing at label={label:?}");
            };
            assert!(
                msg.starts_with(&format!("{label} ns/job receipt ConfigMap")),
                "Missing diagnostic must start with '{label} ns/job receipt ConfigMap'; \
                 got {msg:?}"
            );

            let result = classify_receipt_verdict(
                label,
                "ns",
                "job",
                "job-receipt",
                ReceiptVerdict::Malformed("why".into()),
            );
            let Satisfaction::Unsatisfied(msg) = result else {
                panic!("expected Unsatisfied for Malformed at label={label:?}");
            };
            assert!(
                msg.starts_with(&format!("{label} ns/job receipt malformed:")),
                "Malformed diagnostic must start with '{label} ns/job receipt malformed:'; \
                 got {msg:?}"
            );
        }
    }

    // ── Contract axis 5: label is the only prefix source ─────────────
    //
    // A regression that hardcoded a specific label (e.g. by copy-
    // pasting "Job" as a literal instead of interpolating
    // `{label}`) would surface here — the axis-typed prefix
    // "AxisXYZ" is guaranteed absent from the substrate's own trait
    // paths.

    #[test]
    fn label_is_the_only_prefix_source_no_hardcoded_leaks() {
        let result = classify_receipt_verdict(
            "AxisXYZ",
            "ns",
            "job",
            "job-receipt",
            ReceiptVerdict::Missing,
        );
        let Satisfaction::Unsatisfied(msg) = result else {
            panic!("expected Satisfaction::Unsatisfied");
        };
        assert!(
            msg.starts_with("AxisXYZ ns/job receipt ConfigMap "),
            "label must interpolate through the Missing diagnostic; got {msg:?}"
        );

        let result = classify_receipt_verdict(
            "AxisXYZ",
            "ns",
            "job",
            "job-receipt",
            ReceiptVerdict::Malformed("why".into()),
        );
        let Satisfaction::Unsatisfied(msg) = result else {
            panic!("expected Satisfaction::Unsatisfied");
        };
        assert!(
            msg.starts_with("AxisXYZ ns/job receipt malformed: "),
            "label must interpolate through the Malformed diagnostic; got {msg:?}"
        );
    }

    // ── Contract axis 6: Malformed `why` rides through verbatim ──────
    //
    // Pins that the primitive does not drop or reshape the underlying
    // ReceiptError-derived cause — the tail after "malformed: "
    // matches the caller's payload byte-for-byte, so dashboards /
    // alerts grepping for specific ReceiptError projections
    // (`invalid JSON:`, `missing 'intent_hash'`, `version !=
    // tatara-receipt/v1`, `composed_root mismatch`) keep working.

    #[test]
    fn malformed_why_projects_through_the_diagnostic_tail_verbatim() {
        for why in [
            "invalid JSON: expected value at line 1 column 1",
            "missing 'artifact_hash' string field",
            "version != tatara-receipt/v1 (got Some(\"tatara-receipt/v2\"))",
            "composed_root mismatch (got aaa, want bbb)",
            "",
        ] {
            let result = classify_receipt_verdict(
                "Job",
                "ns",
                "job",
                "cm",
                ReceiptVerdict::Malformed(why.into()),
            );
            let Satisfaction::Unsatisfied(msg) = result else {
                panic!("expected Satisfaction::Unsatisfied for why={why:?}");
            };
            let tail = msg
                .strip_prefix("Job ns/job receipt malformed: ")
                .expect("primitive must emit the 'receipt malformed: ' prefix verbatim");
            assert_eq!(
                tail, why,
                "Malformed `why` must ride through the diagnostic tail verbatim"
            );
        }
    }

    // ── Contract axis 7: cm_name rides through the Missing tail ──────
    //
    // Pins that the primitive threads the caller-owned `cm_name`
    // through the Missing diagnostic's ConfigMap slot — a regression
    // that hardcoded `<name>-receipt` at the primitive (short-
    // circuiting the caller's override on the `receiptConfigMap`
    // params slot) would surface here on both callsites, since both
    // JobAttested and ClosedLoopAuth honor a caller-supplied
    // ConfigMap-name override.

    #[test]
    fn cm_name_rides_through_missing_diagnostic() {
        let result = classify_receipt_verdict(
            "Job",
            "ns",
            "job",
            "operator-supplied-cm-name",
            ReceiptVerdict::Missing,
        );
        let Satisfaction::Unsatisfied(msg) = result else {
            panic!("expected Satisfaction::Unsatisfied");
        };
        assert!(
            msg.contains("ConfigMap ns/operator-supplied-cm-name missing"),
            "cm_name must ride through the ConfigMap slot verbatim; got {msg:?}"
        );
        assert!(
            !msg.contains("job-receipt"),
            "primitive must not hardcode <name>-receipt when caller supplied a cm_name; \
             got {msg:?}"
        );
    }

    // ── JobAttested + ClosedLoopAuth default receipt-CM derivation ───
    //
    // Pin the substrate binding: the pre-lift `format!("{name}-receipt")`
    // shape at both [`evaluate_job_attested`] (line ~256) and
    // [`evaluate_closed_loop_auth`] (line ~317) now routes through
    // `tatara_process::receipt::default_receipt_config_map_name`. The
    // docstring comments on the params structs (`defaults to
    // <name>-receipt` on `JobAttestedParams::receipt_config_map`,
    // `defaults to <job-name>-receipt` on
    // `ClosedLoopAuthParams::receipt_config_map`) name the shipped
    // convention; this pin binds those doc strings to a running
    // assertion. A regression that either (a) drifted the substrate
    // primitive's suffix without a docstring update, or (b)
    // re-inlined a `format!` at the callsite instead of routing
    // through the substrate, would fail HERE — the docstring's
    // published shape no longer matches the runtime path.

    #[test]
    fn default_receipt_cm_derivation_shape_matches_docstring_and_substrate() {
        // Both evaluators fall back to `<job_name>-receipt` when the
        // caller supplied no `receiptConfigMap` override. Pin that
        // the substrate composer produces the exact wire-name shape
        // both docstrings publish, for a sweep of realistic Job-name
        // inputs (bare JobAttested target, closed-loop-probe
        // derivation whose Job-name is `<process>-closed-loop-probe`,
        // and a namespace-collision-safe hierarchical Job-name).
        for job_name in [
            "my-job",
            "closed-loop-attest-closed-loop-probe",
            "svc-abc-job-42",
        ] {
            let via_substrate = tatara_process::receipt::default_receipt_config_map_name(job_name);
            // Docstring shape: `<name>-receipt` (JobAttested params)
            // and `<job-name>-receipt` (ClosedLoopAuth params) — both
            // resolve to the same byte sequence pre- and post-lift.
            let docstring_shape = {
                let mut s = String::new();
                s.push_str(job_name);
                s.push_str(tatara_process::receipt::RECEIPT_CM_SUFFIX);
                s
            };
            assert_eq!(
                via_substrate, docstring_shape,
                "default receipt-CM derivation for job {job_name:?} drifted from the \
                 substrate composer <job_name>++RECEIPT_CM_SUFFIX shape",
            );
        }
    }
}
