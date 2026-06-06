//! `EnvMatrixSpec` — the ephemeral-environment *permutation generator*.
//!
//! One `(defenvmatrix …)` declaration fans a single `EphemeralSpec` base out
//! across a set of named axes into the whole permutation set of environments,
//! spawned together. This is generation-over-composition (Pillar 12) applied
//! to environments: author the matrix once, get every variant.
//!
//! Each permutation overlays its axis values into the base's
//! `aplicacao.values_overlay` (or a well-known `@`-target like `@version`),
//! yielding a distinct canonical spec — so each variant gets its own
//! deterministic `EphemeralEnvId` (`blake3(spec)[:8]`) and FQDN
//! (`{app}.{envId}.{cluster}.{location}.{domain}`) for free, via the existing
//! [`crate::hostname`] machinery. The matrix is workload-agnostic: the base
//! can install *any* OCI chart, so the same primitive sweeps echo servers,
//! gateways, migrations, or test suites.
//!
//! Lisp authoring:
//! ```lisp
//! (defenvmatrix echo-sweep
//!   :base (:aplicacao (:chart-ref "oci://ghcr.io/pleme-io/charts/echo"
//!                      :version "0.1.0" :profile "minimal" :values-overlay ())
//!          :ttl "2h" :teardown Always)
//!   :axes ((:name "version"  :path "@version"     :values ("0.1.0" "0.2.0"))
//!          (:name "replicas" :path "replicaCount" :values (1 3))
//!          (:name "flag"     :path "feature.flag" :values ("on" "off")))
//!   :select Cartesian
//!   :budget (:max-envs 12 :cost-ceiling "$5/h"))
//! ```
//! → 2×2×2 = 8 named `EphemeralSpec`s, each breathe-bounded under the shared
//! `:budget`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tatara_lisp_derive::TataraDomain as DeriveTataraDomain;

use crate::ephemeral::EphemeralSpec;

/// `EnvMatrixSpec` — authors `(defenvmatrix …)`. Expands to a set of named
/// [`EphemeralSpec`] values via [`EnvMatrixSpec::expand`].
#[derive(DeriveTataraDomain, Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[tatara(keyword = "defenvmatrix")]
pub struct EnvMatrixSpec {
    /// The base ephemeral environment every permutation is derived from.
    pub base: EphemeralSpec,

    /// The permutation axes. Each axis ranges over a list of values; the
    /// generated set is the selection (cartesian by default) over all axes.
    pub axes: Vec<MatrixAxis>,

    /// Selection strategy over the axes' product. Defaults to `Cartesian`.
    #[serde(default)]
    pub select: SelectStrategy,

    /// Shared cost / concurrency budget across the whole sweep. The
    /// `cost_ceiling` is the envelope handed to breathe so the entire
    /// permutation set stays cost-bounded.
    #[serde(default)]
    pub budget: MatrixBudget,
}

/// One permutation axis: a named dimension and the values it ranges over.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatrixAxis {
    /// Axis name — used in the generated env name and as a `matrix-axis/<name>`
    /// label. Must be a DNS-label-safe token.
    pub name: String,

    /// Where each value is written on the base spec:
    /// - `@version` / `@profile` / `@chart-ref` → the matching
    ///   `AplicacaoIntent` field (value must be a string),
    /// - any other string → a dot-path into `aplicacao.values_overlay`
    ///   (e.g. `replicaCount`, `image.tag`, `feature.flag`); intermediate
    ///   objects are created as needed,
    /// - empty → the value (which must be a JSON object) is merged at the
    ///   overlay root.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,

    /// The values this axis ranges over, as a JSON array (e.g.
    /// `["v1" "v2"]`, `[1 3]`, `[#t #f]`). Each element is overlaid at
    /// `path`. A non-array (or empty array) contributes no permutations.
    pub values: serde_json::Value,
}

impl MatrixAxis {
    /// The axis values as a slice (empty if `values` is not a JSON array).
    fn vals(&self) -> &[serde_json::Value] {
        self.values.as_array().map(Vec::as_slice).unwrap_or(&[])
    }
}

/// How the permutation set is drawn from the axes.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub enum SelectStrategy {
    /// The full cartesian product of every axis. N envs = Π |axisᵢ|.
    #[default]
    Cartesian,
    /// An explicit list of coordinate tuples — one entry per axis, each a
    /// 0-based index into that axis's `values`. Lets the operator hand-pick
    /// a sparse subset instead of the full product.
    Explicit(Vec<Vec<usize>>),
}

