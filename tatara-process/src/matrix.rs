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
//!   :budget (:max-envs 12 :cost-ceiling "$5/h")
//!   :breathe (:dimensions ((:kind "memory" :floor "128Mi" :ceiling "1Gi")
//!                          (:kind "cpu"    :floor "100m"  :ceiling "1"))
//!             :cooldown-seconds 60 :dry-run #t))
//! ```
//! → 2×2×2 = 8 named `EphemeralSpec`s. `tatara-lispc` renders each as a
//! `Process` CR plus its breathe Band CRs (one per dimension), so the whole
//! sweep is cost-bounded under the shared `:budget` and auto-scales within the
//! `:breathe` floor/ceiling limits.

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

    /// Optional breathe envelope. When set, [`EnvMatrixSpec::breathe_bands`]
    /// emits one breathe Band CR per dimension per env, so each generated
    /// environment auto-scales inside cost-bounded floor/ceiling limits
    /// (idle-shrink → near-floor, breathe-up on demand). This is how the
    /// sweep "fully leverages breathability": author the envelope once, every
    /// permutation inherits it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub breathe: Option<BreatheEnvelope>,
}

/// The breathe envelope inherited by every env in a sweep — the per-dimension
/// homeostasis bounds plus dev-loop-tuned cadence.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BreatheEnvelope {
    /// The resource dimensions to band. Each yields a breathe Band CR
    /// (`MemoryBand` / `CpuBand` / `StorageBand`) per env.
    pub dimensions: Vec<BreatheDimension>,

    /// Band cooldown in seconds between carves — low (e.g. 60s) for fast
    /// dev-loop breathe-up/shrink, vs the fleet default.
    #[serde(default = "default_breathe_cooldown")]
    pub cooldown_seconds: u64,

    /// Start observe-only (`dryRun`) — breathe reports what it WOULD carve
    /// without mutating, until the cost SLA is validated. Default `true`
    /// (safe by default for a fresh sweep).
    #[serde(default = "default_true")]
    pub dry_run: bool,

    /// The workload kind the bands target (default `Deployment`). The band's
    /// `targetRef.name` is the env's Helm release name (the per-env release).
    #[serde(default = "default_target_kind")]
    pub target_kind: String,
}

/// One banded resource dimension: which breathe Band kind and its bounds.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BreatheDimension {
    /// The breathe dimension keyword — `memory`/`mem` → `MemoryBand`,
    /// `cpu` → `CpuBand`, `storage`/`disk` → `StorageBand`. Case-
    /// insensitive. Decoded through the typed [`BreatheDimensionKind`]
    /// closed-set projection: aliases (`mem` / `disk`) decode to the
    /// SAME variant as the primary keyword, then [`BreatheDimensionKind::
    /// band_kind`] projects the CR kind and [`BreatheDimensionKind::
    /// name_segment`] projects the canonical band-name segment so the
    /// emitted band name (`<env>-{name-segment}`) does NOT depend on
    /// which alias the operator wrote. Unrecognized keywords drop the
    /// dimension (no band emitted) — the closed set IS the substrate's
    /// supported axis set.
    pub kind: String,
    /// Floor quantity (the never-shrink-below limit, in the dimension's unit:
    /// bytes-quantity like `128Mi` for memory/storage, millicores like `100m`
    /// for cpu). Idle envs shrink toward this.
    pub floor: String,
    /// Ceiling quantity (the never-grow-above limit) — the cost-bounding wall.
    pub ceiling: String,
}

fn default_breathe_cooldown() -> u64 {
    60
}
fn default_true() -> bool {
    true
}
fn default_target_kind() -> String {
    "Deployment".to_string()
}

