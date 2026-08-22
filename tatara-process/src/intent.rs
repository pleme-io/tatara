//! Intent — where the rendered artifacts come from.
//!
//! Exactly one field on `Intent` must be set. The reconciler's RENDER phase
//! selects a driver based on which variant is present:
//!   - `nix`:        tatara-engine `nix_eval` → resources
//!   - `flux`:       pass through an existing `GitRepository`
//!   - `lisp`:       tatara-lisp reader + macroexpander → resources
//!   - `container`:  emit Deployment/StatefulSet/etc directly (no Helm)
//!   - `aplicacao`:  emit a FluxCD `HelmRelease` for a pleme-io typed
//!                   Aplicacao chart (e.g. `lareira-demo-app`).
//!                   This is the canonical handoff from caixa-shaped
//!                   declarations to in-cluster reconciliation.
//!   - `guest`:      tatara-hospedeiro supervises a Linux VM or WASM
//!                   component. See `tatara/docs/declarative-guests.md`.
//!                   The GuestSpec itself is type-erased here (JSON value)
//!                   so tatara-process stays decoupled from tatara-vm;
//!                   hospedeiro re-parses the value as GuestSpec on boot.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Intent — exactly one variant should be populated.
#[derive(Clone, Debug, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nix: Option<NixIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flux: Option<FluxIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lisp: Option<LispIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<ContainerIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aplicacao: Option<AplicacaoIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guest: Option<GuestIntent>,
}

/// Enum view over the populated variant — convenience for the reconciler.
#[derive(Clone, Debug)]
pub enum IntentVariant<'a> {
    Nix(&'a NixIntent),
    Flux(&'a FluxIntent),
    Lisp(&'a LispIntent),
    Container(&'a ContainerIntent),
    Aplicacao(&'a AplicacaoIntent),
    Guest(&'a GuestIntent),
}

impl IntentVariant<'_> {
    /// Reverse projection — every borrowed variant knows its
    /// `IntentKind` discriminator. Pairs with `IntentKind::select`
    /// so `IntentKind::select(intent).map(|v| v.kind())` round-trips
    /// the closed set; pinned by the substrate testkit
    /// [`crate::tagged_union::assert_variant_round_trip`] shared
    /// across every `<T: TaggedUnion>` implementor. The inherent
    /// method stays load-bearing (the `.kind()` calling convention
    /// pre-dates the trait lift; no consumer needs `use
    /// crate::tagged_union::VariantKind` to reach the reverse
    /// projection) while the trait impl below delegates to this body
    /// as the ground-truth arm-to-Kind mapping.
    pub fn kind(&self) -> IntentKind {
        match self {
            Self::Nix(_) => IntentKind::Nix,
            Self::Flux(_) => IntentKind::Flux,
            Self::Lisp(_) => IntentKind::Lisp,
            Self::Container(_) => IntentKind::Container,
            Self::Aplicacao(_) => IntentKind::Aplicacao,
            Self::Guest(_) => IntentKind::Guest,
        }
    }

    /// Canonical attestation-pillar bytes for the populated variant —
    /// `serde_json::to_vec` on the inner reference, with an empty
    /// fallback that matches the pre-lift Observe-mode shape in
    /// `tatara-reconciler::render`. ONE site owns the per-variant
    /// serialization so adding a 7th variant requires only the
    /// arm here, not the parallel match the pre-lift Observe arm
    /// carried.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        match self {
            Self::Nix(n) => serde_json::to_vec(n).unwrap_or_default(),
            Self::Flux(f) => serde_json::to_vec(f).unwrap_or_default(),
            Self::Lisp(l) => serde_json::to_vec(l).unwrap_or_default(),
            Self::Container(c) => serde_json::to_vec(c).unwrap_or_default(),
            Self::Aplicacao(a) => serde_json::to_vec(a).unwrap_or_default(),
            Self::Guest(g) => serde_json::to_vec(g).unwrap_or_default(),
        }
    }
}

impl crate::tagged_union::VariantKind<IntentKind> for IntentVariant<'_> {
    fn variant_kind(&self) -> IntentKind {
        self.kind()
    }
}

/// Closed-set discriminator over `Intent`'s six tagged-union slots.
/// Single source of truth that drives `Intent::variant`'s ambiguity
/// + emptiness resolver, the `IntentError::Empty` message, and the
/// reverse `IntentVariant::kind` projection. Adding a 7th intent
/// variant lands at one `ALL` entry + one `as_str` arm + one
/// `select` arm + one `IntentVariant::kind` arm — exhaustively
/// checked by the compiler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, tatara_closed_set::DeriveClosedSet)]
#[closed_set(via = "as_str", generate_unknown, display)]
pub enum IntentKind {
    Nix,
    Flux,
    Lisp,
    Container,
    Aplicacao,
    Guest,
}

impl IntentKind {
    /// The closed set of intent kinds — single source of truth that
    /// drives `Intent::variant`'s sweep so a variant added without
    /// an `ALL` entry never reaches the resolver.
    pub const ALL: [Self; 6] = [
        Self::Nix,
        Self::Flux,
        Self::Lisp,
        Self::Container,
        Self::Aplicacao,
        Self::Guest,
    ];

    /// Canonical lower-case wire-format key — matches the serde
    /// `rename_all = "camelCase"` field name on `Intent`. The
    /// `IntentError::Empty` message composes the human-readable
    /// list from this projection so a new variant lands in the
    /// operator-facing diagnostic automatically via the `ALL`
    /// sweep, not via hand-maintained error-string drift.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nix => "nix",
            Self::Flux => "flux",
            Self::Lisp => "lisp",
            Self::Container => "container",
            Self::Aplicacao => "aplicacao",
            Self::Guest => "guest",
        }
    }

    /// Project an `Intent` borrow into the optional typed variant
    /// view for this kind. Returns `None` iff the matching slot is
    /// `None`. Composes the closed-set sweep `Intent::variant`
    /// loops over.
    pub fn select<'a>(self, intent: &'a Intent) -> Option<IntentVariant<'a>> {
        match self {
            Self::Nix => intent.nix.as_ref().map(IntentVariant::Nix),
            Self::Flux => intent.flux.as_ref().map(IntentVariant::Flux),
            Self::Lisp => intent.lisp.as_ref().map(IntentVariant::Lisp),
            Self::Container => intent.container.as_ref().map(IntentVariant::Container),
            Self::Aplicacao => intent.aplicacao.as_ref().map(IntentVariant::Aplicacao),
            Self::Guest => intent.guest.as_ref().map(IntentVariant::Guest),
        }
    }
}

