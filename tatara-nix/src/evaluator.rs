//! Evaluator trait — the typed interface any Nix-equivalent backend plugs into.
//!
//! `tatara-nix` produces typed IR; an `Evaluator` turns that IR into real
//! store paths, build logs, and output artifacts. The trait is backend-agnostic:
//!
//!   - `DryRun`  — pure computation, no side effects (shipped here)
//!   - `SuiEval` — wraps [`sui`](https://github.com/pleme-io/sui) (via adapter in sui crate)
//!   - user impls — any other backend: cached, remote, test, …
//!
//! Because `Derivation::store_path()` is already content-addressed (BLAKE3 of
//! canonical spec), **every evaluator agrees on the root store path for the
//! same input**. Backends differ only in whether they actually produce the
//! artifact and in how they resolve inputs.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::convert::Infallible;

use crate::derivation::Derivation;
use crate::store::{StoreHash, StorePath};

/// Evaluator interface — accept typed IR, produce store paths.
pub trait Evaluator {
    type Error;

    /// Evaluate a single derivation. Concrete backends may build it; `DryRun`
    /// just computes the store path. Every evaluator agrees on the primary
    /// path for the same input.
    fn evaluate(&self, deriv: &Derivation) -> Result<EvaluationResult, Self::Error>;

    /// Evaluate many derivations. Default impl just maps `evaluate`; backends
    /// may override to exploit shared build state (cache, DAG scheduling).
    fn evaluate_many(
        &self,
        derivs: &[Derivation],
    ) -> Result<Vec<EvaluationResult>, Self::Error> {
        derivs.iter().map(|d| self.evaluate(d)).collect()
    }

    /// Compute the dependency plan without executing. All evaluators should
    /// produce the same plan for the same input (content-addressing again).
    fn plan(&self, deriv: &Derivation) -> Result<Plan, Self::Error>;
}

/// Result of evaluating one derivation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationResult {
    /// Primary output store path.
    pub store_path: StorePath,
    /// All declared outputs, keyed by output name (`"out"`, `"doc"`, `"lib"`, …).
    pub outputs: BTreeMap<String, StorePath>,
    /// Build log lines. Empty for DryRun.
    pub log: Vec<String>,
}

/// Dependency plan — the DAG that an evaluator will execute.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    /// Root derivation's store path.
    pub root: StorePath,
    /// Directed edges: `(parent, child)` where `parent` depends on `child`.
    pub edges: Vec<(StorePath, StorePath)>,
    /// Build order — children before parents, parent last.
    pub order: Vec<StorePath>,
}

// ── DryRun — pure, side-effect-free, useful for tests + offline planning ──

/// Evaluator that computes store paths + outputs but does not build.
#[derive(Clone, Copy, Debug, Default)]
pub struct DryRun;

impl Evaluator for DryRun {
    type Error = Infallible;

    fn evaluate(&self, d: &Derivation) -> Result<EvaluationResult, Self::Error> {
        let primary = d.store_path();
        let mut outputs = BTreeMap::new();
        outputs.insert(d.outputs.primary.clone(), primary.clone());
        for extra in &d.outputs.extra {
            // Extra outputs get their own content-addressed path derived from
            // (primary_hash, output_name) — deterministic and independent of
            // the evaluator backend.
            let extra_hash = StoreHash::of(&(&primary.hash.0, extra));
            let extra_path = StorePath::new(
                extra_hash,
                format!("{}-{extra}", d.name),
                d.version.clone(),
            );
            outputs.insert(extra.clone(), extra_path);
        }
        Ok(EvaluationResult {
            store_path: primary,
            outputs,
            log: vec![format!(
                "[DryRun] would evaluate {} ({} inputs, {} phases)",
                d.name,
                d.inputs.len(),
                d.builder.phases.len()
            )],
        })
    }

    fn plan(&self, d: &Derivation) -> Result<Plan, Self::Error> {
        let root = d.store_path();
        let mut edges = Vec::new();
        let mut order = Vec::new();
        // For DryRun we can only emit edges for inputs that are already pinned
        // to a concrete StorePath. Unpinned inputs (name-only) defer to the
        // real evaluator, which must resolve them against a package set.
        for input in &d.inputs {
            if let Some(pinned) = &input.pinned {
                edges.push((root.clone(), pinned.clone()));
                if !order.contains(pinned) {
                    order.push(pinned.clone());
                }
            }
        }
        order.push(root.clone());
        Ok(Plan { root, edges, order })
    }
}

// ── Adapter for external evaluators (the sui integration point) ──────

