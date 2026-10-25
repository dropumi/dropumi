// SPDX-License-Identifier: MPL-2.0
//! Who may connect, and whether an incoming offer needs the user's consent.

use crate::peer::{Intent, TrustLevel};
use crate::time::Timestamp;

/// Receiver discoverability and authorization mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyMode {
    /// Accept nothing.
    Off,
    /// Only trusted devices may send (default).
    TrustedOnly,
    /// Any authenticated device may offer files until `until`; unverified
    /// offers always require consent with SAS display.
    Everyone {
        /// When the mode reverts to [`PolicyMode::TrustedOnly`].
        until: Timestamp,
    },
}

/// What happens to an authorized offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcceptMode {
    /// Ask the user for every offer.
    Prompt,
    /// Accept offers from trusted devices automatically; ask for others.
    AutoTrusted,
    /// Reject every offer without asking.
    Never,
}

/// A receiver's transfer policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferPolicy {
    /// Authorization mode.
    pub mode: PolicyMode,
    /// Consent mode.
    pub accept: AcceptMode,
}

impl Default for TransferPolicy {
    fn default() -> Self {
        Self {
            mode: PolicyMode::TrustedOnly,
            accept: AcceptMode::Prompt,
        }
    }
}

impl TransferPolicy {
    /// The effective mode at `now` (an expired Everyone window reverts).
    #[must_use]
    pub fn effective_mode(&self, now: Timestamp) -> PolicyMode {
        match self.mode {
            PolicyMode::Everyone { until } if now >= until => PolicyMode::TrustedOnly,
            m => m,
        }
    }
}

/// Why a session is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    /// The receiver is off.
    ReceiverOff,
    /// The peer is not trusted and the receiver is in trusted-only mode.
    NotTrusted,
    /// A pairing request arrived while no pairing window is open.
    PairingClosed,
}

/// Result of [`authorize`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorization {
    /// The session may proceed.
    Allow {
        /// Whether the peer is trusted.
        verified: bool,
    },
    /// The session is refused.
    Deny(DenyReason),
}

/// Decides whether an authenticated peer may open a session.
///
/// | Mode / intent | Trusted | Unverified |
/// |---|---|---|
/// | Off | deny | deny |
/// | Pair, window open | allow (verified) | allow (unverified) |
/// | Pair, window closed | deny | deny |
/// | Transfer, TrustedOnly | allow | deny |
/// | Transfer, Everyone (not expired) | allow | allow (unverified) |
#[must_use]
pub fn authorize(
    policy: &TransferPolicy,
    now: Timestamp,
    trust: TrustLevel,
    intent: Intent,
    pairing_window_open: bool,
) -> Authorization {
    let verified = trust.is_trusted();
    match intent {
        Intent::Pair => {
            if pairing_window_open {
                Authorization::Allow { verified }
            } else {
                Authorization::Deny(DenyReason::PairingClosed)
            }
        }
        Intent::Transfer => match policy.effective_mode(now) {
            PolicyMode::Off => Authorization::Deny(DenyReason::ReceiverOff),
            PolicyMode::TrustedOnly if verified => Authorization::Allow { verified },
            PolicyMode::TrustedOnly => Authorization::Deny(DenyReason::NotTrusted),
            PolicyMode::Everyone { .. } => Authorization::Allow { verified },
        },
    }
}

/// How an authorized offer is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Consent {
    /// Accept without asking.
    AutoAccept,
    /// Ask the user.
    Ask,
    /// Reject without asking.
    AutoReject,
}

/// Decides how an authorized offer is handled.
///
/// Invariant: an unverified peer never yields [`Consent::AutoAccept`].
#[must_use]
pub fn consent(verified: bool, accept: AcceptMode) -> Consent {
    match accept {
        AcceptMode::Never => Consent::AutoReject,
        AcceptMode::AutoTrusted if verified => Consent::AutoAccept,
        AcceptMode::AutoTrusted | AcceptMode::Prompt => Consent::Ask,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODES: [PolicyMode; 4] = [
        PolicyMode::Off,
        PolicyMode::TrustedOnly,
        PolicyMode::Everyone {
            until: Timestamp(100),
        },
        PolicyMode::Everyone {
            until: Timestamp(10),
        },
    ];

    #[test]
    fn authorize_matrix_exhaustive() {
        let now = Timestamp(50);
        for mode in MODES {
            for accept in [
                AcceptMode::Prompt,
                AcceptMode::AutoTrusted,
                AcceptMode::Never,
            ] {
                let policy = TransferPolicy { mode, accept };
                for trust in [TrustLevel::Trusted, TrustLevel::Unverified] {
                    for intent in [Intent::Transfer, Intent::Pair] {
                        for window in [false, true] {
                            let got = authorize(&policy, now, trust, intent, window);
                            let verified = trust.is_trusted();
                            let expected = match (intent, policy.effective_mode(now)) {
                                (Intent::Pair, _) if window => Authorization::Allow { verified },
                                (Intent::Pair, _) => Authorization::Deny(DenyReason::PairingClosed),
                                (Intent::Transfer, PolicyMode::Off) => {
                                    Authorization::Deny(DenyReason::ReceiverOff)
                                }
                                (Intent::Transfer, PolicyMode::TrustedOnly) if verified => {
                                    Authorization::Allow { verified }
                                }
                                (Intent::Transfer, PolicyMode::TrustedOnly) => {
                                    Authorization::Deny(DenyReason::NotTrusted)
                                }
                                (Intent::Transfer, PolicyMode::Everyone { .. }) => {
                                    Authorization::Allow { verified }
                                }
                            };
                            assert_eq!(got, expected, "{mode:?} {trust:?} {intent:?} {window}");
                        }
                    }
                }
            }
        }
        // Expired Everyone window behaves as TrustedOnly.
        let expired = TransferPolicy {
            mode: PolicyMode::Everyone {
                until: Timestamp(10),
            },
            accept: AcceptMode::Prompt,
        };
        assert_eq!(
            authorize(
                &expired,
                Timestamp(10),
                TrustLevel::Unverified,
                Intent::Transfer,
                false
            ),
            Authorization::Deny(DenyReason::NotTrusted)
        );
        assert_eq!(
            authorize(
                &expired,
                Timestamp(9),
                TrustLevel::Unverified,
                Intent::Transfer,
                false
            ),
            Authorization::Allow { verified: false }
        );
    }

    #[test]
    fn unverified_never_auto_accepted() {
        for accept in [
            AcceptMode::Prompt,
            AcceptMode::AutoTrusted,
            AcceptMode::Never,
        ] {
            assert_ne!(consent(false, accept), Consent::AutoAccept);
        }
        assert_eq!(consent(true, AcceptMode::AutoTrusted), Consent::AutoAccept);
        assert_eq!(consent(true, AcceptMode::Prompt), Consent::Ask);
        assert_eq!(consent(true, AcceptMode::Never), Consent::AutoReject);
        assert_eq!(consent(false, AcceptMode::AutoTrusted), Consent::Ask);
    }
}
