use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::StreamExt;
use uuid::Uuid;

use tatara_core::catalog::{ServiceEntry, ServiceQuery};
use tatara_core::cluster::types::NodeMeta;
use tatara_core::domain::allocation::Allocation;
use tatara_core::domain::event::EventKind;
use tatara_core::domain::job::{Job, JobSpec, JobStatus};
use tatara_core::domain::release::{CreateReleaseRequest, Release, ReleaseStatus};
use tatara_core::domain::source::{CreateSourceRequest, Source, SourceStatus};
use tatara_engine::catalog::registry::CatalogRegistry;
use tatara_engine::client::executor::Executor;
use tatara_engine::client::log_collector::LogCollector;
use tatara_engine::cluster::store::ClusterStore;
use tatara_engine::drivers::LogEntry;
use tatara_engine::metrics::TataraMetrics;

#[derive(Clone)]
pub struct AppState {
    pub cluster_store: Arc<ClusterStore>,
    pub executor: Arc<Executor>,
    pub log_collector: Arc<LogCollector>,
    pub catalog_registry: Arc<CatalogRegistry>,
    pub metrics: Arc<TataraMetrics>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
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
        .route(
            "/api/v1/allocations/{alloc_id}/logs",
            get(get_allocation_logs),
        )
        // Nodes
        .route("/api/v1/nodes", get(list_nodes))
        .route("/api/v1/nodes/{node_id}/drain", post(drain_node))
        .route(
            "/api/v1/nodes/{node_id}/eligibility",
            post(set_node_eligibility),
        )
        // Events
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/events/stream", get(stream_events))
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
        // Sources
        .route("/api/v1/sources", get(list_sources).post(create_source))
        .route(
            "/api/v1/sources/{source_id}",
            get(get_source).delete(delete_source),
        )
        .route("/api/v1/sources/{source_id}/sync", post(sync_source))
        .route("/api/v1/sources/{source_id}/suspend", post(suspend_source))
        .route("/api/v1/sources/{source_id}/resume", post(resume_source))
        // Catalog (consul-compatible subset)
        .route("/v1/catalog/services", get(catalog_list_services))
        .route("/v1/catalog/service/{name}", get(catalog_get_service))
        .route("/v1/health/service/{name}", get(catalog_health_service))
        // Convergence
        .route("/api/v1/convergence/graph", get(convergence_graph))
        .route("/api/v1/convergence/distance", get(convergence_distance))
        .route("/api/v1/convergence/rate", get(convergence_rate))
        .route("/api/v1/convergence/plan", get(convergence_plan))
        .route(
            "/api/v1/convergence/attestation/{point_id}",
            get(convergence_attestation),
        )
        .route(
            "/api/v1/convergence/compliance/{point_id}",
            get(convergence_compliance),
        )
        .route("/api/v1/convergence/emissions", get(convergence_emissions))
        .route(
            "/api/v1/convergence/substrates",
            get(convergence_substrates),
        )
        // Metrics
        .route("/metrics", get(prometheus_metrics))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

// ── Jobs ──

async fn submit_job(
    State(state): State<AppState>,
    Json(spec): Json<JobSpec>,
) -> Result<Json<Job>, (StatusCode, String)> {
    let job = spec.into_job();
    let result = state
        .cluster_store
        .put_job(job)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(
        job_id = %result.value.id,
        propagated = result.fully_propagated,
        "Job submitted via REST"
    );
    Ok(Json(result.value))
}

async fn list_jobs(State(state): State<AppState>) -> Json<Vec<Job>> {
    Json(state.cluster_store.list_jobs().await)
}

async fn get_job(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<JobDetail>, (StatusCode, String)> {
    let job = state
        .cluster_store
        .get_job(&job_id)
        .await
        .ok_or((StatusCode::NOT_FOUND, "Job not found".to_string()))?;

    let allocations = state.cluster_store.list_allocations_for_job(&job_id).await;

    Ok(Json(JobDetail { job, allocations }))
}

async fn stop_job(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<Job>, (StatusCode, String)> {
    let allocations = state.cluster_store.list_allocations_for_job(&job_id).await;

    for alloc in &allocations {
        if !alloc.is_terminal() {
            if let Err(e) = state
                .executor
                .stop_allocation(&alloc.id, Duration::from_secs(10))
                .await
            {
                tracing::warn!(
                    alloc_id = %alloc.id,
                    error = %e,
                    "Failed to stop allocation"
                );
            }
        }
    }

    let result = state
        .cluster_store
        .update_job_status(&job_id, JobStatus::Dead)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(job_id = %job_id, "Job stopped via REST");
    Ok(Json(result.value))
}

async fn get_job_history(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<Vec<tatara_core::cluster::types::JobVersionEntry>>, (StatusCode, String)> {
    let history = state.cluster_store.get_job_history(&job_id).await;
    if history.is_empty() {
        // Check if job exists at all
        if state.cluster_store.get_job(&job_id).await.is_none() {
            return Err((StatusCode::NOT_FOUND, "Job not found".to_string()));
        }
    }
    Ok(Json(history))
}

async fn rollback_job(
    State(state): State<AppState>,
    Path((job_id, version)): Path<(String, u64)>,
) -> Result<Json<Job>, (StatusCode, String)> {
    let result = state
        .cluster_store
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

    tracing::info!(
        job_id = %job_id,
        version = version,
        "Job rolled back via REST"
    );
    Ok(Json(result.value))
}

// ── Allocations ──

async fn list_allocations(State(state): State<AppState>) -> Json<Vec<Allocation>> {
    Json(state.cluster_store.list_allocations().await)
}

async fn get_allocation(
    State(state): State<AppState>,
    Path(alloc_id): Path<String>,
) -> Result<Json<Allocation>, (StatusCode, String)> {
    let id: Uuid = alloc_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid allocation ID".to_string()))?;

    state
        .cluster_store
        .get_allocation(&id)
        .await
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "Allocation not found".to_string()))
}

async fn get_allocation_logs(
    State(state): State<AppState>,
    Path(alloc_id): Path<String>,
    params: Query<LogQuery>,
) -> Result<Json<Vec<LogEntry>>, (StatusCode, String)> {
    let id: Uuid = alloc_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid allocation ID".to_string()))?;

    let alloc = state
        .cluster_store
        .get_allocation(&id)
        .await
        .ok_or((StatusCode::NOT_FOUND, "Allocation not found".to_string()))?;

    let task_name = params
        .task
        .clone()
        .unwrap_or_else(|| alloc.task_states.keys().next().cloned().unwrap_or_default());

    state
        .log_collector
        .read_logs(&alloc_id, &task_name)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

// ── Nodes ──

async fn list_nodes(State(state): State<AppState>) -> Json<Vec<NodeMeta>> {
    Json(state.cluster_store.list_nodes().await)
}

async fn drain_node(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
    Json(body): Json<DrainRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id: u64 = node_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid node ID".to_string()))?;

    state
        .cluster_store
        .drain_node(id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(node_id = id, "Node drain initiated via REST");

    Ok(Json(serde_json::json!({
        "node_id": id,
        "status": "draining",
        "deadline_secs": body.deadline_secs,
    })))
}

async fn set_node_eligibility(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
    Json(body): Json<EligibilityRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id: u64 = node_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid node ID".to_string()))?;

    state
        .cluster_store
        .set_node_eligibility(id, body.eligible)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({
        "node_id": id,
        "eligible": body.eligible,
    })))
}

// ── Events ──

async fn list_events(
    State(state): State<AppState>,
    params: Query<EventQuery>,
) -> Json<Vec<tatara_core::domain::event::Event>> {
    let kind = params.kind.as_deref().and_then(EventKind::from_str_opt);

    let since = params
        .since
        .as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    Json(state.cluster_store.list_events(kind.as_ref(), since).await)
}

async fn stream_events(
    State(state): State<AppState>,
    params: Query<EventStreamQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<SseEvent, Infallible>>> {
    let kind_filter = params.kind.as_deref().and_then(EventKind::from_str_opt);

    let store = state.cluster_store.clone();

    let stream = async_stream::stream! {
        let mut last_count = 0usize;
        loop {
            let events = store.list_events(kind_filter.as_ref(), None).await;

            // Only send new events since last poll
            if events.len() > last_count {
                for event in &events[last_count..] {
                    let data = serde_json::to_string(event).unwrap_or_default();
                    yield Ok(SseEvent::default().data(data));
                }
                last_count = events.len();
            }

            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ── Releases ──

async fn list_releases(State(state): State<AppState>) -> Json<Vec<Release>> {
    Json(state.cluster_store.list_releases().await)
}

async fn create_release(
    State(state): State<AppState>,
    Json(req): Json<CreateReleaseRequest>,
) -> Result<Json<Release>, (StatusCode, String)> {
    let mut release = Release::new(req.name, req.flake_ref, req.job_id);
    release.flake_rev = req.flake_rev;
    release.status = ReleaseStatus::Active;

    let result = state
        .cluster_store
        .put_release(release)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(result.value))
}

async fn get_release(
    State(state): State<AppState>,
    Path(release_id): Path<String>,
) -> Result<Json<Release>, (StatusCode, String)> {
    let id: Uuid = release_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid release ID".to_string()))?;

    state
        .cluster_store
        .get_release(&id)
        .await
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "Release not found".to_string()))
}

async fn promote_release(
    State(state): State<AppState>,
    Path(release_id): Path<String>,
) -> Result<Json<Release>, (StatusCode, String)> {
    let id: Uuid = release_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid release ID".to_string()))?;

    // Supersede all other active releases
    let releases = state.cluster_store.list_releases().await;
    for rel in &releases {
        if rel.id != id && rel.status == ReleaseStatus::Active {
            let _ = state
                .cluster_store
                .update_release_status(rel.id, ReleaseStatus::Superseded)
                .await;
        }
    }

    let result = state
        .cluster_store
        .update_release_status(id, ReleaseStatus::Active)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(result.value))
}

async fn rollback_release(
    State(state): State<AppState>,
    Path(release_id): Path<String>,
) -> Result<Json<Release>, (StatusCode, String)> {
    let id: Uuid = release_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid release ID".to_string()))?;

    let result = state
        .cluster_store
        .update_release_status(id, ReleaseStatus::RolledBack)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(result.value))
}

// ── Sources ──

async fn list_sources(State(state): State<AppState>) -> Json<Vec<Source>> {
    Json(state.cluster_store.list_sources().await)
}

async fn create_source(
    State(state): State<AppState>,
    Json(req): Json<CreateSourceRequest>,
) -> Result<Json<Source>, (StatusCode, String)> {
    let source = Source::new(req.name, req.kind, req.flake_ref);

    let result = state
        .cluster_store
        .put_source(source)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(
        source_id = %result.value.id,
        name = %result.value.name,
        "Source created via REST"
    );
    Ok(Json(result.value))
}

async fn get_source(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Result<Json<Source>, (StatusCode, String)> {
    let id: Uuid = source_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid source ID".to_string()))?;

    state
        .cluster_store
        .get_source(&id)
        .await
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, "Source not found".to_string()))
}

