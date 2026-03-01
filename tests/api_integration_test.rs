/// API integration tests for tatara.
///
/// These tests exercise the REST API through the in-process test server,
/// verifying full HTTP round-trip behavior without requiring Raft, gossip,
/// or any networking infrastructure.

use tatara::domain::job::{Job, JobSpec, JobStatus};
use tatara::domain::release::{CreateReleaseRequest, Release, ReleaseStatus};
use tatara::testing::server::TestServer;
use tatara::testing::*;

// ── Health ──

#[tokio::test]
async fn test_health_endpoint() {
    let server = TestServer::new();
    let resp = server.get("/health").await;
    resp.assert_ok();
    assert_eq!(resp.body_text(), "ok");
}

// ── Jobs ──

#[tokio::test]
async fn test_list_jobs_empty() {
    let server = TestServer::new();
    let resp = server.get("/api/v1/jobs").await;
    resp.assert_ok();
    let jobs: Vec<Job> = resp.json();
    assert!(jobs.is_empty());
}

#[tokio::test]
async fn test_submit_and_list_job() {
    let server = TestServer::new();

    let spec = job_spec("test-web");
    let resp = server.post("/api/v1/jobs", &spec).await;
    resp.assert_ok();

    let job: Job = resp.json();
    assert_eq!(job.id, "test-web");
    assert_eq!(job.version, 1);
    assert_eq!(job.status, JobStatus::Pending);

    // List should return the job
    let resp = server.get("/api/v1/jobs").await;
    resp.assert_ok();
    let jobs: Vec<Job> = resp.json();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, "test-web");
}

