// SPDX-License-Identifier: MPL-2.0
//! Discovery through the platform's own service (Bonjour, NSD).
//!
//! The app only moves bytes: Rust picks the random instance name, builds and
//! parses the TXT record (`dropumi_infra::txt`, discovery.md section 4),
//! drops IPv6 and loopback addresses, filters out this device's own
//! advertisement and keeps at most [`MAX_PEERS`] devices, because anyone on
//! the network can advertise. Everything found is a hint; identity is only
//! established by the handshake.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex, Weak};

use dropumi_application::ports::Advertisement;
use dropumi_domain::discovery::DiscoveredPeer;
use dropumi_domain::trust::TrustedDevice;
use dropumi_infra::txt;
use dropumi_protocol::limits::SUPPORTED_VERSIONS;

use crate::error::CallbackError;
use crate::mailbox::Mailbox;
use crate::runtime::lock;
use crate::types::{NearbyPeer, NodeEvent};

/// Devices kept in the nearby list.
pub(crate) const MAX_PEERS: usize = 256;
/// The DNS-SD service type.
pub const SERVICE_TYPE: &str = "_dropumi._tcp";

/// One TXT key and its raw value.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TxtEntry {
    /// Key.
    pub key: String,
    /// Raw value bytes.
    pub value: Vec<u8>,
}

/// An advertisement to register.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ServiceAd {
    /// `_dropumi._tcp`.
    pub service_type: String,
    /// Instance name (random; use it unchanged).
    pub instance: String,
    /// TCP port.
    pub port: u16,
    /// TXT record, in order.
    pub txt: Vec<TxtEntry>,
}

/// The platform's discovery service, implemented by the app.
///
/// Every method is called on a thread of its own, never on the main thread
/// of the app and never on the node's runtime thread; it may block briefly.
#[uniffi::export(with_foreign)]
pub trait PlatformDiscovery: Send + Sync {
    /// Registers an advertisement; returns the instance name actually
    /// registered (the platform may rename it on a conflict).
    ///
    /// # Errors
    /// Registration failed; manual addresses keep working.
    fn advertise(&self, ad: ServiceAd) -> Result<String, CallbackError>;

    /// Removes an advertisement (sending goodbye packets where supported).
    ///
    /// # Errors
    /// The platform reported a failure.
    fn withdraw(&self, instance: String) -> Result<(), CallbackError>;

    /// Starts browsing for `_dropumi._tcp` and reports to `sink`.
    ///
    /// # Errors
    /// Browsing could not start.
    fn start_browse(&self, sink: Arc<BrowseSink>) -> Result<(), CallbackError>;

    /// Stops browsing.
    ///
    /// # Errors
    /// The platform reported a failure.
    fn stop_browse(&self) -> Result<(), CallbackError>;
}

/// What the app reports while browsing.
#[derive(Debug, uniffi::Object)]
pub struct BrowseSink {
    table: Weak<PeerTable>,
}

#[uniffi::export]
impl BrowseSink {
    /// A device was resolved: its raw TXT entries, its addresses as text
    /// and its port.
    pub fn resolved(
        &self,
        instance: String,
        txt: Vec<TxtEntry>,
        addresses: Vec<String>,
        port: u16,
    ) {
        if let Some(table) = self.table.upgrade() {
            table.resolved(&instance, &txt, &addresses, port);
        }
    }

    /// A device went away.
    pub fn lost(&self, instance: String) {
        if let Some(table) = self.table.upgrade() {
            table.lost(&instance);
        }
    }

    /// Browsing failed (for example, local network access was denied).
    pub fn failed(&self, reason: String) {
        if let Some(table) = self.table.upgrade() {
            table.mailbox.push(NodeEvent::DiscoveryUnavailable {
                reason: crate::types::display_safe(&reason),
            });
        }
    }
}