/// An external evaluator plugs in by impl'ing `Evaluator`. The concrete
/// adapter lives in the sui crate (feature-gated) to avoid pulling sui into
/// tatara-nix's default build. Sketch of the adapter:
///
/// ```ignore
/// use tatara_nix::{Derivation, EvaluationResult, Evaluator, Plan};
///
/// pub struct SuiEval { pub engine: sui::Engine }
///
/// impl Evaluator for SuiEval {
///     type Error = sui::Error;
///     fn evaluate(&self, d: &Derivation) -> Result<EvaluationResult, Self::Error> {
///         let sui_drv: sui::Derivation = d.into();
///         let result = self.engine.build(&sui_drv)?;
///         Ok(EvaluationResult {
///             store_path: d.store_path(),
///             outputs: result.outputs.into_iter()
///                 .map(|(k, v)| (k, sui_path_to_store_path(v)))
///                 .collect(),
///             log: result.log,
///         })
///     }
///     fn plan(&self, d: &Derivation) -> Result<Plan, Self::Error> { … }
/// }
/// ```
pub mod adapter_notes {}

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derivation::{BuilderPhase, BuilderPhases, InputRef};

    fn mk(name: &str, version: Option<&str>) -> Derivation {
        Derivation {
            name: name.into(),
            version: version.map(String::from),
            inputs: vec![],
            source: Default::default(),
            builder: Default::default(),
            outputs: Default::default(),
            env: vec![],
            sandbox: Default::default(),
        }
    }

    #[test]
    fn dry_run_produces_deterministic_store_path() {
        let d = mk("hello", Some("2.12.1"));
        let r1 = DryRun.evaluate(&d).unwrap();
        let r2 = DryRun.evaluate(&d).unwrap();
        assert_eq!(r1.store_path, r2.store_path);
        assert_eq!(r1.store_path, d.store_path());
    }

    #[test]
    fn dry_run_expands_all_outputs() {
        let mut d = mk("hello", Some("2.12.1"));
        d.outputs.extra = vec!["doc".into(), "dev".into()];
        let r = DryRun.evaluate(&d).unwrap();
        assert_eq!(r.outputs.len(), 3);
        assert!(r.outputs.contains_key("out"));
        assert!(r.outputs.contains_key("doc"));
        assert!(r.outputs.contains_key("dev"));
        // Each extra output gets a distinct content-addressed path.
        let out = &r.outputs["out"];
        let doc = &r.outputs["doc"];
        let dev = &r.outputs["dev"];
        assert_ne!(out.hash, doc.hash);
        assert_ne!(out.hash, dev.hash);
        assert_ne!(doc.hash, dev.hash);
    }

    #[test]
    fn dry_run_log_includes_input_and_phase_counts() {
        let mut d = mk("hello", None);
        d.inputs.push(InputRef {
            name: "gcc".into(),
            version: None,
            pinned: None,
        });
        d.builder = BuilderPhases {
            phases: vec![BuilderPhase::Unpack, BuilderPhase::Build, BuilderPhase::Install],
            commands: Default::default(),
        };
        let r = DryRun.evaluate(&d).unwrap();
        assert_eq!(r.log.len(), 1);
        assert!(r.log[0].contains("hello"));
        assert!(r.log[0].contains("1 inputs"));
        assert!(r.log[0].contains("3 phases"));
    }

    #[test]
    fn plan_emits_edges_only_for_pinned_inputs() {
        let dep = mk("libc", Some("2.38"));
        let dep_path = dep.store_path();
        let mut d = mk("hello", Some("2.12.1"));
        d.inputs.push(InputRef {
            name: "libc".into(),
            version: Some("2.38".into()),
            pinned: Some(dep_path.clone()),
        });
        d.inputs.push(InputRef {
            name: "gcc".into(), // unpinned — plan skips
            version: None,
            pinned: None,
        });
        let plan = DryRun.plan(&d).unwrap();
        assert_eq!(plan.root, d.store_path());
        assert_eq!(plan.edges.len(), 1);
        assert_eq!(plan.edges[0].0, d.store_path());
        assert_eq!(plan.edges[0].1, dep_path);
        // Order: child before parent.
        assert_eq!(plan.order.len(), 2);
        assert_eq!(plan.order[0], dep_path);
        assert_eq!(plan.order[1], d.store_path());
    }

    #[test]
    fn evaluate_many_preserves_order() {
        let a = mk("a", None);
        let b = mk("b", None);
        let c = mk("c", None);
        let results = DryRun.evaluate_many(&[a.clone(), b.clone(), c.clone()]).unwrap();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].store_path, a.store_path());
        assert_eq!(results[1].store_path, b.store_path());
        assert_eq!(results[2].store_path, c.store_path());
    }

    #[test]
    fn identical_derivations_across_evaluators_agree_on_store_path() {
        // Two DryRun instances (stand in for distinct backends) must agree.
        let a = DryRun;
        let b = DryRun;
        let d = mk("hello", Some("2.12.1"));
        assert_eq!(a.evaluate(&d).unwrap().store_path, b.evaluate(&d).unwrap().store_path);
    }

    #[test]
    fn plan_handles_no_inputs() {
        let d = mk("hello", None);
        let plan = DryRun.plan(&d).unwrap();
        assert_eq!(plan.edges.len(), 0);
        assert_eq!(plan.order, vec![d.store_path()]);
    }

    #[test]
    fn plan_deduplicates_repeated_pinned_inputs() {
        let dep = mk("libc", Some("2.38"));
        let dep_path = dep.store_path();
        let mut d = mk("hello", None);
        d.inputs.push(InputRef {
            name: "libc".into(),
            version: Some("2.38".into()),
            pinned: Some(dep_path.clone()),
        });
        d.inputs.push(InputRef {
            name: "libc-duplicate-alias".into(),
            version: Some("2.38".into()),
            pinned: Some(dep_path.clone()),
        });
        let plan = DryRun.plan(&d).unwrap();
        assert_eq!(plan.edges.len(), 2); // both inputs point at same dep — edges record both
        assert_eq!(plan.order.len(), 2); // order dedupes
    }
}
