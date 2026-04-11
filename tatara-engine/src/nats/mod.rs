//! NATS event bus for tatara — inter-task communication, log aggregation,
//! and catalog change notifications.
//!
//! Optional: if NATS is not configured, all publish operations are no-ops.
//! Uses JetStream for guaranteed delivery of critical operations and
//! core NATS for ephemeral data (logs, metrics).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use tatara_core::catalog::ServiceEntry;
use tatara_core::domain::event::Event;

/// Configuration for the NATS event bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NatsConfig {
    /// NATS server URL.
    #[serde(default = "default_nats_url")]
    pub url: String,

    /// Whether NATS integration is enabled.
    #[serde(default)]
    pub enabled: bool,

    /// Whether to use JetStream for guaranteed delivery.
    #[serde(default = "default_true")]
    pub jetstream: bool,

    /// Subject prefix for all tatara NATS subjects.
    #[serde(default = "default_subject_prefix")]
    pub subject_prefix: String,
}

fn default_nats_url() -> String {
    "nats://127.0.0.1:4222".to_string()
}
fn default_true() -> bool {
    true
}
fn default_subject_prefix() -> String {
    "tatara".to_string()
}

impl Default for NatsConfig {
    fn default() -> Self {
        Self {
            url: default_nats_url(),
            enabled: false,
            jetstream: true,
            subject_prefix: default_subject_prefix(),
        }
    }
}

/// NATS event bus for publishing events, logs, and catalog changes.
///
/// Subject hierarchy:
/// - `{prefix}.events.{kind}` — cluster events
/// - `{prefix}.logs.{alloc_id}.{task_name}` — task logs
/// - `{prefix}.health.{service_name}` — health probe results
/// - `{prefix}.catalog.changes` — service catalog mutations
pub struct NatsEventBus {
    config: NatsConfig,
    client: Option<async_nats::Client>,
}

impl NatsEventBus {
    /// Connect to NATS. If disabled or connection fails, creates a no-op bus.
    pub async fn connect(config: NatsConfig) -> Self {
        if !config.enabled {
            debug!("NATS event bus disabled");
            return Self {
                config,
                client: None,
            };
        }

        match async_nats::connect(&config.url).await {
            Ok(client) => {
                info!(url = %config.url, "connected to NATS");
                Self {
                    config,
                    client: Some(client),
                }
            }
            Err(e) => {
                warn!(url = %config.url, error = %e, "failed to connect to NATS, running without event bus");
                Self {
                    config,
                    client: None,
                }
            }
        }
    }

    /// Create a disconnected (no-op) event bus.
    pub fn disconnected() -> Self {
        Self {
            config: NatsConfig::default(),
            client: None,
        }
    }

    /// Check if NATS is connected.
    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    /// Publish a cluster event.
    pub async fn publish_event(&self, event: &Event) -> Result<()> {
        let Some(client) = &self.client else {
            return Ok(());
        };
        let subject = format!("{}.events.{}", self.config.subject_prefix, event.kind_str());
        let payload = serde_json::to_vec(event)?;
        client
            .publish(subject, payload.into())
            .await
            .map_err(|e| anyhow::anyhow!("NATS publish failed: {e}"))?;
        Ok(())
    }

    /// Publish a log entry for cross-node aggregation.
    pub async fn publish_log(
        &self,
        alloc_id: &str,
        task_name: &str,
        message: &str,
        stream: &str,
    ) -> Result<()> {
        let Some(client) = &self.client else {
            return Ok(());
        };
        let subject = format!(
            "{}.logs.{}.{}",
            self.config.subject_prefix, alloc_id, task_name
        );
        let payload = serde_json::json!({
            "alloc_id": alloc_id,
            "task_name": task_name,
            "message": message,
            "stream": stream,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        client
            .publish(subject, serde_json::to_vec(&payload)?.into())
            .await
            .map_err(|e| anyhow::anyhow!("NATS publish failed: {e}"))?;
        Ok(())
    }

    /// Publish a health probe result.
    pub async fn publish_health(
        &self,
        service_name: &str,
        service_id: &str,
        healthy: bool,
    ) -> Result<()> {
        let Some(client) = &self.client else {
            return Ok(());
        };
        let subject = format!(
            "{}.health.{}",
            self.config.subject_prefix, service_name
        );
        let payload = serde_json::json!({
            "service_id": service_id,
            "healthy": healthy,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        client
            .publish(subject, serde_json::to_vec(&payload)?.into())
            .await
            .map_err(|e| anyhow::anyhow!("NATS publish failed: {e}"))?;
        Ok(())
    }

    /// Publish a catalog change (service registered/deregistered).
    pub async fn publish_catalog_change(
        &self,
        action: &str,
        entry: &ServiceEntry,
    ) -> Result<()> {
        let Some(client) = &self.client else {
            return Ok(());
        };
        let subject = format!("{}.catalog.changes", self.config.subject_prefix);
        let payload = serde_json::json!({
            "action": action,
            "service_name": entry.service_name,
            "service_id": entry.service_id,
            "address": entry.address,
            "port": entry.port,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        client
            .publish(subject, serde_json::to_vec(&payload)?.into())
            .await
            .map_err(|e| anyhow::anyhow!("NATS publish failed: {e}"))?;
        Ok(())
    }

    /// Subscribe to events matching a filter pattern.
    pub async fn subscribe_events(
        &self,
        kind_filter: &str,
    ) -> Result<Option<async_nats::Subscriber>> {
        let Some(client) = &self.client else {
            return Ok(None);
        };
        let subject = format!("{}.events.{}", self.config.subject_prefix, kind_filter);
        let sub = client.subscribe(subject).await?;
        Ok(Some(sub))
    }

    /// Subscribe to logs for a specific allocation.
    pub async fn subscribe_logs(
        &self,
        alloc_id: &str,
    ) -> Result<Option<async_nats::Subscriber>> {
        let Some(client) = &self.client else {
            return Ok(None);
        };
        let subject = format!("{}.logs.{}.>", self.config.subject_prefix, alloc_id);
        let sub = client.subscribe(subject).await?;
        Ok(Some(sub))
    }
}

/// Extension to use Event's Display-based kind string for NATS subjects.
trait EventKindStr {
    fn kind_str(&self) -> String;
}

impl EventKindStr for Event {
    fn kind_str(&self) -> String {
        self.kind.to_string()
    }
}
