/// Forge workflow integration tests for tatara.
///
/// These tests verify the full forge lifecycle:
///   forge init → validate → submit → schedule → version → rollback → release
///
/// All tests use the in-process test server (no Raft/gossip required).

use std::collections::HashMap;

use tatara::domain::allocation::{Allocation, AllocationState};
use tatara::domain::event::EventKind;
use tatara::domain::job::{Job, JobStatus};
use tatara::domain::release::{CreateReleaseRequest, Release, ReleaseStatus};
use tatara::testing::server::TestServer;
use tatara::testing::*;

// ── Forge Submission Workflow ──

#[tokio::test]
async fn test_forge_deploy_workflow() {
    let server = TestServer::new();

    // 1. Register a node (scheduler needs something to target)
    server
        .store
        .register_node(node_meta(1, "worker-1", 4000, 8192))
        .await
        .unwrap();

    // 2. Submit a forge-style job
    let spec = forge_job_spec("myapp", "github:user/myapp");
    let resp = server.post("/api/v1/jobs", &spec).await;
    resp.assert_ok();
    let job: Job = resp.json();
    assert_eq!(job.id, "myapp");
    assert_eq!(job.status, JobStatus::Pending);

    // 3. Create a release linking the forge to the job
    let release_req = CreateReleaseRequest {
        name: "myapp".to_string(),
        flake_ref: "github:user/myapp".to_string(),
        job_id: "myapp".to_string(),
        flake_rev: Some("abc123def".to_string()),
    };
    let resp = server.post("/api/v1/releases", &release_req).await;
    resp.assert_ok();
    let release: Release = resp.json();
    assert_eq!(release.status, ReleaseStatus::Active);
    assert_eq!(release.job_id, "myapp");

    // 4. Simulate scheduler placing an allocation
    let alloc = allocation("myapp", "main", "worker-1");
    let alloc_id = alloc.id;
    server.store.put_allocation(alloc).await.unwrap();

    // 5. Verify allocation exists
    let resp = server
        .get(&format!("/api/v1/allocations/{}", alloc_id))
        .await;
    resp.assert_ok();

    // 6. Verify events were generated
    let events = server
        .store
        .list_events(Some(&EventKind::JobSubmitted), None)
        .await;
    assert_eq!(events.len(), 1);

    let alloc_events = server
        .store
        .list_events(Some(&EventKind::AllocationPlaced), None)
        .await;
    assert_eq!(alloc_events.len(), 1);
}

// ── Forge Update + Rollback Workflow ──

#[tokio::test]
async fn test_forge_update_and_rollback() {
    let server = TestServer::new();

    // Deploy v1
    let spec_v1 = job_spec_with_group("myapp", "web", 2, 500, 256);
    server.post("/api/v1/jobs", &spec_v1).await;

    // Deploy v2 (update: increase count to 4)
    let spec_v2 = job_spec_with_group("myapp", "web", 4, 500, 256);
    let resp = server.post("/api/v1/jobs", &spec_v2).await;
    resp.assert_ok();
    let job_v2: Job = resp.json();
    assert_eq!(job_v2.groups[0].count, 4);

    // Check history shows 2 versions
    let resp = server.get("/api/v1/jobs/myapp/history").await;
    resp.assert_ok();
    let history: Vec<serde_json::Value> = resp.json();
    assert_eq!(history.len(), 2);

    // Rollback to v1 (count=2)
    let resp = server
        .post("/api/v1/jobs/myapp/rollback/1", &serde_json::json!({}))
        .await;
    resp.assert_ok();
    let rolled_back: Job = resp.json();
    assert_eq!(rolled_back.groups[0].count, 2);
    assert_eq!(rolled_back.status, JobStatus::Pending);
}

// ── Forge with Multiple Task Groups ──

