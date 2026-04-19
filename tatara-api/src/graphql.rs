use async_graphql::*;
use std::sync::Arc;
use uuid::Uuid;

use tatara_core::cluster::types::NodeMeta as DomainNodeMeta;
use tatara_core::domain::allocation::{
    Allocation as DomainAllocation, AllocationState as DomainAllocState,
    TaskRunState as DomainTaskRunState, TaskState as DomainTaskState,
};
use tatara_core::domain::job::{
    Job as DomainJob, JobSpec, JobStatus as DomainJobStatus, JobType as DomainJobType,
};
use tatara_engine::client::executor::Executor;
use tatara_engine::client::log_collector::LogCollector;
use tatara_engine::cluster::store::ClusterStore;
use tatara_engine::drivers::LogEntry as DomainLogEntry;

pub type TataraSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

pub struct QueryRoot;
pub struct MutationRoot;

// ── GraphQL Enums ──

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
enum JobType {
    Service,
    Batch,
    System,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
enum JobStatus {
    Pending,
    Running,
    Dead,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
enum AllocState {
    Pending,
    Running,
    Complete,
    Failed,
    Lost,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
enum TaskState {
    Pending,
    Running,
    Dead,
}

// ── GraphQL Object Wrappers ──

struct GqlJob(DomainJob);
struct GqlAllocation(DomainAllocation);
struct GqlNode(DomainNodeMeta);
struct GqlTaskState {
    name: String,
    state: DomainTaskState,
}
struct GqlLogEntry(DomainLogEntry);

#[Object]
impl GqlJob {
    async fn id(&self) -> &str {
        &self.0.id
    }
    async fn version(&self) -> u64 {
        self.0.version
    }
    async fn job_type(&self) -> JobType {
        match self.0.job_type {
            DomainJobType::Service => JobType::Service,
            DomainJobType::Batch => JobType::Batch,
            DomainJobType::System => JobType::System,
        }
    }
    async fn status(&self) -> JobStatus {
        match self.0.status {
            DomainJobStatus::Pending => JobStatus::Pending,
            DomainJobStatus::Running => JobStatus::Running,
            DomainJobStatus::Dead => JobStatus::Dead,
        }
    }
    async fn submitted_at(&self) -> String {
        self.0.submitted_at.to_rfc3339()
    }
    async fn group_count(&self) -> usize {
        self.0.groups.len()
    }
}

#[Object]
impl GqlAllocation {
    async fn id(&self) -> String {
        self.0.id.to_string()
    }
    async fn job_id(&self) -> &str {
        &self.0.job_id
    }
    async fn group_name(&self) -> &str {
        &self.0.group_name
    }
    async fn node_id(&self) -> &str {
        &self.0.node_id
    }
    async fn state(&self) -> AllocState {
        match self.0.state {
            DomainAllocState::Pending => AllocState::Pending,
            DomainAllocState::Running => AllocState::Running,
            DomainAllocState::Complete => AllocState::Complete,
            DomainAllocState::Failed => AllocState::Failed,
            DomainAllocState::Lost => AllocState::Lost,
        }
    }
    async fn created_at(&self) -> String {
        self.0.created_at.to_rfc3339()
    }
    async fn task_states(&self) -> Vec<GqlTaskState> {
        self.0
            .task_states
            .iter()
            .map(|(name, state)| GqlTaskState {
                name: name.clone(),
                state: state.clone(),
            })
            .collect()
    }
}

#[Object]
impl GqlTaskState {
    async fn name(&self) -> &str {
        &self.name
    }
    async fn state(&self) -> TaskState {
        match self.state.state {
            DomainTaskRunState::Pending => TaskState::Pending,
            DomainTaskRunState::Running => TaskState::Running,
            DomainTaskRunState::Dead => TaskState::Dead,
        }
    }
    async fn pid(&self) -> Option<u32> {
        self.state.pid
    }
    async fn exit_code(&self) -> Option<i32> {
        self.state.exit_code
    }
    async fn restarts(&self) -> u32 {
        self.state.restarts
    }
}

#[Object]
impl GqlNode {
    async fn node_id(&self) -> u64 {
        self.0.node_id
    }
    async fn hostname(&self) -> &str {
        &self.0.hostname
    }
    async fn http_addr(&self) -> &str {
        &self.0.http_addr
    }
    async fn os(&self) -> &str {
        &self.0.os
    }
    async fn arch(&self) -> &str {
        &self.0.arch
    }
    async fn voter(&self) -> bool {
        self.0.roles.voter
    }
    async fn worker(&self) -> bool {
        self.0.roles.worker
    }
    async fn cpu_total(&self) -> u64 {
        self.0.total_resources.cpu_mhz
    }
    async fn memory_total(&self) -> u64 {
        self.0.total_resources.memory_mb
    }
    async fn cpu_available(&self) -> u64 {
        self.0.available_resources.cpu_mhz
    }
    async fn memory_available(&self) -> u64 {
        self.0.available_resources.memory_mb
    }
    async fn allocations_running(&self) -> u32 {
        self.0.allocations_running
    }
    async fn version(&self) -> &str {
        &self.0.version
    }
    async fn joined_at(&self) -> String {
        self.0.joined_at.to_rfc3339()
    }
}

#[Object]
impl GqlLogEntry {
    async fn task_name(&self) -> &str {
        &self.0.task_name
    }
    async fn message(&self) -> &str {
        &self.0.message
    }
    async fn stream(&self) -> &str {
        &self.0.stream
    }
    async fn timestamp(&self) -> String {
        self.0.timestamp.to_rfc3339()
    }
}

// ── Queries ──

#[Object]
impl QueryRoot {
    async fn jobs(&self, ctx: &Context<'_>) -> Result<Vec<GqlJob>> {
        let store = ctx.data::<Arc<ClusterStore>>()?;
        Ok(store.list_jobs().await.into_iter().map(GqlJob).collect())
    }

