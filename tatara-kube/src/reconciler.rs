use crate::{
    apply, config::KubeConfig, error::KubeError, metrics::ReconcileStats, nix_eval, ordering,
    prune, resource::*,
};
use chrono::Utc;
use kube::Client;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tracing::{error, info, warn};

/// Core reconciliation engine.
///
/// Evaluates Nix flake outputs, diffs against live cluster state,
/// and applies changes via Server-Side Apply.
pub struct KubeReconciler {
    config: KubeConfig,
    last_rev: HashMap<String, String>,
    last_generation: HashMap<String, String>,
}

impl KubeReconciler {
    pub fn new(config: KubeConfig) -> Self {
        Self {
            config,
            last_rev: HashMap::new(),
            last_generation: HashMap::new(),
        }
    }

    /// Run a single reconciliation tick for one cluster.
    pub async fn reconcile_cluster(
        &mut self,
        client: &Client,
        cluster_name: &str,
        cluster_attr: &str,
    ) -> Result<ReconcileStats, KubeError> {
        let start = std::time::Instant::now();

        // 1. Check for flake revision changes
        let metadata = nix_eval::flake_metadata(
            &self.config.flake_ref,
            self.config.flake_metadata_timeout_secs,
        )
        .await?;

        let current_rev = metadata
            .rev
            .unwrap_or_else(|| metadata.last_modified.to_string());

        if let Some(last) = self.last_rev.get(cluster_name) {
            if *last == current_rev {
                info!(cluster = cluster_name, rev = %current_rev, "no changes detected");
                return Ok(ReconcileStats {
                    applied: 0,
                    pruned: 0,
                    unchanged: 0,
                    errors: 0,
                    duration_ms: start.elapsed().as_millis() as u64,
                    timestamp: Utc::now(),
                });
            }
        }

        info!(cluster = cluster_name, rev = %current_rev, "changes detected, evaluating");

        // 2. Evaluate Nix flake
        let eval_start = std::time::Instant::now();
        let raw_resources = nix_eval::eval_cluster_resources(
            &self.config.flake_ref,
            &self.config.system,
            cluster_attr,
            self.config.nix_eval_timeout_secs,
        )
        .await?;
        let eval_ms = eval_start.elapsed().as_millis() as u64;

        info!(
            count = raw_resources.len(),
            eval_ms, "nix eval completed"
        );

        // 3. Parse into ManagedResources
        let mut resources: Vec<ManagedResource> = raw_resources
            .into_iter()
            .filter_map(|v| match ManagedResource::from_value(v) {
                Ok(r) => Some(r),
                Err(e) => {
                    warn!(error = %e, "skipping unparseable resource");
                    None
                }
            })
            .collect();

        // 4. Compute generation hash
        let generation_hash = {
            let canonical: Vec<&str> = resources.iter().map(|r| r.content_hash.as_str()).collect();
            let combined = canonical.join(":");
            format!("{:x}", Sha256::digest(combined.as_bytes()))
        };

        // 5. Inject management labels into every resource
        for resource in &mut resources {
            inject_management_labels(
                &mut resource.manifest,
                cluster_name,
                &generation_hash,
                &resource.content_hash,
            );
        }

        // 6. Sort by dependency order
        ordering::topological_sort(&mut resources);

        // 7. Apply resources via Server-Side Apply
        let mut applied = 0u32;
        let mut errors = 0u32;

        for resource in &resources {
            match apply::server_side_apply(
                client,
                resource,
                &self.config.field_manager,
                self.config.force_apply,
            )
            .await
            {
                Ok(_) => applied += 1,
                Err(e) => {
                    error!(
                        resource = %resource.identity,
                        error = %e,
                        "apply failed"
                    );
                    errors += 1;
                }
            }
        }

        // 8. Prune orphaned resources
        let mut pruned = 0u32;
        if self.config.prune {
            match prune::find_orphans(client, cluster_name, &generation_hash).await {
                Ok(orphans) => {
                    if !orphans.is_empty() {
                        info!(count = orphans.len(), "pruning orphaned resources");
                        let results = prune::prune_orphans(client, &orphans).await;
                        for result in results {
                            match result {
                                Ok(_) => pruned += 1,
                                Err(e) => {
                                    error!(error = %e, "prune failed");
                                    errors += 1;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "orphan detection failed");
                }
            }
        }

        // 9. Update state
        self.last_rev
            .insert(cluster_name.to_string(), current_rev);
        self.last_generation
            .insert(cluster_name.to_string(), generation_hash);

        let stats = ReconcileStats {
            applied,
            pruned,
            unchanged: resources.len() as u32 - applied,
            errors,
            duration_ms: start.elapsed().as_millis() as u64,
            timestamp: Utc::now(),
        };

        info!(
            cluster = cluster_name,
            applied,
            pruned,
            errors,
            duration_ms = stats.duration_ms,
            "reconciliation complete"
        );

        Ok(stats)
    }
}

/// Inject tatara-kube management labels and annotations into a resource manifest.
fn inject_management_labels(
    manifest: &mut serde_json::Value,
    cluster_name: &str,
    generation_hash: &str,
    content_hash: &str,
) {
    let metadata = manifest
        .get_mut("metadata")
        .and_then(|m| m.as_object_mut());

    if let Some(metadata) = metadata {
        // Labels
        let labels = metadata
            .entry("labels")
            .or_insert_with(|| serde_json::json!({}));
        if let Some(labels) = labels.as_object_mut() {
            labels.insert(
                LABEL_MANAGED_BY.to_string(),
                serde_json::Value::String(LABEL_MANAGED_BY_VALUE.to_string()),
            );
            labels.insert(
                LABEL_CLUSTER.to_string(),
                serde_json::Value::String(cluster_name.to_string()),
            );
            labels.insert(
                LABEL_GENERATION.to_string(),
                serde_json::Value::String(generation_hash.to_string()),
            );
        }

        // Annotations
        let annotations = metadata
            .entry("annotations")
            .or_insert_with(|| serde_json::json!({}));
        if let Some(annotations) = annotations.as_object_mut() {
            annotations.insert(
                ANNOTATION_CONTENT_HASH.to_string(),
                serde_json::Value::String(content_hash.to_string()),
            );
            annotations.insert(
                ANNOTATION_APPLIED_AT.to_string(),
                serde_json::Value::String(Utc::now().to_rfc3339()),
            );
        }
    }
}
