//! In-process HTTP test server for tatara API testing.
//!
//! Provides a fully functional tatara REST API backed by an InMemoryStore,
//! without requiring Raft consensus, gossip, or any networking infrastructure.
//!
//! The test server uses Tower's `oneshot` pattern for zero-overhead HTTP testing
//! (no TCP sockets, no port allocation).

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{Request, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

use super::store::InMemoryStore;
use tatara_core::cluster::types::{JobVersionEntry, NodeMeta};
use tatara_core::domain::allocation::Allocation;
use tatara_core::domain::event::EventKind;
use tatara_core::domain::job::{Job, JobSpec, JobStatus};
use tatara_core::domain::release::{CreateReleaseRequest, Release, ReleaseStatus};

/// In-process test server for tatara API integration testing.
///
/// Uses Tower's `oneshot` pattern — no TCP sockets, no port allocation.
/// Each request is processed directly through the axum Router.
pub struct TestServer {
    pub store: Arc<InMemoryStore>,
    router: Router,
}

#[derive(Clone)]
struct TestState {
    store: Arc<InMemoryStore>,
}

impl TestServer {
    /// Create a new test server with an empty store.
    pub fn new() -> Self {
        let store = Arc::new(InMemoryStore::new());
        let state = TestState {
            store: store.clone(),
        };

        let router = Router::new()
            .route("/health", get(health))
            // Jobs
            .route("/api/v1/jobs", get(list_jobs).post(submit_job))
            .route("/api/v1/jobs/{job_id}", get(get_job))
            .route("/api/v1/jobs/{job_id}/stop", post(stop_job))
            .route("/api/v1/jobs/{job_id}/history", get(get_job_history))
            .route(
                "/api/v1/jobs/{job_id}/rollback/{version}",
                post(rollback_job),
            )
            // Allocations
            .route("/api/v1/allocations", get(list_allocations))
            .route("/api/v1/allocations/{alloc_id}", get(get_allocation))
            // Nodes
            .route("/api/v1/nodes", get(list_nodes))
            .route("/api/v1/nodes/{node_id}/drain", post(drain_node))
            .route(
                "/api/v1/nodes/{node_id}/eligibility",
                post(set_node_eligibility),
            )
            // Events
            .route("/api/v1/events", get(list_events))
            // Releases
            .route("/api/v1/releases", get(list_releases).post(create_release))
            .route("/api/v1/releases/{release_id}", get(get_release))
            .route(
                "/api/v1/releases/{release_id}/promote",
                post(promote_release),
            )
            .route(
                "/api/v1/releases/{release_id}/rollback",
                post(rollback_release),
            )
            .with_state(state);

        Self { store, router }
    }

    /// Create a test server with a pre-populated store.
    pub fn with_store(store: Arc<InMemoryStore>) -> Self {
        let state = TestState {
            store: store.clone(),
        };

        // Build same router
        let router = Router::new()
            .route("/health", get(health))
            .route("/api/v1/jobs", get(list_jobs).post(submit_job))
            .route("/api/v1/jobs/{job_id}", get(get_job))
            .route("/api/v1/jobs/{job_id}/stop", post(stop_job))
            .route("/api/v1/jobs/{job_id}/history", get(get_job_history))
            .route(
                "/api/v1/jobs/{job_id}/rollback/{version}",
                post(rollback_job),
            )
            .route("/api/v1/allocations", get(list_allocations))
            .route("/api/v1/allocations/{alloc_id}", get(get_allocation))
            .route("/api/v1/nodes", get(list_nodes))
            .route("/api/v1/nodes/{node_id}/drain", post(drain_node))
            .route(
                "/api/v1/nodes/{node_id}/eligibility",
                post(set_node_eligibility),
            )
            .route("/api/v1/events", get(list_events))
            .route("/api/v1/releases", get(list_releases).post(create_release))
            .route("/api/v1/releases/{release_id}", get(get_release))
            .route(
                "/api/v1/releases/{release_id}/promote",
                post(promote_release),
            )
            .route(
                "/api/v1/releases/{release_id}/rollback",
                post(rollback_release),
            )
            .with_state(state);

        Self { store, router }
    }

    /// Send a GET request to the test server.
    pub async fn get(&self, uri: &str) -> TestResponse {
        let request = Request::builder()
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .unwrap();

        TestResponse::from_response(response).await
    }

    /// Send a POST request with a JSON body.
    pub async fn post<T: serde::Serialize>(&self, uri: &str, body: &T) -> TestResponse {
        let body_bytes = serde_json::to_vec(body).unwrap();

        let request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body_bytes))
            .unwrap();

        let response = self
            .router
            .clone()
            .oneshot(request)
            .await
            .unwrap();

        TestResponse::from_response(response).await
    }
}

