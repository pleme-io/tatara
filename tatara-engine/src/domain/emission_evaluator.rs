//! Emission evaluator — detects triggers and instantiates bounded DAGs.
//!
//! When an asymptotic convergence point's convergence state matches a
//! trigger condition, the evaluator decides whether to instantiate a
//! bounded DAG from the emission schema catalog.

use tatara_core::domain::convergence_state::*;
use tatara_core::domain::emission::*;

/// Evaluates emission triggers against current convergence state.
pub struct EmissionEvaluator;

impl EmissionEvaluator {
    /// Check all triggers in the schema against the current state.
    /// Returns a list of instantiation decisions.
    pub fn evaluate_triggers(
        schema: &EmissionSchema,
        state: &ConvergenceState,
    ) -> Vec<InstantiationDecision> {
        schema
            .triggers
            .iter()
            .filter_map(|trigger| {
                let fires = Self::check_condition(&trigger.condition, state);
                if !fires {
                    return None;
                }

                // Check concurrency limits
                let limit = schema
                    .concurrency_limits
                    .get(&trigger.template_name)
                    .copied()
                    .unwrap_or(usize::MAX);

                // For now, always allow if limit > 0
                if limit == 0 {
                    return Some(InstantiationDecision::Defer {
                        reason: format!(
                            "concurrency limit reached for {}",
                            trigger.template_name
                        ),
                    });
                }

                // Check if template exists in catalog
                let template_exists = schema
                    .templates
                    .iter()
                    .any(|t| t.name == trigger.template_name);

                if !template_exists {
                    return Some(InstantiationDecision::Escalate {
                        reason: format!(
                            "no template '{}' in emission catalog — schema gap",
                            trigger.template_name
                        ),
                    });
                }

                Some(InstantiationDecision::Instantiate {
                    template_name: trigger.template_name.clone(),
                    params: std::collections::HashMap::new(),
                })
            })
            .collect()
    }

