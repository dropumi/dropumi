// SPDX-License-Identifier: MPL-2.0
//! Connection candidates, candidate ranking, the connection state machine and
//! peer expectations (target binding).

use std::net::{IpAddr, SocketAddr};

use crate::discovery::{DiscoveredPeer, ResolveError, Target};
use crate::identity::{Fingerprint, FingerprintPrefix};
use crate::trust::{TrustedDevice, trusted_matching};

/// Where a candidate address came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CandidateSource {
    /// Typed by the user.
    Manual,
    /// Learned from mDNS.
    Mdns,
}

/// An address to try.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionCandidate {
    /// Socket address.
    pub addr: SocketAddr,
    /// Origin of the address.
    pub source: CandidateSource,
}

impl ConnectionCandidate {
    /// A manual candidate.
    #[must_use]
    pub const fn manual(addr: SocketAddr) -> Self {
        Self {
            addr,
            source: CandidateSource::Manual,
        }
    }
}

/// Orders candidates for sequential dialing.
///
/// Manual candidates come first in the user's order. Discovered candidates
/// follow: loopback (only when `allow_loopback`), private IPv4, other IPv4,
/// IPv4 link-local (169.254/16). Discovered IPv6 and disallowed loopback
/// candidates are dropped; duplicates are removed. The sort is stable.
#[must_use]
pub fn rank_candidates(
    candidates: Vec<ConnectionCandidate>,
    allow_loopback: bool,
) -> Vec<ConnectionCandidate> {
    let mut keyed: Vec<(u8, ConnectionCandidate)> = Vec::with_capacity(candidates.len());
    for c in candidates {
        if keyed.iter().any(|(_, k)| k.addr == c.addr) {
            continue;
        }
        let class = match c.source {
            CandidateSource::Manual => 0,
            CandidateSource::Mdns => match c.addr.ip() {
                IpAddr::V4(v4) if v4.is_loopback() => {
                    if allow_loopback {
                        1
                    } else {
                        continue;
                    }
                }
                IpAddr::V4(v4) if v4.is_private() => 2,
                IpAddr::V4(v4) if v4.is_link_local() => 4,
                IpAddr::V4(_) => 3,
                IpAddr::V6(_) => continue,
            },
        };
        keyed.push((class, c));
    }
    keyed.sort_by_key(|(class, _)| *class);
    keyed.into_iter().map(|(_, c)| c).collect()
}

/// Why a connection ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// The session finished normally.
    Normal,
    /// Every candidate failed.
    Exhausted,
    /// The session failed after connecting.
    Failed,
}

/// Connection lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Nothing attempted yet.
    Idle,
    /// Dialing candidate number `attempt` (1-based).
    Connecting {
        /// Attempt number.
        attempt: u32,
    },
    /// TCP connected; running the preamble and handshake.
    Handshaking,
    /// Secure channel established.
    Authenticated {
        /// Whether the peer is in the trust store.
        verified: bool,
    },
    /// Terminal state.
    Closed {
        /// Why the connection ended.
        reason: CloseReason,
    },
}

/// Inputs to [`ConnectionState::apply`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionEvent {
    /// Start dialing the first candidate.
    Dial,
    /// The current candidate failed; try the next one.
    DialFailed,
    /// TCP connected.
    TcpEstablished,
    /// Handshake completed.
    HandshakeDone {
        /// Whether the peer is trusted.
        verified: bool,
    },
    /// The connection closed.
    Close(CloseReason),
}

/// An event that is not valid in the current state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("invalid connection transition from {from:?} on {event:?}")]
pub struct InvalidConnectionTransition {
    /// State before the event.
    pub from: ConnectionState,
    /// Rejected event.
    pub event: ConnectionEvent,
}

impl ConnectionState {
    /// Applies an event.
    ///
    /// # Errors
    /// Returns [`InvalidConnectionTransition`] (leaving the state unchanged)
    /// for transitions outside the table; `Closed` is terminal.
    pub fn apply(&mut self, event: ConnectionEvent) -> Result<(), InvalidConnectionTransition> {
        use ConnectionEvent as E;
        use ConnectionState as S;
        let next = match (*self, event) {
            (S::Idle, E::Dial) => S::Connecting { attempt: 1 },
            (S::Connecting { attempt }, E::DialFailed) => S::Connecting {
                attempt: attempt.saturating_add(1),
            },
            (S::Connecting { .. }, E::TcpEstablished) => S::Handshaking,
            (S::Handshaking, E::HandshakeDone { verified }) => S::Authenticated { verified },
            (S::Closed { .. }, _) => {
                return Err(InvalidConnectionTransition { from: *self, event });
            }
            (_, E::Close(reason)) => S::Closed { reason },
            _ => return Err(InvalidConnectionTransition { from: *self, event }),
        };
        *self = next;
        Ok(())
    }
}