crate::declare_tagged_union_error! {
    pub IntentError,
    empty = "intent has no variant set (one of {0} required)",
    ambiguous = "intent has multiple variants set; exactly one required",
}

/// Slash-joined list of every `IntentKind::as_str()` — composed once
/// at compile time so `IntentError::Empty`'s diagnostic carries the
/// closed-set summary without per-variant string drift. Pinned against
/// the canonical [`tatara_lisp::ClosedSet::labels_joined`] projection
/// by `intent_error_empty_lists_every_kind_in_canonical_order`, so a
/// regression that drifts this `&'static str` constant from the
/// `IntentKind::ALL × as_str` composition fails-loudly at the test
/// site without per-variant inline materialization.
pub(crate) const INTENT_KIND_LIST: &str = "nix/flux/lisp/container/aplicacao/guest";

// `impl FromStr for IntentKind` +
// `impl tatara_lisp::ClosedSet for IntentKind` +
// `impl fmt::Display for IntentKind` +
// `pub struct UnknownIntentKind(pub String)` are all generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown, display)]` on the
// enum declaration above. `label` delegates to the inherent
// `IntentKind::as_str` — the camelCase wire-vocabulary projection
// stays load-bearing (matches the serde `rename_all = "camelCase"`
// field names on `Intent` AND the `IntentVariant::canonical_bytes`
// per-variant arm), while generic `T: ClosedSet` consumers reach the
// STABLE workspace-wide name (`label`). The auto-derived carrier
// label "intent kind" matches the substrate-wide
// `#[error("unknown intent kind: {0}")]` shape every sibling
// closed-set carrier across `tatara-process` renders verbatim.
// Symmetric to [`crate::intent::WorkloadKind`] (the workload-axis
// sibling on the same `ProcessSpec` slice) and every other
// `#[derive(DeriveClosedSet)]` implementor across the crate.

crate::declare_tagged_union_impls! {
    parent = Intent,
    kind = IntentKind,
    variant = IntentVariant,
    error = IntentError,
    kind_list = INTENT_KIND_LIST,
}

/// Nix-sourced intent — tatara-engine's nix_eval driver produces resources.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NixIntent {
    /// Flake reference, e.g., `github:pleme-io/k8s?dir=shared/infrastructure`.
    pub flake_ref: String,
    /// Attribute path within the flake (e.g., `observability`).
    pub attribute: String,
    /// Target system. Defaults to the controller host's system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Attic cache to push the resulting store path into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attic_cache: Option<String>,
    /// Additional `nix build` arguments (e.g., `["--impure"]`).
    #[serde(default)]
    pub extra_args: Vec<String>,
    /// Delegate the actual build to a sibling NixBuild CRD
    /// (bridges to tatara-operator NATS bare-metal builder path).
    #[serde(default)]
    pub delegate_to_nix_build: bool,
}

