// SPDX-License-Identifier: MPL-2.0
//! ConnectToPeer: resolve targets into ranked candidates plus an identity
//! expectation, then dial candidates in order.

use std::net::SocketAddr;
use std::time::Duration;

use dropumi_domain::connection::{
    CloseReason, ConnectionCandidate, ConnectionEvent, ConnectionState, PeerExpectation,
    expectation_for, rank_candidates,
};
use dropumi_domain::discovery::{DiscoveredPeer, Target, resolve_target};
use dropumi_domain::error::{Phase, TransferError};
use dropumi_domain::identity::FingerprintPrefix;

use super::UseCaseError;
use crate::error::TransportError;
use crate::events::{AppEvent, EventSink};
use crate::ports::{Connector, Discovery, TrustStore};

/// Where to connect and whom to expect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// Ranked candidates.
    pub candidates: Vec<ConnectionCandidate>,
    /// Identity the responder must have.
    pub expectation: PeerExpectation,
    /// The discovered device, for name and fingerprint targets.
    pub resolved: Option<DiscoveredPeer>,
}

/// Resolution inputs.
#[derive(Debug, Clone)]
pub struct ResolveParams<'a> {
    /// One target, or several address targets.
    pub targets: &'a [Target],
    /// Extra pin (`--expect-fingerprint`).
    pub explicit: Option<FingerprintPrefix>,
    /// Browse duration for discovery targets.
    pub discover_timeout: Duration,
    /// Accept discovered loopback addresses.
    pub allow_loopback: bool,
}

/// Resolves targets into candidates and an expectation.
///
/// # Errors
/// [`UseCaseError::Usage`] for invalid target combinations,
/// [`UseCaseError::DiscoveryUnavailable`] when a name or fingerprint target
/// needs discovery that is not available, [`UseCaseError::Resolve`] when
/// nothing or several devices match, and [`UseCaseError::Store`] for trust
/// store failures.
pub async fn resolve<C, D, T>(
    connector: &C,
    discovery: Option<&D>,
    trust: &T,
    params: ResolveParams<'_>,
) -> Result<Resolution, UseCaseError>
where
    C: Connector,
    D: Discovery,
    T: TrustStore,
{
    if params.targets.is_empty() {
        return Err(UseCaseError::Usage("no target given".into()));
    }
    if params.targets.len() > 1 && params.targets.iter().any(Target::needs_discovery) {
        return Err(UseCaseError::Usage(
            "several targets are only allowed for addresses".into(),
        ));
    }
    let mut candidates = Vec::new();
    let mut resolved = None;
    for target in params.targets {
        match target {
            Target::Addr(addr) => candidates.push(ConnectionCandidate::manual(*addr)),
            Target::Host { host, port } => {
                let addrs = connector
                    .resolve(host, *port)
                    .await
                    .map_err(|e| UseCaseError::Usage(format!("cannot resolve {host}: {e}")))?;
                candidates.extend(addrs.into_iter().map(ConnectionCandidate::manual));
            }
            Target::Fingerprint(_) | Target::Name(_) => {
                let discovery = discovery.ok_or_else(|| {
                    UseCaseError::DiscoveryUnavailable(
                        "name and fingerprint targets need mDNS discovery".into(),
                    )
                })?;
                let peers = discovery
                    .browse(params.discover_timeout)
                    .await
                    .map_err(|e| UseCaseError::DiscoveryUnavailable(e.to_string()))?;
                let r = resolve_target(&peers, target)?;
                candidates.extend(r.candidates);
                resolved = Some(r.peer.clone());
            }
        }
    }
    let trusted = trust.list()?;
    let expectation =
        expectation_for(params.targets, resolved.as_ref(), &trusted, params.explicit)?;
    let candidates = rank_candidates(candidates, params.allow_loopback);
    if candidates.is_empty() {
        return Err(UseCaseError::Usage(
            "no usable address for the target".into(),
        ));
    }
    Ok(Resolution {
        candidates,
        expectation,
        resolved,
    })
}

/// Dials candidates in order until one connects.
///
/// # Errors
/// `Timeout(Connect)` if every candidate timed out, otherwise
/// `ConnectionLost` (refused or unreachable).
pub async fn dial<C: Connector>(
    connector: &C,
    candidates: &[ConnectionCandidate],
    per_candidate: Duration,
    events: &EventSink,
) -> Result<(C::Stream, SocketAddr), TransferError> {
    let mut state = ConnectionState::Idle;
    let mut all_timed_out = true;
    for (i, c) in candidates.iter().enumerate() {
        let event = if i == 0 {
            ConnectionEvent::Dial
        } else {
            ConnectionEvent::DialFailed
        };
        state
            .apply(event)
            .map_err(|e| TransferError::Internal(e.to_string()))?;
        events.emit(AppEvent::Connecting { addr: c.addr }).await;
        match connector.connect(c.addr, per_candidate).await {
            Ok(stream) => {
                state
                    .apply(ConnectionEvent::TcpEstablished)
                    .map_err(|e| TransferError::Internal(e.to_string()))?;
                return Ok((stream, c.addr));
            }
            Err(e) => {
                tracing::debug!(addr = %c.addr, error = %e, "candidate failed");
                if !matches!(e, TransportError::Timeout) {
                    all_timed_out = false;
                }
            }
        }
    }
    let _ = state.apply(ConnectionEvent::Close(CloseReason::Exhausted));
    Err(if all_timed_out && !candidates.is_empty() {
        TransferError::Timeout(Phase::Connect)
    } else {
        TransferError::ConnectionLost
    })
}
