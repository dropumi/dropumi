// SPDX-License-Identifier: MPL-2.0
//
// One sentence per error, in plain words. Local OS details are shown only
// under "Details" and never logged.

import DropumiBindings
import Foundation

public struct ErrorText: Equatable, Sendable {
    public let title: String
    public let message: String
    /// Whether it signals a possible attack or impersonation.
    public let isSecurity: Bool
}

public enum ErrorCatalog {
    public static func text(for code: ErrorCode) -> ErrorText {
        switch code {
        case .incompatibleVersion:
            return ErrorText(title: String(localized: "Different Dropumi versions"),
                             message: String(localized: "The other device runs a version of Dropumi this one cannot talk to. Update both."),
                             isSecurity: false)
        case .handshakeFailed:
            return ErrorText(title: String(localized: "Secure connection failed"),
                             message: String(localized: "The devices could not set up an encrypted connection. Nothing was sent."),
                             isSecurity: true)
        case .peerIdentityMismatch:
            return ErrorText(title: String(localized: "Not the device you paired with"),
                             message: String(localized: "This device is not the paired device it claims to be. Nothing was sent."),
                             isSecurity: true)
        case .unverifiedPeer:
            return ErrorText(title: String(localized: "Not paired"),
                             message: String(localized: "This device is not paired with yours. Pair first, or send once and compare a code."),
                             isSecurity: false)
        case .notAuthorized:
            return ErrorText(title: String(localized: "Only paired devices"),
                             message: String(localized: "The other device only accepts files from devices it has paired with."),
                             isSecurity: false)
        case .pairingRejected:
            // Both devices get this code, whoever said no, so it blames no one.
            return ErrorText(title: String(localized: "Pairing not confirmed"),
                             message: String(localized: "The code was not confirmed on both devices. Nothing was saved."),
                             isSecurity: false)
        case .sasRejected:
            return ErrorText(title: String(localized: "Codes did not match"),
                             message: String(localized: "The code was not confirmed, so nothing was sent or saved."),
                             isSecurity: true)
        case .protocolViolation:
            return ErrorText(title: String(localized: "Unexpected data"),
                             message: String(localized: "The other device sent something Dropumi does not accept. The transfer stopped."),
                             isSecurity: true)
        case .limitExceeded:
            return ErrorText(title: String(localized: "Too much at once"),
                             message: String(localized: "The other device went past a safety limit. The transfer stopped."),
                             isSecurity: true)
        case .integrity:
            return ErrorText(title: String(localized: "File check failed"),
                             message: String(localized: "A file did not match what was sent and was deleted."),
                             isSecurity: true)
        case .insufficientSpace:
            return ErrorText(title: String(localized: "Not enough space"),
                             message: String(localized: "There is not enough free space for these files."),
                             isSecurity: false)
        case .storage:
            return ErrorText(title: String(localized: "Could not save"),
                             message: String(localized: "A file could not be saved on this device."),
                             isSecurity: false)
        case .sourceChanged:
            return ErrorText(title: String(localized: "A file changed"),
                             message: String(localized: "A file changed while it was being sent, so it was not sent."),
                             isSecurity: false)
        case .connectionLost:
            return ErrorText(title: String(localized: "Connection lost"),
                             message: String(localized: "The connection dropped. Check that both devices are on the same network."),
                             isSecurity: false)
        case .timeout:
            return ErrorText(title: String(localized: "No answer"),
                             message: String(localized: "The other device did not answer in time."),
                             isSecurity: false)
        case .busy:
            return ErrorText(title: String(localized: "Busy"),
                             message: String(localized: "The other device is busy. Try again in a moment."),
                             isSecurity: false)
        case .remoteError:
            return ErrorText(title: String(localized: "The other device stopped"),
                             message: String(localized: "The other device reported a problem and stopped the transfer."),
                             isSecurity: false)
        case .internal:
            return ErrorText(title: String(localized: "Something went wrong"),
                             message: String(localized: "Dropumi hit an unexpected problem. Try again."),
                             isSecurity: false)
        }
    }

    /// The headline for a finished transfer.
    public static func summary(_ result: TransferResult, direction: Direction) -> String {
        switch result.status {
        case .completed:
            return direction == .outgoing
                ? String(localized: "Sent and checked")
                : String(localized: "All files saved and checked")
        case .completedWithFailures:
            return String(localized: "\(result.filesStored) of \(result.filesStored + result.filesFailed) files saved")
        case .rejected:
            switch result.rejectCause {
            case .timeout: return String(localized: "Request expired")
            case .busy: return String(localized: "The device was busy")
            default: return direction == .outgoing ? String(localized: "Declined") : String(localized: "You declined")
            }
        case .cancelled:
            return result.cancelledBy == .local
                ? String(localized: "Cancelled by you")
                : String(localized: "Cancelled by the other device")
        case .failed:
            return text(for: result.error ?? .internal).title
        }
    }

    /// A short message for an error thrown by a node call.
    public static func message(for error: Error) -> String {
        guard let error = error as? DropumiError else {
            return String(localized: "Something went wrong. Try again.")
        }
        switch error {
        case let .InvalidInput(detail), let .NotFound(detail), let .Busy(detail), let .InvalidState(detail):
            return detail
        case .DiscoveryUnavailable:
            return String(localized: "Looking for nearby devices is not available right now.")
        case .Config:
            return String(localized: "Dropumi's settings on this device could not be read. Reset this device's identity in Settings if this keeps happening.")
        case .PromptNotFound:
            return String(localized: "That request is no longer waiting.")
        case .PromptKindMismatch:
            return String(localized: "That answer does not fit this request.")
        case .ShutDown:
            return String(localized: "Dropumi is shutting down.")
        case let .Transfer(code, _):
            return text(for: code).message
        case .Internal:
            return String(localized: "Something went wrong. Try again.")
        }
    }
}