/// Devices currently seen.
pub(crate) struct PeerTable {
    peers: Mutex<HashMap<String, DiscoveredPeer>>,
    own: Mutex<HashSet<String>>,
    mailbox: Arc<Mailbox>,
    /// Reads the trust list for "looks like" hints.
    trusted: Box<dyn Fn() -> Vec<TrustedDevice> + Send + Sync>,
    /// Keep loopback addresses (tests on one host only).
    allow_loopback: bool,
}

impl std::fmt::Debug for PeerTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PeerTable").finish_non_exhaustive()
    }
}

impl PeerTable {
    pub(crate) fn new(
        mailbox: Arc<Mailbox>,
        trusted: Box<dyn Fn() -> Vec<TrustedDevice> + Send + Sync>,
        allow_loopback: bool,
    ) -> Arc<Self> {
        Arc::new(Self {
            peers: Mutex::default(),
            own: Mutex::default(),
            mailbox,
            trusted,
            allow_loopback,
        })
    }

    pub(crate) fn sink(self: &Arc<Self>) -> Arc<BrowseSink> {
        Arc::new(BrowseSink {
            table: Arc::downgrade(self),
        })
    }

    /// Remembers one of this device's own instance names.
    pub(crate) fn add_own(&self, instance: &str) {
        lock(&self.own).insert(instance.to_owned());
        if lock(&self.peers).remove(instance).is_some() {
            self.mailbox.push(NodeEvent::PeerLost {
                instance: instance.to_owned(),
            });
        }
    }

    pub(crate) fn remove_own(&self, instance: &str) {
        lock(&self.own).remove(instance);
    }

    fn resolved(&self, instance: &str, entries: &[TxtEntry], addresses: &[String], port: u16) {
        if instance.is_empty() || port == 0 || lock(&self.own).contains(instance) {
            return;
        }
        let addrs: Vec<IpAddr> = addresses
            .iter()
            .filter_map(|a| a.trim().parse::<Ipv4Addr>().ok())
            .filter(|ip| {
                (self.allow_loopback || !ip.is_loopback())
                    && !ip.is_unspecified()
                    && !ip.is_multicast()
            })
            .map(IpAddr::V4)
            .collect();
        if addrs.is_empty() {
            return;
        }
        // Invalid UTF-8 in a value counts as an empty value (discovery.md
        // section 4.2); keys are text by definition.
        let decoded: Vec<(String, String)> = entries
            .iter()
            .map(|e| {
                (
                    e.key.clone(),
                    String::from_utf8(e.value.clone()).unwrap_or_default(),
                )
            })
            .collect();
        let hints = txt::parse(decoded.iter().map(|(k, v)| (k.as_str(), v.as_str())));
        let peer = DiscoveredPeer {
            instance: crate::types::display_safe(instance),
            name: hints.name,
            fp_hint: hints.fp_hint,
            addrs,
            port,
            versions: hints.versions,
        };
        {
            let mut peers = lock(&self.peers);
            if !peers.contains_key(&peer.instance) && peers.len() >= MAX_PEERS {
                return;
            }
            peers.insert(peer.instance.clone(), peer.clone());
        }
        let nearby = self.nearby(&peer, &(self.trusted)());
        self.mailbox.push(NodeEvent::PeerFound { peer: nearby });
    }

    fn lost(&self, instance: &str) {
        let key = crate::types::display_safe(instance);
        if lock(&self.peers).remove(&key).is_some() {
            self.mailbox.push(NodeEvent::PeerLost { instance: key });
        }
    }

    /// Forgets every device (browsing stopped).
    pub(crate) fn clear(&self) {
        let gone: Vec<String> = lock(&self.peers).drain().map(|(k, _)| k).collect();
        for instance in gone {
            self.mailbox.push(NodeEvent::PeerLost { instance });
        }
    }

    pub(crate) fn get(&self, instance: &str) -> Option<DiscoveredPeer> {
        lock(&self.peers).get(instance).cloned()
    }

