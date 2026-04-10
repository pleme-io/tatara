use crate::config::ClusterTarget;
use crate::error::KubeError;
use kube::Client;
use std::collections::HashMap;
use tracing::info;

/// Manages connections to multiple Kubernetes clusters.
pub struct ClusterManager {
    clients: HashMap<String, Client>,
}

impl ClusterManager {
    /// Build K8s clients for all configured clusters.
    pub async fn from_config(
        clusters: &HashMap<String, ClusterTarget>,
    ) -> Result<Self, KubeError> {
        let mut clients = HashMap::new();

        for (name, target) in clusters {
            if !target.enabled {
                continue;
            }

            let client = match (&target.kubeconfig, &target.context) {
                (Some(path), ctx) => {
                    let kubeconfig =
                        kube::config::Kubeconfig::read_from(path).map_err(|e| {
                            KubeError::ClusterUnreachable {
                                name: name.clone(),
                                reason: e.to_string(),
                            }
                        })?;
                    let config = kube::Config::from_custom_kubeconfig(
                        kubeconfig,
                        &kube::config::KubeConfigOptions {
                            context: ctx.clone(),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| KubeError::ClusterUnreachable {
                        name: name.clone(),
                        reason: e.to_string(),
                    })?;
                    Client::try_from(config).map_err(|e| KubeError::ClusterUnreachable {
                        name: name.clone(),
                        reason: e.to_string(),
                    })?
                }
                (None, _) => {
                    Client::try_default()
                        .await
                        .map_err(|e| KubeError::ClusterUnreachable {
                            name: name.clone(),
                            reason: e.to_string(),
                        })?
                }
            };

            // Verify connectivity
            client
                .apiserver_version()
                .await
                .map_err(|e| KubeError::ClusterUnreachable {
                    name: name.clone(),
                    reason: e.to_string(),
                })?;

            info!(cluster = %name, "connected to cluster");
            clients.insert(name.clone(), client);
        }

        Ok(Self { clients })
    }

    /// Build a single client from default kubeconfig.
    pub async fn default_client() -> Result<Self, KubeError> {
        let client = Client::try_default().await.map_err(|e| {
            KubeError::ClusterUnreachable {
                name: "default".to_string(),
                reason: e.to_string(),
            }
        })?;
        let mut clients = HashMap::new();
        clients.insert("default".to_string(), client);
        Ok(Self { clients })
    }

    pub fn get(&self, cluster_name: &str) -> Option<&Client> {
        self.clients.get(cluster_name)
    }

    pub fn cluster_names(&self) -> Vec<&str> {
        self.clients.keys().map(|s| s.as_str()).collect()
    }
}
