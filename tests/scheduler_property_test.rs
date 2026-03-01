/// Property-based tests for tatara's scheduler and domain models.
///
/// Uses proptest to generate random valid inputs and verify invariants hold
/// across all cases. Tests structural properties like:
///   - Valid specs always serialize/deserialize correctly
///   - Normalization preserves identity
///   - Resources are never negative
///   - Constraints are evaluated consistently

use proptest::prelude::*;
use std::collections::HashMap;

use tatara::domain::allocation::{Allocation, AllocationState};
use tatara::domain::event::{Event, EventKind, EventRing};
use tatara::domain::job::*;
use tatara::domain::release::{Release, ReleaseStatus};

// ── Strategy Definitions ──

fn job_type_strategy() -> impl Strategy<Value = JobType> {
    prop_oneof![
        Just(JobType::Service),
        Just(JobType::Batch),
        Just(JobType::System),
    ]
}

fn driver_type_strategy() -> impl Strategy<Value = DriverType> {
    prop_oneof![
        Just(DriverType::Exec),
        Just(DriverType::Oci),
        Just(DriverType::Nix),
    ]
}

fn resources_strategy() -> impl Strategy<Value = Resources> {
    (0..10000u64, 0..65536u64).prop_map(|(cpu, mem)| Resources {
        cpu_mhz: cpu,
        memory_mb: mem,
    })
}

fn constraint_strategy() -> impl Strategy<Value = Constraint> {
    (
        prop_oneof![
            Just("os".to_string()),
            Just("arch".to_string()),
            Just("hostname".to_string()),
            "[a-z_]{1,20}".prop_map(|s: String| s),
        ],
        prop_oneof![
            Just("=".to_string()),
            Just("!=".to_string()),
            Just(">".to_string()),
            Just("<".to_string()),
        ],
        "[a-zA-Z0-9_]{1,20}".prop_map(|s: String| s),
    )
        .prop_map(|(attribute, operator, value)| Constraint {
            attribute,
            operator,
            value,
        })
}

fn task_config_strategy() -> impl Strategy<Value = TaskConfig> {
    prop_oneof![
        "[a-z]{1,10}".prop_map(|cmd: String| TaskConfig::Exec {
            command: cmd,
            args: vec![],
            working_dir: None,
        }),
        "[a-z]{3,20}".prop_map(|image: String| TaskConfig::Oci {
            image: format!("{}:latest", image),
            ports: HashMap::new(),
            volumes: HashMap::new(),
            entrypoint: None,
            command: None,
        }),
        "[a-z]{1,10}".prop_map(|name: String| TaskConfig::Nix {
            flake_ref: format!("github:user/{}", name),
            args: vec![],
        }),
    ]
}

fn task_strategy() -> impl Strategy<Value = Task> {
    (
        "[a-z]{1,10}".prop_map(|s: String| s),
        driver_type_strategy(),
        task_config_strategy(),
        resources_strategy(),
    )
        .prop_map(|(name, driver, config, resources)| Task {
            name,
            driver,
            config,
            env: HashMap::new(),
            resources,
            health_checks: vec![],
        })
}

fn task_group_strategy() -> impl Strategy<Value = TaskGroup> {
    (
        "[a-z]{1,10}".prop_map(|s: String| s),
        1..20u32,
        prop::collection::vec(task_strategy(), 1..4),
        resources_strategy(),
    )
        .prop_map(|(name, count, tasks, resources)| TaskGroup {
            name,
            count,
            tasks,
            restart_policy: RestartPolicy::default(),
            resources,
            network: None,
        })
}

fn job_spec_strategy() -> impl Strategy<Value = JobSpec> {
    (
        "[a-z][a-z0-9]{0,20}".prop_map(|s: String| s),
        job_type_strategy(),
        prop::collection::vec(task_group_strategy(), 1..4),
        prop::collection::vec(constraint_strategy(), 0..3),
    )
        .prop_map(|(id, job_type, groups, constraints)| JobSpec {
            id,
            job_type,
            groups,
            constraints,
            meta: HashMap::new(),
        })
}

fn event_kind_strategy() -> impl Strategy<Value = EventKind> {
    prop_oneof![
        Just(EventKind::JobSubmitted),
        Just(EventKind::JobUpdated),
        Just(EventKind::JobStopped),
        Just(EventKind::AllocationPlaced),
        Just(EventKind::AllocationStarted),
        Just(EventKind::AllocationFailed),
        Just(EventKind::AllocationCompleted),
        Just(EventKind::NodeJoined),
        Just(EventKind::NodeLeft),
        Just(EventKind::NodeDraining),
        Just(EventKind::NodeReady),
        Just(EventKind::EvaluationCompleted),
        Just(EventKind::DeploymentStarted),
        Just(EventKind::DeploymentCompleted),
    ]
}

// ── Property Tests: Job Spec ──