/// FluxCD passthrough intent — reuse an existing GitRepository.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FluxIntent {
    /// Name of an existing `GitRepository` (typically in `flux-system`).
    pub git_repository: String,
    /// Path inside the repository that the Kustomization will apply.
    pub path: String,
    /// Optional namespace of the GitRepository CR (defaults to `flux-system`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_repository_namespace: Option<String>,
    /// Optional target namespace for the emitted Kustomization.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_namespace: Option<String>,
    /// SOPS decryption — defaults to true to match pleme-io conventions.
    #[serde(default = "default_true")]
    pub decrypt_sops: bool,
    /// If set, additionally emit a HelmRelease for this chart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helm_chart: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helm_values: Option<BTreeMap<String, serde_json::Value>>,
}

fn default_true() -> bool {
    true
}

/// Lisp-sourced intent — tatara-lisp reader + macroexpander produces resources.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LispIntent {
    /// Raw S-expression source, OR `include:<path>` / `configmap:<name>/<key>` pointer.
    pub source: String,
    /// Reader dialect / version tag.
    #[serde(default = "default_reader")]
    pub reader: String,
    /// Macro form version.
    #[serde(default = "default_version")]
    pub version: String,
    /// Symbols injected into the reader env (e.g., `cluster`, `region`).
    #[serde(default)]
    pub bindings: BTreeMap<String, serde_json::Value>,
}

fn default_reader() -> String {
    "tatara-lisp".to_string()
}
fn default_version() -> String {
    "v1".to_string()
}

/// Aplicacao intent — emit a FluxCD `HelmRelease` for a pleme-io
/// typed Aplicacao chart. The chart owns its own sub-chart DAG;
/// the reconciler only watches `HelmRelease.status.conditions[type=Ready]`.
///
/// This is the canonical handoff from caixa `(defaplicacao …)` declarations
/// (which the typescape renders to this Intent) into in-cluster
/// reconciliation. Closed-loop ephemeral test environments use this
/// variant with `:lifetime :ephemeral` on the surrounding ProcessSpec.
///
/// Example (Lisp):
/// ```lisp
/// :intent (:aplicacao
///           (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-demo-app"
///            :version "0.5.5"
///            :profile "all-in-one"
///            :values-overlay (:cluster (:name "ephemeral-test-01")
///                             :persistence false
///                             :compliance (:overlays []))))
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AplicacaoIntent {
    /// Helm chart reference. OCI (`oci://…`) or repo-relative (`pleme-io/lareira-demo-app`).
    pub chart_ref: String,
    /// Chart version (Helm semver constraint; `">=0.5.5"` allowed).
    pub version: String,
    /// Architecture profile from the chart's `values/*.yaml` family
    /// (e.g. `all-in-one`, `saas-internal`).
    /// Leave empty to use chart defaults.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub profile: String,
    /// Typed values overlay merged on top of the profile.
    /// Free-form JSON to keep tatara-process decoupled from chart schemas.
    #[serde(default)]
    #[schemars(schema_with = "crate::schema_helpers::preserve_unknown_object")]
    pub values_overlay: serde_json::Value,
    /// HelmRelease name override. Defaults to the Process's PID-derived name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_name: Option<String>,
    /// Target namespace for the chart. Defaults to the Process's namespace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_namespace: Option<String>,
    /// Install timeout (`humantime` duration). Empty = chart-controller default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_timeout: Option<String>,
}

/// Container intent — direct Deployment/StatefulSet/etc, no Helm.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContainerIntent {
    pub image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicas: Option<i32>,
    #[serde(default)]
    pub command: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub workload_kind: WorkloadKind,
}

/// K8s workload kind the `container` intent renders into. PascalCase
/// values match the K8s `kind:` field on the emitted manifest verbatim,
/// so `as_str` doubles as the canonical `kind:` projection at render time.
#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    JsonSchema,
    Default,
    tatara_closed_set::DeriveClosedSet,
)]
#[serde(rename_all = "PascalCase")]
#[closed_set(via = "as_str", generate_unknown, display)]
pub enum WorkloadKind {
    #[default]
    Deployment,
    StatefulSet,
    DaemonSet,
    Job,
    CronJob,
}

impl WorkloadKind {
    /// The closed set of workload kinds — single source of truth that
    /// drives the `as_str` / Display / `FromStr` triad and the typed
    /// `api_version` / `is_batch` projections. Adding a sixth variant
    /// lands at one `ALL` entry + one `as_str` arm + one arm in each
    /// projection — exhaustively checked by the compiler (the `[Self; 5]`
    /// array literal forces the arity).
    ///
    /// Sibling closed-set lifts on the same `ProcessSpec` axis:
    /// [`crate::encapsulates::EncapsulationMode::ALL`],
    /// [`crate::export::ExportTrigger::ALL`],
    /// [`crate::export::ReportFormat::ALL`],
    /// [`crate::lifetime::TeardownPolicy::ALL`],
    /// [`crate::intent::IntentKind::ALL`],
    /// [`crate::lifetime::LifetimeKind::ALL`],
    /// [`crate::boundary::ConditionKind::ALL`],
    /// [`crate::phase::ProcessPhase::ALL`],
    /// [`crate::signal::ProcessSignal::ALL`].
    pub const ALL: [Self; 5] = [
        Self::Deployment,
        Self::StatefulSet,
        Self::DaemonSet,
        Self::Job,
        Self::CronJob,
    ];

