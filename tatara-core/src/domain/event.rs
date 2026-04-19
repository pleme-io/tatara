use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use uuid::Uuid;

/// An event representing a state change in the cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub kind: EventKind,
    pub payload: serde_json::Value,
}

/// Categories of events emitted by the cluster.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    JobSubmitted,
    JobUpdated,
    JobStopped,
    AllocationPlaced,
    AllocationStarted,
    AllocationFailed,
    AllocationCompleted,
    NodeJoined,
    NodeLeft,
    NodeDraining,
    NodeReady,
    EvaluationCompleted,
    DeploymentStarted,
    DeploymentCompleted,
    AllocationRestarted,
    AllocationLost,
    AllocationRescheduled,
    ReconcileCompleted,
    SpecDriftDetected,
    RollingUpdateStarted,
    RollingUpdateCompleted,
    SourceCreated,
    SourceReconciled,
    SourceFailed,
    SourceSuspended,
    SourceResumed,
    SourceJobCreated,
    SourceJobUpdated,
    SourceJobRemoved,
}

impl std::fmt::Display for EventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JobSubmitted => write!(f, "job_submitted"),
            Self::JobUpdated => write!(f, "job_updated"),
            Self::JobStopped => write!(f, "job_stopped"),
            Self::AllocationPlaced => write!(f, "allocation_placed"),
            Self::AllocationStarted => write!(f, "allocation_started"),
            Self::AllocationFailed => write!(f, "allocation_failed"),
            Self::AllocationCompleted => write!(f, "allocation_completed"),
            Self::NodeJoined => write!(f, "node_joined"),
            Self::NodeLeft => write!(f, "node_left"),
            Self::NodeDraining => write!(f, "node_draining"),
            Self::NodeReady => write!(f, "node_ready"),
            Self::EvaluationCompleted => write!(f, "evaluation_completed"),
            Self::DeploymentStarted => write!(f, "deployment_started"),
            Self::DeploymentCompleted => write!(f, "deployment_completed"),
            Self::AllocationRestarted => write!(f, "allocation_restarted"),
            Self::AllocationLost => write!(f, "allocation_lost"),
            Self::AllocationRescheduled => write!(f, "allocation_rescheduled"),
            Self::ReconcileCompleted => write!(f, "reconcile_completed"),
            Self::SpecDriftDetected => write!(f, "spec_drift_detected"),
            Self::RollingUpdateStarted => write!(f, "rolling_update_started"),
            Self::RollingUpdateCompleted => write!(f, "rolling_update_completed"),
            Self::SourceCreated => write!(f, "source_created"),
            Self::SourceReconciled => write!(f, "source_reconciled"),
            Self::SourceFailed => write!(f, "source_failed"),
            Self::SourceSuspended => write!(f, "source_suspended"),
            Self::SourceResumed => write!(f, "source_resumed"),
            Self::SourceJobCreated => write!(f, "source_job_created"),
            Self::SourceJobUpdated => write!(f, "source_job_updated"),
            Self::SourceJobRemoved => write!(f, "source_job_removed"),
        }
    }
}

impl EventKind {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s {
            "job_submitted" => Some(Self::JobSubmitted),
            "job_updated" => Some(Self::JobUpdated),
            "job_stopped" => Some(Self::JobStopped),
            "allocation_placed" => Some(Self::AllocationPlaced),
            "allocation_started" => Some(Self::AllocationStarted),
            "allocation_failed" => Some(Self::AllocationFailed),
            "allocation_completed" => Some(Self::AllocationCompleted),
            "node_joined" => Some(Self::NodeJoined),
            "node_left" => Some(Self::NodeLeft),
            "node_draining" => Some(Self::NodeDraining),
            "node_ready" => Some(Self::NodeReady),
            "evaluation_completed" => Some(Self::EvaluationCompleted),
            "deployment_started" => Some(Self::DeploymentStarted),
            "deployment_completed" => Some(Self::DeploymentCompleted),
            "allocation_restarted" => Some(Self::AllocationRestarted),
            "allocation_lost" => Some(Self::AllocationLost),
            "allocation_rescheduled" => Some(Self::AllocationRescheduled),
            "reconcile_completed" => Some(Self::ReconcileCompleted),
            "spec_drift_detected" => Some(Self::SpecDriftDetected),
            "rolling_update_started" => Some(Self::RollingUpdateStarted),
            "rolling_update_completed" => Some(Self::RollingUpdateCompleted),
            "source_created" => Some(Self::SourceCreated),
            "source_reconciled" => Some(Self::SourceReconciled),
            "source_failed" => Some(Self::SourceFailed),
            "source_suspended" => Some(Self::SourceSuspended),
            "source_resumed" => Some(Self::SourceResumed),
            "source_job_created" => Some(Self::SourceJobCreated),
            "source_job_updated" => Some(Self::SourceJobUpdated),
            "source_job_removed" => Some(Self::SourceJobRemoved),
            _ => None,
        }
    }
}

impl Event {
    pub fn new(kind: EventKind, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            kind,
            payload,
        }
    }
}

/// Ring buffer for events with a configurable capacity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRing {
    events: VecDeque<Event>,
    capacity: usize,
}

impl Default for EventRing {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
            capacity: 10_000,
        }
    }
}

impl EventRing {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, event: Event) {
        if self.events.len() >= self.capacity {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    pub fn list(&self) -> &VecDeque<Event> {
        &self.events
    }

    /// List events filtered by kind and/or since timestamp.
    pub fn query(&self, kind: Option<&EventKind>, since: Option<DateTime<Utc>>) -> Vec<&Event> {
        self.events
            .iter()
            .filter(|e| {
                kind.map_or(true, |k| &e.kind == k) && since.map_or(true, |s| e.timestamp >= s)
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_buffer_capacity() {
        let mut ring = EventRing::with_capacity(3);
        for i in 0..5 {
            ring.push(Event::new(
                EventKind::JobSubmitted,
                serde_json::json!({ "i": i }),
            ));
        }
        assert_eq!(ring.len(), 3);
        // Oldest events should be evicted
        let events: Vec<_> = ring.list().iter().collect();
        assert_eq!(events[0].payload["i"], 2);
        assert_eq!(events[1].payload["i"], 3);
        assert_eq!(events[2].payload["i"], 4);
    }

    #[test]
    fn test_query_by_kind() {
        let mut ring = EventRing::default();
        ring.push(Event::new(EventKind::JobSubmitted, serde_json::json!({})));
        ring.push(Event::new(EventKind::NodeJoined, serde_json::json!({})));
        ring.push(Event::new(EventKind::JobSubmitted, serde_json::json!({})));

        let filtered = ring.query(Some(&EventKind::JobSubmitted), None);
        assert_eq!(filtered.len(), 2);
    }
}
