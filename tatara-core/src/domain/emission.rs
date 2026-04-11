//! Emission schemas for asymptotic convergence points.
//!
//! An asymptotic point runs in perpetuity and produces bounded DAGs
//! from a catalog of known templates. The emission schema declares
//! what bounded DAG types an asymptotic point can produce, when to
//! produce them, and concurrency limits.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::convergence_state::{ConvergencePointType, SubstrateType};

/// The catalog of bounded DAG templates an asymptotic point can emit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionSchema {
    /// Available bounded point templates.
    pub templates: Vec<BoundedPointTemplate>,
    /// Trigger conditions for each template.
    pub triggers: Vec<EmissionTrigger>,
    /// Max concurrent DAGs per template name.
    pub concurrency_limits: HashMap<String, usize>,
}

/// A pre-defined bounded DAG template ready to instantiate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoundedPointTemplate {
    /// Template name (e.g., "migration", "cert-rotation", "scaling").
    pub name: String,
    /// Structure type for the produced convergence point.
    pub point_type: ConvergencePointType,
    /// Which substrate this bounded DAG operates on.
    pub substrate: SubstrateType,
    /// Human-readable description.
    pub description: String,
    /// Expected preconditions.
    pub preconditions: Vec<String>,
    /// Expected postconditions.
    pub postconditions: Vec<String>,
}

/// A trigger condition that causes an asymptotic point to emit a bounded DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionTrigger {
    /// Which template to instantiate when this trigger fires.
    pub template_name: String,
    /// The condition that fires this trigger.
    pub condition: TriggerCondition,
}

/// The condition that fires an emission trigger.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerCondition {
    /// Fire when a metric crosses a threshold.
    Threshold { metric: String, value: f64 },
    /// Fire on a schedule.
    Schedule { cron: String },
    /// Fire on an event.
    Event { event_type: String },
    /// Fire only when manually requested.
    Manual,
}

/// The decision made when a trigger fires.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstantiationDecision {
    /// Instantiate the bounded DAG from the template.
    Instantiate {
        template_name: String,
        params: HashMap<String, String>,
    },
    /// Conditions aren't right — defer to next evaluation.
    Defer { reason: String },
    /// No known template matches — escalate (schema gap).
    Escalate { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_emission_schema_serde() {
        let schema = EmissionSchema {
            templates: vec![BoundedPointTemplate {
                name: "migration".into(),
                point_type: ConvergencePointType::Transform,
                substrate: SubstrateType::Compute,
                description: "Migrate workload to cheaper substrate".into(),
                preconditions: vec!["budget_approved".into()],
                postconditions: vec!["workload_healthy".into()],
            }],
            triggers: vec![EmissionTrigger {
                template_name: "migration".into(),
                condition: TriggerCondition::Threshold {
                    metric: "cost_per_hour".into(),
                    value: 0.10,
                },
            }],
            concurrency_limits: HashMap::from([("migration".into(), 2)]),
        };
        let json = serde_json::to_string(&schema).unwrap();
        let parsed: EmissionSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.templates.len(), 1);
        assert_eq!(parsed.triggers.len(), 1);
    }

    #[test]
    fn test_trigger_conditions() {
        let threshold = TriggerCondition::Threshold {
            metric: "cost".into(),
            value: 0.5,
        };
        let schedule = TriggerCondition::Schedule {
            cron: "0 * * * *".into(),
        };
        let event = TriggerCondition::Event {
            event_type: "spot_price_change".into(),
        };
        let manual = TriggerCondition::Manual;

        for condition in [threshold, schedule, event, manual] {
            let json = serde_json::to_string(&condition).unwrap();
            let _: TriggerCondition = serde_json::from_str(&json).unwrap();
        }
    }

    #[test]
    fn test_instantiation_decisions() {
        let instantiate = InstantiationDecision::Instantiate {
            template_name: "migration".into(),
            params: HashMap::from([("target_node".into(), "node-2".into())]),
        };
        let defer = InstantiationDecision::Defer {
            reason: "too many concurrent migrations".into(),
        };
        let escalate = InstantiationDecision::Escalate {
            reason: "unknown trigger pattern".into(),
        };

        for decision in [instantiate, defer, escalate] {
            let json = serde_json::to_string(&decision).unwrap();
            let _: InstantiationDecision = serde_json::from_str(&json).unwrap();
        }
    }
}