/// One permutation axis: a named dimension and the values it ranges over.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MatrixAxis {
    /// Axis name — used in the generated env name and as a `matrix-axis/<name>`
    /// label. Must be a DNS-label-safe token.
    pub name: String,

    /// Where each value is written on the base spec:
    /// - A typed [`MatrixTarget`] marker ([`MatrixTarget::Version`] /
    ///   [`MatrixTarget::Profile`] / [`MatrixTarget::ChartRef`]) → the
    ///   matching `AplicacaoIntent` field (value must be a string). The
    ///   path-string surface for each variant is canonicalized by
    ///   [`MatrixTarget::marker`] (`"@version"` / `"@profile"` /
    ///   `"@chart-ref"`) and decoded by [`MatrixTarget::from_path`].
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
            let val = axis
                .vals()
                .get(val_idx)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
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
        // Each permutation is a distinct Helm release named for the env, so the
        // variants coexist and breathe bands can target each one by name.
        spec.aplicacao.release_name = Some(name.clone());
        NamedEphemeral { name, spec }
    }

    /// Emit the breathe Band CRs for one generated env — one per envelope
    /// dimension (`MemoryBand` / `CpuBand` / `StorageBand`). Empty when no
    /// `:breathe` envelope is declared. Each band targets the env's Helm
    /// release (a `Deployment` by default) and inherits the sweep's
    /// `:budget :cost-ceiling` as a `breathe.pleme.io/cost-ceiling` annotation,
    /// so the controller gates the whole sweep against one cost budget.
    pub fn breathe_bands(&self, env: &NamedEphemeral) -> Vec<serde_json::Value> {
        let Some(envelope) = &self.breathe else {
            return vec![];
        };
        let target_name = env
            .spec
            .aplicacao
            .release_name
            .clone()
            .unwrap_or_else(|| env.name.clone());
        let namespace = env
            .spec
            .aplicacao
            .target_namespace
            .clone()
            .unwrap_or_else(|| env.name.clone());
        let mut annotations = serde_json::Map::new();
        if let Some(ceiling) = &self.budget.cost_ceiling {
            annotations.insert(
                "breathe.pleme.io/cost-ceiling".to_string(),
                serde_json::Value::String(ceiling.clone()),
            );
        }
        envelope
            .dimensions
            .iter()
            .filter_map(|dim| {
                let kind = BreatheDimensionKind::from_keyword(&dim.kind)?;
                Some(serde_json::json!({
                    "apiVersion": "breathe.pleme.io/v1",
                    "kind": kind.band_kind(),
                    "metadata": {
                        "name": format!("{}-{}", env.name, kind.name_segment()),
                        "namespace": namespace,
                        "labels": { "matrix-env": env.name },
                        "annotations": annotations,
                    },
                    "spec": {
                        "targetRef": { "kind": envelope.target_kind, "name": target_name },
                        "floor": dim.floor,
                        "ceiling": dim.ceiling,
                        "cooldownSeconds": envelope.cooldown_seconds,
                        "dryRun": envelope.dry_run,
                    },
                }))
            })
            .collect()
    }
}

/// Closed-set typed identifier for the three reachable breathe Band CR
/// kinds a [`BreatheDimension::kind`] keyword can target — [`Self::Memory`]
/// → `MemoryBand`, [`Self::Cpu`] → `CpuBand`, [`Self::Storage`] →
/// `StorageBand` — as a Rust enum, so the (keyword-set, CR-kind,
/// name-segment) triple binds at ONE site on the typed algebra rather
/// than at three byte-identical string-literal sites scattered across
/// [`EnvMatrixSpec::breathe_bands`] and the deleted `band_kind_for`
/// helper.
///
/// Pre-lift the dispatch lived as a string-input / `&'static str`-output
/// `band_kind_for` helper paired with an inline
/// `dim.kind.to_ascii_lowercase()` composing the band's metadata-name
/// segment. The two arms of the pairing did NOT canonicalize together:
/// `band_kind_for("mem")` and `band_kind_for("memory")` both projected
/// to `"MemoryBand"`, but the inline name-segment site echoed the
/// operator's raw alias (`<env>-mem` vs `<env>-memory`), so a single
/// matrix sweep that wrote one dimension as `"mem"` and another as
/// `"memory"` produced two bands with drift-shaped names and no compile
/// or runtime signal that the names depended on operator-side alias
/// choice. Post-lift the pairing binds at ONE typed projection
/// ([`Self::band_kind`] + [`Self::name_segment`]) — both the CR kind
/// AND the name segment derive from the same closed-set variant, so
/// every alias canonicalizes to ONE band-name shape regardless of how
/// the operator spelled the dimension keyword.
///
/// Adding a fourth dimension (e.g. `Network` → `NetworkBand`,
/// name-segment `"network"`) extends the enum AND the three projection
/// arms ([`Self::from_keyword`], [`Self::band_kind`],
/// [`Self::name_segment`]) in lockstep — rustc binds the extension
/// through exhaustiveness over the closed enum so a partial extension
/// that forgets ONE projection becomes a compile error rather than a
/// runtime drift where the new band-kind projects but the name-segment
/// falls back to the raw keyword.
///
/// Sibling closed-set lift to this file's [`MatrixTarget`]
/// (three-of-three magic-target identifier on the same `EphemeralSpec`
/// algebra) and to tatara-lisp's `QuoteForm` (four-of-four homoiconic
/// prefix-wrappers), `UnquoteForm` (two-of-four template-marker
/// subset), `MacroDefHead` (two-of-two macro-definition heads), and
/// `CompilerSpecIoStage` (disk-persistence surface) closed-set
/// algebras: those enums key their respective dispatch / projection
/// variants on a typed identity carried inside the variant; this enum
/// keys the three reachable breathe-dimension CR-kind / name-segment
/// pairs on a typed marker identity.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the closed set
/// of breathe-dimension keywords becomes a TYPE rather than three
/// `&'static str` literals at one site and a raw-keyword `to_ascii_
/// lowercase()` at another. A typo in any arm becomes a compile error
/// against the typed projection. THEORY.md §VI.1 — generation over
/// composition; the (keyword-set, CR-kind, name-segment) triple was
/// load-bearing across two sites yet enforced by per-site call-site
/// discipline — past the ≥2 PRIME-DIRECTIVE trigger once the
/// structural shape is named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BreatheDimensionKind {
    /// `memory` / `mem` → `MemoryBand`, name-segment `"memory"`.
    Memory,
    /// `cpu` → `CpuBand`, name-segment `"cpu"`.
    Cpu,
    /// `storage` / `disk` → `StorageBand`, name-segment `"storage"`.
    Storage,
}

