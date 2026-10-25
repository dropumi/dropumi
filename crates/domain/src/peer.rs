// SPDX-License-Identifier: MPL-2.0
//! Authenticated peers and their capabilities.

use crate::identity::DeviceIdentity;

/// Whether a peer's key is in the local trust store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrustLevel {
    /// The peer's fingerprint is in the trust store.
    Trusted,
    /// The peer authenticated a key that is not in the trust store.
    Unverified,
}

impl TrustLevel {
    /// `true` for [`TrustLevel::Trusted`].
    #[must_use]
    pub const fn is_trusted(self) -> bool {
        matches!(self, Self::Trusted)
    }
}

/// Why a session was opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Intent {
    /// Send files.
    Transfer,
    /// Pair devices (exchange trust after SAS confirmation).
    Pair,
}

impl Intent {
    /// Wire code.
    #[must_use]
    pub const fn code(self) -> u64 {
        match self {
            Self::Transfer => 1,
            Self::Pair => 2,
        }
    }

    /// Decodes a wire code.
    #[must_use]
    pub const fn from_code(code: u64) -> Option<Self> {
        match code {
            1 => Some(Self::Transfer),
            2 => Some(Self::Pair),
            _ => None,
        }
    }
}

/// Capabilities a peer declared in its authenticated Hello.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PeerCapabilities {
    /// Negotiated protocol version.
    pub protocol_version: u16,
    /// Optional feature ids (protocol v1 defines none).
    pub features: Vec<u16>,
    /// Supported file hash algorithm ids (1 = BLAKE3-256).
    pub hash_algs: Vec<u16>,
}

/// An authenticated remote device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    /// Authenticated key plus the peer's self-declared name.
    pub identity: DeviceIdentity,
    /// Local trust decision for the key.
    pub trust: TrustLevel,
    /// Declared capabilities.
    pub capabilities: PeerCapabilities,
}
