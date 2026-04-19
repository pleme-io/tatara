//! Test fixture builders for creating domain objects.
//!
//! These functions provide convenient constructors for test data,
//! using sensible defaults while allowing customization.

use chrono::Utc;
use std::collections::HashMap;

use tatara_core::cluster::types::{NodeMeta, NodeRoles};
use tatara_core::domain::allocation::Allocation;
use tatara_core::domain::job::*;
use tatara_core::domain::release::{Release, ReleaseStatus};

/// Create a minimal job with sensible defaults.
pub fn job(id: &str) -> Job {
    job_with_group(id, "main", 1, 500, 256)
}

/// Create a job with a specific task group configuration.
pub fn job_with_group(id: &str, group_name: &str, count: u32, cpu_mhz: u64, memory_mb: u64) -> Job {
    Job {
        id: id.to_string(),
        version: 1,
        job_type: JobType::Service,
        status: JobStatus::Pending,
        submitted_at: Utc::now(),
        groups: vec![TaskGroup {
            name: group_name.to_string(),
            count,
            tasks: vec![task("app", cpu_mhz, memory_mb)],
            restart_policy: RestartPolicy::default(),
            resources: Resources { cpu_mhz, memory_mb },
            network: None,
            secrets: vec![],
            volumes: vec![],
            service_name: None,
        }],
        constraints: vec![],
        meta: HashMap::new(),
        spec_hash: None,
    }
}

/// Create a job spec (pre-submission form).
pub fn job_spec(id: &str) -> JobSpec {
    job_spec_with_group(id, "main", 1, 500, 256)
}

/// Create a job spec with specific group configuration.
pub fn job_spec_with_group(
    id: &str,
    group_name: &str,
    count: u32,
    cpu_mhz: u64,
    memory_mb: u64,
) -> JobSpec {
    JobSpec {
        id: id.to_string(),
        job_type: JobType::Service,
        groups: vec![TaskGroup {
            name: group_name.to_string(),
            count,
            tasks: vec![task("app", cpu_mhz, memory_mb)],
            restart_policy: RestartPolicy::default(),
            resources: Resources { cpu_mhz, memory_mb },
            network: None,
            secrets: vec![],
            volumes: vec![],
            service_name: None,
        }],
        constraints: vec![],
        meta: HashMap::new(),
    }
}

/// Create a batch job spec.
pub fn batch_job_spec(id: &str, cpu_mhz: u64, memory_mb: u64) -> JobSpec {
    JobSpec {
        id: id.to_string(),
        job_type: JobType::Batch,
        groups: vec![TaskGroup {
            name: "main".to_string(),
            count: 1,
            tasks: vec![task("worker", cpu_mhz, memory_mb)],
            restart_policy: RestartPolicy {
                mode: RestartMode::Never,
                ..Default::default()
            },
            resources: Resources { cpu_mhz, memory_mb },
            network: None,
            secrets: vec![],
            volumes: vec![],
            service_name: None,
        }],
        constraints: vec![],
        meta: HashMap::new(),
    }
}

/// Create a job spec with constraints.
pub fn constrained_job_spec(id: &str, constraints: Vec<Constraint>) -> JobSpec {
    let mut spec = job_spec(id);
    spec.constraints = constraints;
    spec
}

/// Create a job spec that looks like a forge-deployed workload.
pub fn forge_job_spec(name: &str, flake_ref: &str) -> JobSpec {
    JobSpec {
        id: name.to_string(),
        job_type: JobType::Service,
        groups: vec![TaskGroup {
            name: "main".to_string(),
            count: 1,
            tasks: vec![Task {
                name: "app".to_string(),
                driver: DriverType::Nix,
                config: TaskConfig::Nix {
                    flake_ref: flake_ref.to_string(),
                    args: vec![],
                },
                env: HashMap::new(),
                resources: Resources {
                    cpu_mhz: 500,
                    memory_mb: 256,
                },
                health_checks: vec![],
                volume_claims: vec![],
            }],
            restart_policy: RestartPolicy::default(),
            resources: Resources {
                cpu_mhz: 500,
                memory_mb: 256,
            },
            network: None,
            secrets: vec![],
            volumes: vec![],
            service_name: None,
        }],
        constraints: vec![],
        meta: {
            let mut m = HashMap::new();
            m.insert("forge".to_string(), "true".to_string());
            m.insert("flake_ref".to_string(), flake_ref.to_string());
            m
        },
    }
}

/// Create an exec task with given resource requirements.
pub fn task(name: &str, cpu_mhz: u64, memory_mb: u64) -> Task {
    Task {
        name: name.to_string(),
        driver: DriverType::Exec,
        config: TaskConfig::Exec {
            command: "echo".to_string(),
            args: vec!["hello".to_string()],
            working_dir: None,
        },
        env: HashMap::new(),
        resources: Resources { cpu_mhz, memory_mb },
        health_checks: vec![],
        volume_claims: vec![],
    }
}

/// Create a node metadata entry.
pub fn node_meta(node_id: u64, hostname: &str, cpu_mhz: u64, memory_mb: u64) -> NodeMeta {
    NodeMeta {
        node_id,
        hostname: hostname.to_string(),
        http_addr: format!("127.0.0.1:{}", 4646 + node_id),
        gossip_addr: format!("127.0.0.1:{}", 4648 + node_id),
        raft_addr: format!("127.0.0.1:{}", 4649 + node_id),
        os: "linux".to_string(),
        arch: "x86_64".to_string(),
        roles: NodeRoles::default(),
        drivers: vec![DriverType::Exec, DriverType::Nix],
        total_resources: Resources { cpu_mhz, memory_mb },
        available_resources: Resources { cpu_mhz, memory_mb },
        allocations_running: 0,
        joined_at: Utc::now(),
        version: "0.2.0".to_string(),
        eligible: true,
        wireguard_pubkey: None,
        tunnel_address: None,
    }
}

/// Create a node metadata entry with custom attributes baked into os/arch.
pub fn node_meta_with_os(
    node_id: u64,
    hostname: &str,
    os: &str,
    arch: &str,
    cpu_mhz: u64,
    memory_mb: u64,
) -> NodeMeta {
    let mut meta = node_meta(node_id, hostname, cpu_mhz, memory_mb);
    meta.os = os.to_string();
    meta.arch = arch.to_string();
    meta
}

/// Create an allocation for a job on a node.
pub fn allocation(job_id: &str, group_name: &str, node_id: &str) -> Allocation {
    Allocation::new(
        job_id.to_string(),
        group_name.to_string(),
        node_id.to_string(),
        vec!["app".to_string()],
    )
}

/// Create a constraint.
pub fn constraint(attribute: &str, operator: &str, value: &str) -> Constraint {
    Constraint {
        attribute: attribute.to_string(),
        operator: operator.to_string(),
        value: value.to_string(),
    }
}

/// Create an active release.
pub fn release(name: &str, flake_ref: &str, job_id: &str) -> Release {
    let mut r = Release::new(name.to_string(), flake_ref.to_string(), job_id.to_string());
    r.status = ReleaseStatus::Active;
    r
}

/// Create a pending release.
pub fn pending_release(name: &str, flake_ref: &str, job_id: &str) -> Release {
    Release::new(name.to_string(), flake_ref.to_string(), job_id.to_string())
}