    /// Canonical PascalCase wire-format projection — matches the serde
    /// `rename_all = "PascalCase"` output verbatim AND the K8s manifest
    /// `kind:` field the `container` intent's future renderer will emit.
    /// Used by Display (single source of truth), by `FromStr` to identify
    /// the variant from its annotation / status-field representation, and
    /// by operator-facing reason strings without reaching for `{:?}` Debug
    /// formatting. Pinned by `workload_kind_as_str_matches_serde`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Deployment => "Deployment",
            Self::StatefulSet => "StatefulSet",
            Self::DaemonSet => "DaemonSet",
            Self::Job => "Job",
            Self::CronJob => "CronJob",
        }
    }

    /// Canonical K8s `apiVersion:` projection — `apps/v1` for the
    /// long-running workload trio, `batch/v1` for the batch pair.
    /// Single source of truth for the apiVersion the `container` intent
    /// renderer will stamp on the emitted manifest; pinned by
    /// `workload_kind_projection_truth_table` so a future variant lands
    /// at one arm here, not at every render site that previously
    /// hand-rolled `match kind { Job | CronJob => "batch/v1", _ => … }`.
    ///
    /// Closed-set match (not `matches!`) so adding a sixth variant
    /// triggers the compiler's exhaustiveness check at this site
    /// rather than silently defaulting to either group.
    pub const fn api_version(self) -> &'static str {
        match self {
            Self::Deployment | Self::StatefulSet | Self::DaemonSet => "apps/v1",
            Self::Job | Self::CronJob => "batch/v1",
        }
    }

    /// True iff the workload kind is a batch (terminating) workload —
    /// `Job` or `CronJob`. Drives the future container renderer's
    /// decision between persistent / one-shot retry semantics and lets
    /// the lifetime clock distinguish "naturally terminates" from "runs
    /// until SIGTERM" without re-deriving the partition from
    /// `api_version() == "batch/v1"`.
    ///
    /// Closed-set match (not `matches!`) so adding a sixth variant
    /// triggers the compiler's exhaustiveness check at this site.
    pub const fn is_batch(self) -> bool {
        match self {
            Self::Job | Self::CronJob => true,
            Self::Deployment | Self::StatefulSet | Self::DaemonSet => false,
        }
    }
}

// `impl FromStr for WorkloadKind` +
// `impl tatara_lisp::ClosedSet for WorkloadKind` +
// `impl fmt::Display for WorkloadKind` +
// `pub struct UnknownWorkloadKind(pub String)` are all generated by
// `#[derive(tatara_closed_set::DeriveClosedSet)]` +
// `#[closed_set(via = "as_str", generate_unknown, display)]` on the
// enum declaration above. `label` delegates to the inherent
// `WorkloadKind::as_str` — the PascalCase wire-vocabulary projection
// stays load-bearing (matches the serde `rename_all = "PascalCase"`
// output AND the K8s manifest `kind:` field verbatim), while generic
// `T: ClosedSet` consumers reach the STABLE workspace-wide name
// (`label`). The auto-derived carrier label "workload kind" matches
// the prior hand-rolled `#[error("unknown workload kind: {0}")]`
// annotation byte-for-byte. Symmetric to every other
// `#[derive(DeriveClosedSet)]` implementor across the crate.

