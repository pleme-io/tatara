//! Intent — where the rendered artifacts come from.
//!
//! Exactly one field on `Intent` must be set. The reconciler's RENDER phase
//! selects a driver based on which variant is present:
//!   - `nix`:        tatara-engine `nix_eval` → resources
//!   - `flux`:       pass through an existing `GitRepository`
//!   - `lisp`:       tatara-lisp reader + macroexpander → resources
//!   - `container`:  emit Deployment/StatefulSet/etc directly (no Helm)
//!   - `aplicacao`:  emit a FluxCD `HelmRelease` for a pleme-io typed
//!                   Aplicacao chart (e.g. `lareira-akeyless-deployment`).
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
    /// the closed set; pinned by `intent_kind_round_trips_through_variant_kind`.
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

/// Closed-set discriminator over `Intent`'s six tagged-union slots.
/// Single source of truth that drives `Intent::variant`'s ambiguity
/// + emptiness resolver, the `IntentError::Empty` message, and the
/// reverse `IntentVariant::kind` projection. Adding a 7th intent
/// variant lands at one `ALL` entry + one `as_str` arm + one
/// `select` arm + one `IntentVariant::kind` arm — exhaustively
/// checked by the compiler.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum IntentError {
    #[error("intent has no variant set (one of {0} required)")]
    Empty(&'static str),
    #[error("intent has multiple variants set; exactly one required")]
    Ambiguous,
}

/// Slash-joined list of every `IntentKind::as_str()` — composed once
/// at compile time so `IntentError::Empty`'s diagnostic carries the
/// closed-set summary without per-variant string drift.
const INTENT_KIND_LIST: &str = "nix/flux/lisp/container/aplicacao/guest";