    /// Check whether a trigger condition fires given current state.
    fn check_condition(condition: &TriggerCondition, state: &ConvergenceState) -> bool {
        match condition {
            TriggerCondition::Threshold { value, .. } => {
                // Fire if current distance exceeds threshold
                state.distance.numeric() > *value
            }
            TriggerCondition::Event { .. } => {
                // Event triggers are checked externally — always false in polling mode
                false
            }
            TriggerCondition::Schedule { .. } => {
                // Schedule triggers are checked by a cron evaluator — not here
                false
            }
            TriggerCondition::Manual => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_schema() -> EmissionSchema {
        EmissionSchema {
            templates: vec![
                BoundedPointTemplate {
                    name: "migration".into(),
                    point_type: ConvergencePointType::Transform,
                    substrate: SubstrateType::Compute,
                    description: "Migrate workload".into(),
                    preconditions: vec![],
                    postconditions: vec![],
                },
                BoundedPointTemplate {
                    name: "scaling".into(),
                    point_type: ConvergencePointType::Fork,
                    substrate: SubstrateType::Compute,
                    description: "Scale workload".into(),
                    preconditions: vec![],
                    postconditions: vec![],
                },
            ],
            triggers: vec![
                EmissionTrigger {
                    template_name: "migration".into(),
                    condition: TriggerCondition::Threshold {
                        metric: "cost".into(),
                        value: 0.5,
                    },
                },
                EmissionTrigger {
                    template_name: "scaling".into(),
                    condition: TriggerCondition::Threshold {
                        metric: "utilization".into(),
                        value: 0.8,
                    },
                },
            ],
            concurrency_limits: HashMap::from([
                ("migration".into(), 2),
                ("scaling".into(), 1),
            ]),
        }
    }

    #[test]
    fn test_threshold_trigger_fires() {
        let schema = make_schema();
        let mut state = ConvergenceState::new("test");
        state.distance = ConvergenceDistance::Diverged {
            reason: "cost too high".into(),
        }; // numeric = 1.0, above 0.5

        let decisions = EmissionEvaluator::evaluate_triggers(&schema, &state);
        // Both triggers should fire (distance 1.0 > 0.5 and 1.0 > 0.8)
        assert_eq!(decisions.len(), 2);
        assert!(matches!(
            &decisions[0],
            InstantiationDecision::Instantiate { template_name, .. } if template_name == "migration"
        ));
    }

    #[test]
    fn test_threshold_below_does_not_fire() {
        let schema = make_schema();
        let state = ConvergenceState {
            distance: ConvergenceDistance::Partial {
                matching: 9,
                total: 10,
                pending: vec![],
            }, // numeric = 0.1
            ..ConvergenceState::new("test")
        };

        let decisions = EmissionEvaluator::evaluate_triggers(&schema, &state);
        // 0.1 < 0.5, neither trigger fires
        assert!(decisions.is_empty());
    }

    #[test]
    fn test_missing_template_escalates() {
        let schema = EmissionSchema {
            templates: vec![], // empty catalog
            triggers: vec![EmissionTrigger {
                template_name: "nonexistent".into(),
                condition: TriggerCondition::Threshold {
                    metric: "x".into(),
                    value: 0.0,
                },
            }],
            concurrency_limits: HashMap::new(),
        };
        let mut state = ConvergenceState::new("test");
        state.distance = ConvergenceDistance::Diverged {
            reason: "diverged".into(),
        };

        let decisions = EmissionEvaluator::evaluate_triggers(&schema, &state);
        assert_eq!(decisions.len(), 1);
        assert!(matches!(&decisions[0], InstantiationDecision::Escalate { .. }));
    }

    #[test]
    fn test_zero_concurrency_defers() {
        let schema = EmissionSchema {
            templates: vec![BoundedPointTemplate {
                name: "blocked".into(),
                point_type: ConvergencePointType::Transform,
                substrate: SubstrateType::Compute,
                description: "blocked".into(),
                preconditions: vec![],
                postconditions: vec![],
            }],
            triggers: vec![EmissionTrigger {
                template_name: "blocked".into(),
                condition: TriggerCondition::Threshold {
                    metric: "x".into(),
                    value: 0.0,
                },
            }],
            concurrency_limits: HashMap::from([("blocked".into(), 0)]),
        };
        let mut state = ConvergenceState::new("test");
        state.distance = ConvergenceDistance::Diverged {
            reason: "diverged".into(),
        };

        let decisions = EmissionEvaluator::evaluate_triggers(&schema, &state);
        assert_eq!(decisions.len(), 1);
        assert!(matches!(&decisions[0], InstantiationDecision::Defer { .. }));
    }

    #[test]
    fn test_converged_state_no_triggers() {
        let schema = make_schema();
        let state = ConvergenceState {
            distance: ConvergenceDistance::Converged, // numeric = 0.0
            ..ConvergenceState::new("test")
        };

        let decisions = EmissionEvaluator::evaluate_triggers(&schema, &state);
        assert!(decisions.is_empty());
    }

    #[test]
    fn test_manual_trigger_never_fires() {
        let schema = EmissionSchema {
            templates: vec![BoundedPointTemplate {
                name: "manual_op".into(),
                point_type: ConvergencePointType::Gate,
                substrate: SubstrateType::Regulatory,
                description: "manual".into(),
                preconditions: vec![],
                postconditions: vec![],
            }],
            triggers: vec![EmissionTrigger {
                template_name: "manual_op".into(),
                condition: TriggerCondition::Manual,
            }],
            concurrency_limits: HashMap::new(),
        };
        let mut state = ConvergenceState::new("test");
        state.distance = ConvergenceDistance::Diverged {
            reason: "diverged".into(),
        };

        let decisions = EmissionEvaluator::evaluate_triggers(&schema, &state);
        assert!(decisions.is_empty());
    }
}