/// Shared budget across the sweep.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct MatrixBudget {
    /// Hard cap on the number of envs the sweep spawns (`0` = no cap). When
    /// the selection exceeds this, the first `max_envs` (in selection order)
    /// are kept and the rest are dropped — callers should `log` the drop so
    /// truncation is never silent.
    #[serde(default)]
    pub max_envs: u32,

    /// Cost ceiling for the whole sweep — a free-form budget string (e.g.
    /// `"$5/h"`). Surfaced to breathe as the shared envelope cost SLA; the
    /// controller gates how many permutations run concurrently against
    /// `band.status.observedCostRemaining`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_ceiling: Option<String>,

    /// Per-env `max_concurrent` override. When set, replaces the base's
    /// value on every generated spec; when `None`, each variant keeps the
    /// base's `max_concurrent`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent: Option<u32>,
}

/// A single generated environment: a deterministic name plus the lowered
/// [`EphemeralSpec`]. The name is `{matrix}-{axis-value}…`; the env's
/// `EphemeralEnvId` is derived downstream from the spec's canonical hash, so
/// distinct overlays ⇒ distinct ids ⇒ distinct FQDNs automatically.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct NamedEphemeral {
    /// DNS-label-safe instance name, `{matrix}-{axis-slug}…`.
    pub name: String,
    /// The concrete ephemeral spec for this permutation.
    pub spec: EphemeralSpec,
}

impl EnvMatrixSpec {
    /// The selection coordinates (one Vec per generated env; each is one
    /// 0-based value index per axis), before the `max_envs` cap.
    pub fn coordinates(&self) -> Vec<Vec<usize>> {
        match &self.select {
            SelectStrategy::Cartesian => {
                let lengths: Vec<usize> = self.axes.iter().map(|a| a.vals().len()).collect();
                cartesian(&lengths)
            }
            SelectStrategy::Explicit(coords) => coords
                .iter()
                .filter(|c| self.coord_in_bounds(c))
                .cloned()
                .collect(),
        }
    }

    /// True iff `coord` has one in-bounds index per axis.
    fn coord_in_bounds(&self, coord: &[usize]) -> bool {
        coord.len() == self.axes.len()
            && coord
                .iter()
                .zip(&self.axes)
                .all(|(&i, ax)| i < ax.vals().len())
    }

    /// How many environments this matrix *would* generate before the
    /// `max_envs` cap (the full selection size).
    pub fn selection_size(&self) -> usize {
        self.coordinates().len()
    }

    /// Expand the matrix into the concrete, capped set of named ephemeral
    /// specs. `matrix_name` is the `(defenvmatrix <name> …)` name — the prefix
    /// for every generated env name.
    pub fn expand(&self, matrix_name: &str) -> Vec<NamedEphemeral> {
        let coords = self.coordinates();
        let capped: Vec<Vec<usize>> = match self.budget.max_envs {
            0 => coords,
            n => coords.into_iter().take(n as usize).collect(),
        };
        capped
            .into_iter()
            .map(|coord| self.materialize(matrix_name, &coord))
            .collect()
    }

    /// Build one variant from a coordinate.
    fn materialize(&self, matrix_name: &str, coord: &[usize]) -> NamedEphemeral {
        let mut spec = self.base.clone();
        let mut suffix = Vec::with_capacity(coord.len());
        for (axis_idx, &val_idx) in coord.iter().enumerate() {
            let axis = &self.axes[axis_idx];
            let val = axis.vals().get(val_idx).cloned().unwrap_or(serde_json::Value::Null);
            apply_axis(&mut spec, &axis.path, val.clone());
            suffix.push(format!("{}-{}", slug(&axis.name), slug_value(&val)));
        }
        if let Some(mc) = self.budget.max_concurrent {
            spec.max_concurrent = mc;
        }
        let name = if suffix.is_empty() {
            matrix_name.to_string()
        } else {
            format!("{}-{}", matrix_name, suffix.join("-"))
        };
        NamedEphemeral { name, spec }
    }
}