    pub(crate) fn list(&self) -> Vec<NearbyPeer> {
        let trusted = (self.trusted)();
        let mut out: Vec<NearbyPeer> = lock(&self.peers)
            .values()
            .map(|p| self.nearby(p, &trusted))
            .collect();
        out.sort_by(|a, b| {
            a.name
                .cmp(&b.name)
                .then_with(|| a.instance.cmp(&b.instance))
        });
        out
    }

    fn nearby(&self, p: &DiscoveredPeer, trusted: &[TrustedDevice]) -> NearbyPeer {
        NearbyPeer {
            instance: p.instance.clone(),
            name: p.name.as_str().to_owned(),
            fingerprint_hint: p.fp_hint.clone(),
            compatible: p.versions.iter().any(|v| SUPPORTED_VERSIONS.contains(v)),
            hint_alias: p
                .fp_hint
                .as_deref()
                .and_then(|h| single_trust_match(h, trusted))
                .and_then(|d| d.alias.as_ref().map(|a| a.as_str().to_owned())),
        }
    }
}

/// The one trusted device whose fingerprint starts with `hint`.
pub(crate) fn single_trust_match<'a>(
    hint: &str,
    trusted: &'a [TrustedDevice],
) -> Option<&'a TrustedDevice> {
    let mut matches = trusted
        .iter()
        .filter(|d| d.fingerprint.to_full().starts_with(hint));
    let first = matches.next()?;
    matches.next().is_none().then_some(first)
}

