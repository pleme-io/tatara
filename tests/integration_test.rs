/// Integration tests for tatara.
///
/// These tests verify end-to-end behavior of the domain models,
/// cluster state machine, and API layer without requiring a running server.

// Note: Integration tests that require a running server are marked with
// #[ignore] and can be run with `cargo test -- --ignored`.

mod domain_tests {
    use tatara::domain::allocation::{Allocation, AllocationState};
    use tatara::domain::event::{Event, EventKind, EventRing};
    use tatara::domain::job::{
        Constraint, Job, JobSpec, JobStatus, JobType, Resources, RestartPolicy, Task, TaskConfig,
        TaskGroup,
    };
    use tatara::domain::node::{Node, NodeStatus};
    use tatara::domain::release::{Release, ReleaseStatus};

    #[test]
    fn test_job_spec_into_job() {
        let spec = JobSpec {
            id: "test-job".to_string(),
            job_type: JobType::Service,
            groups: vec![TaskGroup {
                name: "web".to_string(),
                count: 3,
                tasks: vec![Task {
                    name: "nginx".to_string(),
                    driver: tatara::domain::job::DriverType::Exec,
                    config: TaskConfig::Exec {
                        command: "echo".to_string(),
                        args: vec!["hello".to_string()],
                        working_dir: None,
                    },
                    env: Default::default(),
                    resources: Resources {
                        cpu_mhz: 100,
                        memory_mb: 256,
                    },
                    health_checks: vec![],
                }],
                restart_policy: RestartPolicy::default(),
                resources: Resources {
                    cpu_mhz: 100,
                    memory_mb: 256,
                },
                network: None,
            }],
            constraints: vec![],
            meta: Default::default(),
        };

        let job = spec.into_job();
        assert_eq!(job.id, "test-job");
        assert_eq!(job.version, 1);
        assert_eq!(job.status, JobStatus::Pending);
        assert_eq!(job.groups.len(), 1);
        assert_eq!(job.groups[0].count, 3);
    }

    #[test]
    fn test_allocation_lifecycle() {
        let alloc = Allocation::new(
            "job-1".to_string(),
            "web".to_string(),
            "node-1".to_string(),
            vec!["nginx".to_string()],
        );

        assert_eq!(alloc.state, AllocationState::Pending);
        assert_eq!(alloc.job_id, "job-1");
        assert!(!alloc.is_terminal());
    }

    #[test]
    fn test_allocation_terminal_states() {
        let mut alloc = Allocation::new(
            "job-1".to_string(),
            "web".to_string(),
            "node-1".to_string(),
            vec!["nginx".to_string()],
        );

        alloc.state = AllocationState::Complete;
        assert!(alloc.is_terminal());

        alloc.state = AllocationState::Failed;
        assert!(alloc.is_terminal());

        alloc.state = AllocationState::Running;
        assert!(!alloc.is_terminal());
    }

    #[test]
    fn test_node_local_creates_valid_node() {
        let node = Node::local();
        assert_eq!(node.status, NodeStatus::Ready);
        assert!(node.eligible);
        assert!(node.total_resources.cpu_mhz > 0);
        assert!(node.attributes.contains_key("os"));
        assert!(node.attributes.contains_key("arch"));
    }

    #[test]
    fn test_event_ring_ordering() {
        let mut ring = EventRing::with_capacity(100);
        for i in 0..5 {
            ring.push(Event::new(
                EventKind::JobSubmitted,
                serde_json::json!({ "seq": i }),
            ));
        }

        let events: Vec<_> = ring.list().iter().collect();
        assert_eq!(events.len(), 5);
        for i in 0..5 {
            assert_eq!(events[i].payload["seq"], i);
        }
    }

    #[test]
    fn test_release_creation() {
        let release = Release::new(
            "myapp".to_string(),
            "github:user/myapp".to_string(),
            "myapp-job".to_string(),
        );

        assert_eq!(release.name, "myapp");
        assert_eq!(release.status, ReleaseStatus::Pending);
        assert_eq!(release.version, 1);
    }

    #[test]
    fn test_constraint_serialization() {
        let constraint = Constraint {
            attribute: "os".to_string(),
            operator: "=".to_string(),
            value: "linux".to_string(),
        };

        let json = serde_json::to_string(&constraint).unwrap();
        let deserialized: Constraint = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.attribute, "os");
        assert_eq!(deserialized.operator, "=");
        assert_eq!(deserialized.value, "linux");
    }

    #[test]
    fn test_resources_default() {
        let resources = Resources::default();
        assert_eq!(resources.cpu_mhz, 0);
        assert_eq!(resources.memory_mb, 0);
    }
}

mod cluster_state_tests {
    use std::collections::HashMap;