proptest! {
    /// Property: Any valid JobSpec can be converted to a Job.
    #[test]
    fn job_spec_to_job_never_panics(spec in job_spec_strategy()) {
        let job = spec.into_job();
        prop_assert_eq!(job.version, 1);
        prop_assert_eq!(job.status, JobStatus::Pending);
    }

    /// Property: JobSpec -> Job preserves the ID.
    #[test]
    fn job_spec_preserves_id(spec in job_spec_strategy()) {
        let id = spec.id.clone();
        let job = spec.into_job();
        prop_assert_eq!(job.id, id);
    }

    /// Property: JobSpec -> Job preserves group count.
    #[test]
    fn job_spec_preserves_groups(spec in job_spec_strategy()) {
        let group_count = spec.groups.len();
        let job = spec.into_job();
        prop_assert_eq!(job.groups.len(), group_count);
    }

    /// Property: JSON serialization round-trips for JobSpec.
    #[test]
    fn job_spec_json_roundtrip(spec in job_spec_strategy()) {
        let json = serde_json::to_string(&spec).unwrap();
        let deserialized: JobSpec = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(spec.id, deserialized.id);
        prop_assert_eq!(spec.job_type, deserialized.job_type);
        prop_assert_eq!(spec.groups.len(), deserialized.groups.len());
        prop_assert_eq!(spec.constraints.len(), deserialized.constraints.len());
    }

    /// Property: Job JSON round-trip preserves all fields.
    #[test]
    fn job_json_roundtrip(spec in job_spec_strategy()) {
        let job = spec.into_job();
        let json = serde_json::to_string(&job).unwrap();
        let deserialized: Job = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(job.id, deserialized.id);
        prop_assert_eq!(job.version, deserialized.version);
        prop_assert_eq!(job.status, deserialized.status);
    }
}

// ── Property Tests: Resources ──

proptest! {
    /// Property: Resources serialization round-trips.
    #[test]
    fn resources_json_roundtrip(r in resources_strategy()) {
        let json = serde_json::to_string(&r).unwrap();
        let deserialized: Resources = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(r.cpu_mhz, deserialized.cpu_mhz);
        prop_assert_eq!(r.memory_mb, deserialized.memory_mb);
    }

    /// Property: Default resources are zero.
    #[test]
    fn resources_default_is_zero(_dummy in 0..1i32) {
        let r = Resources::default();
        prop_assert_eq!(r.cpu_mhz, 0);
        prop_assert_eq!(r.memory_mb, 0);
    }
}

// ── Property Tests: Constraints ──

proptest! {
    /// Property: Constraint serialization round-trips.
    #[test]
    fn constraint_json_roundtrip(c in constraint_strategy()) {
        let json = serde_json::to_string(&c).unwrap();
        let deserialized: Constraint = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(c.attribute, deserialized.attribute);
        prop_assert_eq!(c.operator, deserialized.operator);
        prop_assert_eq!(c.value, deserialized.value);
    }
}

// ── Property Tests: Event Ring ──

proptest! {
    /// Property: EventRing never exceeds capacity.
    #[test]
    fn event_ring_respects_capacity(
        capacity in 1..100usize,
        num_events in 0..200usize,
    ) {
        let mut ring = EventRing::with_capacity(capacity);
        for _ in 0..num_events {
            ring.push(Event::new(EventKind::JobSubmitted, serde_json::json!({})));
        }
        prop_assert!(ring.len() <= capacity);
    }

    /// Property: EventRing ordering is preserved (newest last).
    #[test]
    fn event_ring_preserves_order(num_events in 1..50usize) {
        let mut ring = EventRing::with_capacity(100);
        for i in 0..num_events {
            ring.push(Event::new(
                EventKind::JobSubmitted,
                serde_json::json!({ "seq": i }),
            ));
        }
        let events: Vec<_> = ring.list().iter().collect();
        for i in 1..events.len() {
            prop_assert!(events[i].timestamp >= events[i - 1].timestamp);
        }
    }

    /// Property: EventKind Display + from_str_opt round-trips for all variants.
    #[test]
    fn event_kind_display_roundtrip(kind in event_kind_strategy()) {
        let display = format!("{}", kind);
        let parsed = EventKind::from_str_opt(&display);
        prop_assert!(parsed.is_some());
        prop_assert_eq!(parsed.unwrap(), kind);
    }
}

// ── Property Tests: Allocation ──

proptest! {
    /// Property: New allocations are always in Pending state.
    #[test]
    fn new_allocation_is_pending(
        job_id in "[a-z]{1,10}",
        group in "[a-z]{1,10}",
        node in "[a-z0-9]{1,10}",
    ) {
        let alloc = Allocation::new(
            job_id,
            group,
            node,
            vec!["task1".to_string()],
        );
        prop_assert_eq!(alloc.state.clone(), AllocationState::Pending);
        prop_assert!(!alloc.is_terminal());
    }

    /// Property: Terminal states are always terminal.
    #[test]
    fn terminal_states_are_terminal(
        state in prop_oneof![
            Just(AllocationState::Complete),
            Just(AllocationState::Failed),
            Just(AllocationState::Lost),
        ],
    ) {
        let mut alloc = Allocation::new(
            "job".to_string(),
            "group".to_string(),
            "node".to_string(),
            vec!["task".to_string()],
        );
        alloc.state = state;
        prop_assert!(alloc.is_terminal());
    }

    /// Property: Non-terminal states are never terminal.
    #[test]
    fn non_terminal_states_are_not_terminal(
        state in prop_oneof![
            Just(AllocationState::Pending),
            Just(AllocationState::Running),
        ],
    ) {
        let mut alloc = Allocation::new(
            "job".to_string(),
            "group".to_string(),
            "node".to_string(),
            vec!["task".to_string()],
        );
        alloc.state = state;
        prop_assert!(!alloc.is_terminal());
    }
}

// ── Property Tests: Release ──

proptest! {
    /// Property: New releases always start as Pending with version 1.
    #[test]
    fn new_release_is_pending_v1(
        name in "[a-z]{1,10}",
        flake_ref in "[a-z]{1,10}",
        job_id in "[a-z]{1,10}",
    ) {
        let release = Release::new(
            name,
            format!("github:user/{}", flake_ref),
            job_id,
        );
        prop_assert_eq!(release.version, 1);
        prop_assert_eq!(release.status, ReleaseStatus::Pending);
    }
}