impl Default for TestServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Response from the test server with convenient assertion methods.
pub struct TestResponse {
    pub status: StatusCode,
    pub body: Vec<u8>,
}

impl TestResponse {
    async fn from_response(response: axum::http::Response<Body>) -> Self {
        let status = response.status();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec();
        Self { status, body }
    }

    /// Assert the response status is 200 OK.
    pub fn assert_ok(&self) {
        assert_eq!(
            self.status,
            StatusCode::OK,
            "Expected 200 OK, got {}. Body: {}",
            self.status,
            self.body_text()
        );
    }

    /// Assert the response status matches.
    pub fn assert_status(&self, expected: StatusCode) {
        assert_eq!(
            self.status, expected,
            "Expected {}, got {}. Body: {}",
            expected,
            self.status,
            self.body_text()
        );
    }

    /// Deserialize the response body as JSON.
    pub fn json<T: serde::de::DeserializeOwned>(&self) -> T {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| {
            panic!(
                "Failed to deserialize response body as {}: {}. Body: {}",
                std::any::type_name::<T>(),
                e,
                self.body_text()
            )
        })
    }

    /// Get the response body as a string.
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
}

// ── Handlers (mirror production API but use InMemoryStore) ──

async fn health() -> &'static str {
    "ok"
}

async fn submit_job(
    State(state): State<TestState>,
    Json(spec): Json<JobSpec>,
) -> Result<Json<Job>, (StatusCode, String)> {
    let job = spec.into_job();
    let job = state
        .store
        .put_job(job)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(job))
}

async fn list_jobs(State(state): State<TestState>) -> Json<Vec<Job>> {
    Json(state.store.list_jobs().await)
}

#[derive(serde::Serialize)]
struct JobDetail {
    job: Job,
    allocations: Vec<Allocation>,
}

async fn get_job(
    State(state): State<TestState>,
    Path(job_id): Path<String>,
) -> Result<Json<JobDetail>, (StatusCode, String)> {
    let job = state
        .store
        .get_job(&job_id)
        .await
        .ok_or((StatusCode::NOT_FOUND, "Job not found".to_string()))?;

    let allocations = state.store.list_allocations_for_job(&job_id).await;
    Ok(Json(JobDetail { job, allocations }))
}

async fn stop_job(
    State(state): State<TestState>,
    Path(job_id): Path<String>,
) -> Result<Json<Job>, (StatusCode, String)> {
    let job = state
        .store
        .update_job_status(&job_id, JobStatus::Dead)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(job))
}

async fn get_job_history(
    State(state): State<TestState>,
    Path(job_id): Path<String>,
) -> Result<Json<Vec<JobVersionEntry>>, (StatusCode, String)> {
    let history = state.store.get_job_history(&job_id).await;
    if history.is_empty() {
        if state.store.get_job(&job_id).await.is_none() {
            return Err((StatusCode::NOT_FOUND, "Job not found".to_string()));
        }
    }
    Ok(Json(history))
}