/// What the authenticated responder key must satisfy before the initiator
/// reveals its own identity (sends Noise msg3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PeerExpectation {
    /// Exact fingerprint required, if the target pins one trust entry.
    pub exact: Option<Fingerprint>,
    /// Prefixes that must all match.
    pub prefixes: Vec<FingerprintPrefix>,
}

impl PeerExpectation {
    /// Whether `fp` satisfies every constraint.
    #[must_use]
    pub fn matches(&self, fp: &Fingerprint) -> bool {
        self.exact.as_ref().is_none_or(|e| e == fp) && self.prefixes.iter().all(|p| p.matches(fp))
    }

    /// `true` when there is no constraint.
    #[must_use]
    pub fn is_unconstrained(&self) -> bool {
        self.exact.is_none() && self.prefixes.is_empty()
    }
}

/// Derives the identity expectation for a send or pair target.
///
/// | Target | Expectation |
/// |---|---|
/// | fingerprint prefix `p` | `p`; plus `exact` when `p` matches exactly one trust entry; `Ambiguous` when it matches several |
/// | name | the resolved peer's TXT hint as a prefix; plus `exact` when the hint matches exactly one trust entry |
/// | address / host | none |
/// | `explicit` (`--expect-fingerprint`) | always added as a prefix |
///
/// Several targets are only allowed for addresses, so they add nothing.
///
/// # Errors
/// [`ResolveError::Ambiguous`] when a fingerprint prefix matches several
/// trust entries.
pub fn expectation_for(
    targets: &[Target],
    resolved: Option<&DiscoveredPeer>,
    trusted: &[TrustedDevice],
    explicit: Option<FingerprintPrefix>,
) -> Result<PeerExpectation, ResolveError> {
    let mut exp = PeerExpectation::default();
    if let [single] = targets {
        match single {
            Target::Fingerprint(prefix) => {
                let hits = trusted_matching(trusted, prefix);
                match hits.as_slice() {
                    [] => {}
                    [one] => exp.exact = Some(one.fingerprint),
                    many => {
                        return Err(ResolveError::Ambiguous(
                            many.iter()
                                .map(|d| {
                                    let alias = d.alias.as_ref().map_or("unnamed", |a| a.as_str());
                                    format!("{alias} ({})", d.fingerprint.to_display())
                                })
                                .collect(),
                        ));
                    }
                }
                exp.prefixes.push(prefix.clone());
            }
            Target::Name(_) => {
                if let Some(hint) = resolved.and_then(|p| p.fp_hint.as_deref())
                    && let Ok(prefix) = FingerprintPrefix::parse(hint)
                {
                    let hits = trusted_matching(trusted, &prefix);
                    if let [one] = hits.as_slice() {
                        exp.exact = Some(one.fingerprint);
                    }
                    exp.prefixes.push(prefix);
                }
            }
            Target::Addr(_) | Target::Host { .. } => {}
        }
    }
    if let Some(p) = explicit {
        exp.prefixes.push(p);
    }
    Ok(exp)
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::*;
    use crate::device::DeviceName;
    use crate::time::Timestamp;
    use crate::trust::PairingMethod;

    fn cand(ip: IpAddr, source: CandidateSource) -> ConnectionCandidate {
        ConnectionCandidate {
            addr: SocketAddr::new(ip, 47321),
            source,
        }
    }

    fn fp(byte: u8) -> Fingerprint {
        Fingerprint::from_digest([byte; 32])
    }

    fn trusted(f: Fingerprint, alias: &str) -> TrustedDevice {
        TrustedDevice {
            fingerprint: f,
            public_key: None,
            alias: Some(DeviceName::sanitize(alias)),
            method: PairingMethod::Manual,
            added_at: Timestamp(1),
            last_seen: None,
        }
    }

    #[test]
    fn rank_candidates_order() {
        let v4 = |a, b, c, d| IpAddr::V4(Ipv4Addr::new(a, b, c, d));
        let input = vec![
            cand(v4(169, 254, 1, 1), CandidateSource::Mdns),
            cand(v4(8, 8, 8, 8), CandidateSource::Mdns),
            cand(v4(10, 0, 0, 2), CandidateSource::Manual),
            cand(v4(127, 0, 0, 1), CandidateSource::Mdns),
            cand(v4(192, 168, 1, 5), CandidateSource::Mdns),
            cand(IpAddr::V6(Ipv6Addr::LOCALHOST), CandidateSource::Mdns),
            cand(v4(10, 0, 0, 1), CandidateSource::Manual),
            cand(v4(192, 168, 1, 5), CandidateSource::Mdns),
        ];
        let ranked: Vec<IpAddr> = rank_candidates(input.clone(), false)
            .iter()
            .map(|c| c.addr.ip())
            .collect();
        assert_eq!(
            ranked,
            vec![
                v4(10, 0, 0, 2),
                v4(10, 0, 0, 1),
                v4(192, 168, 1, 5),
                v4(8, 8, 8, 8),
                v4(169, 254, 1, 1)
            ]
        );
        let with_lo = rank_candidates(input, true);
        assert_eq!(with_lo[2].addr.ip(), v4(127, 0, 0, 1));
        // A manual IPv6 address is kept.
        let manual_v6 = rank_candidates(
            vec![cand(
                IpAddr::V6(Ipv6Addr::LOCALHOST),
                CandidateSource::Manual,
            )],
            false,
        );
        assert_eq!(manual_v6.len(), 1);
    }

    #[test]
    fn state_transitions() {
        let mut s = ConnectionState::Idle;
        s.apply(ConnectionEvent::Dial).unwrap();
        s.apply(ConnectionEvent::DialFailed).unwrap();
        assert_eq!(s, ConnectionState::Connecting { attempt: 2 });
        s.apply(ConnectionEvent::TcpEstablished).unwrap();
        assert!(s.apply(ConnectionEvent::Dial).is_err());
        assert_eq!(s, ConnectionState::Handshaking);
        s.apply(ConnectionEvent::HandshakeDone { verified: true })
            .unwrap();
        s.apply(ConnectionEvent::Close(CloseReason::Normal))
            .unwrap();
        assert!(
            s.apply(ConnectionEvent::Close(CloseReason::Failed))
                .is_err()
        );
        assert_eq!(
            s,
            ConnectionState::Closed {
                reason: CloseReason::Normal
            }
        );
        let mut idle = ConnectionState::Idle;
        assert!(idle.apply(ConnectionEvent::TcpEstablished).is_err());
        idle.apply(ConnectionEvent::Close(CloseReason::Exhausted))
            .unwrap();
    }

    #[test]
    fn expectation_rules() {
        let a = fp(0x11);
        let b = fp(0x22);
        let store = vec![trusted(a, "Alpha"), trusted(b, "Beta")];

        // Fingerprint prefix that matches exactly one trust entry pins it.
        let pa = FingerprintPrefix::of(&a, 8);
        let e = expectation_for(&[Target::Fingerprint(pa.clone())], None, &store, None).unwrap();
        assert_eq!(e.exact, Some(a));
        assert!(e.matches(&a));
        assert!(!e.matches(&b));

        // A prefix shared by two trust entries is ambiguous.
        let twin = {
            let mut d = *a.as_bytes();
            d[31] ^= 1;
            Fingerprint::from_digest(d)
        };
        let store2 = vec![trusted(a, "Alpha"), trusted(twin, "Twin")];
        assert!(matches!(
            expectation_for(&[Target::Fingerprint(pa.clone())], None, &store2, None),
            Err(ResolveError::Ambiguous(v)) if v.len() == 2
        ));

        // A name resolves through the TXT hint, which pins the trust entry.
        let peer = DiscoveredPeer {
            instance: "x".into(),
            name: DeviceName::sanitize("Alpha"),
            fp_hint: Some(a.hint()),
            addrs: vec![],
            port: 1,
            versions: vec![1],
        };
        let e =
            expectation_for(&[Target::Name("Alpha".into())], Some(&peer), &store, None).unwrap();
        assert_eq!(e.exact, Some(a));
        assert!(!e.matches(&b), "trusted B must not satisfy a send to Alpha");

        // Name without a hint: unconstrained (policy still refuses unverified peers).
        let mut nohint = peer.clone();
        nohint.fp_hint = None;
        let e =
            expectation_for(&[Target::Name("Alpha".into())], Some(&nohint), &store, None).unwrap();
        assert!(e.is_unconstrained());

        // Address: no constraint unless --expect-fingerprint is given.
        let addr = Target::parse("127.0.0.1:47321").unwrap();
        let e = expectation_for(std::slice::from_ref(&addr), None, &store, None).unwrap();
        assert!(e.is_unconstrained());
        let pin = FingerprintPrefix::of(&b, 24);
        let e = expectation_for(&[addr], None, &store, Some(pin)).unwrap();
        assert!(e.matches(&b));
        assert!(!e.matches(&a));
    }
}