#[tokio::test]
async fn test_forge_multi_group_deployment() {
    let server = TestServer::new();

    // Register nodes
    server
        .store
        .register_node(node_meta(1, "node-1", 4000, 8192))
        .await
        .unwrap();
    server
        .store
        .register_node(node_meta(2, "node-2", 4000, 8192))
        .await
        .unwrap();

    // Submit a multi-group job (web + worker)
    let spec = serde_json::json!({
        "id": "multi-group-app",
        "job_type": "service",
        "groups": [
            {
                "name": "web",
                "count": 2,
                "tasks": [{
                    "name": "nginx",
                    "driver": "exec",
                    "config": { "type": "exec", "command": "nginx", "args": [] },
                    "resources": { "cpu_mhz": 500, "memory_mb": 256 }
                }],
                "resources": { "cpu_mhz": 500, "memory_mb": 256 }
            },
            {
                "name": "worker",
                "count": 3,
                "tasks": [{
                    "name": "worker",
                    "driver": "exec",
                    "config": { "type": "exec", "command": "worker", "args": [] },
                    "resources": { "cpu_mhz": 1000, "memory_mb": 512 }
                }],
                "resources": { "cpu_mhz": 1000, "memory_mb": 512 }
            }
        ],
        "constraints": [],
        "meta": { "forge": "true" }
    });

    let resp = server.post("/api/v1/jobs", &spec).await;
    resp.assert_ok();

    let job: Job = resp.json();
    assert_eq!(job.groups.len(), 2);
    assert_eq!(job.groups[0].name, "web");
    assert_eq!(job.groups[0].count, 2);
    assert_eq!(job.groups[1].name, "worker");
    assert_eq!(job.groups[1].count, 3);
}

// ── Forge with Constraints ──

#[tokio::test]
async fn test_forge_with_os_constraint() {
    let server = TestServer::new();

    let spec = constrained_job_spec(
        "linux-only",
        vec![constraint("os", "=", "linux")],
    );

    let resp = server.post("/api/v1/jobs", &spec).await;
    resp.assert_ok();

    let job: Job = resp.json();
    assert_eq!(job.constraints.len(), 1);
    assert_eq!(job.constraints[0].attribute, "os");
}

// ── Release Promotion Chain ──

#[tokio::test]
async fn test_release_promotion_chain() {
    let server = TestServer::new();

    // Deploy v1
    let spec_v1 = forge_job_spec("app", "github:user/app#v1");
    server.post("/api/v1/jobs", &spec_v1).await;
    let req_v1 = CreateReleaseRequest {
        name: "app-v1".to_string(),
        flake_ref: "github:user/app#v1".to_string(),
        job_id: "app".to_string(),
        flake_rev: Some("v1hash".to_string()),
    };
    let resp = server.post("/api/v1/releases", &req_v1).await;
    let release_v1: Release = resp.json();

    // Deploy v2
    let req_v2 = CreateReleaseRequest {
        name: "app-v2".to_string(),
        flake_ref: "github:user/app#v2".to_string(),
        job_id: "app".to_string(),
        flake_rev: Some("v2hash".to_string()),
    };
    let resp = server.post("/api/v1/releases", &req_v2).await;
    let release_v2: Release = resp.json();

    // Deploy v3
    let req_v3 = CreateReleaseRequest {
        name: "app-v3".to_string(),
        flake_ref: "github:user/app#v3".to_string(),
        job_id: "app".to_string(),
        flake_rev: Some("v3hash".to_string()),
    };
    let resp = server.post("/api/v1/releases", &req_v3).await;
    let release_v3: Release = resp.json();

    // Promote v3 — should supersede v1 and v2
    server
        .post(
            &format!("/api/v1/releases/{}/promote", release_v3.id),
            &serde_json::json!({}),
        )
        .await;

    // Verify v1 and v2 are superseded
    let resp = server
        .get(&format!("/api/v1/releases/{}", release_v1.id))
        .await;
    let r1: Release = resp.json();
    assert_eq!(r1.status, ReleaseStatus::Superseded);

    let resp = server
        .get(&format!("/api/v1/releases/{}", release_v2.id))
        .await;
    let r2: Release = resp.json();
    assert_eq!(r2.status, ReleaseStatus::Superseded);

    let resp = server
        .get(&format!("/api/v1/releases/{}", release_v3.id))
        .await;
    let r3: Release = resp.json();
    assert_eq!(r3.status, ReleaseStatus::Active);
}

