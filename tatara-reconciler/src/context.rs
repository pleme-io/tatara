//! Shared reconciler context — K8s client, config, metrics.

use std::sync::Arc;

use kube::Client;

#[derive(Clone)]
pub struct Context {
    pub kube: Client,
    pub config: Arc<ReconcilerConfig>,
}

#[derive(Clone, Debug)]
pub struct ReconcilerConfig {
    /// Namespace the controller runs in (for ProcessTable singleton lookups).
    pub controller_namespace: String,
    /// Default boundary timeout if `spec.boundary.timeout` is unset.
    pub default_boundary_timeout_seconds: u64,
    /// Default requeue interval between heartbeats.
    pub heartbeat_seconds: u64,
    /// Name of the cluster-scoped ProcessTable singleton.
    pub process_table_name: String,
    /// Container image the reconciler stamps into each
    /// tatara-export-worker Job emitted during the `Releasing`
    /// phase. Operators override via the reconciler's Helm chart
    /// values.
    pub export_worker_image: String,
    /// ServiceAccount the export-worker Jobs run as. Operators
    /// provision it (Role + RoleBinding granting list/get/patch on
    /// ConfigMaps + get on Processes) via the same Helm chart that
    /// ships the reconciler.
    pub export_worker_service_account: String,

    /// **R9 fleet routing config** — cluster + location + domain
    /// segments stamped into every emitted FQDN. Matches the
    /// `nix/lib/fleet-domains.nix mkHostname` pattern.
    /// Per-cluster overrides via the reconciler Helm chart.
    pub cluster: String,
    pub location: String,
    pub domain: String,

    /// External-dns target — the cluster's ingress loadbalancer
    /// hostname (or CNAME-able equivalent). When set, the
    /// reconciler emits DNSEndpoint resources pointing all FQDNs
    /// at this target. None ⇒ Ingress emits but DNS does not.
    pub dns_lb_target: Option<String>,
}

impl Default for ReconcilerConfig {
    fn default() -> Self {
        Self {
            controller_namespace: "tatara-system".into(),
            default_boundary_timeout_seconds: 900,
            heartbeat_seconds: 30,
            process_table_name: "proc".into(),
            export_worker_image: "ghcr.io/pleme-io/tatara-export-worker:0.2.0".into(),
            export_worker_service_account: "tatara-export-worker".into(),
            cluster: "pleme-dev".into(),
            location: "use1".into(),
            domain: "quero.lol".into(),
            dns_lb_target: None,
        }
    }
}
