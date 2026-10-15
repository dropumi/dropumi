// SPDX-License-Identifier: MPL-2.0
//
// Finished transfers, kept on this device only (never synced, never sent)
// and cleared by the user.

import DropumiBindings
import Foundation

public struct HistoryEntry: Codable, Equatable, Identifiable, Sendable {
    public enum Outcome: String, Codable, Sendable {
        case completed, partial, rejected, cancelled, failed
    }

    public let id: String
    public let outgoing: Bool
    /// The peer as shown when it ended (alias first).
    public let peerName: String
    public let peerVerified: Bool
    public let fileNames: [String]
    public let totalBytes: UInt64
    public let outcome: Outcome
    /// The headline shown for the outcome.
    public let summary: String
    public let finishedAt: Date
    /// For received files: their final names in the receive folder.
    public let savedAs: [String]

    public init(
        id: String, outgoing: Bool, peerName: String, peerVerified: Bool, fileNames: [String],
        totalBytes: UInt64, outcome: Outcome, summary: String, finishedAt: Date, savedAs: [String]
    ) {
        self.id = id
        self.outgoing = outgoing
        self.peerName = peerName
        self.peerVerified = peerVerified
        self.fileNames = fileNames
        self.totalBytes = totalBytes
        self.outcome = outcome
        self.summary = summary
        self.finishedAt = finishedAt
        self.savedAs = savedAs
    }

    static func outcome(of status: TransferStatus) -> Outcome {
        switch status {
        case .completed: return .completed
        case .completedWithFailures: return .partial
        case .rejected: return .rejected
        case .cancelled: return .cancelled
        case .failed: return .failed
        }
    }
}

/// A JSON file with at most `limit` entries, newest first.
public struct HistoryStore: Sendable {
    public let file: URL
    public let limit: Int

    public init(file: URL, limit: Int = 200) {
        self.file = file
        self.limit = limit
    }

    public func load() -> [HistoryEntry] {
        guard let data = try? Data(contentsOf: file),
              let entries = try? JSONDecoder.history.decode([HistoryEntry].self, from: data)
        else { return [] }
        return Array(entries.prefix(limit))
    }

    public func save(_ entries: [HistoryEntry]) {
        let capped = Array(entries.prefix(limit))
        guard let data = try? JSONEncoder.history.encode(capped) else { return }
        try? FileManager.default.createDirectory(
            at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        #if os(iOS)
        try? data.write(to: file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
        #else
        try? data.write(to: file, options: [.atomic])
        #endif
    }
}

extension JSONEncoder {
    static let history: JSONEncoder = {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        return e
    }()
}

extension JSONDecoder {
    static let history: JSONDecoder = {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return d
    }()
}
