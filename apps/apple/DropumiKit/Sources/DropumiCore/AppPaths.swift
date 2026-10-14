// SPDX-License-Identifier: MPL-2.0
//
// Where the app keeps its configuration, received files and staged copies.
//
// - Configuration (identity, trust store): `Application Support/Dropumi`
//   inside the app's own container, excluded from backups so a restored
//   backup never clones the identity (ADR-0014). Rust creates it 0700.
// - Received files: iOS `Documents/Received` (shown in Files); macOS the
//   folder the user chose, by default `~/Downloads/Dropumi`.
// - Outgoing copies (Photos, drops, the share inbox): `tmp/outgoing`.

import Foundation

public struct AppPaths: Sendable {
    public let configDir: URL
    public let receiveDir: URL
    public let outgoingDir: URL
    public let historyFile: URL

    public init(configDir: URL, receiveDir: URL, outgoingDir: URL, historyFile: URL) {
        self.configDir = configDir
        self.receiveDir = receiveDir
        self.outgoingDir = outgoingDir
        self.historyFile = historyFile
    }

    /// The app's standard locations. `receiveDir` overrides the default
    /// receive folder (macOS, a folder the user chose).
    public static func standard(receiveDir: URL? = nil, fileManager: FileManager = .default) throws -> AppPaths {
        let support = try fileManager.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let config = support.appendingPathComponent("Dropumi", isDirectory: true)
        let receive = try receiveDir ?? defaultReceiveDir(fileManager: fileManager)
        let outgoing = fileManager.temporaryDirectory.appendingPathComponent("outgoing", isDirectory: true)
        return AppPaths(
            configDir: config,
            receiveDir: receive,
            outgoingDir: outgoing,
            historyFile: support.appendingPathComponent("history.json")
        )
    }

    /// Creates the folders the app needs. The configuration folder is left
    /// to Rust (which creates it 0700 and checks it); only its parent is
    /// prepared, and the folder is excluded from backups once it exists.
    public func prepare(fileManager: FileManager = .default) throws {
        try fileManager.createDirectory(at: configDir.deletingLastPathComponent(), withIntermediateDirectories: true)
        try fileManager.createDirectory(at: receiveDir, withIntermediateDirectories: true)
        try fileManager.createDirectory(at: outgoingDir, withIntermediateDirectories: true)
    }

    /// Marks the configuration folder as excluded from backups (iCloud and
    /// computer backups) and protected until first unlock.
    public func protectConfig(fileManager: FileManager = .default) {
        var url = configDir
        guard fileManager.fileExists(atPath: url.path) else { return }
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try? url.setResourceValues(values)
        #if os(iOS)
        try? fileManager.setAttributes(
            [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: url.path)
        #endif
    }

    /// Deletes staged outgoing copies (after a transfer, and at launch).
    public func clearOutgoing(fileManager: FileManager = .default) {
        try? fileManager.removeItem(at: outgoingDir)
        try? fileManager.createDirectory(at: outgoingDir, withIntermediateDirectories: true)
    }

    static func defaultReceiveDir(fileManager: FileManager) throws -> URL {
        #if os(iOS)
        let documents = try fileManager.url(
            for: .documentDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        return documents.appendingPathComponent("Received", isDirectory: true)
        #else
        // In the sandbox this is a link from the container to the real
        // Downloads folder; use the real path.
        let downloads = try fileManager.url(
            for: .downloadsDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        return downloads.resolvingSymlinksInPath().appendingPathComponent("Dropumi", isDirectory: true)
        #endif
    }

    /// Temporary, isolated paths for tests and harness runs.
    public static func temporary(fileManager: FileManager = .default) throws -> AppPaths {
        let root = fileManager.temporaryDirectory
            .appendingPathComponent("dropumi-test-\(UUID().uuidString)", isDirectory: true)
        try fileManager.createDirectory(at: root, withIntermediateDirectories: true)
        return AppPaths(
            configDir: root.appendingPathComponent("config", isDirectory: true),
            receiveDir: root.appendingPathComponent("inbox", isDirectory: true),
            outgoingDir: root.appendingPathComponent("outgoing", isDirectory: true),
            historyFile: root.appendingPathComponent("history.json")
        )
    }
}