    use tatara::cluster::types::{ClusterState, NodeMeta, NodeRoles};
    use tatara::domain::allocation::Allocation;
    use tatara::domain::event::EventRing;
    use tatara::domain::job::{
        Job, JobSpec, JobStatus, JobType, Resources, Task, TaskConfig, TaskGroup,
    };

    #[test]
    fn test_cluster_state_default() {
        let state = ClusterState::default();
        assert!(state.jobs.is_empty());
        assert!(state.allocations.is_empty());
        assert!(state.nodes.is_empty());
        assert!(state.events.is_empty());
        assert!(state.job_history.is_empty());
        assert!(state.releases.is_empty());
    }

    #[test]
    fn test_cluster_state_job_insertion() {
        let mut state = ClusterState::default();

        let spec = JobSpec {
            id: "test".to_string(),
            job_type: JobType::Service,
            groups: vec![],
            constraints: vec![],
            meta: HashMap::new(),
        };
        let job = spec.into_job();

        state.jobs.insert(job.id.clone(), job.clone());
        assert_eq!(state.jobs.len(), 1);
        assert_eq!(state.jobs["test"].version, 1);
    }

    #[test]
    fn test_cluster_state_serialization_roundtrip() {
        let mut state = ClusterState::default();

        // Add a node
        let meta = NodeMeta {
            node_id: 1,
            hostname: "test-node".to_string(),
            http_addr: "127.0.0.1:4646".to_string(),
            gossip_addr: "127.0.0.1:4648".to_string(),
            raft_addr: "127.0.0.1:4649".to_string(),
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
            roles: NodeRoles::default(),
            drivers: vec![],
            total_resources: Resources {
                cpu_mhz: 4000,
                memory_mb: 8192,
            },
            available_resources: Resources {
                cpu_mhz: 3000,
                memory_mb: 6144,
            },
            allocations_running: 2,
            joined_at: chrono::Utc::now(),
            version: "0.2.0".to_string(),
            eligible: true,
        };
        state.nodes.insert(1, meta);

        // Serialize + deserialize
        let json = serde_json::to_string(&state).unwrap();
        let deserialized: ClusterState = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.nodes.len(), 1);
        assert_eq!(deserialized.nodes[&1].hostname, "test-node");
        assert!(deserialized.nodes[&1].eligible);
    }
}

mod scheduling_tests {
    use std::collections::HashMap;

    use tatara::domain::job::{
        Constraint, JobSpec, JobType, Resources, Task, TaskConfig, TaskGroup,
    };
    use tatara::domain::node::{Node, NodeStatus};

    fn make_node(id: &str, cpu: u64, mem: u64) -> Node {
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
            attributes: {
                let mut m = HashMap::new();
                m.insert("os".to_string(), "linux".to_string());
                m.insert("arch".to_string(), "x86_64".to_string());
                m
            },
            drivers: vec![],
            last_heartbeat: chrono::Utc::now(),
            allocations: Vec::new(),
        }
    }

    fn make_job_spec(id: &str, cpu: u64, mem: u64, count: u32) -> JobSpec {
        JobSpec {
            id: id.to_string(),
            job_type: JobType::Service,
            groups: vec![TaskGroup {
                name: "main".to_string(),
                count,
                tasks: vec![Task {
                    name: "app".to_string(),
                    driver: tatara::domain::job::DriverType::Exec,
                    config: TaskConfig::Exec {
                        command: "echo".to_string(),
                        args: vec![],
                        working_dir: None,
                    },
                    env: Default::default(),
                    resources: Resources {
                        cpu_mhz: cpu,
                        memory_mb: mem,
                    },
                    health_checks: vec![],
                }],
                restart_policy: Default::default(),
                resources: Resources {
                    cpu_mhz: cpu,
                    memory_mb: mem,
                },
                network: None,
            }],
            constraints: vec![],
            meta: Default::default(),
        }
    }

    #[test]
    fn test_job_spec_with_constraints() {
        let mut spec = make_job_spec("constrained-job", 500, 256, 2);
        spec.constraints = vec![
            Constraint {
                attribute: "os".to_string(),
                operator: "=".to_string(),
                value: "linux".to_string(),
            },
            Constraint {
                attribute: "arch".to_string(),
                operator: "=".to_string(),
                value: "x86_64".to_string(),
            },
        ];

        let job = spec.into_job();
        assert_eq!(job.constraints.len(), 2);
        assert_eq!(job.groups[0].resources.cpu_mhz, 500);
    }

    #[test]
    fn test_node_eligibility_flag() {
        let mut node = make_node("n1", 4000, 2048);
        assert!(node.eligible);

        node.eligible = false;
        assert!(!node.eligible);

        // Verify serialization roundtrip
        let json = serde_json::to_string(&node).unwrap();
        let deserialized: Node = serde_json::from_str(&json).unwrap();
        assert!(!deserialized.eligible);
    }
}