// ── Node Drain During Forge Deployment ──

#[tokio::test]
async fn test_node_drain_during_deployment() {
    let server = TestServer::new();

    // Register two nodes
    server
        .store
        .register_node(node_meta(1, "node-1", 4000, 8192))
        .await
        .unwrap();
    server
        .store
        .register_node(node_meta(2, "node-2", 4000, 8192))
        .await
        .unwrap();

    // Deploy a forge job
    let spec = forge_job_spec("draining-app", "github:user/app");
    server.post("/api/v1/jobs", &spec).await;

    // Place allocation on node-1
    let alloc = allocation("draining-app", "main", "1");
    server.store.put_allocation(alloc).await.unwrap();

    // Drain node-1
    let resp = server
        .post(
            "/api/v1/nodes/1/drain",
            &serde_json::json!({ "deadline_secs": 30 }),
        )
        .await;
    resp.assert_ok();

    // Verify node-1 is drained
    let nodes = server.store.list_nodes().await;
    let node1 = nodes.iter().find(|n| n.node_id == 1).unwrap();
    assert!(!node1.eligible);

    // Verify drain event was emitted
    let events = server
        .store
        .list_events(Some(&EventKind::NodeDraining), None)
        .await;
    assert_eq!(events.len(), 1);
}

// ── Allocation State Transitions ──

#[tokio::test]
async fn test_allocation_lifecycle_through_api() {
    let server = TestServer::new();

    // Submit job + create allocation
    let spec = job_spec("lifecycle-test");
    server.post("/api/v1/jobs", &spec).await;

    let alloc = allocation("lifecycle-test", "main", "node-1");
    let alloc_id = alloc.id;
    server.store.put_allocation(alloc).await.unwrap();

    // Transition: Pending → Running
    server
        .store
        .update_allocation_state(alloc_id, AllocationState::Running, HashMap::new())
        .await
        .unwrap();

    let running_alloc = server.store.get_allocation(&alloc_id).await.unwrap();
    assert_eq!(running_alloc.state, AllocationState::Running);

    // Transition: Running → Complete
    server
        .store
        .update_allocation_state(alloc_id, AllocationState::Complete, HashMap::new())
        .await
        .unwrap();

    let complete_alloc = server.store.get_allocation(&alloc_id).await.unwrap();
    assert_eq!(complete_alloc.state, AllocationState::Complete);
    assert!(complete_alloc.is_terminal());

    // Verify events were emitted
    let events = server.store.list_events(None, None).await;
    let alloc_events: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EventKind::AllocationPlaced
                    | EventKind::AllocationStarted
                    | EventKind::AllocationCompleted
            )
        })
        .collect();
    assert_eq!(alloc_events.len(), 3);
}

// ── Event Stream Verification ──

#[tokio::test]
async fn test_full_event_audit_trail() {
    let server = TestServer::new();

    // Register node
    server
        .store
        .register_node(node_meta(1, "audit-node", 4000, 8192))
        .await
        .unwrap();

    // Submit job
    let spec = job_spec("audit-job");
    server.post("/api/v1/jobs", &spec).await;

    // Create allocation
    let alloc = allocation("audit-job", "main", "1");
    server.store.put_allocation(alloc).await.unwrap();

    // Stop job
    server
        .post("/api/v1/jobs/audit-job/stop", &serde_json::json!({}))
        .await;

    // Verify complete event trail
    let all_events = server.store.list_events(None, None).await;
    assert!(all_events.len() >= 4); // NodeJoined + JobSubmitted + AllocationPlaced + JobStopped

    // Filter specific event kinds
    let node_events = server
        .store
        .list_events(Some(&EventKind::NodeJoined), None)
        .await;
    assert_eq!(node_events.len(), 1);

    let job_events = server
        .store
        .list_events(Some(&EventKind::JobSubmitted), None)
        .await;
    assert_eq!(job_events.len(), 1);
}