impl BreatheDimensionKind {
    /// Decode a [`BreatheDimension::kind`] keyword (case-insensitive)
    /// into the typed marker, or `None` for keywords that aren't in
    /// the closed set (they fall through to the `filter_map` drop in
    /// [`EnvMatrixSpec::breathe_bands`] — the dimension contributes
    /// no band). Closed-set primary inverse of [`Self::band_kind`]
    /// and [`Self::name_segment`]: every primary / alias keyword for
    /// a variant decodes back to that variant.
    #[must_use]
    pub fn from_keyword(kw: &str) -> Option<Self> {
        match kw.to_ascii_lowercase().as_str() {
            "memory" | "mem" => Some(Self::Memory),
            "cpu" => Some(Self::Cpu),
            "storage" | "disk" => Some(Self::Storage),
            _ => None,
        }
    }

    /// Canonical breathe Band CR kind — the `kind:` field on the
    /// emitted Band CR (`MemoryBand` / `CpuBand` / `StorageBand`).
    /// Projects through `&'static str` (no allocation) so consumers
    /// (the `breathe_bands` emitter, future CRD discovery, future
    /// LSP completion lists) compose with the same shape
    /// `tatara_lisp`'s `QuoteForm::prefix` / `MatrixTarget::marker`
    /// closed-set surfaces use.
    #[must_use]
    pub fn band_kind(self) -> &'static str {
        match self {
            Self::Memory => "MemoryBand",
            Self::Cpu => "CpuBand",
            Self::Storage => "StorageBand",
        }
    }

    /// Canonical lower-case keyword used as the band metadata-name
    /// segment (`{env-name}-{name-segment}`). Pinned to the variant
    /// rather than echoed from the operator-side alias: a sweep that
    /// declares `(:kind "mem" …)` produces `<env>-memory`, NOT
    /// `<env>-mem` — every alias funnels to ONE deterministic band
    /// name so two dimensions written with two different aliases
    /// (`"mem"` and `"memory"`) cannot collide-by-shape into
    /// indistinguishable band names; the typed projection is the
    /// canonical-name boundary the substrate's deterministic-output
    /// posture relies on.
    #[must_use]
    pub fn name_segment(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::Cpu => "cpu",
            Self::Storage => "storage",
        }
    }
}

/// Closed-set typed identifier for the `@`-prefixed magic targets a
/// [`MatrixAxis::path`] can write into on the base [`EphemeralSpec`] — the
/// three reachable aplicacao-field write targets ([`Self::Version`] →
/// `aplicacao.version`, [`Self::Profile`] → `aplicacao.profile`,
/// [`Self::ChartRef`] → `aplicacao.chart_ref`) — as a Rust enum, so the
/// three-way (path-string, aplicacao-field) pairing binds at ONE site on
/// the typed algebra rather than at three byte-identical inline arms in
/// [`apply_axis`].
///
/// Pre-lift the magic-target dispatch lived as three arms in
/// [`apply_axis`], each opening its own `val.as_str().to_string() →
/// field = …` skeleton paired with its own `&'static str` literal arm
/// label. The (path-literal, aplicacao-field) pairing was load-bearing
/// across three sites yet only enforced by call-site discipline — a
/// regression that swapped two assignment targets (e.g. routed
/// `"@version"` to `chart_ref`) type-checked but silently mis-applied
/// every operator's matrix sweep. Post-lift the pairing binds at ONE
/// typed projection ([`Self::apply`]) the substrate's invariant relies on:
/// rustc's closed-set match across [`Self`] enforces that every variant
/// has exactly one apply arm and exactly one [`Self::marker`] arm, and
/// the bidirectional contract `from_path(t.marker()) == Some(t)` makes the
/// decode + canonical-marker round-trip a TYPE rather than three string
/// literals scattered across the file.
///
/// Adding a fourth magic-target (e.g. `@release-name` →
/// `aplicacao.release_name`, `@target-namespace` →
/// `aplicacao.target_namespace`) extends the enum AND the three
/// projection arms ([`Self::from_path`], [`Self::marker`], [`Self::apply`])
/// in lockstep — rustc binds the extension through exhaustiveness over
/// the closed enum so a partial extension that forgets ONE projection
/// becomes a compile error rather than a runtime drift.
///
/// Sibling closed-set lift to tatara-lisp's `QuoteForm` (four-of-four
/// homoiconic prefix-wrappers), `UnquoteForm` (two-of-four template-
/// marker subset), `MacroDefHead` (two-of-two macro-definition heads),
/// and `CompilerSpecIoStage` (disk-persistence surface) closed-set
/// algebras: those enums key their respective dispatch / projection
/// variants on a typed identity carried inside the variant; this enum
/// keys the three reachable matrix-axis aplicacao-field write targets
/// on a typed marker identity.
///
/// Theory anchor: THEORY.md §V.1 — knowable platform; the closed set of
/// `@`-prefixed magic targets becomes a TYPE rather than three
/// `&'static str` literals scattered across [`apply_axis`]. A typo in
/// any arm becomes a compile error against the typed projection.
/// THEORY.md §VI.1 — generation over composition; the (path-literal,
/// aplicacao-field) pairing appeared at three arms — past the ≥2
/// PRIME-DIRECTIVE trigger once the structural shape is named.
/// THEORY.md §II.1 invariant 1 — typed entry; the matrix-axis
/// path-string to typed-target decoding IS the typed-entry gate at the
/// `MatrixAxis::path` boundary, and naming the closed-set identity
/// lifts the gate from per-site literal discipline to ONE method
/// the substrate's diagnostic promotions hang off of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixTarget {
    /// `@version` → `aplicacao.version`.
    Version,
    /// `@profile` → `aplicacao.profile`.
    Profile,
    /// `@chart-ref` → `aplicacao.chart_ref`.
    ChartRef,
}