#[tokio::test]
async fn test_get_job_detail() {
    let server = TestServer::new();

    let spec = job_spec("detail-test");
    server.post("/api/v1/jobs", &spec).await;

    let resp = server.get("/api/v1/jobs/detail-test").await;
    resp.assert_ok();

    let detail: serde_json::Value = resp.json();
    assert_eq!(detail["job"]["id"], "detail-test");
    assert!(detail["allocations"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_get_nonexistent_job() {
    let server = TestServer::new();
    let resp = server.get("/api/v1/jobs/does-not-exist").await;
    resp.assert_status(axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_stop_job() {
    let server = TestServer::new();

    let spec = job_spec("stop-me");
    server.post("/api/v1/jobs", &spec).await;

    let resp = server
        .post("/api/v1/jobs/stop-me/stop", &serde_json::json!({}))
        .await;
    resp.assert_ok();

    let job: Job = resp.json();
    assert_eq!(job.status, JobStatus::Dead);
}

#[tokio::test]
async fn test_job_history() {
    let server = TestServer::new();

    let spec = job_spec("history-job");
    server.post("/api/v1/jobs", &spec).await;

    let resp = server.get("/api/v1/jobs/history-job/history").await;
    resp.assert_ok();

    let history: Vec<serde_json::Value> = resp.json();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["version"], 1);
}

#[tokio::test]
async fn test_job_rollback() {
    let server = TestServer::new();

    // Submit initial version
    let spec = job_spec_with_group("rollback-job", "web", 3, 500, 256);
    server.post("/api/v1/jobs", &spec).await;

    // Stop job (changes version)
    server
        .post("/api/v1/jobs/rollback-job/stop", &serde_json::json!({}))
        .await;

    // Rollback to version 1
    let resp = server
        .post(
            "/api/v1/jobs/rollback-job/rollback/1",
            &serde_json::json!({}),
        )
        .await;
    resp.assert_ok();

    let job: Job = resp.json();
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(job.groups[0].count, 3);
}

#[tokio::test]
async fn test_job_rollback_nonexistent_version() {
    let server = TestServer::new();

    let spec = job_spec("rollback-fail");
    server.post("/api/v1/jobs", &spec).await;

    let resp = server
        .post(
            "/api/v1/jobs/rollback-fail/rollback/99",
            &serde_json::json!({}),
        )
        .await;
    resp.assert_status(axum::http::StatusCode::NOT_FOUND);
}

// ── Allocations ──

#[tokio::test]
async fn test_list_allocations_empty() {
    let server = TestServer::new();
    let resp = server.get("/api/v1/allocations").await;
    resp.assert_ok();
    let allocs: Vec<serde_json::Value> = resp.json();
    assert!(allocs.is_empty());
}

#[tokio::test]
async fn test_allocation_crud_via_store() {
    let server = TestServer::new();

    // Insert an allocation directly through the store
    let alloc = allocation("job-1", "web", "node-1");
    let alloc_id = alloc.id;
    server.store.put_allocation(alloc).await.unwrap();

    // List should show it
    let resp = server.get("/api/v1/allocations").await;
    resp.assert_ok();
    let allocs: Vec<serde_json::Value> = resp.json();
    assert_eq!(allocs.len(), 1);

    // Get by ID
    let resp = server
        .get(&format!("/api/v1/allocations/{}", alloc_id))
        .await;
    resp.assert_ok();
}

#[tokio::test]
async fn test_get_allocation_invalid_id() {
    let server = TestServer::new();
    let resp = server.get("/api/v1/allocations/not-a-uuid").await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST);
}

// ── Nodes ──

#[tokio::test]
async fn test_list_nodes_empty() {
    let server = TestServer::new();
    let resp = server.get("/api/v1/nodes").await;
    resp.assert_ok();
    let nodes: Vec<serde_json::Value> = resp.json();
    assert!(nodes.is_empty());
}

#[tokio::test]
async fn test_register_and_list_nodes() {
    let server = TestServer::new();

    server
        .store
        .register_node(node_meta(1, "node-1", 4000, 8192))
        .await
        .unwrap();
    server
        .store
        .register_node(node_meta(2, "node-2", 2000, 4096))
        .await
        .unwrap();

    let resp = server.get("/api/v1/nodes").await;
    resp.assert_ok();
    let nodes: Vec<serde_json::Value> = resp.json();
    assert_eq!(nodes.len(), 2);
}

#[tokio::test]
async fn test_drain_node() {
    let server = TestServer::new();

    server
        .store
        .register_node(node_meta(1, "drain-me", 4000, 8192))
        .await
        .unwrap();

    let resp = server
        .post(
            "/api/v1/nodes/1/drain",
            &serde_json::json!({ "deadline_secs": 30 }),
        )
        .await;
    resp.assert_ok();

    let body: serde_json::Value = resp.json();
    assert_eq!(body["status"], "draining");

    // Verify node is no longer eligible
    let nodes = server.store.list_nodes().await;
    assert!(!nodes[0].eligible);
}

#[tokio::test]
async fn test_node_eligibility_toggle() {
    let server = TestServer::new();

    server
        .store
        .register_node(node_meta(1, "toggle-node", 4000, 8192))
        .await
        .unwrap();

    // Disable eligibility
    let resp = server
        .post(
            "/api/v1/nodes/1/eligibility",
            &serde_json::json!({ "eligible": false }),
        )
        .await;
    resp.assert_ok();

    let nodes = server.store.list_nodes().await;
    assert!(!nodes[0].eligible);

    // Re-enable
    let resp = server
        .post(
            "/api/v1/nodes/1/eligibility",
            &serde_json::json!({ "eligible": true }),
        )
        .await;
    resp.assert_ok();

    let nodes = server.store.list_nodes().await;
    assert!(nodes[0].eligible);
}

// ── Events ──

#[tokio::test]
async fn test_events_generated_by_operations() {
    let server = TestServer::new();

    // Submit a job (generates JobSubmitted event)
    let spec = job_spec("event-test");
    server.post("/api/v1/jobs", &spec).await;

    // Stop it (generates JobStopped event)
    server
        .post("/api/v1/jobs/event-test/stop", &serde_json::json!({}))
        .await;

    // List all events
    let resp = server.get("/api/v1/events").await;
    resp.assert_ok();
    let events: Vec<serde_json::Value> = resp.json();
    assert!(events.len() >= 2);

    // Filter by kind
    let resp = server.get("/api/v1/events?kind=job_submitted").await;
    resp.assert_ok();
    let events: Vec<serde_json::Value> = resp.json();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "job_submitted");
}

// ── Releases ──

#[tokio::test]
async fn test_create_and_list_releases() {
    let server = TestServer::new();

    let req = CreateReleaseRequest {
        name: "myapp".to_string(),
        flake_ref: "github:user/myapp".to_string(),
        job_id: "myapp-job".to_string(),
        flake_rev: Some("abc123".to_string()),
    };
    let resp = server.post("/api/v1/releases", &req).await;
    resp.assert_ok();

    let release: Release = resp.json();
    assert_eq!(release.name, "myapp");
    assert_eq!(release.status, ReleaseStatus::Active);
    assert_eq!(release.flake_rev, Some("abc123".to_string()));

    // List
    let resp = server.get("/api/v1/releases").await;
    resp.assert_ok();
    let releases: Vec<Release> = resp.json();
    assert_eq!(releases.len(), 1);
}

#[tokio::test]
async fn test_promote_release_supersedes_others() {
    let server = TestServer::new();

    // Create two releases
    let req1 = CreateReleaseRequest {
        name: "v1".to_string(),
        flake_ref: "github:user/app#v1".to_string(),
        job_id: "app-v1".to_string(),
        flake_rev: None,
    };
    let resp1 = server.post("/api/v1/releases", &req1).await;
    let release1: Release = resp1.json();

    let req2 = CreateReleaseRequest {
        name: "v2".to_string(),
        flake_ref: "github:user/app#v2".to_string(),
        job_id: "app-v2".to_string(),
        flake_rev: None,
    };
    let resp2 = server.post("/api/v1/releases", &req2).await;
    let release2: Release = resp2.json();

    // Promote v2 — should supersede v1
    let resp = server
        .post(
            &format!("/api/v1/releases/{}/promote", release2.id),
            &serde_json::json!({}),
        )
        .await;
    resp.assert_ok();

    // Check v1 is now superseded
    let resp = server
        .get(&format!("/api/v1/releases/{}", release1.id))
        .await;
    resp.assert_ok();
    let r1: Release = resp.json();
    assert_eq!(r1.status, ReleaseStatus::Superseded);
}

#[tokio::test]
async fn test_rollback_release() {
    let server = TestServer::new();

    let req = CreateReleaseRequest {
        name: "rollback-me".to_string(),
        flake_ref: "github:user/app".to_string(),
        job_id: "app-job".to_string(),
        flake_rev: None,
    };
    let resp = server.post("/api/v1/releases", &req).await;
    let release: Release = resp.json();

    let resp = server
        .post(
            &format!("/api/v1/releases/{}/rollback", release.id),
            &serde_json::json!({}),
        )
        .await;
    resp.assert_ok();
    let rolled_back: Release = resp.json();
    assert_eq!(rolled_back.status, ReleaseStatus::RolledBack);
}

// ── Multiple operations (workflow tests) ──

#[tokio::test]
async fn test_submit_multiple_jobs_and_list() {
    let server = TestServer::new();

    for i in 0..5 {
        let spec = job_spec(&format!("job-{}", i));
        server.post("/api/v1/jobs", &spec).await;
    }

    let resp = server.get("/api/v1/jobs").await;
    resp.assert_ok();
    let jobs: Vec<Job> = resp.json();
    assert_eq!(jobs.len(), 5);
}

#[tokio::test]
async fn test_batch_job_submission() {
    let server = TestServer::new();

    let spec = batch_job_spec("batch-worker", 1000, 512);
    let resp = server.post("/api/v1/jobs", &spec).await;
    resp.assert_ok();

    let job: Job = resp.json();
    assert_eq!(job.id, "batch-worker");
}

#[tokio::test]
async fn test_forge_style_job_submission() {
    let server = TestServer::new();

    let spec = forge_job_spec("myapp", "github:user/myapp");
    let resp = server.post("/api/v1/jobs", &spec).await;
    resp.assert_ok();

    let job: Job = resp.json();
    assert_eq!(job.id, "myapp");
    assert_eq!(job.meta.get("forge").unwrap(), "true");
    assert_eq!(
        job.meta.get("flake_ref").unwrap(),
        "github:user/myapp"
    );
}
