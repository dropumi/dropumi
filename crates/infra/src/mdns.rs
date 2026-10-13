// SPDX-License-Identifier: MPL-2.0
//! mDNS / DNS-SD discovery via `mdns-sd` (cargo feature `mdns`).
//!
//! Service `_dropumi._tcp.local.`, a random instance and host name
//! `dropumi-<8 hex>` per advertisement, TXT hints `v`, `n`, `fp`. IPv4 only;
//! loopback interfaces are used only when explicitly allowed (tests).
//! Everything learned here is a hint: identity is verified by the handshake.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use dropumi_application::error::DiscoveryError;
use dropumi_application::ports::{AdHandle, Advertisement, Discovery};
use dropumi_domain::discovery::DiscoveredPeer;
use dropumi_protocol::random;
use mdns_sd::{IfKind, ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::time::Instant;

use crate::txt;

/// DNS-SD service type.
pub const SERVICE_TYPE: &str = "_dropumi._tcp.local.";

/// mDNS discovery and advertisement.
pub struct MdnsDiscovery {
    daemon: OnceLock<Result<ServiceDaemon, String>>,
    allow_loopback: bool,
}

impl std::fmt::Debug for MdnsDiscovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MdnsDiscovery")
            .field("started", &self.daemon.get().is_some())
            .field("allow_loopback", &self.allow_loopback)
            .finish()
    }
}

impl MdnsDiscovery {
    /// A lazily started mDNS daemon.
    #[must_use]
    pub fn new(allow_loopback: bool) -> Self {
        Self {
            daemon: OnceLock::new(),
            allow_loopback,
        }
    }

    fn daemon(&self) -> Result<&ServiceDaemon, DiscoveryError> {
        self.daemon
            .get_or_init(|| {
                let d = ServiceDaemon::new().map_err(|e| e.to_string())?;
                d.disable_interface(IfKind::IPv6)
                    .map_err(|e| e.to_string())?;
                if !self.allow_loopback {
                    d.disable_interface(IfKind::LoopbackV4)
                        .map_err(|e| e.to_string())?;
                }
                Ok(d)
            })
            .as_ref()
            .map_err(|e| DiscoveryError::Unavailable(e.clone()))
    }
}

impl Drop for MdnsDiscovery {
    fn drop(&mut self) {
        if let Some(Ok(d)) = self.daemon.get()
            && let Ok(rx) = d.shutdown()
        {
            let _ = rx.recv_timeout(Duration::from_secs(1));
        }
    }
}

fn random_label() -> String {
    let mut b = [0u8; 4];
    let _ = random::fill(&mut b);
    format!("dropumi-{:02x}{:02x}{:02x}{:02x}", b[0], b[1], b[2], b[3])
}

/// A registered advertisement.
pub struct MdnsAd {
    daemon: ServiceDaemon,
    fullname: String,
    instance: String,
}

impl std::fmt::Debug for MdnsAd {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MdnsAd")
            .field("instance", &self.instance)
            .finish_non_exhaustive()
    }
}

impl AdHandle for MdnsAd {
    async fn stop(self) {
        let daemon = self.daemon.clone();
        let fullname = self.fullname.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(rx) = daemon.unregister(&fullname) {
                let _ = rx.recv_timeout(Duration::from_secs(1));
            }
        })
        .await;
    }

    fn instance(&self) -> &str {
        &self.instance
    }
}

impl Discovery for MdnsDiscovery {
    type Handle = MdnsAd;

    async fn advertise(&self, ad: Advertisement) -> Result<MdnsAd, DiscoveryError> {
        let daemon = self.daemon()?.clone();
        let label = random_label();
        let host = format!("{label}.local.");
        let props: HashMap<String, String> = txt::build(&ad).into_iter().collect();
        let info = ServiceInfo::new(SERVICE_TYPE, &label, &host, (), ad.port, props)
            .map_err(|e| DiscoveryError::Other(e.to_string()))?
            .enable_addr_auto();
        let fullname = info.get_fullname().to_owned();
        daemon
            .register(info)
            .map_err(|e| DiscoveryError::Unavailable(e.to_string()))?;
        Ok(MdnsAd {
            daemon,
            fullname,
            instance: label,
        })
    }

    async fn browse(&self, timeout: Duration) -> Result<Vec<DiscoveredPeer>, DiscoveryError> {
        let daemon = self.daemon()?;
        let rx = daemon
            .browse(SERVICE_TYPE)
            .map_err(|e| DiscoveryError::Unavailable(e.to_string()))?;
        let deadline = Instant::now() + timeout;
        let mut found: HashMap<String, DiscoveredPeer> = HashMap::new();
        loop {
            let event = match tokio::time::timeout_at(deadline, rx.recv_async()).await {
                Err(_) | Ok(Err(_)) => break,
                Ok(Ok(e)) => e,
            };
            match event {
                ServiceEvent::ServiceResolved(info) => {
                    let hints =
                        txt::parse(info.txt_properties.iter().map(|p| (p.key(), p.val_str())));
                    let mut addrs: Vec<std::net::IpAddr> = info
                        .get_addresses_v4()
                        .into_iter()
                        .map(std::net::IpAddr::V4)
                        .collect();
                    addrs.sort();
                    found.insert(
                        info.get_fullname().to_owned(),
                        DiscoveredPeer {
                            instance: info.get_fullname().to_owned(),
                            name: hints.name,
                            fp_hint: hints.fp_hint,
                            addrs,
                            port: info.get_port(),
                            versions: hints.versions,
                        },
                    );
                }
                ServiceEvent::ServiceRemoved(_, fullname) => {
                    found.remove(&fullname);
                }
                _ => {}
            }
        }
        let _ = daemon.stop_browse(SERVICE_TYPE);
        let mut peers: Vec<DiscoveredPeer> = found.into_values().collect();
        peers.sort_by(|a, b| a.instance.cmp(&b.instance));
        Ok(peers)
    }
}
