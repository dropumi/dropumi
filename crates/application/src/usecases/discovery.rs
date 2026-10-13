// SPDX-License-Identifier: MPL-2.0
//! DiscoverNearbyDevices and ManageDiscoverability.
//!
//! Discovery results are hints: names and fingerprint hints are never proof
//! of identity.

use std::time::Duration;

use dropumi_domain::discovery::DiscoveredPeer;
use dropumi_protocol::limits::SUPPORTED_VERSIONS;

use crate::error::DiscoveryError;
use crate::events::{AppEvent, EventSink};
use crate::identity::LocalIdentity;
use crate::ports::{AdHandle, Advertisement, Discovery};

/// Browses for `timeout` and emits one `PeerFound` per device.
///
/// # Errors
/// [`DiscoveryError`] if discovery cannot run.
pub async fn discover<D: Discovery>(
    discovery: &D,
    timeout: Duration,
    events: &EventSink,
) -> Result<Vec<DiscoveredPeer>, DiscoveryError> {
    let peers = discovery.browse(timeout).await?;
    for p in &peers {
        events.emit(AppEvent::PeerFound(p.clone())).await;
    }
    Ok(peers)
}

/// Starts advertising this device on `port`. Failure is reported as an
/// event: manual addressing keeps working without discovery.
pub async fn advertise<D: Discovery>(
    discovery: &D,
    identity: &LocalIdentity,
    port: u16,
    events: &EventSink,
) -> Option<D::Handle> {
    let ad = Advertisement {
        name: identity.name().clone(),
        fp_hint: identity.fingerprint().hint(),
        port,
        versions: SUPPORTED_VERSIONS.to_vec(),
    };
    match discovery.advertise(ad).await {
        Ok(handle) => {
            events
                .emit(AppEvent::Advertising {
                    instance: handle.instance().to_owned(),
                })
                .await;
            Some(handle)
        }
        Err(e) => {
            events
                .emit(AppEvent::DiscoveryUnavailable {
                    reason: e.to_string(),
                })
                .await;
            None
        }
    }
}