impl MatrixTarget {
    /// Decode a [`MatrixAxis::path`] string into the typed marker, or
    /// `None` for paths that aren't reserved `@`-prefixed magic targets
    /// (they fall through to [`overlay_at_path`] semantics inside
    /// [`apply_axis`]). Closed-set dual of [`Self::marker`]: for every
    /// variant `t`, `from_path(t.marker()) == Some(t)`.
    #[must_use]
    pub fn from_path(path: &str) -> Option<Self> {
        match path {
            "@version" => Some(Self::Version),
            "@profile" => Some(Self::Profile),
            "@chart-ref" => Some(Self::ChartRef),
            _ => None,
        }
    }

    /// Canonical `&'static str` marker — the `@`-prefixed path literal
    /// each variant decodes from. Bidirectional dual of [`Self::from_path`]:
    /// for every variant `t`, `from_path(t.marker()) == Some(t)`. The
    /// `&'static str` lifetime lets consumers (axis-path docstrings,
    /// future `tatara-check` typed-target enumerators, future LSP
    /// completion lists) project through this method without an
    /// allocation, parallel to how `tatara_lisp`'s `QuoteForm::prefix`
    /// / `UnquoteForm::marker` / `CompilerSpecIoStage::operation`
    /// project their closed-set surfaces.
    #[must_use]
    pub fn marker(self) -> &'static str {
        match self {
            Self::Version => "@version",
            Self::Profile => "@profile",
            Self::ChartRef => "@chart-ref",
        }
    }

    /// Apply a string value into the targeted [`AplicacaoIntent`] field
    /// on `spec.aplicacao`. Non-string values are silently ignored —
    /// matching the pre-lift [`apply_axis`] posture (the magic-target
    /// arms only acted when `val.as_str()` succeeded; non-string axis
    /// values for a magic target are dropped, NOT routed to the
    /// overlay). The (variant, field-assignment) pairing binds at ONE
    /// match arm rather than three byte-identical sites — a regression
    /// that drifts ONE arm's field target (e.g. routes
    /// [`Self::Version`] to `chart_ref`) becomes a compile error
    /// against the typed projection.
    pub fn apply(self, spec: &mut EphemeralSpec, val: &serde_json::Value) {
        let Some(s) = val.as_str() else {
            return;
        };
        let s = s.to_string();
        match self {
            Self::Version => spec.aplicacao.version = s,
            Self::Profile => spec.aplicacao.profile = s,
            Self::ChartRef => spec.aplicacao.chart_ref = s,
        }
    }
}