/// Apply one axis value to a spec at the axis's path.
fn apply_axis(spec: &mut EphemeralSpec, path: &str, val: serde_json::Value) {
    match path {
        "@version" => {
            if let Some(s) = val.as_str() {
                spec.aplicacao.version = s.to_string();
            }
        }
        "@profile" => {
            if let Some(s) = val.as_str() {
                spec.aplicacao.profile = s.to_string();
            }
        }
        "@chart-ref" => {
            if let Some(s) = val.as_str() {
                spec.aplicacao.chart_ref = s.to_string();
            }
        }
        p => overlay_at_path(&mut spec.aplicacao.values_overlay, p, val),
    }
}

/// Cartesian product of axis lengths → mixed-radix coordinate list. Any
/// zero-length axis yields the empty set (no env can range over no values).
fn cartesian(lengths: &[usize]) -> Vec<Vec<usize>> {
    if lengths.is_empty() {
        return vec![vec![]];
    }
    if lengths.iter().any(|&l| l == 0) {
        return vec![];
    }
    let total: usize = lengths.iter().product();
    (0..total)
        .map(|n| {
            let mut rem = n;
            lengths
                .iter()
                .map(|&l| {
                    let d = rem % l;
                    rem /= l;
                    d
                })
                .collect()
        })
        .collect()
}

/// Set `val` into a JSON object at a dot-path, creating intermediate objects.
/// Empty path merges an object value at the root.
fn overlay_at_path(root: &mut serde_json::Value, path: &str, val: serde_json::Value) {
    if !root.is_object() {
        *root = serde_json::Value::Object(Default::default());
    }
    if path.is_empty() {
        if let Some(obj) = val.as_object() {
            let r = root.as_object_mut().expect("root is object");
            for (k, v) in obj {
                r.insert(k.clone(), v.clone());
            }
        }
        return;
    }
    let parts: Vec<&str> = path.split('.').collect();
    let mut cur = root;
    for part in &parts[..parts.len() - 1] {
        if !cur.is_object() {
            *cur = serde_json::Value::Object(Default::default());
        }
        cur = cur
            .as_object_mut()
            .expect("object")
            .entry((*part).to_string())
            .or_insert_with(|| serde_json::Value::Object(Default::default()));
    }
    if !cur.is_object() {
        *cur = serde_json::Value::Object(Default::default());
    }
    cur.as_object_mut()
        .expect("object")
        .insert(parts[parts.len() - 1].to_string(), val);
}

/// DNS-label-safe slug of a token: lowercase, non-`[a-z0-9-]` → `-`, collapse
/// runs, trim leading/trailing `-`. Empty → `"x"`.
fn slug(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for c in s.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "x".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Slug of a JSON value (string content, number, or bool) for env names.
fn slug_value(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => slug(s),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => slug(&n.to_string()),
        other => slug(&other.to_string()),
    }
}