async fn rollback_job(
    State(state): State<TestState>,
    Path((job_id, version)): Path<(String, u64)>,
) -> Result<Json<Job>, (StatusCode, String)> {
    let job = state
        .store
        .rollback_job(&job_id, version)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                (StatusCode::NOT_FOUND, msg)
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, msg)
            }
        })?;
    Ok(Json(job))
}

async fn list_allocations(State(state): State<TestState>) -> Json<Vec<Allocation>> {
    Json(state.store.list_allocations().await)
}

async fn get_allocation(
    State(state): State<TestState>,
    Path(alloc_id): Path<String>,
) -> Result<Json<Allocation>, (StatusCode, String)> {
    let id: Uuid = alloc_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid allocation ID".to_string()))?;

    state
        .store
        .get_allocation(&id)
        .await
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "Allocation not found".to_string()))
}

async fn list_nodes(State(state): State<TestState>) -> Json<Vec<NodeMeta>> {
    Json(state.store.list_nodes().await)
}

#[derive(Deserialize)]
struct DrainRequest {
    #[serde(default)]
    deadline_secs: Option<u64>,
}

async fn drain_node(
    State(state): State<TestState>,
    Path(node_id): Path<String>,
    Json(body): Json<DrainRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id: u64 = node_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid node ID".to_string()))?;

    state
        .store
        .drain_node(id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "node_id": id,
        "status": "draining",
        "deadline_secs": body.deadline_secs,
    })))
}

#[derive(Deserialize)]
struct EligibilityRequest {
    eligible: bool,
}

async fn set_node_eligibility(
    State(state): State<TestState>,
    Path(node_id): Path<String>,
    Json(body): Json<EligibilityRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id: u64 = node_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid node ID".to_string()))?;

    state
        .store
        .set_node_eligibility(id, body.eligible)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "node_id": id,
        "eligible": body.eligible,
    })))
}

#[derive(Deserialize)]
struct EventQuery {
    kind: Option<String>,
    since: Option<String>,
}

async fn list_events(
    State(state): State<TestState>,
    params: Query<EventQuery>,
) -> Json<Vec<tatara_core::domain::event::Event>> {
    let kind = params
        .kind
        .as_deref()
        .and_then(EventKind::from_str_opt);

    let since = params
        .since
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    Json(state.store.list_events(kind.as_ref(), since).await)
}

async fn list_releases(State(state): State<TestState>) -> Json<Vec<Release>> {
    Json(state.store.list_releases().await)
}

async fn create_release(
    State(state): State<TestState>,
    Json(req): Json<CreateReleaseRequest>,
) -> Result<Json<Release>, (StatusCode, String)> {
    let mut release = Release::new(req.name, req.flake_ref, req.job_id);
    release.flake_rev = req.flake_rev;
    release.status = ReleaseStatus::Active;

    let release = state
        .store
        .put_release(release)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(release))
}

async fn get_release(
    State(state): State<TestState>,
    Path(release_id): Path<String>,
) -> Result<Json<Release>, (StatusCode, String)> {
    let id: Uuid = release_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid release ID".to_string()))?;

    state
        .store
        .get_release(&id)
        .await
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "Release not found".to_string()))
}

async fn promote_release(
    State(state): State<TestState>,
    Path(release_id): Path<String>,
) -> Result<Json<Release>, (StatusCode, String)> {
    let id: Uuid = release_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid release ID".to_string()))?;

    // Supersede other active releases
    let releases = state.store.list_releases().await;
    for rel in &releases {
        if rel.id != id && rel.status == ReleaseStatus::Active {
            let _ = state
                .store
                .update_release_status(rel.id, ReleaseStatus::Superseded)
                .await;
        }
    }

    let release = state
        .store
        .update_release_status(id, ReleaseStatus::Active)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(release))
}

async fn rollback_release(
    State(state): State<TestState>,
    Path(release_id): Path<String>,
) -> Result<Json<Release>, (StatusCode, String)> {
    let id: Uuid = release_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid release ID".to_string()))?;

    let release = state
        .store
        .update_release_status(id, ReleaseStatus::RolledBack)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(release))
}