    async fn job(&self, ctx: &Context<'_>, id: String) -> Result<Option<GqlJob>> {
        let store = ctx.data::<Arc<ClusterStore>>()?;
        Ok(store.get_job(&id).await.map(GqlJob))
    }

    async fn allocations(
        &self,
        ctx: &Context<'_>,
        job_id: Option<String>,
    ) -> Result<Vec<GqlAllocation>> {
        let store = ctx.data::<Arc<ClusterStore>>()?;
        let allocs = match job_id {
            Some(id) => store.list_allocations_for_job(&id).await,
            None => store.list_allocations().await,
        };
        Ok(allocs.into_iter().map(GqlAllocation).collect())
    }

    async fn allocation(&self, ctx: &Context<'_>, id: String) -> Result<Option<GqlAllocation>> {
        let store = ctx.data::<Arc<ClusterStore>>()?;
        let uuid: Uuid = id.parse()?;
        Ok(store.get_allocation(&uuid).await.map(GqlAllocation))
    }

    async fn nodes(&self, ctx: &Context<'_>) -> Result<Vec<GqlNode>> {
        let store = ctx.data::<Arc<ClusterStore>>()?;
        Ok(store.list_nodes().await.into_iter().map(GqlNode).collect())
    }

    async fn logs(
        &self,
        ctx: &Context<'_>,
        alloc_id: String,
        task_name: Option<String>,
    ) -> Result<Vec<GqlLogEntry>> {
        let store = ctx.data::<Arc<ClusterStore>>()?;
        let collector = ctx.data::<Arc<LogCollector>>()?;

        let uuid: Uuid = alloc_id.parse()?;
        let alloc = store
            .get_allocation(&uuid)
            .await
            .ok_or_else(|| Error::new("Allocation not found"))?;

        let task = task_name
            .unwrap_or_else(|| alloc.task_states.keys().next().cloned().unwrap_or_default());

        let entries = collector
            .read_logs(&alloc_id, &task)
            .await
            .map_err(|e| Error::new(e.to_string()))?;

        Ok(entries.into_iter().map(GqlLogEntry).collect())
    }
}

// ── Mutations ──

#[Object]
impl MutationRoot {
    async fn submit_job(&self, ctx: &Context<'_>, spec: String) -> Result<GqlJob> {
        let store = ctx.data::<Arc<ClusterStore>>()?;
        let job_spec: JobSpec = serde_json::from_str(&spec)
            .map_err(|e| Error::new(format!("Invalid job spec: {}", e)))?;
        let job = job_spec.into_job();
        let result = store
            .put_job(job)
            .await
            .map_err(|e| Error::new(e.to_string()))?;
        tracing::info!(
            job_id = %result.value.id,
            propagated = result.fully_propagated,
            "Job submitted via GraphQL"
        );
        Ok(GqlJob(result.value))
    }

    async fn stop_job(&self, ctx: &Context<'_>, job_id: String) -> Result<GqlJob> {
        let store = ctx.data::<Arc<ClusterStore>>()?;
        let executor = ctx.data::<Arc<Executor>>()?;

        let allocations = store.list_allocations_for_job(&job_id).await;
        for alloc in &allocations {
            if !alloc.is_terminal() {
                let _ = executor
                    .stop_allocation(&alloc.id, std::time::Duration::from_secs(10))
                    .await;
            }
        }

        let result = store
            .update_job_status(&job_id, DomainJobStatus::Dead)
            .await
            .map_err(|e| Error::new(e.to_string()))?;

        tracing::info!(job_id = %job_id, "Job stopped via GraphQL");
        Ok(GqlJob(result.value))
    }
}

pub fn build_schema(
    cluster_store: Arc<ClusterStore>,
    executor: Arc<Executor>,
    log_collector: Arc<LogCollector>,
) -> TataraSchema {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(cluster_store)
        .data(executor)
        .data(log_collector)
        .finish()
}
