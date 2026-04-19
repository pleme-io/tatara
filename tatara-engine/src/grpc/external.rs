#[cfg(feature = "grpc")]
use std::sync::Arc;

#[cfg(feature = "grpc")]
use tonic::{Request, Response, Status};

#[cfg(feature = "grpc")]
use crate::client::executor::Executor;
#[cfg(feature = "grpc")]
use crate::client::log_collector::LogCollector;
#[cfg(feature = "grpc")]
use crate::domain::state_store::StateStore;
#[cfg(feature = "grpc")]
use tatara_core::domain::job::JobSpec;

#[cfg(feature = "grpc")]
use super::proto::tatara_api_server::TataraApi;
#[cfg(feature = "grpc")]
use super::proto::*;

#[cfg(feature = "grpc")]
pub struct TataraApiService {
    pub store: Arc<StateStore>,
    pub executor: Arc<Executor>,
    pub log_collector: Arc<LogCollector>,
}

#[cfg(feature = "grpc")]
#[tonic::async_trait]
impl TataraApi for TataraApiService {
    async fn submit_job(
        &self,
        request: Request<SubmitJobRequest>,
    ) -> Result<Response<SubmitJobResponse>, Status> {
        let req = request.into_inner();
        let proto_spec = req
            .spec
            .ok_or_else(|| Status::invalid_argument("Missing job spec"))?;

        // Convert proto spec to domain spec
        let json =
            serde_json::to_string(&proto_spec).map_err(|e| Status::internal(e.to_string()))?;
        let spec: JobSpec =
            serde_json::from_str(&json).map_err(|e| Status::invalid_argument(e.to_string()))?;

        let job = spec.into_job();
        let job_id = job.id.clone();
        let version = job.version;

        self.store
            .put_job(job)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        tracing::info!(job_id = %job_id, "Job submitted via gRPC");

        Ok(Response::new(SubmitJobResponse { job_id, version }))
    }

    async fn stop_job(
        &self,
        request: Request<StopJobRequest>,
    ) -> Result<Response<StopJobResponse>, Status> {
        let job_id = request.into_inner().job_id;

        let allocations = self.store.list_allocations_for_job(&job_id).await;
        for alloc in &allocations {
            if !alloc.is_terminal() {
                let _ = self
                    .executor
                    .stop_allocation(&alloc.id, std::time::Duration::from_secs(10))
                    .await;
            }
        }

        self.store
            .update_job(&job_id, |j| {
                j.status = tatara_core::domain::job::JobStatus::Dead;
            })
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("Job not found"))?;

        Ok(Response::new(StopJobResponse {}))
    }

    async fn get_job(
        &self,
        request: Request<GetJobRequest>,
    ) -> Result<Response<GetJobResponse>, Status> {
        let job_id = request.into_inner().job_id;

        let job = self
            .store
            .get_job(&job_id)
            .await
            .ok_or_else(|| Status::not_found("Job not found"))?;

        let json = serde_json::to_string(&job).map_err(|e| Status::internal(e.to_string()))?;
        let proto_job: JobProto =
            serde_json::from_str(&json).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(GetJobResponse {
            job: Some(proto_job),
            allocations: vec![],
        }))
    }

    async fn list_jobs(
        &self,
        _request: Request<ListJobsRequest>,
    ) -> Result<Response<ListJobsResponse>, Status> {
        let jobs = self.store.list_jobs().await;
        let proto_jobs: Vec<JobProto> = jobs
            .iter()
            .filter_map(|j| {
                serde_json::to_string(j)
                    .ok()
                    .and_then(|json| serde_json::from_str(&json).ok())
            })
            .collect();

        Ok(Response::new(ListJobsResponse { jobs: proto_jobs }))
    }

    async fn get_allocation(
        &self,
        request: Request<GetAllocationRequest>,
    ) -> Result<Response<GetAllocationResponse>, Status> {
        let alloc_id: uuid::Uuid = request
            .into_inner()
            .allocation_id
            .parse()
            .map_err(|_| Status::invalid_argument("Invalid allocation ID"))?;

        let alloc = self
            .store
            .get_allocation(&alloc_id)
            .await
            .ok_or_else(|| Status::not_found("Allocation not found"))?;

        let json = serde_json::to_string(&alloc).map_err(|e| Status::internal(e.to_string()))?;
        let proto: AllocationProto =
            serde_json::from_str(&json).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(GetAllocationResponse {
            allocation: Some(proto),
        }))
    }

    async fn list_allocations(
        &self,
        request: Request<ListAllocationsRequest>,
    ) -> Result<Response<ListAllocationsResponse>, Status> {
        let job_id = request.into_inner().job_id;
        let allocs = if job_id.is_empty() {
            self.store.list_allocations().await
        } else {
            self.store.list_allocations_for_job(&job_id).await
        };

        let proto_allocs: Vec<AllocationProto> = allocs
            .iter()
            .filter_map(|a| {
                serde_json::to_string(a)
                    .ok()
                    .and_then(|json| serde_json::from_str(&json).ok())
            })
            .collect();

        Ok(Response::new(ListAllocationsResponse {
            allocations: proto_allocs,
        }))
    }

    async fn list_nodes(
        &self,
        _request: Request<ListNodesRequest>,
    ) -> Result<Response<ListNodesResponse>, Status> {
        let nodes = self.store.list_nodes().await;
        let proto_nodes: Vec<NodeProto> = nodes
            .iter()
            .filter_map(|n| {
                serde_json::to_string(n)
                    .ok()
                    .and_then(|json| serde_json::from_str(&json).ok())
            })
            .collect();

        Ok(Response::new(ListNodesResponse { nodes: proto_nodes }))
    }

    type StreamLogsStream = tokio_stream::wrappers::ReceiverStream<Result<LogEntry, Status>>;

    async fn stream_logs(
        &self,
        request: Request<StreamLogsRequest>,
    ) -> Result<Response<Self::StreamLogsStream>, Status> {
        let req = request.into_inner();
        let (tx, rx) = tokio::sync::mpsc::channel(256);

        let alloc_id = req.allocation_id.clone();
        let task_name = req.task_name.clone();
        let collector = self.log_collector.clone();

        tokio::spawn(async move {
            match collector.read_logs(&alloc_id, &task_name).await {
                Ok(entries) => {
                    for entry in entries {
                        let proto = LogEntry {
                            task_name: entry.task_name,
                            message: entry.message,
                            stream: entry.stream,
                            timestamp: None,
                        };
                        if tx.send(Ok(proto)).await.is_err() {
                            return;
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(Status::internal(e.to_string()))).await;
                }
            }
        });

        Ok(Response::new(tokio_stream::wrappers::ReceiverStream::new(
            rx,
        )))
    }
}