/// Compile a `(defenvmatrix …)` Lisp source into named `EnvMatrixSpec` values.
pub fn compile_env_matrix_source(
    src: &str,
) -> tatara_lisp::Result<Vec<tatara_lisp::NamedDefinition<EnvMatrixSpec>>> {
    tatara_lisp::compile_named::<EnvMatrixSpec>(src)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crd::ProcessSpec;
    use crate::intent::{AplicacaoIntent, IntentVariant};
    use crate::lifetime::TeardownPolicy;

    fn base() -> EphemeralSpec {
        EphemeralSpec {
            aplicacao: AplicacaoIntent {
                chart_ref: "oci://ghcr.io/pleme-io/charts/echo".into(),
                version: "0.1.0".into(),
                profile: "minimal".into(),
                values_overlay: serde_json::json!({}),
                release_name: None,
                target_namespace: None,
                install_timeout: None,
            },
            ttl: "2h".into(),
            teardown: TeardownPolicy::Always,
            max_concurrent: 1,
            postconditions: vec![],
            preconditions: vec![],
            verify_timeout: None,
            classification: None,
            parent: None,
            exports: vec![],
            routing: None,
        }
    }

    fn matrix() -> EnvMatrixSpec {
        EnvMatrixSpec {
            base: base(),
            axes: vec![
                MatrixAxis {
                    name: "version".into(),
                    path: "@version".into(),
                    values: serde_json::json!(["0.1.0", "0.2.0"]),
                },
                MatrixAxis {
                    name: "replicas".into(),
                    path: "replicaCount".into(),
                    values: serde_json::json!([1, 3]),
                },
                MatrixAxis {
                    name: "flag".into(),
                    path: "feature.flag".into(),
                    values: serde_json::json!(["on", "off"]),
                },
            ],
            select: SelectStrategy::Cartesian,
            budget: MatrixBudget::default(),
        }
    }

    #[test]
    fn cartesian_count_is_product_of_axes() {
        let m = matrix();
        assert_eq!(m.selection_size(), 2 * 2 * 2);
        let envs = m.expand("echo-sweep");
        assert_eq!(envs.len(), 8);
    }

    #[test]
    fn each_permutation_overlays_its_axis_values() {
        let envs = matrix().expand("echo-sweep");
        // Find the v0.2.0 / replicas=3 / flag=off variant.
        let target = envs
            .iter()
            .find(|e| {
                e.spec.aplicacao.version == "0.2.0"
                    && e.spec.aplicacao.values_overlay["replicaCount"] == 3
                    && e.spec.aplicacao.values_overlay["feature"]["flag"] == "off"
            })
            .expect("the v0.2.0/3/off permutation exists");
        // Name carries the axis slugs.
        assert!(target.name.starts_with("echo-sweep-"));
        assert!(target.name.contains("version-0-2-0"));
        assert!(target.name.contains("replicas-3"));
        assert!(target.name.contains("flag-off"));
    }

    #[test]
    fn names_are_unique_and_dns_safe() {
        let envs = matrix().expand("echo-sweep");
        let names: std::collections::BTreeSet<_> = envs.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names.len(), envs.len(), "all names distinct");
        for e in &envs {
            assert!(
                e.name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "name {} is DNS-safe",
                e.name
            );
        }
    }

    #[test]
    fn max_envs_caps_and_overrides_concurrency() {
        let mut m = matrix();
        m.budget.max_envs = 3;
        m.budget.max_concurrent = Some(5);
        let envs = m.expand("echo-sweep");
        assert_eq!(envs.len(), 3);
        assert!(envs.iter().all(|e| e.spec.max_concurrent == 5));
    }

    #[test]
    fn explicit_selection_picks_a_subset() {
        let mut m = matrix();
        // Just two hand-picked corners of the cube.
        m.select = SelectStrategy::Explicit(vec![vec![0, 0, 0], vec![1, 1, 1]]);
        let envs = m.expand("echo-sweep");
        assert_eq!(envs.len(), 2);
        assert_eq!(envs[0].spec.aplicacao.version, "0.1.0");
        assert_eq!(envs[1].spec.aplicacao.version, "0.2.0");
    }

    #[test]
    fn each_variant_lowers_to_a_process_spec() {
        let envs = matrix().expand("echo-sweep");
        for e in &envs {
            let ps: ProcessSpec = e.spec.clone().into();
            assert!(matches!(
                ps.intent.variant().unwrap(),
                IntentVariant::Aplicacao(_)
            ));
        }
    }

    #[test]
    fn overlay_at_nested_path_creates_intermediate_objects() {
        let mut root = serde_json::json!({"existing": 1});
        overlay_at_path(&mut root, "a.b.c", serde_json::json!("v"));
        assert_eq!(root["a"]["b"]["c"], "v");
        assert_eq!(root["existing"], 1, "existing keys preserved");
    }

    #[test]
    fn env_matrix_lisp_round_trip() {
        let src = r#"
            (defenvmatrix echo-sweep
              :base (:aplicacao (:chart-ref "oci://ghcr.io/pleme-io/charts/echo"
                                 :version "0.1.0" :profile "minimal" :values-overlay ())
                     :ttl "2h" :teardown Always)
              :axes ((:name "version"  :path "@version"     :values ("0.1.0" "0.2.0"))
                     (:name "replicas" :path "replicaCount" :values (1 3)))
              :select Cartesian
              :budget (:max-envs 12 :cost-ceiling "$5/h"))
        "#;
        let defs = compile_env_matrix_source(src).expect("compile");
        assert_eq!(defs.len(), 1);
        let d = &defs[0];
        assert_eq!(d.name, "echo-sweep");
        assert_eq!(d.spec.axes.len(), 2);
        assert_eq!(d.spec.budget.max_envs, 12);
        assert_eq!(d.spec.budget.cost_ceiling.as_deref(), Some("$5/h"));
        let envs = d.spec.expand(&d.name);
        assert_eq!(envs.len(), 4); // 2 × 2
    }
}