/// Guest intent — the Process is a Linux VM or WASM component supervised
/// by `tatara-hospedeiro`. See `tatara/docs/declarative-guests.md`.
///
/// The actual `GuestSpec` is stored as a serde JSON value to keep
/// `tatara-process` decoupled from `tatara-vm`. Hospedeiro re-parses
/// the value as the concrete `tatara_vm::GuestSpec` at boot time; a
/// round-trip test on the tatara-vm side guarantees the shape.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GuestIntent {
    /// The (defguest …) spec as JSON. Shape matches `tatara_vm::GuestSpec`.
    #[schemars(schema_with = "crate::schema_helpers::preserve_unknown_object")]
    pub spec: serde_json::Value,

    /// Where to write per-guest state on the host (logs, socket, PID file).
    /// Defaults to `~/.local/state/tatara/guests/<name>/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_dir: Option<String>,

    /// Whether hospedeiro is allowed to pull guest artifacts from a remote
    /// transport (Attic, ssh-ng) if not already present locally. The
    /// default is taken from the GuestSpec's `buildOn` field; setting
    /// this explicitly overrides at the intent layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_remote_build: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_intent_errors() {
        let i = Intent::default();
        match i.variant().unwrap_err() {
            IntentError::Empty(list) => assert_eq!(list, INTENT_KIND_LIST),
            other => panic!("expected Empty, got {other:?}"),
        }
    }

    #[test]
    fn exactly_one_ok() {
        let i = Intent {
            nix: Some(NixIntent {
                flake_ref: "github:a/b".into(),
                attribute: "x".into(),
                system: None,
                attic_cache: None,
                extra_args: vec![],
                delegate_to_nix_build: false,
            }),
            ..Intent::default()
        };
        assert!(matches!(i.variant().unwrap(), IntentVariant::Nix(_)));
    }

    /// AMBIGUOUS-PATH CONTRACT: when two slots are populated the
    /// resolver yields `Ambiguous`, exhaustively across every pair in
    /// `ALL × ALL` (excluding the diagonal). Routes through the
    /// substrate primitive
    /// [`crate::tagged_union::assert_two_slots_ambiguous`] shared with
    /// the sibling
    /// `encapsulation_kind_two_slots_is_ambiguous_across_every_pair`
    /// / `artifact_source_two_slots_is_ambiguous_across_every_pair`
    /// / `vector_channel_two_slots_is_ambiguous_across_every_pair`
    /// sites. Subsumes the pre-lift hand-authored two-pair probes
    /// (`nix + flux`, `nix + guest`) with exhaustive `6 × 5 = 30`
    /// coverage — every off-diagonal pair on `IntentKind` is pinned.
    #[test]
    fn intent_two_slots_is_ambiguous_across_every_pair() {
        crate::tagged_union::assert_two_slots_ambiguous::<Intent, _>(two_slot_intent);
    }

    #[test]
    fn guest_intent_selects_its_variant() {
        let i = Intent {
            guest: Some(GuestIntent {
                spec: serde_json::json!({
                    "name": "fast-fn",
                    "kind": { "kind": "wasm", "runtime": "wasmtime",
                              "wasiPreview": "p2",
                              "component": { "kind": "flake",
                                             "value": {"url":"github:x/y","attr":"wasi"} },
                              "features": { "simd": true } },
                    "cmdline": []
                }),
                state_dir: None,
                allow_remote_build: Some(true),
            }),
            ..Intent::default()
        };
        match i.variant().unwrap() {
            IntentVariant::Guest(g) => {
                assert_eq!(g.spec["name"], "fast-fn");
                assert_eq!(g.allow_remote_build, Some(true));
            }
            other => panic!("expected Guest, got {other:?}"),
        }
    }

    #[test]
    fn aplicacao_intent_selects_its_variant() {
        let i = Intent {
            aplicacao: Some(AplicacaoIntent {
                chart_ref: "oci://ghcr.io/pleme-io/charts/lareira-demo-app".into(),
                version: "0.5.5".into(),
                profile: "all-in-one".into(),
                values_overlay: serde_json::json!({ "cluster": { "name": "test-01" } }),
                release_name: None,
                target_namespace: None,
                install_timeout: Some("25m".into()),
            }),
            ..Intent::default()
        };
        match i.variant().unwrap() {
            IntentVariant::Aplicacao(a) => {
                assert_eq!(a.profile, "all-in-one");
                assert_eq!(a.version, "0.5.5");
                assert_eq!(a.install_timeout.as_deref(), Some("25m"));
            }
            other => panic!("expected Aplicacao, got {other:?}"),
        }
    }

    /// Structural well-formedness of [`IntentKind`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all structural invariants (`ALL` is
    /// non-empty, every variant round-trips through `label ↔
    /// parse_label`, labels are pairwise distinct, `""` is outside
    /// the closed set, the `UnknownIntentKind` carrier's Display
    /// renders the substrate-wide `"unknown intent kind: <input>"`
    /// shape, `labels()` equals the natural `ALL × label` projection,
    /// `parse_label_with_hint` composes `parse_label` +
    /// `suggest_closest` verbatim) at ONE call site. Replaces the
    /// hand-derived `intent_kind_all_is_unique_and_complete` —
    /// clause (1)+(3) of the testkit subsume the uniqueness +
    /// non-emptiness sweep that test pinned independently.
    #[test]
    fn intent_kind_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<IntentKind>();
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. Symmetric to the
    /// sibling `workload_kind_display_matches_as_str` invariant; if a
    /// reviewer accidentally re-introduces an inline match in Display,
    /// this test would fail the moment a variant rename touches one
    /// site but not the other.
    ///
    /// Routes through the substrate primitive
    /// [`crate::tagged_union::assert_display_matches_label`], which
    /// composes `<T as ClosedSet>::label` against `T::to_string`
    /// byte-identically for every `<T: ClosedSet + Display>`
    /// implementor — the Display-alignment testkit shared with every
    /// sibling `X_display_matches_as_str` site across the crate.
    /// Pre-lift the 27 bodies each restated the same
    /// `for k in K::ALL { assert_eq!(k.to_string(), k.as_str()) }`
    /// two-line probe at the test surface; post-lift the projection
    /// lives at ONE substrate primitive and every site binds through
    /// a single call.
    #[test]
    fn intent_kind_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<IntentKind>();
    }

    /// CANONICAL-KEY CONTRACT: each variant's `as_str()` matches the
    /// camelCase serde field name on `Intent`. A future rename of
    /// any field lands here at one site — and the `Empty` diagnostic
    /// composed from `INTENT_KIND_LIST` stays coherent with the
    /// wire format.
    ///
    /// Routes through the substrate primitive
    /// [`crate::tagged_union::assert_single_slot_key_matches_label`],
    /// which pins the exactly-one-key + name-equality projection
    /// byte-identically for every `<T: TaggedUnion + Serialize>`
    /// implementor — the wire-alignment testkit shared with the sibling
    /// `encapsulation_target_as_str_matches_field_name` /
    /// `artifact_kind_as_str_matches_field_name` /
    /// `channel_kind_as_str_matches_field_name` sites. Pre-lift the
    /// four bodies each restated the same serialize-and-inspect sweep
    /// at the test surface (three through a weaker YAML-substring
    /// check; this site alone through the strong JSON-object exactly-
    /// one form); post-lift the projection lives at ONE substrate
    /// primitive and every site binds through a single call — the
    /// three YAML sites simultaneously upgrade to the strong exactly-
    /// one form.
    #[test]
    fn intent_kind_as_str_matches_intent_field_name() {
        crate::tagged_union::assert_single_slot_key_matches_label::<Intent, _>(single_slot_intent);
    }

    /// ROUND-TRIP CONTRACT: `IntentKind::select(intent).map(|v|
    /// v.kind()) == Some(kind)`. The reverse `IntentVariant::kind`
    /// projection composes the closed set in both directions — a
    /// regression that misroutes a select arm (e.g. `Self::Nix =>
    /// intent.flux.as_ref()...`) fails loudly here.
    ///
    /// Routes through the substrate primitive
    /// [`crate::tagged_union::assert_variant_round_trip`], which
    /// composes [`crate::tagged_union::VariantSelector::select`]
    /// (forward) with [`crate::tagged_union::VariantKind::variant_kind`]
    /// (reverse) byte-identically for every `<T: TaggedUnion>`
    /// implementor — the round-trip testkit shared with the sibling
    /// `artifact_kind_round_trips_through_variant_kind` /
    /// `channel_kind_round_trips_through_variant_kind` /
    /// `encapsulation_target_round_trips_through_variant_target`
    /// sites. Pre-lift the four bodies each restated the same
    /// two-arm round-trip probe at the test surface; post-lift the
    /// projection lives at ONE substrate primitive and every site
    /// binds through a single call.
    #[test]
    fn intent_kind_round_trips_through_variant_kind() {
        crate::tagged_union::assert_variant_round_trip::<Intent, _>(single_slot_intent);
    }

    /// EMPTY-DIAGNOSTIC CONTRACT: the closed-set kind list embedded
    /// in `IntentError::Empty` echoes the canonical join of every
    /// `IntentKind::as_str()` projection. A variant added without
    /// updating `INTENT_KIND_LIST` (or a renamed variant) shows up
    /// here as a mismatch.
    ///
    /// Routes through the substrate primitive
    /// [`crate::tagged_union::assert_kind_list_matches_closed_set`],
    /// which composes `<T::Kind as ClosedSet>::labels_joined("/")`
    /// against `<T as TaggedUnion>::KIND_LIST` byte-identically for
    /// every implementor — the diagnostic-stability testkit shared
    /// with the sibling `artifact_error_empty_lists_every_kind_in_canonical_order`
    /// / `channel_error_empty_lists_every_kind_in_canonical_order`
    /// / `encapsulation_kind_error_empty_lists_every_target_in_canonical_order`
    /// sites. Pre-lift the four bodies each restated the same
    /// two-argument `assert_eq!(<XxxKind as ClosedSet>::labels_joined("/"),
    /// XXX_KIND_LIST)` comparison at the test surface; post-lift
    /// the projection lives at ONE substrate primitive and every
    /// site binds through a single call.
    #[test]
    fn intent_error_empty_lists_every_kind_in_canonical_order() {
        crate::tagged_union::assert_kind_list_matches_closed_set::<Intent>();
    }

    /// CANONICAL-BYTES CONTRACT: every populated variant yields the
    /// SAME bytes as `serde_json::to_vec` on the inner reference.
    /// Pins the lift of the parallel observe-mode match in
    /// `tatara-reconciler::render` to this single method.
    #[test]
    fn intent_variant_canonical_bytes_matches_inner_serialize() {
        for kind in IntentKind::ALL {
            let i = single_slot_intent(kind);
            let v = i.variant().expect("exactly-one variant");
            let via_method = v.canonical_bytes();
            let expected: Vec<u8> = match &v {
                IntentVariant::Nix(n) => serde_json::to_vec(n).unwrap_or_default(),
                IntentVariant::Flux(f) => serde_json::to_vec(f).unwrap_or_default(),
                IntentVariant::Lisp(l) => serde_json::to_vec(l).unwrap_or_default(),
                IntentVariant::Container(c) => serde_json::to_vec(c).unwrap_or_default(),
                IntentVariant::Aplicacao(a) => serde_json::to_vec(a).unwrap_or_default(),
                IntentVariant::Guest(g) => serde_json::to_vec(g).unwrap_or_default(),
            };
            assert_eq!(
                via_method, expected,
                "canonical_bytes mismatch for {kind:?}"
            );
            assert!(!via_method.is_empty(), "{kind:?} produced empty bytes");
        }
    }

    /// Construct an `Intent` with two slots populated — drives the
    /// pairwise `Ambiguous` sweep through the substrate primitive
    /// [`crate::tagged_union::assert_two_slots_ambiguous`]. Composes
    /// the single-slot constructor on top of itself per-field so ONE
    /// source of truth for per-variant inner payloads is preserved.
    /// Mirrors `two_slot_source` / `two_slot_channel` / `two_slot_kind`
    /// in shape across `ProcessSpec`'s tagged-union axis.
    fn two_slot_intent(a: IntentKind, b: IntentKind) -> Intent {
        let ia = single_slot_intent(a);
        let ib = single_slot_intent(b);
        Intent {
            nix: ia.nix.or(ib.nix),
            flux: ia.flux.or(ib.flux),
            lisp: ia.lisp.or(ib.lisp),
            container: ia.container.or(ib.container),
            aplicacao: ia.aplicacao.or(ib.aplicacao),
            guest: ia.guest.or(ib.guest),
        }
    }

    /// Construct an `Intent` with exactly the given kind's slot
    /// populated by a minimal valid inner spec. Shared across the
    /// closed-set property tests so they each cover every variant
    /// without restating the construction table.
    fn single_slot_intent(kind: IntentKind) -> Intent {
        match kind {
            IntentKind::Nix => Intent {
                nix: Some(NixIntent {
                    flake_ref: "github:a/b".into(),
                    attribute: "x".into(),
                    system: None,
                    attic_cache: None,
                    extra_args: vec![],
                    delegate_to_nix_build: false,
                }),
                ..Intent::default()
            },
            IntentKind::Flux => Intent {
                flux: Some(FluxIntent {
                    git_repository: "g".into(),
                    path: "p".into(),
                    git_repository_namespace: None,
                    target_namespace: None,
                    decrypt_sops: true,
                    helm_chart: None,
                    helm_values: None,
                }),
                ..Intent::default()
            },
            IntentKind::Lisp => Intent {
                lisp: Some(LispIntent {
                    source: "()".into(),
                    reader: "tatara-lisp".into(),
                    version: "v1".into(),
                    bindings: BTreeMap::new(),
                }),
                ..Intent::default()
            },
            IntentKind::Container => Intent {
                container: Some(ContainerIntent {
                    image: "ghcr.io/x:1".into(),
                    replicas: Some(1),
                    command: vec![],
                    args: vec![],
                    env: BTreeMap::new(),
                    workload_kind: WorkloadKind::default(),
                }),
                ..Intent::default()
            },
            IntentKind::Aplicacao => Intent {
                aplicacao: Some(AplicacaoIntent {
                    chart_ref: "oci://ghcr.io/x".into(),
                    version: "0.1.0".into(),
                    profile: String::new(),
                    values_overlay: serde_json::Value::Null,
                    release_name: None,
                    target_namespace: None,
                    install_timeout: None,
                }),
                ..Intent::default()
            },
            IntentKind::Guest => Intent {
                guest: Some(GuestIntent {
                    spec: serde_json::json!({"name": "guest-1"}),
                    state_dir: None,
                    allow_remote_build: None,
                }),
                ..Intent::default()
            },
        }
    }

    // ── closed-set algebra for WorkloadKind (ALL × as_str × Display ×
    //    FromStr × api_version × is_batch) ─────────────────────────────

    /// Structural well-formedness of [`WorkloadKind`] as a
    /// [`tatara_lisp::ClosedSet`] implementor — the workspace-wide
    /// testkit lift that pins all three structural invariants (`ALL`
    /// is non-empty, every variant round-trips through `label ↔
    /// parse_label`, labels are pairwise distinct, `""` is outside the
    /// closed set) at ONE call site. Replaces the hand-derived
    /// `workload_kind_all_is_unique_and_complete` +
    /// `workload_kind_roundtrip_via_as_str` + the empty-input arm of
    /// `unknown_workload_kind_errors`. `FromStr` delegates to
    /// `<Self as tatara_closed_set::ClosedSet>::parse_label`, so this helper
    /// exercises the same code path the reconciler hits when parsing a
    /// K8s `kind:`-shaped value back to the typed workload kind.
    #[test]
    fn workload_kind_is_well_formed_closed_set() {
        tatara_closed_set::assert_closed_set_well_formed::<WorkloadKind>();
    }

    /// CANONICAL-KEY CONTRACT: every variant's `as_str()` matches serde's
    /// PascalCase output verbatim. A future variant rename (or an
    /// `as_str` arm typo) lands at one site, instead of drifting
    /// between the typed surface, the K8s `kind:` manifest field, and
    /// the YAML wire format the reconciler / operator both read.
    #[test]
    fn workload_kind_as_str_matches_serde() {
        crate::tagged_union::assert_label_matches_serde_serialization::<WorkloadKind>();
    }

    /// The Display impl IS `as_str` — pinning this lets future callers
    /// reach for either projection without drift. If a reviewer
    /// accidentally re-introduces an inline match in Display, this
    /// test would fail the moment a variant rename touches one site
    /// but not the other.
    #[test]
    fn workload_kind_display_matches_as_str() {
        crate::tagged_union::assert_display_matches_label::<WorkloadKind>();
    }

    /// `FromStr` rejects strings that aren't in the canonical
    /// projection — lowercased / typo / unrelated — and the error
    /// echoes the input verbatim so the operator-facing diagnostic
    /// carries the offending value, not a normalized form. The
    /// empty-input arm is pinned by
    /// [`workload_kind_is_well_formed_closed_set`] via the
    /// `tatara_lisp::ClosedSet` testkit; the cases here pin the
    /// verbatim-echo contract on the [`UnknownWorkloadKind`]
    /// newtype, which the trait's `make_unknown` can't see.
    #[test]
    fn unknown_workload_kind_errors() {
        use std::str::FromStr;
        for bad in ["deployment", "JOB", "ReplicaSet", "Pod"] {
            let err = WorkloadKind::from_str(bad).unwrap_err();
            assert_eq!(err.0, bad, "error payload should echo input verbatim");
        }
    }

    #[test]
    fn workload_kind_default_is_deployment() {
        assert_eq!(WorkloadKind::default(), WorkloadKind::Deployment);
    }

    /// TRUTH-TABLE CONTRACT: `api_version` / `is_batch` agree with the
    /// documented (kind) -> (apiVersion, is_batch) table for every
    /// variant. A new variant in `WorkloadKind` without extending
    /// either projection's match is caught by the compiler (closed-set
    /// match in each method); adding a variant without extending its
    /// truth row is caught here. Also pins the invariant
    /// `is_batch <=> api_version == "batch/v1"`, so a future renderer
    /// can route on either projection without re-deriving the partition.
    #[test]
    fn workload_kind_projection_truth_table() {
        let table: &[(WorkloadKind, &str, bool)] = &[
            // (kind, api_version, is_batch)
            (WorkloadKind::Deployment, "apps/v1", false),
            (WorkloadKind::StatefulSet, "apps/v1", false),
            (WorkloadKind::DaemonSet, "apps/v1", false),
            (WorkloadKind::Job, "batch/v1", true),
            (WorkloadKind::CronJob, "batch/v1", true),
        ];
        assert_eq!(table.len(), WorkloadKind::ALL.len());
        for (kind, api, batch) in table {
            assert_eq!(kind.api_version(), *api, "api_version drift for {kind:?}");
            assert_eq!(kind.is_batch(), *batch, "is_batch drift for {kind:?}");
            assert_eq!(
                kind.is_batch(),
                kind.api_version() == "batch/v1",
                "is_batch / api_version partition disagrees for {kind:?}"
            );
        }
    }

    #[test]
    fn aplicacao_plus_flux_is_ambiguous() {
        let i = Intent {
            aplicacao: Some(AplicacaoIntent {
                chart_ref: "x".into(),
                version: "1".into(),
                profile: String::new(),
                values_overlay: serde_json::Value::Null,
                release_name: None,
                target_namespace: None,
                install_timeout: None,
            }),
            flux: Some(FluxIntent {
                git_repository: "g".into(),
                path: "p".into(),
                git_repository_namespace: None,
                target_namespace: None,
                decrypt_sops: true,
                helm_chart: None,
                helm_values: None,
            }),
            ..Intent::default()
        };
        assert_eq!(i.variant().unwrap_err(), IntentError::Ambiguous);
    }
}
