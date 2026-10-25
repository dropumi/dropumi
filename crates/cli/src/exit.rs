// SPDX-License-Identifier: MPL-2.0
//! Process exit codes.
//!
//! | Code | Meaning |
//! |---|---|
//! | 0 | success |
//! | 1 | internal error |
//! | 2 | usage, invalid target, mDNS not available |
//! | 3 | rejected |
//! | 4 | cancelled |
//! | 5 | network: connection lost or refused, timeout, busy |
//! | 6 | authentication or trust failure |
//! | 7 | incompatible protocol version |
//! | 8 | integrity failure, or completed with failures |
//! | 9 | local storage or configuration (including insecure permissions) |
//! | 10 | the peer violated the protocol or reported an error |
//! | 130 | interrupted (Ctrl-C) |

use std::process::ExitCode;

use dropumi_application::events::TransferOutcome;
use dropumi_application::usecases::UseCaseError;
use dropumi_domain::error::TransferError;

/// Exit codes.
pub mod code {
    pub const OK: u8 = 0;
    pub const INTERNAL: u8 = 1;
    pub const USAGE: u8 = 2;
    pub const REJECTED: u8 = 3;
    pub const CANCELLED: u8 = 4;
    pub const NETWORK: u8 = 5;
    pub const AUTH: u8 = 6;
    pub const VERSION: u8 = 7;
    pub const INTEGRITY: u8 = 8;
    pub const LOCAL: u8 = 9;
    pub const PEER: u8 = 10;
    pub const INTERRUPTED: u8 = 130;
}

/// Exit code for a transfer error.
#[must_use]
pub fn for_error(e: &TransferError) -> u8 {
    match e.code() {
        "incompatible_version" => code::VERSION,
        "handshake_failed"
        | "peer_identity_mismatch"
        | "unverified_peer"
        | "not_authorized"
        | "pairing_rejected"
        | "sas_rejected" => code::AUTH,
        "protocol_violation" | "limit_exceeded" | "remote_error" => code::PEER,
        "integrity" => code::INTEGRITY,
        "storage" | "insufficient_space" | "source_changed" => code::LOCAL,
        "connection_lost" | "timeout" | "busy" => code::NETWORK,
        _ => code::INTERNAL,
    }
}

/// Exit code for a transfer outcome.
#[must_use]
pub fn for_outcome(o: &TransferOutcome) -> u8 {
    match o {
        TransferOutcome::Completed { .. } => code::OK,
        TransferOutcome::CompletedWithFailures { .. } => code::INTEGRITY,
        TransferOutcome::Rejected { .. } => code::REJECTED,
        TransferOutcome::Cancelled { .. } => code::CANCELLED,
        TransferOutcome::Failed { error, .. } => for_error(error),
    }
}

/// Exit code for a use-case error.
#[must_use]
pub fn for_usecase(e: &UseCaseError) -> u8 {
    match e {
        UseCaseError::Usage(_)
        | UseCaseError::Resolve(_)
        | UseCaseError::DiscoveryUnavailable(_) => code::USAGE,
        UseCaseError::Store(_) => code::LOCAL,
        UseCaseError::Transfer(t) => for_error(t),
        _ => code::INTERNAL,
    }
}

/// Converts to a process exit code.
#[must_use]
pub fn exit(code: u8) -> ExitCode {
    ExitCode::from(code)
}

#[cfg(test)]
mod tests {
    use dropumi_domain::error::Phase;
    use dropumi_domain::transfer::{CancelReason, Party, RejectReason, TransferId};

    use super::*;

    #[test]
    fn exit_code_mapping_table() {
        let id = TransferId::from_bytes([0; 16]);
        let cases: Vec<(TransferError, u8)> = vec![
            (TransferError::IncompatibleVersion { peer: vec![] }, 7),
            (TransferError::HandshakeFailed, 6),
            (TransferError::PeerIdentityMismatch, 6),
            (TransferError::UnverifiedPeer, 6),
            (TransferError::NotAuthorized, 6),
            (TransferError::PairingRejected, 6),
            (TransferError::SasRejected, 6),
            (TransferError::ProtocolViolation("x"), 10),
            (TransferError::LimitExceeded("x"), 10),
            (TransferError::Remote { code: 1 }, 10),
            (TransferError::Integrity, 8),
            (TransferError::Storage("x".into()), 9),
            (TransferError::InsufficientSpace, 9),
            (TransferError::ConnectionLost, 5),
            (TransferError::Timeout(Phase::Write), 5),
            (TransferError::Busy, 5),
            (TransferError::Internal("x".into()), 1),
        ];
        for (e, c) in cases {
            assert_eq!(for_error(&e), c, "{e:?}");
        }
        assert_eq!(
            for_outcome(&TransferOutcome::Rejected {
                transfer_id: id,
                reason: RejectReason::Declined
            }),
            3
        );
        assert_eq!(
            for_outcome(&TransferOutcome::Cancelled {
                transfer_id: None,
                by: Party::Peer,
                reason: CancelReason::Storage
            }),
            4
        );
        assert_eq!(
            for_outcome(&TransferOutcome::CompletedWithFailures {
                transfer_id: id,
                files_stored: 1,
                failed: 1,
                bytes: 0,
                elapsed_ms: 0
            }),
            8
        );
        assert_eq!(for_usecase(&UseCaseError::Usage("x".into())), 2);
    }
}
