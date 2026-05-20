//! `EphemeralDefaults` — typed operator-facing config for the ephemeral
//! envelope of `tatara-reconciler`. Loaded via shikumi (ConfigStore +
//! hot-reload + ArcSwap); shipped to operators through three module
//! surfaces (HM / NixOS / Darwin).
//!
//! Operator-facing knobs only — *not* per-Process spec (that lives in
//! the typed `Lifetime::Ephemeral` slot on `ProcessSpec`). These are
//! the fleet-wide defaults the operator sets ONCE per cluster:
//!
//! - **default TTL** for ephemeral Processes whose spec omits it
//!   (back-compat surface, since `Lifetime::Ephemeral.ttl` defaults to
//!   `"1h"` typed-side already)
//! - **max concurrent** ephemeral Processes cluster-wide (cost ceiling)
//! - **default registry** to pull Aplicacao chart references from
//! - **root CA name** the saguão vigia policy issues per-namespace
//!   intermediates against
//! - **default chart ref** used when `(defephemeral …)` omits `:chart-ref`
//!
//! XDG search path: `${XDG_CONFIG_HOME:-~/.config}/tatara-reconciler/
//! ephemeral.yaml`. Env override prefix: `TATARA_RECONCILER_EPHEMERAL_*`.
//! Hot-reload: changes pick up within shikumi's debounce window
//! (~250ms) — controller swaps the Arc on the next reconcile loop.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Operator-facing ephemeral defaults — fleet-wide knobs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct EphemeralDefaults {
    /// Default TTL when a Process's `lifetime.ephemeral.ttl` field is
    /// empty. Parsed by humantime (`"1h"`, `"30m"`, `"2h30m"`).
    /// Default: `"1h"`.
    #[serde(default = "default_ttl")]
    pub default_ttl: String,

    /// Maximum concurrent ephemeral Processes cluster-wide. `0` = no cap.
    /// The reconciler enforces this in `handle_pending` before
    /// transitioning out of Pending. Default: `0`.
    #[serde(default)]
    pub max_concurrent_per_cluster: u32,

    /// Default OCI registry for Aplicacao chart refs. When operators
    /// author short refs (`"akeyless-deployment:0.5.5"`), the reconciler
    /// expands to `oci://<registry>/<ref>`. Default:
    /// `"ghcr.io/pleme-io/charts"`.
    #[serde(default = "default_registry")]
    pub registry: String,

    /// Name of the saguão-owned cluster-wide root CA. The reconciler's
    /// PROVISIONING phase auto-creates a per-namespace intermediate
    /// `cert-manager.io/v1::Issuer` chained to this root when the
    /// namespace carries label `tatara.pleme.io/ephemeral=true`.
    /// Default: `"saguao-fleet-root"`.
    #[serde(default = "default_root_ca")]
    pub root_ca_name: String,

    /// Default chart reference used when `(defephemeral :aplicacao …
    /// :chart-ref …)` is omitted. Useful for fleet-wide single-product
    /// deployments (e.g., a homelab cluster that only ever runs
    /// `lareira-akeyless-deployment`). Default: empty (no fallback).
    #[serde(default)]
    pub default_chart_ref: String,

    /// Whether to auto-emit an `OCIRepository` peer for `oci://` chart
    /// refs. Default: `true`. Operators can disable when they
    /// pre-create OCIRepositories cluster-wide.
    #[serde(default = "default_true")]
    pub emit_oci_repository: bool,
}

fn default_ttl() -> String {
    "1h".to_string()
}
fn default_registry() -> String {
    "ghcr.io/pleme-io/charts".to_string()
}
fn default_root_ca() -> String {
    "saguao-fleet-root".to_string()
}
fn default_true() -> bool {
    true
}

impl Default for EphemeralDefaults {
    fn default() -> Self {
        Self {
            default_ttl: default_ttl(),
            max_concurrent_per_cluster: 0,
            registry: default_registry(),
            root_ca_name: default_root_ca(),
            default_chart_ref: String::new(),
            emit_oci_repository: true,
        }
    }
}

/// Errors loading `EphemeralDefaults` from disk or env.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// shikumi propagated an IO/parse/schema error.
    #[error("shikumi error: {0}")]
    Shikumi(String),
}

/// Load `EphemeralDefaults` via shikumi's ConfigStore + hot-reload.
///
/// Reads:
/// 1. struct defaults
/// 2. env vars prefixed `TATARA_RECONCILER_EPHEMERAL_`
/// 3. YAML at `path` (typically `~/.config/tatara-reconciler/ephemeral.yaml`)
///
/// Hot-reload: when the file changes, `on_reload(&EphemeralDefaults)`
/// fires. Caller's atomic `Arc<EphemeralDefaults>` (held via
/// `ConfigStore::get`) swaps on next read.
pub fn load_and_watch<F>(
    path: &Path,
    on_reload: F,
) -> std::result::Result<shikumi::ConfigStore<EphemeralDefaults>, LoadError>
where
    F: Fn(&EphemeralDefaults) + Send + Sync + 'static,
{
    shikumi::ConfigStore::load_and_watch(
        path,
        "TATARA_RECONCILER_EPHEMERAL",
        on_reload,
    )
    .map_err(|e| LoadError::Shikumi(e.to_string()))
}

/// One-shot load (no watching) — useful for tests and short-lived CLIs.
pub fn load(path: &Path) -> std::result::Result<Arc<EphemeralDefaults>, LoadError> {
    let store = shikumi::ConfigStore::<EphemeralDefaults>::load(
        path,
        "TATARA_RECONCILER_EPHEMERAL",
    )
    .map_err(|e| LoadError::Shikumi(e.to_string()))?;
    Ok(store.get().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn struct_defaults_round_trip() {
        let d = EphemeralDefaults::default();
        assert_eq!(d.default_ttl, "1h");
        assert_eq!(d.registry, "ghcr.io/pleme-io/charts");
        assert_eq!(d.root_ca_name, "saguao-fleet-root");
        assert!(d.default_chart_ref.is_empty());
        assert_eq!(d.max_concurrent_per_cluster, 0);
        assert!(d.emit_oci_repository);
    }

    #[test]
    fn serde_yaml_round_trip() {
        let d = EphemeralDefaults {
            default_ttl: "30m".into(),
            max_concurrent_per_cluster: 8,
            registry: "ghcr.io/example/charts".into(),
            root_ca_name: "homelab-root".into(),
            default_chart_ref:
                "oci://ghcr.io/pleme-io/charts/lareira-akeyless-deployment".into(),
            emit_oci_repository: false,
        };
        let yaml = serde_yaml::to_string(&d).unwrap();
        let back: EphemeralDefaults = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn deny_unknown_fields_protects_schema() {
        let yaml = r#"
default_ttl: "1h"
forged_extra: "boom"
"#;
        let err: Result<EphemeralDefaults, _> = serde_yaml::from_str(yaml);
        assert!(err.is_err());
    }

    #[test]
    fn empty_yaml_resolves_to_defaults() {
        let yaml = "{}";
        let d: EphemeralDefaults = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(d, EphemeralDefaults::default());
    }
}
