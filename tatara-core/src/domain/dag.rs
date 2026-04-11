//! Job dependency DAG types and topological sort.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// A dependency on another job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobDependency {
    /// ID of the job this depends on.
    pub job_id: String,

    /// Condition that must be met.
    #[serde(default)]
    pub condition: DependencyCondition,
}

/// What condition the dependency must meet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DependencyCondition {
    /// Dependency must have at least one healthy running allocation.
    #[default]
    Healthy,
    /// Dependency must have completed (for batch jobs).
    Complete,
    /// Dependency must have produced an output.
    OutputReady,
}

/// An output produced by a job (e.g., a Nix store path).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobOutput {
    pub key: String,
    pub value: String,
}

/// Topological sort of job IDs using Kahn's algorithm.
/// Returns sorted order or an error listing the cycle participants.
pub fn topological_sort(
    job_ids: &[String],
    dependencies: &HashMap<String, Vec<String>>,
) -> Result<Vec<String>, Vec<String>> {
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();

    for id in job_ids {
        in_degree.entry(id.as_str()).or_insert(0);
        adj.entry(id.as_str()).or_default();
    }

    let job_set: HashSet<&str> = job_ids.iter().map(|s| s.as_str()).collect();

    for (job_id, deps) in dependencies {
        for dep in deps {
            // Validate that dependencies reference existing jobs
            if !job_set.contains(dep.as_str()) {
                return Err(vec![format!(
                    "job '{}' depends on unknown job '{}'",
                    job_id, dep
                )]);
            }
            adj.entry(dep.as_str()).or_default().push(job_id.as_str());
            *in_degree.entry(job_id.as_str()).or_insert(0) += 1;
        }
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&id, _)| id)
        .collect();

    let mut sorted = Vec::new();

    while let Some(node) = queue.pop_front() {
        sorted.push(node.to_string());
        if let Some(neighbors) = adj.get(node) {
            for &neighbor in neighbors {
                if let Some(deg) = in_degree.get_mut(neighbor) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }

    if sorted.len() == job_ids.len() {
        Ok(sorted)
    } else {
        let cycle: Vec<String> = in_degree
            .iter()
            .filter(|(_, &deg)| deg > 0)
            .map(|(&id, _)| id.to_string())
            .collect();
        Err(cycle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_dag() {
        let ids = vec!["a".into(), "b".into(), "c".into()];
        let mut deps = HashMap::new();
        deps.insert("b".into(), vec!["a".into()]);
        deps.insert("c".into(), vec!["b".into()]);

        let sorted = topological_sort(&ids, &deps).unwrap();
        assert_eq!(sorted, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parallel_deps() {
        let ids = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let mut deps = HashMap::new();
        deps.insert("c".into(), vec!["a".into(), "b".into()]);
        deps.insert("d".into(), vec!["c".into()]);

        let sorted = topological_sort(&ids, &deps).unwrap();
        // a and b can be in any order, but must come before c, which comes before d
        let pos_a = sorted.iter().position(|x| x == "a").unwrap();
        let pos_b = sorted.iter().position(|x| x == "b").unwrap();
        let pos_c = sorted.iter().position(|x| x == "c").unwrap();
        let pos_d = sorted.iter().position(|x| x == "d").unwrap();
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_c);
        assert!(pos_c < pos_d);
    }

    #[test]
    fn test_cycle_detection() {
        let ids = vec!["a".into(), "b".into(), "c".into()];
        let mut deps = HashMap::new();
        deps.insert("a".into(), vec!["c".into()]);
        deps.insert("b".into(), vec!["a".into()]);
        deps.insert("c".into(), vec!["b".into()]);

        let result = topological_sort(&ids, &deps);
        assert!(result.is_err());
    }

    #[test]
    fn test_no_deps() {
        let ids = vec!["a".into(), "b".into(), "c".into()];
        let deps = HashMap::new();

        let sorted = topological_sort(&ids, &deps).unwrap();
        assert_eq!(sorted.len(), 3);
    }
}
