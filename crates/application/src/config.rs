// SPDX-License-Identifier: MPL-2.0
//! Timeouts, admission limits and use-case requests.

use std::path::PathBuf;
use std::time::Duration;

use dropumi_domain::discovery::Target;
use dropumi_domain::identity::FingerprintPrefix;
use dropumi_domain::policy::TransferPolicy;

/// Every timeout in one place (protocol v1 defaults).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    /// TCP connect, per candidate.
    pub connect: Duration,
    /// Preamble, handshake and both Hellos.
    pub handshake: Duration,
    /// Receiver waiting for the offer from a trusted peer.
    pub offer_verified: Duration,
    /// Receiver waiting for the offer from an unverified peer (covers the
    /// sender's SAS confirmation).
    pub offer_unverified: Duration,
    /// Receiving user's decision.
    pub consent: Duration,
    /// Sender waiting for Accept/Reject.
    pub decision: Duration,
    /// SAS confirmation and the peer's pairing result.
    pub sas: Duration,
    /// Receiver idle (no data) while a transfer is running.
    pub idle: Duration,
    /// A pending write making no progress.
    pub write_stall: Duration,
    /// Sender waiting for CompleteAck.
    pub complete_ack: Duration,
    /// Draining after a Cancel or Error.
    pub drain: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(3),
            handshake: Duration::from_secs(10),
            offer_verified: Duration::from_secs(30),
            offer_unverified: Duration::from_secs(150),
            consent: Duration::from_secs(120),
            decision: Duration::from_secs(150),
            sas: Duration::from_secs(120),
            idle: Duration::from_secs(30),
            write_stall: Duration::from_secs(120),
            complete_ack: Duration::from_secs(60),
            drain: Duration::from_secs(2),
        }
    }
}

/// Connection admission limits for a receiver.
///
/// One authenticated session per peer fingerprint is not configurable: protocol
/// v1 §17.2 fixes it at 1, and [`Admission`](crate::usecases::admission::Admission)
/// always enforces it (a second session from the same fingerprint gets `BUSY`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionLimits {
    /// Unauthenticated connections in total.
    pub preauth_global: usize,
    /// Unauthenticated connections per remote IP.
    pub preauth_per_ip: usize,
    /// Authenticated sessions in total.
    pub sessions: usize,
}

impl Default for AdmissionLimits {
    fn default() -> Self {
        Self {
            preauth_global: 16,
            preauth_per_ip: 4,
            sessions: 4,
        }
    }
}

/// What to send and to whom.
#[derive(Debug, Clone)]
pub struct SendRequest {
    /// One target, or several address targets tried in order.
    pub targets: Vec<Target>,
    /// Files to send.
    pub paths: Vec<PathBuf>,
    /// Extra identity pin (`--expect-fingerprint`).
    pub expect: Option<FingerprintPrefix>,
    /// Allow an unverified receiver after local SAS confirmation.
    pub allow_unverified: bool,
    /// Optional bandwidth cap in bytes per second (testing and pacing).
    pub throttle_bps: Option<u64>,
    /// How long to browse for name or fingerprint targets.
    pub discover_timeout: Duration,
    /// Accept discovered loopback addresses.
    pub allow_loopback: bool,
}

/// Receiver behaviour.
#[derive(Debug, Clone, Copy)]
pub struct ReceiveConfig {
    /// Authorization and consent policy.
    pub policy: TransferPolicy,
    /// Stop after the first session that delivered an offer.
    pub once: bool,
}
