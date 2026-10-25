// SPDX-License-Identifier: MPL-2.0

import DropumiBindings
import Foundation

/// The core's transfer progress (not Foundation's `Progress`).
public typealias TransferProgress = DropumiBindings.Progress

/// Where a transfer stands. Only `.finished` with `TransferStatus.completed`
/// is a success; everything before it is in progress.
public enum TransferPhase: Equatable, Sendable {
    /// Sending: connecting and checking the other device.
    case connecting
    /// Sending: the offer was sent; the other person has not answered.
    case waitingForAcceptance
    /// Receiving: this device's user has not answered yet.
    case waitingForAnswer
    /// Data is moving.
    case transferring
    /// It ended.
    case finished(TransferResult)
}

/// One stored or failed file of a transfer.
public struct FileResult: Equatable, Sendable {
    public let index: UInt32
    public let status: FileStatus
    /// For received files: the final name in the receive folder.
    public let savedAs: String?
    /// For received files: the BLAKE3 digest (hex).
    public let blake3: String?
}

/// One transfer, observed by its card. Progress updates re-render only this
/// object's views.
@MainActor
@Observable
public final class TransferRecord: Identifiable {
    /// `out-<operation>` or `in-<transfer id>`.
    public let id: String
    public let direction: Direction
    public let operation: UInt64?
    public var transferId: String?
    /// The authenticated peer, once the channel is up.
    public var peer: PeerInfo?
    /// What the UI calls the peer before authentication (the chosen device).
    public var targetName: String
    public var fileNames: [String]
    public var fileCount: Int
    public var totalBytes: UInt64
    public var progress: TransferProgress?
    public var phase: TransferPhase
    public var sessionCode: String?
    public var fileResults: [FileResult] = []
    public let startedAt: Date
    public var finishedAt: Date?
    private var samples: [(at: Date, bytes: UInt64)] = []

    public init(
        id: String, direction: Direction, operation: UInt64?, targetName: String,
        fileNames: [String], totalBytes: UInt64, phase: TransferPhase, startedAt: Date
    ) {
        self.id = id
        self.direction = direction
        self.operation = operation
        self.targetName = targetName
        self.fileNames = fileNames
        self.fileCount = fileNames.count
        self.totalBytes = totalBytes
        self.phase = phase
        self.startedAt = startedAt
    }

    /// The name to show for the other device: the local alias first, else
    /// the name it claims (to be labelled as a claim), else the chosen name.
    public var peerLabel: PeerLabel {
        if let peer {
            return PeerLabel(peer: peer)
        }
        return PeerLabel(title: targetName, claim: nil, verified: false)
    }

    public var isFinished: Bool {
        if case .finished = phase { return true }
        return false
    }

    /// The result, once finished.
    public var result: TransferResult? {
        if case let .finished(r) = phase { return r }
        return nil
    }

    /// Records progress and keeps a short window of samples for the speed.
    func record(_ progress: TransferProgress, at now: Date) {
        self.progress = progress
        if case .waitingForAcceptance = phase { phase = .transferring }
        if case .waitingForAnswer = phase { phase = .transferring }
        samples.append((now, progress.bytesDone))
        samples.removeAll { now.timeIntervalSince($0.at) > 3 }
    }

    /// Bytes per second over the last few seconds, once two samples exist
    /// at least one second apart.
    public var bytesPerSecond: Double? {
        guard let first = samples.first, let last = samples.last else { return nil }
        let seconds = last.at.timeIntervalSince(first.at)
        guard seconds >= 1, last.bytes >= first.bytes else { return nil }
        return Double(last.bytes - first.bytes) / seconds
    }

    /// Seconds left, once the speed is known.
    public var secondsLeft: Double? {
        guard let speed = bytesPerSecond, speed > 0, let progress else { return nil }
        let left = progress.bytesTotal > progress.bytesDone ? progress.bytesTotal - progress.bytesDone : 0
        return Double(left) / speed
    }

    /// 0...1, or nil before any progress.
    public var fraction: Double? {
        guard let progress, progress.bytesTotal > 0 else { return nil }
        return min(1, Double(progress.bytesDone) / Double(progress.bytesTotal))
    }
}

/// How to show a peer (ADR-0004): the local alias first; a name the peer
/// claims is shown as a claim; unverified peers are labelled.
public struct PeerLabel: Equatable, Sendable {
    /// Alias, or "Unverified device".
    public let title: String
    /// The name the peer claims, when different from the title.
    public let claim: String?
    /// Whether the peer is a paired device.
    public let verified: Bool

    public init(title: String, claim: String?, verified: Bool) {
        self.title = title
        self.claim = claim
        self.verified = verified
    }

    public init(peer: PeerInfo) {
        if peer.trusted, let alias = peer.alias {
            self.title = alias
            self.claim = peer.claimedName == alias ? nil : peer.claimedName
            self.verified = true
        } else if peer.trusted {
            self.title = peer.claimedName
            self.claim = nil
            self.verified = true
        } else {
            self.title = String(localized: "Unverified device")
            self.claim = peer.claimedName
            self.verified = false
        }
    }
}
