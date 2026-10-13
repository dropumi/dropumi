// SPDX-License-Identifier: MPL-2.0
//! Discovery results and user-supplied targets.
//!
//! Discovery data (names, TXT hints, addresses) is never proof of identity:
//! it only produces connection candidates and, for name or fingerprint
//! targets, an expectation that the authenticated key must satisfy.

use std::net::{IpAddr, SocketAddr};

use crate::connection::{CandidateSource, ConnectionCandidate};
use crate::device::DeviceName;
use crate::identity::{FINGERPRINT_HINT_LEN, FingerprintPrefix, looks_like_fingerprint};

/// A device seen through a discovery mechanism (unauthenticated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredPeer {
    /// Discovery instance name (random, per advertisement).
    pub instance: String,
    /// Advertised, sanitized device name.
    pub name: DeviceName,
    /// Advertised 8-character fingerprint hint, if valid.
    pub fp_hint: Option<String>,
    /// Advertised addresses.
    pub addrs: Vec<IpAddr>,
    /// Advertised TCP port.
    pub port: u16,
    /// Advertised protocol versions.
    pub versions: Vec<u16>,
}

/// A change observed by discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiscoveryEvent {
    /// A new device appeared.
    Found(DiscoveredPeer),
    /// A known device changed its record.
    Updated(DiscoveredPeer),
    /// A device went away.
    Lost {
        /// Instance name of the device.
        instance: String,
    },
}

/// What the user asked to connect to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A literal socket address such as `192.168.1.20:47321` or `[::1]:47321`.
    Addr(SocketAddr),
    /// A host name and port to resolve.
    Host {
        /// Host name.
        host: String,
        /// TCP port.
        port: u16,
    },
    /// A fingerprint prefix (at least 8 base32 characters).
    Fingerprint(FingerprintPrefix),
    /// An exact device name (case-insensitive).
    Name(String),
}

impl Target {
    /// Parses a user-supplied target.
    ///
    /// Order: socket address, `host:port`, fingerprint prefix (≥8 base32
    /// characters, `-` allowed), otherwise a device name.
    ///
    /// # Errors
    /// Returns [`TargetParseError`] for empty input or port 0.
    pub fn parse(raw: &str) -> Result<Self, TargetParseError> {
        let s = raw.trim();
        if s.is_empty() {
            return Err(TargetParseError::Empty);
        }
        if let Ok(addr) = s.parse::<SocketAddr>() {
            if addr.port() == 0 {
                return Err(TargetParseError::PortZero);
            }
            return Ok(Self::Addr(addr));
        }
        if let Some((host, port)) = s.rsplit_once(':')
            && is_hostname(host)
            && let Ok(port) = port.parse::<u16>()
        {
            if port == 0 {
                return Err(TargetParseError::PortZero);
            }
            return Ok(Self::Host {
                host: host.to_owned(),
                port,
            });
        }
        if looks_like_fingerprint(s)
            && let Ok(prefix) = FingerprintPrefix::parse(s)
        {
            return Ok(Self::Fingerprint(prefix));
        }
        Ok(Self::Name(s.to_owned()))
    }

    /// `true` for targets that need discovery to produce candidates.
    #[must_use]
    pub const fn needs_discovery(&self) -> bool {
        matches!(self, Self::Fingerprint(_) | Self::Name(_))
    }
}

fn is_hostname(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
        && !s.starts_with('-')
        && !s.starts_with('.')
}

/// Errors parsing a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TargetParseError {
    /// Empty input.
    #[error("empty target")]
    Empty,
    /// Port 0 cannot be connected to.
    #[error("port 0 is not a valid destination port")]
    PortZero,
}

/// Errors resolving a target against discovery results.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ResolveError {
    /// Nothing matched.
    #[error("no nearby device matches the target")]
    NotFound,
    /// More than one device (or trust entry) matched.
    #[error("the target is ambiguous; it matches: {}", .0.join(", "))]
    Ambiguous(Vec<String>),
    /// The target kind cannot be resolved here.
    #[error("the target cannot be resolved from discovery results")]
    NotDiscoverable,
}

/// The result of resolving a discovery target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved<'a> {
    /// Candidates to try, in advertisement order (rank them before use).
    pub candidates: Vec<ConnectionCandidate>,
    /// The matched discovered peer.
    pub peer: &'a DiscoveredPeer,
}

