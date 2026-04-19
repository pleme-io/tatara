//! Structured events with correlation IDs for workload lifecycle tracing.
//!
//! Every phase transition emits a TracedEvent with a correlation_id that
//! traces the workload from submission → placement → warming → executing
//! → contraction → terminal. This enables Charity Majors-style
//! observability: structured events > metrics > logs.
//!
//! Events flow through NATS → Vector → Loki/DataFusion for querying.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A structured event carrying full context for observability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracedEvent {
    /// Unique event ID.
    pub id: Uuid,
    /// When this event occurred.
    pub timestamp: DateTime<Utc>,
    /// Correlation ID — traces the workload through its entire lifecycle.
    /// Same correlation_id from job submission through final terminal state.
    pub correlation_id: Uuid,
    /// Parent event ID (for causal ordering within a correlation).
    pub parent_id: Option<Uuid>,
    /// Event category.
    pub category: EventCategory,
    /// Event action.
    pub action: String,
    /// Structured context fields.
    pub fields: HashMap<String, serde_json::Value>,
    /// Which node emitted this event.
    pub node_id: Option<u64>,
    /// Duration of the operation (if applicable).
    pub duration_ms: Option<u64>,
    /// Severity level.
    pub level: EventLevel,
}

/// Event categories for structured querying.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    /// Job lifecycle events.
    Job,
    /// Allocation lifecycle events.
    Allocation,
    /// Task lifecycle events.
    Task,
    /// Node lifecycle events.
    Node,
    /// Scheduling events.
    Scheduling,
    /// Networking events (mesh, policy, flow).
    Network,
    /// Health check events.
    Health,
    /// Secret/security events.
    Security,
    /// Build/cache events.
    Build,
}

/// Event severity levels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum EventLevel {
    #[default]
    Info,
    Warning,
    Error,
    Debug,
}

impl TracedEvent {
    /// Create a new traced event.
    pub fn new(correlation_id: Uuid, category: EventCategory, action: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            correlation_id,
            parent_id: None,
            category,
            action: action.into(),
            fields: HashMap::new(),
            node_id: None,
            duration_ms: None,
            level: EventLevel::Info,
        }
    }

    /// Set parent event for causal ordering.
    pub fn with_parent(mut self, parent_id: Uuid) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    /// Set the emitting node.
    pub fn with_node(mut self, node_id: u64) -> Self {
        self.node_id = Some(node_id);
        self
    }

    /// Set operation duration.
    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }

    /// Set severity level.
    pub fn with_level(mut self, level: EventLevel) -> Self {
        self.level = level;
        self
    }

    /// Add a structured field.
    pub fn field(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Convenience: create an allocation phase transition event.
    pub fn allocation_phase(
        correlation_id: Uuid,
        alloc_id: Uuid,
        from_phase: &str,
        to_phase: &str,
    ) -> Self {
        Self::new(
            correlation_id,
            EventCategory::Allocation,
            "phase_transition",
        )
        .field("alloc_id", alloc_id.to_string())
        .field("from_phase", from_phase)
        .field("to_phase", to_phase)
    }

    /// Convenience: create a scheduling decision event.
    pub fn scheduling_decision(
        correlation_id: Uuid,
        job_id: &str,
        node_id: u64,
        driver: &str,
    ) -> Self {
        Self::new(
            correlation_id,
            EventCategory::Scheduling,
            "allocation_placed",
        )
        .field("job_id", job_id)
        .field("node_id", serde_json::Value::Number(node_id.into()))
        .field("driver", driver)
    }

    /// Convenience: create a health check event.
    pub fn health_check(
        correlation_id: Uuid,
        service_name: &str,
        healthy: bool,
        latency_ms: u64,
    ) -> Self {
        Self::new(correlation_id, EventCategory::Health, "probe_result")
            .field("service_name", service_name)
            .field("healthy", healthy)
            .with_duration(latency_ms)
    }
}

/// A correlation context that tracks a workload through its lifecycle.
///
/// # Thread Safety
/// This type is NOT thread-safe. It uses interior mutation via `&mut self`
/// in `emit()`. Each workload should own a single CorrelationContext.
/// For concurrent access, wrap in `Arc<Mutex<CorrelationContext>>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationContext {
    /// The correlation ID for this workload.
    pub correlation_id: Uuid,
    /// Job ID.
    pub job_id: String,
    /// Events emitted so far (for causal chain).
    pub event_count: u64,
    /// Last event ID (for parent linking).
    pub last_event_id: Option<Uuid>,
}

impl CorrelationContext {
    pub fn new(job_id: &str) -> Self {
        Self {
            correlation_id: Uuid::new_v4(),
            job_id: job_id.to_string(),
            event_count: 0,
            last_event_id: None,
        }
    }

    /// Create a new event in this correlation context.
    pub fn emit(&mut self, category: EventCategory, action: impl Into<String>) -> TracedEvent {
        let event = TracedEvent::new(self.correlation_id, category, action);
        let event = if let Some(parent) = self.last_event_id {
            event.with_parent(parent)
        } else {
            event
        };
        self.last_event_id = Some(event.id);
        self.event_count += 1;
        event
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_traced_event_creation() {
        let corr_id = Uuid::new_v4();
        let event = TracedEvent::new(corr_id, EventCategory::Job, "submitted")
            .field("job_id", "web-service")
            .with_node(42);

        assert_eq!(event.correlation_id, corr_id);
        assert_eq!(event.category, EventCategory::Job);
        assert_eq!(event.action, "submitted");
        assert_eq!(event.node_id, Some(42));
        assert_eq!(event.fields["job_id"], "web-service");
    }

    #[test]
    fn test_correlation_context() {
        let mut ctx = CorrelationContext::new("web-service");

        let e1 = ctx.emit(EventCategory::Job, "submitted");
        assert!(e1.parent_id.is_none());
        assert_eq!(ctx.event_count, 1);

        let e2 = ctx.emit(EventCategory::Scheduling, "placed");
        assert_eq!(e2.parent_id, Some(e1.id));
        assert_eq!(ctx.event_count, 2);

        let e3 = ctx.emit(EventCategory::Allocation, "warming");
        assert_eq!(e3.parent_id, Some(e2.id));
        assert_eq!(e3.correlation_id, e1.correlation_id);
    }

    #[test]
    fn test_convenience_constructors() {
        let corr = Uuid::new_v4();
        let alloc = Uuid::new_v4();

        let phase = TracedEvent::allocation_phase(corr, alloc, "warming", "executing");
        assert_eq!(phase.fields["from_phase"], "warming");
        assert_eq!(phase.fields["to_phase"], "executing");

        let sched = TracedEvent::scheduling_decision(corr, "web", 42, "wasi");
        assert_eq!(sched.fields["driver"], "wasi");

        let health = TracedEvent::health_check(corr, "web", true, 5);
        assert_eq!(health.duration_ms, Some(5));
    }

    #[test]
    fn test_serde_roundtrip() {
        let event = TracedEvent::new(Uuid::new_v4(), EventCategory::Network, "flow_detected")
            .field("bytes", 1024)
            .with_level(EventLevel::Debug);

        let json = serde_json::to_string(&event).unwrap();
        let back: TracedEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.category, EventCategory::Network);
        assert_eq!(back.level, EventLevel::Debug);
    }
}
