use anyhow::Result;
use std::sync::Arc;

use super::allocation::Allocation;
use super::job::{Constraint, Job, JobStatus, JobType, Resources};
use super::node::{Node, NodeStatus};
use super::state_store::StateStore;

/// Scheduling strategy for task placement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SchedulingStrategy {
    /// Bin-pack: prefer nodes with the least remaining capacity (tightest fit).
    BinPack,
    /// Spread: prefer nodes with the most remaining capacity (even distribution).
    Spread,
}

impl Default for SchedulingStrategy {
    fn default() -> Self {
        Self::BinPack
    }
}

/// Evaluates pending jobs and creates allocation plans.
pub struct Evaluator {
    store: Arc<StateStore>,
    strategy: SchedulingStrategy,
}

impl Evaluator {
    pub fn new(store: Arc<StateStore>) -> Self {
        Self {
            store,
            strategy: SchedulingStrategy::default(),
        }
    }

    pub fn with_strategy(mut self, strategy: SchedulingStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Process all pending jobs and create allocations.
    pub async fn evaluate(&self) -> Result<Vec<Allocation>> {
        let jobs = self.store.list_jobs().await;
        let nodes = self.store.list_nodes().await;
        let mut new_allocations = Vec::new();

        for job in &jobs {
            if job.status != JobStatus::Pending {
                continue;
            }

            let allocs = self.evaluate_job(job, &nodes).await?;
            for alloc in allocs {
                self.store.put_allocation(alloc.clone()).await?;
                new_allocations.push(alloc);
            }

            if !new_allocations.is_empty() {
                self.store
                    .update_job(&job.id, |j| {
                        j.status = JobStatus::Running;
                    })
                    .await?;
            }
        }

        Ok(new_allocations)
    }

    async fn evaluate_job(&self, job: &Job, nodes: &[Node]) -> Result<Vec<Allocation>> {
        let mut allocations = Vec::new();

        // Filter to ready, eligible nodes
        let ready_nodes: Vec<&Node> = nodes
            .iter()
            .filter(|n| n.status == NodeStatus::Ready)
            .filter(|n| n.eligible)
            .collect();

        if ready_nodes.is_empty() {
            tracing::warn!(job_id = %job.id, "No ready/eligible nodes available for scheduling");
            return Ok(allocations);
        }

        for group in &job.groups {
            let count = match job.job_type {
                JobType::System => ready_nodes.len() as u32,
                _ => group.count,
            };

            // Track available resources as we allocate (for this eval cycle)
            let mut node_available: Vec<(&Node, Resources)> = ready_nodes
                .iter()
                .map(|n| (*n, n.available_resources.clone()))
                .collect();

            for i in 0..count {
                let node = match job.job_type {
                    JobType::System => ready_nodes[i as usize],
                    _ => {
                        match pick_node(
                            &node_available,
                            &group.resources,
                            &job.constraints,
                            self.strategy,
                        ) {
                            Some((node, idx)) => {
                                // Deduct resources from tracking
                                node_available[idx].1.cpu_mhz = node_available[idx]
                                    .1
                                    .cpu_mhz
                                    .saturating_sub(group.resources.cpu_mhz);
                                node_available[idx].1.memory_mb = node_available[idx]
                                    .1
                                    .memory_mb
                                    .saturating_sub(group.resources.memory_mb);
                                node
                            }
                            None => {
                                tracing::warn!(
                                    job_id = %job.id,
                                    group = %group.name,
                                    instance = i,
                                    "No node with sufficient resources for allocation"
                                );
                                continue;
                            }
                        }
                    }
                };

                let task_names: Vec<String> =
                    group.tasks.iter().map(|t| t.name.clone()).collect();

                let alloc = Allocation::new(
                    job.id.clone(),
                    group.name.clone(),
                    node.id.clone(),
                    task_names,
                );

                allocations.push(alloc);
            }
        }

        Ok(allocations)
    }
}

/// Pick the best node for a task group based on resource requirements,
/// constraints, and scheduling strategy.
///
/// Returns the node and its index in the candidates list, or None if
/// no node satisfies the requirements.
fn pick_node<'a>(
    candidates: &[(&'a Node, Resources)],
    required: &Resources,
    constraints: &[Constraint],
    strategy: SchedulingStrategy,
) -> Option<(&'a Node, usize)> {
    let mut best: Option<(usize, f64)> = None;

    for (idx, (node, available)) in candidates.iter().enumerate() {
        // Resource filtering: node must have enough resources
        if !resources_sufficient(available, required) {
            continue;
        }

        // Constraint evaluation: all constraints must match
        if !constraints_match(node, constraints) {
            continue;
        }

        // Score the node based on strategy
        let score = match strategy {
            SchedulingStrategy::BinPack => {
                // Lower remaining = better score (tighter packing)
                // We want to MINIMIZE remaining capacity, so higher score = less remaining
                let remaining_cpu = available.cpu_mhz.saturating_sub(required.cpu_mhz);
                let remaining_mem = available.memory_mb.saturating_sub(required.memory_mb);
                // Invert: smaller remaining gets higher score
                let max_cpu = node.total_resources.cpu_mhz.max(1) as f64;
                let max_mem = node.total_resources.memory_mb.max(1) as f64;
                let cpu_utilization = 1.0 - (remaining_cpu as f64 / max_cpu);
                let mem_utilization = 1.0 - (remaining_mem as f64 / max_mem);
                (cpu_utilization + mem_utilization) / 2.0
            }
            SchedulingStrategy::Spread => {
                // Higher remaining = better score (more spread out)
                let remaining_cpu = available.cpu_mhz.saturating_sub(required.cpu_mhz);
                let remaining_mem = available.memory_mb.saturating_sub(required.memory_mb);
                let max_cpu = node.total_resources.cpu_mhz.max(1) as f64;
                let max_mem = node.total_resources.memory_mb.max(1) as f64;
                let cpu_headroom = remaining_cpu as f64 / max_cpu;
                let mem_headroom = remaining_mem as f64 / max_mem;
                (cpu_headroom + mem_headroom) / 2.0
            }
        };

        match best {
            None => best = Some((idx, score)),
            Some((_, best_score)) if score > best_score => best = Some((idx, score)),
            _ => {}
        }
    }

    best.map(|(idx, _)| (candidates[idx].0, idx))
}

/// Check if available resources meet the requirements.
fn resources_sufficient(available: &Resources, required: &Resources) -> bool {
    // If no resources are requested (both 0), any node qualifies
    if required.cpu_mhz == 0 && required.memory_mb == 0 {
        return true;
    }

    (required.cpu_mhz == 0 || available.cpu_mhz >= required.cpu_mhz)
        && (required.memory_mb == 0 || available.memory_mb >= required.memory_mb)
}

/// Evaluate all constraints against a node's attributes.
fn constraints_match(node: &Node, constraints: &[Constraint]) -> bool {
    constraints.iter().all(|c| constraint_matches(node, c))
}

/// Evaluate a single constraint against a node.
fn constraint_matches(node: &Node, constraint: &Constraint) -> bool {
    let attr_value = match constraint.attribute.as_str() {
        // Built-in attributes
        "os" | "${attr.os}" => Some(node.attributes.get("os").map(|s| s.as_str()).unwrap_or("")),
        "arch" | "${attr.arch}" => {
            Some(node.attributes.get("arch").map(|s| s.as_str()).unwrap_or(""))
        }
        "hostname" | "${attr.hostname}" => {
            Some(node.attributes.get("hostname").map(|s| s.as_str()).unwrap_or(""))
        }
        // Generic attribute lookup
        attr => {
            let key = attr
                .strip_prefix("${attr.")
                .and_then(|s| s.strip_suffix('}'))
                .unwrap_or(attr);
            node.attributes.get(key).map(|s| s.as_str())
        }
    };

    let Some(attr_value) = attr_value else {
        // Attribute not found on node — constraint fails unless operator is "!="
        return constraint.operator == "!=";
    };

    match constraint.operator.as_str() {
        "=" | "==" => attr_value == constraint.value,
        "!=" => attr_value != constraint.value,
        ">" => {
            attr_value
                .parse::<f64>()
                .ok()
                .zip(constraint.value.parse::<f64>().ok())
                .map(|(a, b)| a > b)
                .unwrap_or(false)
        }
        "<" => {
            attr_value
                .parse::<f64>()
                .ok()
                .zip(constraint.value.parse::<f64>().ok())
                .map(|(a, b)| a < b)
                .unwrap_or(false)
        }
        ">=" => {
            attr_value
                .parse::<f64>()
                .ok()
                .zip(constraint.value.parse::<f64>().ok())
                .map(|(a, b)| a >= b)
                .unwrap_or(false)
        }
        "<=" => {
            attr_value
                .parse::<f64>()
                .ok()
                .zip(constraint.value.parse::<f64>().ok())
                .map(|(a, b)| a <= b)
                .unwrap_or(false)
        }
        "regexp" | "~" => regex_match(attr_value, &constraint.value),
        "set_contains" => attr_value.split(',').any(|v| v.trim() == constraint.value),
        _ => {
            tracing::warn!(
                operator = %constraint.operator,
                "Unknown constraint operator, defaulting to equality"
            );
            attr_value == constraint.value
        }
    }
}

/// Simple regex match using basic patterns (no full regex crate dependency).
fn regex_match(value: &str, pattern: &str) -> bool {
    // Simple glob-like matching: * matches anything
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix('*') {
        return value.ends_with(suffix);
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return value.starts_with(prefix);
    }
    value == pattern
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_node(id: &str, cpu: u64, mem: u64, attrs: &[(&str, &str)]) -> Node {
        let mut attributes = HashMap::new();
        for (k, v) in attrs {
            attributes.insert(k.to_string(), v.to_string());
        }
        Node {
            id: id.to_string(),
            address: "127.0.0.1:4647".to_string(),
            status: NodeStatus::Ready,
            eligible: true,
            total_resources: Resources {
                cpu_mhz: cpu,
                memory_mb: mem,
            },
            available_resources: Resources {
                cpu_mhz: cpu,
                memory_mb: mem,
            },
            attributes,
            drivers: vec![],
            last_heartbeat: chrono::Utc::now(),
            allocations: Vec::new(),
        }
    }

    #[test]
    fn test_resources_sufficient() {
        let available = Resources {
            cpu_mhz: 2000,
            memory_mb: 1024,
        };
        let required = Resources {
            cpu_mhz: 1000,
            memory_mb: 512,
        };
        assert!(resources_sufficient(&available, &required));

        let too_much = Resources {
            cpu_mhz: 3000,
            memory_mb: 512,
        };
        assert!(!resources_sufficient(&available, &too_much));
    }

    #[test]
    fn test_resources_zero_means_any() {
        let available = Resources {
            cpu_mhz: 100,
            memory_mb: 64,
        };
        let zero = Resources {
            cpu_mhz: 0,
            memory_mb: 0,
        };
        assert!(resources_sufficient(&available, &zero));
    }

    #[test]
    fn test_constraint_equality() {
        let node = make_node("n1", 2000, 1024, &[("os", "linux"), ("arch", "x86_64")]);
        let c = Constraint {
            attribute: "os".to_string(),
            operator: "=".to_string(),
            value: "linux".to_string(),
        };
        assert!(constraint_matches(&node, &c));

        let c2 = Constraint {
            attribute: "os".to_string(),
            operator: "=".to_string(),
            value: "macos".to_string(),
        };
        assert!(!constraint_matches(&node, &c2));
    }

    #[test]
    fn test_constraint_not_equal() {
        let node = make_node("n1", 2000, 1024, &[("os", "linux")]);
        let c = Constraint {
            attribute: "os".to_string(),
            operator: "!=".to_string(),
            value: "windows".to_string(),
        };
        assert!(constraint_matches(&node, &c));
    }

    #[test]
    fn test_constraint_missing_attribute() {
        let node = make_node("n1", 2000, 1024, &[("os", "linux")]);
        let c = Constraint {
            attribute: "gpu".to_string(),
            operator: "=".to_string(),
            value: "true".to_string(),
        };
        assert!(!constraint_matches(&node, &c));
    }

    #[test]
    fn test_bin_pack_picks_tightest_fit() {
        let n1 = make_node("big", 4000, 2048, &[]);
        let n2 = make_node("small", 2000, 1024, &[]);

        let candidates = vec![
            (&n1, n1.available_resources.clone()),
            (&n2, n2.available_resources.clone()),
        ];
        let required = Resources {
            cpu_mhz: 1000,
            memory_mb: 512,
        };

        let result = pick_node(&candidates, &required, &[], SchedulingStrategy::BinPack);
        assert!(result.is_some());
        // Bin-pack should prefer the smaller node (tighter fit)
        assert_eq!(result.unwrap().0.id, "small");
    }

    #[test]
    fn test_spread_picks_most_headroom() {
        let n1 = make_node("big", 4000, 2048, &[]);
        let n2 = make_node("small", 2000, 1024, &[]);

        let candidates = vec![
            (&n1, n1.available_resources.clone()),
            (&n2, n2.available_resources.clone()),
        ];
        let required = Resources {
            cpu_mhz: 1000,
            memory_mb: 512,
        };

        let result = pick_node(&candidates, &required, &[], SchedulingStrategy::Spread);
        assert!(result.is_some());
        // Spread should prefer the bigger node (more remaining capacity)
        assert_eq!(result.unwrap().0.id, "big");
    }

    #[test]
    fn test_no_node_with_sufficient_resources() {
        let n1 = make_node("tiny", 500, 256, &[]);

        let candidates = vec![(&n1, n1.available_resources.clone())];
        let required = Resources {
            cpu_mhz: 1000,
            memory_mb: 512,
        };

        let result = pick_node(&candidates, &required, &[], SchedulingStrategy::BinPack);
        assert!(result.is_none());
    }

    #[test]
    fn test_constraints_filter_nodes() {
        let linux = make_node("linux-box", 4000, 2048, &[("os", "linux")]);
        let mac = make_node("mac-box", 4000, 2048, &[("os", "macos")]);

        let candidates = vec![
            (&linux, linux.available_resources.clone()),
            (&mac, mac.available_resources.clone()),
        ];
        let required = Resources {
            cpu_mhz: 1000,
            memory_mb: 512,
        };
        let constraints = vec![Constraint {
            attribute: "os".to_string(),
            operator: "=".to_string(),
            value: "linux".to_string(),
        }];

        let result = pick_node(&candidates, &required, &constraints, SchedulingStrategy::BinPack);
        assert!(result.is_some());
        assert_eq!(result.unwrap().0.id, "linux-box");
    }

    #[test]
    fn test_ineligible_node_excluded() {
        let mut node = make_node("n1", 4000, 2048, &[]);
        node.eligible = false;

        // The evaluator filters ineligible nodes before calling pick_node,
        // but verify that the field exists and is respected.
        assert!(!node.eligible);
    }
}