async fn delete_source(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let id: Uuid = source_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid source ID".to_string()))?;

    // Get source to find managed jobs
    let source = state
        .cluster_store
        .get_source(&id)
        .await
        .ok_or((StatusCode::NOT_FOUND, "Source not found".to_string()))?;

    // Stop all managed jobs
    for job_name in source.managed_jobs.keys() {
        let _ = state
            .cluster_store
            .update_job_status(job_name, JobStatus::Dead)
            .await;
    }

    state
        .cluster_store
        .delete_source(id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(source_id = %source_id, "Source deleted via REST");
    Ok(Json(serde_json::json!({ "deleted": source_id })))
}

async fn sync_source(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Result<Json<Source>, (StatusCode, String)> {
    let id: Uuid = source_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid source ID".to_string()))?;

    // Force re-evaluation by clearing last_rev
    let result = state
        .cluster_store
        .update_source(id, SourceStatus::Pending, None, None, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(source_id = %source_id, "Source sync triggered via REST");
    Ok(Json(result.value))
}

async fn suspend_source(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Result<Json<Source>, (StatusCode, String)> {
    let id: Uuid = source_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid source ID".to_string()))?;

    let result = state
        .cluster_store
        .update_source(id, SourceStatus::Suspended, None, None, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(source_id = %source_id, "Source suspended via REST");
    Ok(Json(result.value))
}

async fn resume_source(
    State(state): State<AppState>,
    Path(source_id): Path<String>,
) -> Result<Json<Source>, (StatusCode, String)> {
    let id: Uuid = source_id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid source ID".to_string()))?;

    let result = state
        .cluster_store
        .update_source(id, SourceStatus::Pending, None, None, None)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::info!(source_id = %source_id, "Source resumed via REST");
    Ok(Json(result.value))
}

// ── Types ──

#[derive(serde::Serialize)]
struct JobDetail {
    job: Job,
    allocations: Vec<Allocation>,
}

#[derive(Deserialize)]
struct LogQuery {
    task: Option<String>,
}

#[derive(Deserialize)]
struct DrainRequest {
    #[serde(default)]
    deadline_secs: Option<u64>,
}

#[derive(Deserialize)]
struct EligibilityRequest {
    eligible: bool,
}

#[derive(Deserialize)]
struct EventQuery {
    kind: Option<String>,
    since: Option<String>,
}

#[derive(Deserialize)]
struct EventStreamQuery {
    kind: Option<String>,
}

// ── Catalog ──

async fn catalog_list_services(State(state): State<AppState>) -> Json<Vec<String>> {
    Json(state.catalog_registry.list_services().await)
}

async fn catalog_get_service(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Json<Vec<ServiceEntry>> {
    Json(state.catalog_registry.get_service(&name).await)
}

#[derive(Deserialize)]
struct HealthQuery {
    #[serde(default)]
    passing: Option<bool>,
}

async fn catalog_health_service(
    State(state): State<AppState>,
    Path(name): Path<String>,
    params: Query<HealthQuery>,
) -> Json<Vec<ServiceEntry>> {
    let query = ServiceQuery {
        service: name,
        healthy_only: params.passing.unwrap_or(false),
        ..Default::default()
    };
    Json(state.catalog_registry.query(&query).await)
}

// ── Convergence ──

async fn convergence_graph(State(_state): State<AppState>) -> Json<serde_json::Value> {
    // Returns the current convergence graph across all substrates.
    // Full implementation reads from the convergence engine's SubstrateManager.
    Json(serde_json::json!({
        "points": {},
        "edges": [],
        "substrates": [],
        "status": "no active convergence graph"
    }))
}

async fn convergence_distance(State(_state): State<AppState>) -> Json<serde_json::Value> {
    // Returns per-substrate convergence distance vector.
    Json(serde_json::json!({
        "distances": {},
        "overall": 0.0,
        "is_converged": true,
        "substrate_count": 0
    }))
}

async fn convergence_rate(State(_state): State<AppState>) -> Json<serde_json::Value> {
    // Returns convergence rate per point.
    Json(serde_json::json!({
        "rates": {},
        "overall_rate": 0.0,
        "oscillating_count": 0
    }))
}

async fn convergence_plan(State(_state): State<AppState>) -> Json<serde_json::Value> {
    // Returns the current convergence plan.
    Json(serde_json::json!({
        "execution_order": [],
        "critical_path": [],
        "cache_hits": 0,
        "compliance_bindings": 0
    }))
}

async fn convergence_attestation(
    State(_state): State<AppState>,
    Path(point_id): Path<String>,
) -> Json<serde_json::Value> {
    // Returns attestation for a specific convergence point.
    Json(serde_json::json!({
        "point_id": point_id,
        "attestation": null,
        "generation": 0
    }))
}

async fn convergence_compliance(
    State(_state): State<AppState>,
    Path(point_id): Path<String>,
) -> Json<serde_json::Value> {
    // Returns compliance status for a specific convergence point.
    Json(serde_json::json!({
        "point_id": point_id,
        "bindings": [],
        "all_satisfied": true
    }))
}

async fn convergence_emissions(State(_state): State<AppState>) -> Json<serde_json::Value> {
    // Returns emission schemas and recent instantiations.
    Json(serde_json::json!({
        "schemas": [],
        "recent_instantiations": []
    }))
}

async fn convergence_substrates(State(_state): State<AppState>) -> Json<serde_json::Value> {
    // Returns per-substrate DAG status.
    Json(serde_json::json!({
        "substrates": []
    }))
}

// ── Metrics ──

async fn prometheus_metrics(
    State(state): State<AppState>,
) -> ([(axum::http::header::HeaderName, &'static str); 1], String) {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        state.metrics.render_prometheus(),
    )
}