/// Resolves a fingerprint or name target against discovered peers.
///
/// # Errors
/// [`ResolveError::NotFound`] if nothing matches,
/// [`ResolveError::Ambiguous`] if several devices match, and
/// [`ResolveError::NotDiscoverable`] for address and host targets.
pub fn resolve_target<'a>(
    peers: &'a [DiscoveredPeer],
    target: &Target,
) -> Result<Resolved<'a>, ResolveError> {
    let matches: Vec<&DiscoveredPeer> = match target {
        Target::Fingerprint(prefix) => {
            let head = prefix
                .as_str()
                .get(..FINGERPRINT_HINT_LEN)
                .unwrap_or_default();
            peers
                .iter()
                .filter(|p| p.fp_hint.as_deref() == Some(head))
                .collect()
        }
        Target::Name(name) => {
            let wanted = DeviceName::sanitize(name).as_str().to_lowercase();
            peers
                .iter()
                .filter(|p| p.name.as_str().to_lowercase() == wanted)
                .collect()
        }
        Target::Addr(_) | Target::Host { .. } => return Err(ResolveError::NotDiscoverable),
    };
    match matches.as_slice() {
        [] => Err(ResolveError::NotFound),
        [peer] => Ok(Resolved {
            candidates: peer
                .addrs
                .iter()
                .map(|ip| ConnectionCandidate {
                    addr: SocketAddr::new(*ip, peer.port),
                    source: CandidateSource::Mdns,
                })
                .collect(),
            peer,
        }),
        many => Err(ResolveError::Ambiguous(
            many.iter()
                .map(|p| {
                    format!(
                        "{} ({})",
                        p.name,
                        p.fp_hint.as_deref().unwrap_or("no fingerprint hint")
                    )
                })
                .collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use super::*;

    fn peer(instance: &str, name: &str, hint: Option<&str>) -> DiscoveredPeer {
        DiscoveredPeer {
            instance: instance.into(),
            name: DeviceName::sanitize(name),
            fp_hint: hint.map(Into::into),
            addrs: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 7))],
            port: 47321,
            versions: vec![1],
        }
    }

    #[test]
    fn parse_targets() {
        assert!(matches!(
            Target::parse("192.168.1.2:47321"),
            Ok(Target::Addr(_))
        ));
        assert!(matches!(Target::parse("[::1]:9"), Ok(Target::Addr(_))));
        assert_eq!(
            Target::parse("laptop.local:47321"),
            Ok(Target::Host {
                host: "laptop.local".into(),
                port: 47321
            })
        );
        assert!(matches!(
            Target::parse("abcd-efgh"),
            Ok(Target::Fingerprint(_))
        ));
        assert_eq!(
            Target::parse("Alice's Mac"),
            Ok(Target::Name("Alice's Mac".into()))
        );
        assert_eq!(Target::parse("  "), Err(TargetParseError::Empty));
        assert_eq!(
            Target::parse("127.0.0.1:0"),
            Err(TargetParseError::PortZero)
        );
        assert!(matches!(Target::parse("Dropumi-AB12"), Ok(Target::Name(_))));
    }

    #[test]
    fn resolve_target_ambiguous() {
        let peers = vec![
            peer("a", "Office PC", Some("ABCDEFGH")),
            peer("b", "office pc", Some("ZZZZZZZZ")),
            peer("c", "Laptop", Some("ABCDEFGH")),
        ];
        assert!(matches!(
            resolve_target(&peers, &Target::Name("OFFICE PC".into())),
            Err(ResolveError::Ambiguous(v)) if v.len() == 2
        ));
        let fp = Target::parse("abcdefghij").unwrap();
        assert!(matches!(
            resolve_target(&peers, &fp),
            Err(ResolveError::Ambiguous(_))
        ));
        let r = resolve_target(&peers, &Target::parse("zzzzzzzz").unwrap()).unwrap();
        assert_eq!(r.peer.instance, "b");
        assert_eq!(r.candidates.len(), 1);
        assert_eq!(r.candidates[0].source, CandidateSource::Mdns);
        assert_eq!(
            resolve_target(&peers, &Target::Name("Nobody".into())),
            Err(ResolveError::NotFound)
        );
        assert_eq!(
            resolve_target(&peers, &Target::parse("10.0.0.1:1").unwrap()),
            Err(ResolveError::NotDiscoverable)
        );
    }
}