impl Intent {
    /// Resolve to exactly one variant. Errors on zero or many.
    /// Sweeps over `IntentKind::ALL` so a 7th variant added with an
    /// `ALL` entry is structurally honored at this site — no
    /// parallel `is_some()` count array, no if-let-else chain, no
    /// `unreachable!()`. The Empty diagnostic carries the closed-set
    /// list via `INTENT_KIND_LIST`.
    pub fn variant(&self) -> Result<IntentVariant<'_>, IntentError> {
        let mut found: Option<IntentVariant<'_>> = None;
        for kind in IntentKind::ALL {
            if let Some(v) = kind.select(self) {
                if found.is_some() {
                    return Err(IntentError::Ambiguous);
                }
                found = Some(v);
            }
        }
        found.ok_or(IntentError::Empty(INTENT_KIND_LIST))
    }
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
///           (:chart-ref "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment"
///            :version "0.5.5"
///            :profile "gateway-with-internal-saas"
///            :values-overlay (:cluster (:name "ephemeral-test-01")
///                             :persistence false
///                             :compliance (:overlays []))))
/// ```
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AplicacaoIntent {
    /// Helm chart reference. OCI (`oci://…`) or repo-relative (`pleme-io/lareira-akeyless-deployment`).
    pub chart_ref: String,
    /// Chart version (Helm semver constraint; `">=0.5.5"` allowed).
    pub version: String,
    /// Architecture profile from the chart's `values/*.yaml` family
    /// (e.g. `gateway-with-internal-saas`, `saas-internal`).
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "PascalCase")]
pub enum WorkloadKind {
    #[default]
    Deployment,
    StatefulSet,
    DaemonSet,
    Job,
    CronJob,
}

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

    #[test]
    fn two_variants_ambiguous() {
        let i = Intent {
            nix: Some(NixIntent {
                flake_ref: "a".into(),
                attribute: "b".into(),
                system: None,
                attic_cache: None,
                extra_args: vec![],
                delegate_to_nix_build: false,
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
    fn guest_plus_nix_is_ambiguous() {
        let i = Intent {
            nix: Some(NixIntent {
                flake_ref: "github:a/b".into(),
                attribute: "x".into(),
                system: None,
                attic_cache: None,
                extra_args: vec![],
                delegate_to_nix_build: false,
            }),
            guest: Some(GuestIntent {
                spec: serde_json::json!({"name": "x"}),
                state_dir: None,
                allow_remote_build: None,
            }),
            ..Intent::default()
        };
        assert_eq!(i.variant().unwrap_err(), IntentError::Ambiguous);
    }

    #[test]
    fn aplicacao_intent_selects_its_variant() {
        let i = Intent {
            aplicacao: Some(AplicacaoIntent {
                chart_ref: "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment".into(),
                version: "0.5.5".into(),
                profile: "gateway-with-internal-saas".into(),
                values_overlay: serde_json::json!({ "cluster": { "name": "test-01" } }),
                release_name: None,
                target_namespace: None,
                install_timeout: Some("25m".into()),
            }),
            ..Intent::default()
        };
        match i.variant().unwrap() {
            IntentVariant::Aplicacao(a) => {
                assert_eq!(a.profile, "gateway-with-internal-saas");
                assert_eq!(a.version, "0.5.5");
                assert_eq!(a.install_timeout.as_deref(), Some("25m"));
            }
            other => panic!("expected Aplicacao, got {other:?}"),
        }
    }

    /// `ALL` is the source of truth for the resolver sweep — pin its
    /// closure so a variant added without an `ALL` entry fails here
    /// (via the uniqueness check) before drifting `variant()`.
    #[test]
    fn intent_kind_all_is_unique_and_complete() {
        let mut seen = std::collections::HashSet::new();
        for kind in IntentKind::ALL {
            assert!(seen.insert(kind), "duplicate variant in ALL: {kind:?}");
        }
        assert_eq!(seen.len(), IntentKind::ALL.len());
    }

    /// CANONICAL-KEY CONTRACT: each variant's `as_str()` matches the
    /// camelCase serde field name on `Intent`. A future rename of
    /// any field lands here at one site — and the `Empty` diagnostic
    /// composed from `INTENT_KIND_LIST` stays coherent with the
    /// wire format.
    #[test]
    fn intent_kind_as_str_matches_intent_field_name() {
        for kind in IntentKind::ALL {
            // Pre-serialize an `Intent` carrying just this kind's
            // slot populated; the only key in the resulting JSON
            // object must equal `kind.as_str()`.
            let i = match kind {
                IntentKind::Nix => Intent {
                    nix: Some(NixIntent {
                        flake_ref: "f".into(),
                        attribute: "a".into(),
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
                        image: "x".into(),
                        replicas: None,
                        command: vec![],
                        args: vec![],
                        env: BTreeMap::new(),
                        workload_kind: WorkloadKind::default(),
                    }),
                    ..Intent::default()
                },
                IntentKind::Aplicacao => Intent {
                    aplicacao: Some(AplicacaoIntent {
                        chart_ref: "x".into(),
                        version: "1".into(),
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
                        spec: serde_json::json!({"name": "x"}),
                        state_dir: None,
                        allow_remote_build: None,
                    }),
                    ..Intent::default()
                },
            };
            let v = serde_json::to_value(&i).expect("Intent serializes");
            let obj = v.as_object().expect("Intent serializes to object");
            let keys: Vec<&String> = obj.keys().collect();
            assert_eq!(
                keys.len(),
                1,
                "exactly one slot populated for kind {kind:?}, got {keys:?}"
            );
            assert_eq!(
                keys[0],
                kind.as_str(),
                "as_str() must match serde field name for {kind:?}"
            );
        }
    }

    /// ROUND-TRIP CONTRACT: `IntentKind::select(intent).map(|v|
    /// v.kind()) == Some(kind)`. The reverse `IntentVariant::kind`
    /// projection composes the closed set in both directions — a
    /// regression that misroutes a select arm (e.g. `Self::Nix =>
    /// intent.flux.as_ref()...`) fails loudly here.
    #[test]
    fn intent_kind_round_trips_through_variant_kind() {
        for kind in IntentKind::ALL {
            let i = single_slot_intent(kind);
            let v = kind.select(&i).expect("populated slot must select");
            assert_eq!(v.kind(), kind, "round-trip failed for {kind:?}");
            // And the resolver lands on the same variant.
            assert_eq!(
                i.variant().expect("exactly-one variant").kind(),
                kind,
                "variant() resolver disagreed on {kind:?}"
            );
        }
    }

    /// EMPTY-DIAGNOSTIC CONTRACT: the closed-set kind list embedded
    /// in `IntentError::Empty` echoes the canonical join of every
    /// `IntentKind::as_str()` projection. A variant added without
    /// updating `INTENT_KIND_LIST` (or a renamed variant) shows up
    /// here as a mismatch.
    #[test]
    fn intent_error_empty_lists_every_kind_in_canonical_order() {
        let parts: Vec<&'static str> = IntentKind::ALL.iter().map(|k| k.as_str()).collect();
        let joined = parts.join("/");
        assert_eq!(joined, INTENT_KIND_LIST);
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
