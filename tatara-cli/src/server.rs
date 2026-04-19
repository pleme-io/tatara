use anyhow::{Context, Result};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::extract::{Path, State};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

use tatara_api::graphql::{self, TataraSchema};
use tatara_api::rest;
use tatara_core::cluster::roles;
use tatara_core::cluster::types::NodeMeta;
use tatara_core::config::ServerConfig;
use tatara_engine::client::executor::Executor;
use tatara_engine::client::log_collector::LogCollector;
use tatara_engine::cluster::gossip::GossipCluster;
use tatara_engine::cluster::membership::MembershipReconciler;
use tatara_engine::cluster::raft_node::RaftCluster;
use tatara_engine::cluster::raft_sm::TypeConfig;
use tatara_engine::cluster::store::ClusterStore;
use tatara_engine::domain::reconciler::Reconciler;
use tatara_engine::domain::scheduler::Scheduler;
use tatara_engine::domain::state_store::StateStore;
use tatara_engine::drivers::DriverRegistry;
use tatara_engine::kindling_bridge::identity;
use tatara_engine::p2p::cache::DataCache;
use tatara_engine::p2p::transfer::TransferEngine;

pub async fn run(config: ServerConfig) -> Result<()> {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&config.log_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();

    info!("tatara server starting");

    // ── Kindling identity (from file) ──
    let identity_path = config
        .kindling
        .identity_path
        .as_deref()
        .map(std::path::Path::new);
    let kindling_id = identity::load_identity(identity_path)?;

    // ── Kindling daemon API client ──
    let kindling_client =
        tatara_engine::kindling_bridge::client::probe_kindling(&config.kindling.daemon_addr).await;

    // Prefer API identity over file, fall back to system hostname
    let hostname = if let Some(ref client) = kindling_client {
        match client.identity().await {
            Ok(Some(api_id)) if !api_id.hostname.is_empty() => {
                info!(hostname = %api_id.hostname, "Using hostname from kindling API");
                api_id.hostname
            }
            _ => kindling_id
                .as_ref()
                .map(|id| id.hostname.clone())
                .unwrap_or_else(|| {
                    hostname::get()
                        .map(|h| h.to_string_lossy().to_string())
                        .unwrap_or_else(|_| "unknown".to_string())
                }),
        }
    } else {
        kindling_id
            .as_ref()
            .map(|id| id.hostname.clone())
            .unwrap_or_else(|| {
                hostname::get()
                    .map(|h| h.to_string_lossy().to_string())
                    .unwrap_or_else(|_| "unknown".to_string())
            })
    };

    // ── Node roles ──
    let node_roles = roles::resolve_roles(&config.cluster.roles);

    // ── Drivers ──
    let drivers = Arc::new(DriverRegistry::new().await);
    info!(
        drivers = ?drivers.available_drivers(),
        "Available drivers"
    );

    let driver_types = drivers.available_drivers();

    // ── Node identity ──
    let node_id = match &kindling_id {
        Some(id) => identity::derive_node_id(&id.hostname),
        None => identity::derive_node_id(&hostname),
    };

    let node_meta = match &kindling_id {
        Some(id) => identity::build_node_meta(
            id,
            node_roles.clone(),
            &config.http_addr,
            &config.cluster.gossip_addr,
            &config.cluster.raft_addr,
            driver_types,
        ),
        None => {
            // Fallback: build meta from system info
            let cpu_mhz = std::thread::available_parallelism()
                .map(|p| p.get() as u64 * 1000)
                .unwrap_or(1000);
            NodeMeta {
                node_id,
                hostname: hostname.clone(),
                http_addr: config.http_addr.clone(),
                gossip_addr: config.cluster.gossip_addr.clone(),
                raft_addr: config.cluster.raft_addr.clone(),
                os: std::env::consts::OS.to_string(),
                arch: std::env::consts::ARCH.to_string(),
                roles: node_roles.clone(),
                drivers: driver_types,
                total_resources: tatara_core::domain::job::Resources {
                    cpu_mhz,
                    memory_mb: 0,
                },
                available_resources: tatara_core::domain::job::Resources {
                    cpu_mhz,
                    memory_mb: 0,
                },
                allocations_running: 0,
                joined_at: chrono::Utc::now(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                eligible: true,
                wireguard_pubkey: None,
                tunnel_address: None,
            }
        }
    };

    info!(
        node_id = node_id,
        hostname = %hostname,
        voter = node_roles.voter,
        worker = node_roles.worker,
        "Node identity resolved"
    );

    // ── State store (file-backed, for executor local operations) ──
    let local_store = Arc::new(
        StateStore::new(&config.state.dir)
            .await
            .context("Failed to initialize state store")?,
    );

    // ── P2P data cache ──
    let p2p_cache = Arc::new(
        DataCache::new(&config.p2p.cache_dir)
            .await
            .context("Failed to initialize P2P cache")?,
    );

    // ── Gossip cluster ──
    let gossip_addr: SocketAddr = config
        .cluster
        .gossip_addr
        .parse()
        .context("Invalid gossip address")?;

    // Collect seed peers from config + kindling fleet
    let mut seed_peers = config.cluster.seed_peers.clone();
    if config.cluster.kindling_fleet_seeds {
        if let Some(ref id) = kindling_id {
            let gossip_port = gossip_addr.port();
            let fleet_peers = identity::fleet_seed_peers(&id.fleet, gossip_port);
            seed_peers.extend(fleet_peers);
        }
    }

    // Discover via mDNS
    if config.cluster.mdns_discovery {
        info!("Discovering peers via mDNS...");
        match tatara_engine::cluster::discovery::discover_peers(
            &config.cluster.cluster_id,
            Duration::from_secs(3),
        )
        .await
        {
            Ok(mdns_peers) => {
                info!(count = mdns_peers.len(), "mDNS peers discovered");
                seed_peers.extend(mdns_peers);
            }
            Err(e) => {
                warn!(error = %e, "mDNS discovery failed, continuing");
            }
        }
    }

    let gossip = Arc::new(
        GossipCluster::start(
            node_id,
            gossip_addr,
            seed_peers,
            &config.cluster.cluster_id,
            &node_meta,
        )
        .await
        .context("Failed to start gossip cluster")?,
    );

    // ── Raft consensus ──
    let raft_data_dir = config.state.dir.join("raft");
    let raft = Arc::new(
        RaftCluster::start(node_id, &config.cluster.raft_addr, &raft_data_dir)
            .await
            .context("Failed to start Raft node")?,
    );

    // Bootstrap single-node if no peers and auto_bootstrap enabled
    if config.cluster.auto_bootstrap && gossip.live_nodes().len() <= 1 {
        info!("No peers found, bootstrapping single-node Raft cluster");
        if let Err(e) = raft.bootstrap_single(&config.cluster.raft_addr).await {
            // May fail if already initialized — that's OK
            info!(error = %e, "Raft bootstrap skipped (may be already initialized)");
        }
    }

    // ── Cluster store (in-memory reads, Raft writes with propagation) ──
    let cluster_store = Arc::new(ClusterStore::new(raft.clone()));

    // Register this node in the cluster via Raft
    if let Err(e) = cluster_store.register_node(node_meta.clone()).await {
        warn!(error = %e, "Failed to register node via Raft (may not be leader yet)");
    }

    // ── Transfer engine (P2P + gossip) ──
    let transfer = Arc::new(TransferEngine::new(p2p_cache.clone(), gossip.clone()));

    // ── Publish kindling report to P2P ──
    if config.kindling.publish_reports {
        let report_path = config
            .kindling
            .report_path
            .as_deref()
            .map(std::path::Path::new);
        if let Ok(Some(report)) = tatara_engine::kindling_bridge::report::load_report(report_path) {
            if let Err(e) = tatara_engine::kindling_bridge::report::publish_report(
                &transfer, &report, &hostname,
            )
            .await
            {
                warn!(error = %e, "Failed to publish kindling report to P2P");
            }
        }
    }

    // ── Membership reconciler ──
    let reconciler = Arc::new(MembershipReconciler::new(gossip.clone(), raft.clone()));
    let reconciler_handle = reconciler.clone();
    tokio::spawn(async move {
        if let Err(e) = reconciler_handle.run().await {
            tracing::error!(error = %e, "Membership reconciler failed");
        }
    });

    // ── Subsystems ──
    let port_allocator = Arc::new(tatara_engine::domain::port_allocator::PortAllocator::new(
        config.ports.range_start,
        config.ports.range_end,
    ));
    let catalog_registry = Arc::new(tatara_engine::catalog::registry::CatalogRegistry::new());
    let metrics = tatara_engine::metrics::TataraMetrics::new();
    let volume_manager = Arc::new(tatara_engine::domain::volume_manager::VolumeManager::new(
        config.volumes.dir.clone(),
    ));
    let secret_resolver = Arc::new(tatara_engine::secrets::SecretResolver::new());
    let nats_config = tatara_engine::nats::NatsConfig {
        enabled: config.nats.enabled,
        url: config.nats.url.clone(),
        ..Default::default()
    };
    let nats_bus = Arc::new(tatara_engine::nats::NatsEventBus::connect(nats_config).await);
    let probe_executor = Arc::new(tatara_engine::domain::health_probe::ProbeExecutor::new());

    info!(
        nats = config.nats.enabled,
        sui = config.sui.daemon_addr.is_some(),
        port_range = %format!("{}-{}", config.ports.range_start, config.ports.range_end),
        "subsystems initialized"
    );

    // ── Executor (local store for fast task tracking + Raft for cluster visibility) ──
    let alloc_dir = config.state.dir.join("alloc");
    let executor = Arc::new(
        Executor::new(local_store.clone(), drivers.clone(), alloc_dir.clone())
            .with_cluster_store(cluster_store.clone()),
    );

    let log_collector = Arc::new(LogCollector::new(alloc_dir));

    // ── GraphQL schema (reads from cluster in-memory, writes through Raft) ──
    let schema = graphql::build_schema(
        cluster_store.clone(),
        executor.clone(),
        log_collector.clone(),
    );

    // ── REST router (reads from cluster in-memory, writes through Raft) ──
    let rest_state = rest::AppState {
        cluster_store: cluster_store.clone(),
        executor: executor.clone(),
        log_collector: log_collector.clone(),
        catalog_registry: catalog_registry.clone(),
        metrics: metrics.clone(),
    };

    let graphql_router = Router::new()
        .route("/graphql", get(graphql_playground).post(graphql_handler))
        .with_state(schema);

    // ── Raft HTTP endpoints ──
    let raft_state = RaftEndpointState { raft: raft.clone() };
    let raft_router = Router::new()
        .route("/raft/append", post(raft_append))
        .route("/raft/snapshot", post(raft_install_snapshot))
        .route("/raft/vote", post(raft_vote))
        .with_state(raft_state);

    // ── P2P HTTP endpoints ──
    let p2p_state = P2pEndpointState {
        transfer: transfer.clone(),
    };
    let p2p_router = Router::new()
        .route("/p2p/chunks/{hash}", get(p2p_serve_chunk))
        .route("/p2p/manifests/{hash}", get(p2p_serve_manifest))
        .with_state(p2p_state);

    // ── Compose all routes ──
    let app = rest::router(rest_state)
        .merge(graphql_router)
        .merge(raft_router)
        .merge(p2p_router)
        .layer(TraceLayer::new_for_http());

    // ── Scheduler loop (reads from Raft, leader-affinity) ──
    let cluster_store_adapter = Arc::new(
        tatara_engine::domain::store_adapter::ClusterStoreAdapter::new(cluster_store.clone()),
    );
    let scheduler = Scheduler::new(
        cluster_store_adapter.clone(),
        executor.clone(),
        config.scheduler.eval_interval_secs,
    );
    tokio::spawn(async move {
        if let Err(e) = scheduler.run().await {
            tracing::error!(error = %e, "Scheduler loop failed");
        }
    });

    // ── Reconciler loop (replaces health check loop) ──
    let mut reconciler = Reconciler::new(
        local_store.clone(),
        executor.clone(),
        config.reconciler.clone(),
    );
    tokio::spawn(async move {
        if let Err(e) = reconciler.run().await {
            tracing::error!(error = %e, "Reconciler loop failed");
        }
    });

    // ── mDNS announcement ──
    let _mdns_announcer = if config.cluster.mdns_discovery {
        // Determine local IP for mDNS
        let ip = local_ip().unwrap_or_else(|| std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
        let http_port: u16 = config
            .http_addr
            .split(':')
            .last()
            .and_then(|p| p.parse().ok())
            .unwrap_or(4646);
        let raft_port: u16 = config
            .cluster
            .raft_addr
            .split(':')
            .last()
            .and_then(|p| p.parse().ok())
            .unwrap_or(4649);

        match tatara_engine::cluster::discovery::MdnsAnnouncer::new(
            &format!("tatara-{}", node_id),
            &hostname,
            ip,
            gossip_addr.port(),
            http_port,
            raft_port,
            &config.cluster.cluster_id,
        ) {
            Ok(announcer) => Some(announcer),
            Err(e) => {
                warn!(error = %e, "Failed to start mDNS announcer");
                None
            }
        }
    } else {
        None
    };

    // ── Listen ──
    let listener = tokio::net::TcpListener::bind(&config.http_addr)
        .await
        .with_context(|| format!("Failed to bind to {}", config.http_addr))?;

    info!(
        addr = %config.http_addr,
        gossip = %config.cluster.gossip_addr,
        raft = %config.cluster.raft_addr,
        cluster_id = %config.cluster.cluster_id,
        "Listening for HTTP + GraphQL + Raft + P2P"
    );

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("Server error")?;

    // Cleanup
    if let Some(announcer) = _mdns_announcer {
        let _ = announcer.shutdown();
    }

    info!("tatara server stopped");
    Ok(())
}

// ── GraphQL handlers ──

async fn graphql_playground() -> Html<String> {
    Html(async_graphql::http::playground_source(
        async_graphql::http::GraphQLPlaygroundConfig::new("/graphql"),
    ))
}

async fn graphql_handler(
    State(schema): State<TataraSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

// ── Raft HTTP handlers ──

#[derive(Clone)]
struct RaftEndpointState {
    raft: Arc<RaftCluster>,
}

async fn raft_append(
    State(state): State<RaftEndpointState>,
    Json(rpc): Json<openraft::raft::AppendEntriesRequest<TypeConfig>>,
) -> impl IntoResponse {
    let resp = state.raft.raft.append_entries(rpc).await;
    match resp {
        Ok(r) => Json(r).into_response(),
        Err(e) => {
            let body = serde_json::to_string(&e.to_string()).unwrap_or_default();
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
        }
    }
}

async fn raft_install_snapshot(
    State(state): State<RaftEndpointState>,
    Json(rpc): Json<openraft::raft::InstallSnapshotRequest<TypeConfig>>,
) -> impl IntoResponse {
    let resp = state.raft.raft.install_snapshot(rpc).await;
    match resp {
        Ok(r) => Json(r).into_response(),
        Err(e) => {
            let body = serde_json::to_string(&e.to_string()).unwrap_or_default();
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
        }
    }
}

async fn raft_vote(
    State(state): State<RaftEndpointState>,
    Json(rpc): Json<openraft::raft::VoteRequest<tatara_core::cluster::types::NodeId>>,
) -> impl IntoResponse {
    let resp = state.raft.raft.vote(rpc).await;
    match resp {
        Ok(r) => Json(r).into_response(),
        Err(e) => {
            let body = serde_json::to_string(&e.to_string()).unwrap_or_default();
            (axum::http::StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
        }
    }
}

// ── P2P HTTP handlers ──

#[derive(Clone)]
struct P2pEndpointState {
    transfer: Arc<TransferEngine>,
}

async fn p2p_serve_chunk(
    State(state): State<P2pEndpointState>,
    Path(hash): Path<String>,
) -> impl IntoResponse {
    match state.transfer.serve_chunk(&hash).await {
        Ok(Some(chunk)) => (axum::http::StatusCode::OK, chunk.data).into_response(),
        Ok(None) => axum::http::StatusCode::NOT_FOUND.into_response(),
        Err(_) => axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn p2p_serve_manifest(
    State(state): State<P2pEndpointState>,
    Path(hash): Path<String>,
) -> impl IntoResponse {
    match state.transfer.serve_manifest(&hash).await {
        Some(manifest) => Json(manifest).into_response(),
        None => axum::http::StatusCode::NOT_FOUND.into_response(),
    }
}

// ── Utilities ──

async fn shutdown_signal() {
    // Drain coordinator via tsunagu — shared SIGTERM/SIGINT handler across
    // the pleme-io daemon fleet.
    tsunagu::ShutdownController::install().token().wait().await;
    info!("drain signal received");
}

/// Best-effort detection of a non-loopback local IP address.
fn local_ip() -> Option<std::net::IpAddr> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}