/// The TXT entries and instance name for this device's advertisement.
pub(crate) fn service_ad(ad: &Advertisement) -> ServiceAd {
    ServiceAd {
        service_type: SERVICE_TYPE.to_owned(),
        instance: txt::random_label(),
        port: ad.port,
        txt: txt::build(ad)
            .into_iter()
            .map(|(key, value)| TxtEntry {
                key,
                value: value.into_bytes(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mailbox::Next;
    use dropumi_domain::device::DeviceName;
    use dropumi_domain::identity::{Fingerprint, PublicKey};
    use dropumi_domain::time::Timestamp;
    use dropumi_domain::trust::PairingMethod;
    use std::time::{Duration, Instant};

    fn entry(k: &str, v: &[u8]) -> TxtEntry {
        TxtEntry {
            key: k.into(),
            value: v.to_vec(),
        }
    }

    fn table(trusted: Vec<TrustedDevice>) -> (Arc<PeerTable>, Arc<Mailbox>) {
        let mailbox = Arc::new(Mailbox::default());
        let t = PeerTable::new(
            Arc::clone(&mailbox),
            Box::new(move || trusted.clone()),
            false,
        );
        (t, mailbox)
    }

    fn events(m: &Mailbox) -> Vec<NodeEvent> {
        match m.next(Instant::now() + Duration::from_secs(10)) {
            Next::Deliver(b) => b,
            _ => Vec::new(),
        }
    }

    fn trusted(b: u8, alias: &str) -> TrustedDevice {
        TrustedDevice {
            fingerprint: Fingerprint::from_digest([b; 32]),
            public_key: Some(PublicKey::from_bytes([b; 32])),
            alias: Some(DeviceName::sanitize(alias)),
            method: PairingMethod::Sas,
            added_at: Timestamp(1),
            last_seen: None,
        }
    }

    #[test]
    fn browse_sink_parses_txt_with_infra_rules() {
        let (t, m) = table(Vec::new());
        let sink = t.sink();
        sink.resolved(
            "dropumi-0a0b0c0d".into(),
            vec![
                entry("v", b"1,2"),
                entry("n", b"Office\x07 PC"),
                entry("fp", b"ABCDEFG2"),
                entry("x", b"ignored"),
            ],
            vec!["192.168.1.20".into()],
            47_321,
        );
        let p = t.get("dropumi-0a0b0c0d").unwrap();
        assert_eq!(p.name.as_str(), "Office PC");
        assert_eq!(p.fp_hint.as_deref(), Some("ABCDEFG2"));
        assert_eq!(p.versions, vec![1, 2]);
        assert!(
            matches!(events(&m).as_slice(), [NodeEvent::PeerFound { peer }] if peer.compatible)
        );
    }

    #[test]
    fn invalid_utf8_txt_value_treated_as_empty() {
        let (t, _m) = table(Vec::new());
        t.sink().resolved(
            "dropumi-00000001".into(),
            vec![entry("n", &[0xff, 0xfe]), entry("fp", &[0xc3])],
            vec!["10.0.0.2".into()],
            1,
        );
        let p = t.get("dropumi-00000001").unwrap();
        assert_eq!(p.fp_hint, None);
        assert_eq!(p.name, DeviceName::sanitize(""));
    }

    #[test]
    fn ipv6_and_loopback_filtered() {
        let (t, _m) = table(Vec::new());
        let sink = t.sink();
        sink.resolved(
            "dropumi-00000002".into(),
            Vec::new(),
            vec!["::1".into(), "fe80::1".into(), "127.0.0.1".into()],
            1,
        );
        assert!(t.get("dropumi-00000002").is_none(), "no usable address");
        sink.resolved(
            "dropumi-00000003".into(),
            Vec::new(),
            vec!["fe80::1".into(), "192.168.0.9".into()],
            1,
        );
        assert_eq!(
            t.get("dropumi-00000003").unwrap().addrs,
            vec!["192.168.0.9".parse::<IpAddr>().unwrap()]
        );
    }

    #[test]
    fn peer_table_capped() {
        let (t, _m) = table(Vec::new());
        let sink = t.sink();
        for i in 0..(MAX_PEERS + 50) {
            sink.resolved(
                format!("dropumi-{i:08x}"),
                Vec::new(),
                vec!["10.0.0.2".into()],
                1,
            );
        }
        assert_eq!(t.list().len(), MAX_PEERS);
    }

    #[test]
    fn own_advertisement_not_listed_and_lost_removes_peer() {
        let (t, _m) = table(Vec::new());
        let sink = t.sink();
        t.add_own("dropumi-aaaaaaaa");
        sink.resolved(
            "dropumi-aaaaaaaa".into(),
            Vec::new(),
            vec!["10.0.0.2".into()],
            1,
        );
        assert!(t.list().is_empty());
        sink.resolved(
            "dropumi-bbbbbbbb".into(),
            Vec::new(),
            vec!["10.0.0.3".into()],
            1,
        );
        assert_eq!(t.list().len(), 1);
        sink.lost("dropumi-bbbbbbbb".into());
        assert!(t.list().is_empty());
    }

    #[test]
    fn hint_alias_only_for_a_single_trust_match() {
        let one = trusted(1, "Studio Mac");
        let hint = one.fingerprint.hint();
        let (t, _m) = table(vec![one.clone()]);
        t.sink().resolved(
            "dropumi-00000004".into(),
            vec![entry("fp", hint.as_bytes())],
            vec!["10.0.0.4".into()],
            1,
        );
        assert_eq!(t.list()[0].hint_alias.as_deref(), Some("Studio Mac"));
        // Two trusted devices sharing the hint: no alias is suggested.
        let twin = TrustedDevice {
            alias: Some(DeviceName::sanitize("Twin")),
            ..one
        };
        assert!(single_trust_match(&hint, &[twin.clone(), twin]).is_none());
    }

    #[test]
    fn advertise_uses_random_instance() {
        let ad = Advertisement {
            name: DeviceName::sanitize("Phone"),
            fp_hint: "ABCDEFG2".into(),
            port: 47_321,
            versions: vec![1],
        };
        let (a, b) = (service_ad(&ad), service_ad(&ad));
        assert!(a.instance.starts_with("dropumi-"));
        assert_ne!(a.instance, b.instance);
        assert!(!a.instance.contains("Phone"));
        assert_eq!(a.service_type, SERVICE_TYPE);
        let keys: Vec<&str> = a.txt.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["v", "n", "fp"]);
    }
}