/// Apply one axis value to a spec at the axis's path. Routes through the
/// substrate's [`MatrixTarget`] closed-set dispatch — `@`-prefixed magic
/// targets bind through [`MatrixTarget::from_path`] + [`MatrixTarget::apply`]
/// at ONE typed site rather than three inline match arms; everything
/// else falls through to [`overlay_at_path`].
fn apply_axis(spec: &mut EphemeralSpec, path: &str, val: serde_json::Value) {
    if let Some(target) = MatrixTarget::from_path(path) {
        target.apply(spec, &val);
    } else {
        overlay_at_path(&mut spec.aplicacao.values_overlay, path, val);
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
            breathe: None,
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
    fn breathe_envelope_emits_bands_per_env() {
        let mut m = matrix();
        m.budget.cost_ceiling = Some("$5/h".into());
        m.breathe = Some(BreatheEnvelope {
            dimensions: vec![
                BreatheDimension {
                    kind: "memory".into(),
                    floor: "128Mi".into(),
                    ceiling: "1Gi".into(),
                },
                BreatheDimension {
                    kind: "cpu".into(),
                    floor: "100m".into(),
                    ceiling: "1".into(),
                },
            ],
            cooldown_seconds: 60,
            dry_run: true,
            target_kind: "Deployment".into(),
        });
        let envs = m.expand("echo-sweep");
        let env0 = &envs[0];
        let bands = m.breathe_bands(env0);
        assert_eq!(bands.len(), 2, "one band per dimension");
        let mem = &bands[0];
        assert_eq!(mem["kind"], "MemoryBand");
        assert_eq!(mem["spec"]["targetRef"]["kind"], "Deployment");
        // The band targets the env's per-env Helm release (= env name).
        assert_eq!(mem["spec"]["targetRef"]["name"], env0.name.as_str());
        assert_eq!(mem["spec"]["floor"], "128Mi");
        assert_eq!(mem["spec"]["ceiling"], "1Gi");
        assert_eq!(mem["spec"]["dryRun"], true);
        // The sweep's cost budget rides on every band as an annotation.
        assert_eq!(
            mem["metadata"]["annotations"]["breathe.pleme.io/cost-ceiling"],
            "$5/h"
        );
        assert_eq!(bands[1]["kind"], "CpuBand");

        // No envelope ⇒ no bands.
        let m2 = matrix();
        assert!(m2.breathe_bands(&m2.expand("x")[0]).is_empty());
    }

    #[test]
    fn overlay_at_nested_path_creates_intermediate_objects() {
        let mut root = serde_json::json!({"existing": 1});
        overlay_at_path(&mut root, "a.b.c", serde_json::json!("v"));
        assert_eq!(root["a"]["b"]["c"], "v");
        assert_eq!(root["existing"], 1, "existing keys preserved");
    }

    // ── MatrixTarget: closed-set magic-target dispatch ──────────────
    //
    // The three `@`-prefixed magic-target arms (`@version` /
    // `@profile` / `@chart-ref`) inside the pre-lift `apply_axis` body
    // collapse onto the typed `MatrixTarget` closed-set enum. The
    // tests below pin three structural contracts the lift establishes
    // — bidirection (`from_path ↔ marker`), per-variant apply
    // semantics (each variant writes exactly its named field), and
    // the soft-projection posture (`from_path` returns `None` for
    // non-magic paths so they cascade to `overlay_at_path`).

    #[test]
    fn matrix_target_from_path_round_trips_through_marker_for_every_variant() {
        // BIDIRECTION CONTRACT: for every `MatrixTarget` variant,
        // decoding its canonical marker through `from_path` yields
        // the same variant. Sibling-arm sweep so the three pairings
        // stay load-bearing under reordering refactors — a regression
        // that drifts ONE arm's `from_path → marker` round-trip (e.g.
        // routes `@version` through to `Profile`) fails loudly here.
        for variant in [
            MatrixTarget::Version,
            MatrixTarget::Profile,
            MatrixTarget::ChartRef,
        ] {
            assert_eq!(
                MatrixTarget::from_path(variant.marker()),
                Some(variant),
                "from_path(marker) must round-trip to {variant:?}"
            );
        }
    }

    #[test]
    fn matrix_target_marker_renders_canonical_at_prefixed_path_for_every_variant() {
        // CANONICAL-MARKER CONTRACT: each variant's `marker()` projects
        // to its canonical `@`-prefixed path literal. Pins the literal
        // identity at the typed projection rather than at the inline
        // arms in pre-lift `apply_axis` so a future renaming (e.g.
        // hyphenated `@chart-ref` → `@chartRef` to match camelCase
        // serde rename) lands at ONE method body.
        assert_eq!(MatrixTarget::Version.marker(), "@version");
        assert_eq!(MatrixTarget::Profile.marker(), "@profile");
        assert_eq!(MatrixTarget::ChartRef.marker(), "@chart-ref");
    }

    #[test]
    fn matrix_target_apply_writes_string_value_to_targeted_aplicacao_field() {
        // PER-VARIANT APPLY CONTRACT: each variant's `apply` writes
        // exclusively to its named `aplicacao` field. Pin BOTH the
        // target-field write AND the non-write of the two sibling
        // fields — a regression that drifts ONE arm's assignment
        // target (e.g. routes `Version → chart_ref`) silently
        // corrupts every operator's matrix sweep and would not
        // surface without an explicit per-arm pin.
        let mut spec = base();
        MatrixTarget::Version.apply(&mut spec, &serde_json::json!("9.9.9"));
        assert_eq!(spec.aplicacao.version, "9.9.9");
        assert_eq!(spec.aplicacao.profile, "minimal", "profile untouched");
        assert_eq!(
            spec.aplicacao.chart_ref, "oci://ghcr.io/pleme-io/charts/echo",
            "chart_ref untouched"
        );

        let mut spec = base();
        MatrixTarget::Profile.apply(&mut spec, &serde_json::json!("airgapped"));
        assert_eq!(spec.aplicacao.profile, "airgapped");
        assert_eq!(spec.aplicacao.version, "0.1.0", "version untouched");

        let mut spec = base();
        MatrixTarget::ChartRef.apply(
            &mut spec,
            &serde_json::json!("oci://example.com/charts/other"),
        );
        assert_eq!(spec.aplicacao.chart_ref, "oci://example.com/charts/other");
        assert_eq!(spec.aplicacao.version, "0.1.0", "version untouched");
    }

    #[test]
    fn matrix_target_apply_silently_ignores_non_string_values_for_every_variant() {
        // NON-STRING-VALUE CONTRACT: magic-target arms accept ONLY
        // string values; ints / bools / nulls / arrays / objects are
        // silently dropped (they don't route to `overlay_at_path` —
        // the path already matched a magic target so the fallthrough
        // never fires). Pin the drop-on-non-string posture across all
        // three variants × five non-string shapes so a regression
        // that starts routing through `val.to_string()` (which would
        // stringify `42` into `"42"` and silently mis-write the
        // field) fails loudly here.
        let non_string_values = [
            serde_json::json!(42),
            serde_json::json!(true),
            serde_json::json!(null),
            serde_json::json!([1, 2]),
            serde_json::json!({ "k": "v" }),
        ];
        for variant in [
            MatrixTarget::Version,
            MatrixTarget::Profile,
            MatrixTarget::ChartRef,
        ] {
            for val in &non_string_values {
                let mut spec = base();
                let before = (
                    spec.aplicacao.version.clone(),
                    spec.aplicacao.profile.clone(),
                    spec.aplicacao.chart_ref.clone(),
                );
                variant.apply(&mut spec, val);
                let after = (
                    spec.aplicacao.version.clone(),
                    spec.aplicacao.profile.clone(),
                    spec.aplicacao.chart_ref.clone(),
                );
                assert_eq!(
                    before, after,
                    "{variant:?}.apply({val}) must NOT mutate aplicacao on non-string"
                );
            }
        }
    }

    #[test]
    fn matrix_target_from_path_rejects_non_magic_path_strings_to_cascade_through_overlay() {
        // SOFT-PROJECTION CONTRACT: `from_path` returns `None` for
        // every shape that isn't an `@`-prefixed reserved magic
        // target — the empty path, plain dotted-paths, plain
        // identifiers, and even `@`-prefixed strings that aren't in
        // the closed set. The `None` return is load-bearing: it
        // signals `apply_axis` to cascade into `overlay_at_path`, so
        // a regression that starts admitting near-miss `@`-prefixes
        // (e.g. `@chart-Ref` via case-insensitive matching) would
        // silently route plain overlay paths through the magic-target
        // dispatch — fails loudly here.
        for non_magic in [
            "",
            "replicaCount",
            "feature.flag",
            "image.tag",
            "@versoin",  // typo → not a magic target
            "@chartRef", // missing hyphen → not a magic target
            "version",   // missing `@` prefix → not a magic target
        ] {
            assert_eq!(
                MatrixTarget::from_path(non_magic),
                None,
                "{non_magic:?} must NOT decode as a magic target"
            );
        }
    }

    #[test]
    fn apply_axis_routes_magic_target_paths_through_matrix_target_apply() {
        // PATH-UNIFORMITY CONTRACT (apply_axis side): the lifted
        // `apply_axis` routes its three magic-target arms through
        // `MatrixTarget::from_path` + `MatrixTarget::apply`. Pin that
        // the legacy per-arm assignment and the typed-projection
        // composition AGREE bit-for-bit across every magic target —
        // a regression in `apply_axis` that bypasses the typed
        // projection (e.g. reverts to inline `match path { ... }`
        // arms) AND accidentally swaps two field targets silently
        // corrupts every operator's matrix sweep; this test catches
        // the drift via the typed-marker dispatch.
        for variant in [
            MatrixTarget::Version,
            MatrixTarget::Profile,
            MatrixTarget::ChartRef,
        ] {
            let mut via_axis = base();
            apply_axis(
                &mut via_axis,
                variant.marker(),
                serde_json::json!("sentinel-VAL"),
            );
            let mut via_target = base();
            variant.apply(&mut via_target, &serde_json::json!("sentinel-VAL"));
            assert_eq!(
                via_axis.aplicacao.version, via_target.aplicacao.version,
                "{variant:?}: apply_axis.version drifted from MatrixTarget::apply"
            );
            assert_eq!(
                via_axis.aplicacao.profile, via_target.aplicacao.profile,
                "{variant:?}: apply_axis.profile drifted from MatrixTarget::apply"
            );
            assert_eq!(
                via_axis.aplicacao.chart_ref, via_target.aplicacao.chart_ref,
                "{variant:?}: apply_axis.chart_ref drifted from MatrixTarget::apply"
            );
        }
    }

    // ── BreatheDimensionKind: closed-set dimension dispatch ───────────
    //
    // The string-input `band_kind_for` helper paired with an inline
    // `dim.kind.to_ascii_lowercase()` name-segment site collapses onto
    // the typed `BreatheDimensionKind` closed-set enum. The tests
    // below pin five structural contracts the lift establishes —
    // primary-keyword decode, alias-equivalence (each alias decodes to
    // the SAME variant as the primary), per-variant `band_kind` /
    // `name_segment` projection, unknown-keyword drop, and the
    // canonical-name contract that `breathe_bands` emits the SAME
    // band-name regardless of which alias the operator wrote (the
    // load-bearing improvement over pre-lift's per-alias name drift).
    const BREATHE_DIM_VARIANTS: [BreatheDimensionKind; 3] = [
        BreatheDimensionKind::Memory,
        BreatheDimensionKind::Cpu,
        BreatheDimensionKind::Storage,
    ];

    #[test]
    fn breathe_dimension_kind_from_keyword_round_trips_through_band_kind_for_every_variant() {
        // PRIMARY-KEYWORD CONTRACT: for every `BreatheDimensionKind`
        // variant, decoding its `name_segment` (the canonical primary
        // keyword `memory` / `cpu` / `storage`) through `from_keyword`
        // yields the same variant. Sibling-arm sweep so the three
        // pairings stay load-bearing under reordering refactors — a
        // regression that drifts ONE arm's `from_keyword → name_segment`
        // round-trip (e.g. routes `"cpu"` through to `Memory`) fails
        // loudly here.
        for variant in BREATHE_DIM_VARIANTS {
            assert_eq!(
                BreatheDimensionKind::from_keyword(variant.name_segment()),
                Some(variant),
                "from_keyword(name_segment) must round-trip to {variant:?}"
            );
        }
    }

    #[test]
    fn breathe_dimension_kind_aliases_decode_to_the_same_variant_as_the_primary_keyword() {
        // ALIAS-EQUIVALENCE CONTRACT: aliases (`mem` for Memory,
        // `disk` for Storage) decode to the SAME variant as the
        // primary keyword. Cpu has no alias so the (variant,
        // alias-set) table is asymmetric — pin every (alias, variant)
        // pair explicitly so a regression that adds a wrong alias
        // (e.g. `"hdd" → Cpu`) fails loudly here.
        let pairs: &[(&str, BreatheDimensionKind)] = &[
            ("mem", BreatheDimensionKind::Memory),
            ("memory", BreatheDimensionKind::Memory),
            ("MEM", BreatheDimensionKind::Memory),
            ("Memory", BreatheDimensionKind::Memory),
            ("cpu", BreatheDimensionKind::Cpu),
            ("CPU", BreatheDimensionKind::Cpu),
            ("Cpu", BreatheDimensionKind::Cpu),
            ("disk", BreatheDimensionKind::Storage),
            ("storage", BreatheDimensionKind::Storage),
            ("DISK", BreatheDimensionKind::Storage),
            ("Storage", BreatheDimensionKind::Storage),
        ];
        for (keyword, expected) in pairs {
            assert_eq!(
                BreatheDimensionKind::from_keyword(keyword),
                Some(*expected),
                "from_keyword({keyword:?}) must decode as {expected:?}"
            );
        }
    }

    #[test]
    fn breathe_dimension_kind_band_kind_projects_canonical_cr_kind_for_every_variant() {
        // CANONICAL-CR-KIND CONTRACT: each variant's `band_kind()`
        // projects to its canonical breathe Band CR kind literal
        // (`MemoryBand` / `CpuBand` / `StorageBand`) — the wire-format
        // string the `kind:` field on the emitted CR carries. Pins
        // the literal identity at the typed projection rather than
        // at the inline arms in pre-lift `band_kind_for` so a future
        // rename (e.g. `MemoryBand` → `MemBand`) lands at ONE method
        // body.
        assert_eq!(BreatheDimensionKind::Memory.band_kind(), "MemoryBand");
        assert_eq!(BreatheDimensionKind::Cpu.band_kind(), "CpuBand");
        assert_eq!(BreatheDimensionKind::Storage.band_kind(), "StorageBand");
    }

    #[test]
    fn breathe_dimension_kind_name_segment_canonicalizes_aliases_to_the_primary_keyword() {
        // CANONICAL-NAME-SEGMENT CONTRACT: for every alias of a
        // variant, `from_keyword(alias).name_segment()` MUST equal
        // the primary-keyword name segment — NOT the alias the
        // operator wrote. Pre-lift the band metadata name echoed
        // `dim.kind.to_ascii_lowercase()` so an operator who wrote
        // `(:kind "mem" …)` got a band named `<env>-mem` while
        // another who wrote `(:kind "memory" …)` got `<env>-memory`;
        // two semantically-identical sweeps produced two different
        // band-name surfaces and no test caught the drift. Post-lift
        // the name segment binds to the typed variant so EVERY alias
        // funnels to ONE canonical band name.
        let pairs: &[(&str, &str)] = &[
            ("mem", "memory"),
            ("memory", "memory"),
            ("MEM", "memory"),
            ("cpu", "cpu"),
            ("CPU", "cpu"),
            ("disk", "storage"),
            ("storage", "storage"),
            ("DISK", "storage"),
        ];
        for (alias, canonical) in pairs {
            let kind = BreatheDimensionKind::from_keyword(alias)
                .expect("alias must decode to a known dimension");
            assert_eq!(
                kind.name_segment(),
                *canonical,
                "from_keyword({alias:?}).name_segment() must canonicalize to {canonical:?}"
            );
        }
    }

    #[test]
    fn breathe_dimension_kind_from_keyword_rejects_unknown_keywords() {
        // UNKNOWN-KEYWORD CONTRACT: `from_keyword` returns `None` for
        // every shape outside the closed set — the empty string,
        // near-miss typos, and dimension keywords the substrate does
        // not (yet) support. The `None` return is load-bearing: it
        // signals `breathe_bands` to drop the dimension via
        // `filter_map`, so a regression that starts admitting
        // near-miss keywords (e.g. case-fold matching `"net" →
        // Network` against a Network variant that doesn't exist)
        // would silently route unrelated dimensions through the
        // dispatch — fails loudly here.
        for unknown in [
            "", "network", // not yet a supported dimension
            "gpu",     // not yet a supported dimension
            "memoryy", // typo
            "cp",      // truncated
            "diskz",   // suffix
        ] {
            assert_eq!(
                BreatheDimensionKind::from_keyword(unknown),
                None,
                "{unknown:?} must NOT decode as a breathe dimension"
            );
        }
    }

    #[test]
    fn breathe_bands_emits_canonical_name_segment_regardless_of_operator_alias() {
        // END-TO-END CANONICAL-NAME CONTRACT (breathe_bands side):
        // two sweeps that declare the SAME dimension under two
        // different aliases (`(:kind "mem" …)` vs `(:kind "memory"
        // …)`) emit Band CRs with the SAME band metadata name. Pre-
        // lift this assertion FAILED — the `"mem"` sweep produced
        // `<env>-mem` and the `"memory"` sweep produced `<env>-
        // memory`. Post-lift the name segment routes through
        // `BreatheDimensionKind::name_segment` so every alias funnels
        // to one canonical name. A regression that reverts to
        // echoing `dim.kind.to_ascii_lowercase()` would silently
        // re-introduce the drift; this test catches it.
        let envelope = |kind: &str| BreatheEnvelope {
            dimensions: vec![BreatheDimension {
                kind: kind.into(),
                floor: "128Mi".into(),
                ceiling: "1Gi".into(),
            }],
            cooldown_seconds: 60,
            dry_run: true,
            target_kind: "Deployment".into(),
        };
        let mut m_mem = matrix();
        m_mem.breathe = Some(envelope("mem"));
        let mut m_memory = matrix();
        m_memory.breathe = Some(envelope("memory"));
        let mut m_upper = matrix();
        m_upper.breathe = Some(envelope("MEMORY"));

        let envs = matrix().expand("echo-sweep");
        let env0 = &envs[0];

        let bands_mem = m_mem.breathe_bands(env0);
        let bands_memory = m_memory.breathe_bands(env0);
        let bands_upper = m_upper.breathe_bands(env0);

        assert_eq!(bands_mem.len(), 1);
        assert_eq!(bands_memory.len(), 1);
        assert_eq!(bands_upper.len(), 1);

        let expected_name = format!("{}-memory", env0.name);
        assert_eq!(bands_mem[0]["metadata"]["name"], expected_name);
        assert_eq!(bands_memory[0]["metadata"]["name"], expected_name);
        assert_eq!(bands_upper[0]["metadata"]["name"], expected_name);

        // The CR kind also canonicalizes — every alias projects to
        // `MemoryBand` (the wire-format kind).
        assert_eq!(bands_mem[0]["kind"], "MemoryBand");
        assert_eq!(bands_memory[0]["kind"], "MemoryBand");
        assert_eq!(bands_upper[0]["kind"], "MemoryBand");
    }

    #[test]
    fn breathe_bands_drops_dimensions_with_unknown_kind_keywords() {
        // UNKNOWN-DIMENSION DROP CONTRACT (breathe_bands side): a
        // dimension whose keyword `from_keyword` doesn't recognize
        // drops out via `filter_map` — the sweep continues with the
        // remaining recognized dimensions. Pin the drop here so a
        // regression that starts emitting bands with raw / unmapped
        // `kind:` values (e.g. an inline fallback that bypasses the
        // typed projection) fails loudly.
        let mut m = matrix();
        m.breathe = Some(BreatheEnvelope {
            dimensions: vec![
                BreatheDimension {
                    kind: "memory".into(),
                    floor: "128Mi".into(),
                    ceiling: "1Gi".into(),
                },
                BreatheDimension {
                    kind: "network".into(), // unrecognized — must drop
                    floor: "1Mbps".into(),
                    ceiling: "100Mbps".into(),
                },
                BreatheDimension {
                    kind: "cpu".into(),
                    floor: "100m".into(),
                    ceiling: "1".into(),
                },
            ],
            cooldown_seconds: 60,
            dry_run: true,
            target_kind: "Deployment".into(),
        });
        let envs = m.expand("echo-sweep");
        let bands = m.breathe_bands(&envs[0]);
        assert_eq!(bands.len(), 2, "unknown dimension `network` must drop");
        assert_eq!(bands[0]["kind"], "MemoryBand");
        assert_eq!(bands[1]["kind"], "CpuBand");
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
