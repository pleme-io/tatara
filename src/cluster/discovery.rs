use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashSet;
use std::net::IpAddr;
use std::time::Duration;
use tracing::{debug, info};

const SERVICE_TYPE: &str = "_tatara._tcp.local.";

/// Announces this node's presence via mDNS on the local network.
pub struct MdnsAnnouncer {
    daemon: ServiceDaemon,
}

impl MdnsAnnouncer {
    pub fn new(
        instance_name: &str,
        hostname: &str,
        ip: IpAddr,
        gossip_port: u16,
        http_port: u16,
        raft_port: u16,
        cluster_id: &str,
    ) -> Result<Self> {
        let daemon = ServiceDaemon::new()
            .map_err(|e| anyhow::anyhow!("Failed to create mDNS daemon: {}", e))?;

        let properties = [
            ("cluster", cluster_id),
            ("gossip_port", &gossip_port.to_string()),
            ("http_port", &http_port.to_string()),
            ("raft_port", &raft_port.to_string()),
        ];

        let host_label = format!("{}.", hostname);
        let service = ServiceInfo::new(
            SERVICE_TYPE,
            instance_name,
            &host_label,
            ip,
            gossip_port,
            &properties[..],
        )
        .map_err(|e| anyhow::anyhow!("Failed to create mDNS service info: {}", e))?;

        daemon
            .register(service)
            .map_err(|e| anyhow::anyhow!("Failed to register mDNS service: {}", e))?;

        info!(
            instance = instance_name,
            ip = %ip,
            gossip_port = gossip_port,
            "mDNS service announced"
        );

        Ok(Self { daemon })
    }

    pub fn shutdown(self) -> Result<()> {
        self.daemon
            .shutdown()
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("mDNS shutdown error: {}", e))
    }
}

/// Discovers tatara peers on the local network via mDNS.
/// Returns a list of gossip addresses (ip:port).
pub async fn discover_peers(
    cluster_id: &str,
    timeout: Duration,
) -> Result<Vec<String>> {
    let daemon = ServiceDaemon::new()
        .map_err(|e| anyhow::anyhow!("Failed to create mDNS browser: {}", e))?;

    let receiver = daemon
        .browse(SERVICE_TYPE)
        .map_err(|e| anyhow::anyhow!("Failed to browse mDNS: {}", e))?;

    let mut peers = HashSet::new();
    let deadline = tokio::time::Instant::now() + timeout;

    info!(timeout = ?timeout, "Discovering peers via mDNS...");

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }

        match tokio::time::timeout(remaining, tokio::task::spawn_blocking({
            let receiver = receiver.clone();
            move || receiver.recv_timeout(Duration::from_millis(500))
        }))
        .await
        {
            Ok(Ok(Ok(ServiceEvent::ServiceResolved(info)))) => {
                // Check cluster ID matches
                let props = info.get_properties();
                let svc_cluster = props
                    .get_property_val_str("cluster")
                    .unwrap_or("");

                if svc_cluster != cluster_id {
                    debug!(
                        found_cluster = svc_cluster,
                        our_cluster = cluster_id,
                        "Ignoring peer from different cluster"
                    );
                    continue;
                }

                for addr in info.get_addresses() {
                    let gossip_port = info.get_port();
                    let peer = format!("{}:{}", addr, gossip_port);
                    if peers.insert(peer.clone()) {
                        info!(peer = %peer, "Discovered peer via mDNS");
                    }
                }
            }
            Ok(Ok(Ok(_))) => {} // Other events
            Ok(Ok(Err(_))) => {} // Timeout on recv
            Ok(Err(_)) | Err(_) => break,
        }
    }

    let _ = daemon.shutdown();
    Ok(peers.into_iter().collect())
}
