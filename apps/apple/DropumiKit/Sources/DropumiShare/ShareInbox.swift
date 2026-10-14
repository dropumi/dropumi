// SPDX-License-Identifier: MPL-2.0
//
// The hand-off between the iOS Share Extension and the app (decision D1):
// the extension never loads the Rust core. It copies the shared items into
// a staging folder in the App Group container, writes a manifest with the
// names and sizes it copied, and only then renames the folder into place.
// The app ignores staging folders and checks every file against its
// manifest before it offers to send it, so a half-copied file (an extension
// killed mid-copy) is never sent.

import Foundation

/// One item listed in a batch's manifest.
public struct ShareItem: Codable, Equatable, Sendable {
    /// File name inside the batch folder (a single path component).
    public let name: String
    /// Size in bytes when it was copied.
    public let size: UInt64

    public init(name: String, size: UInt64) {
        self.name = name
        self.size = size
    }
}

/// The manifest written last, before a batch becomes visible.
public struct ShareManifest: Codable, Equatable, Sendable {
    public static let currentVersion = 1
    public let version: Int
    public let createdAt: Date
    public let items: [ShareItem]

    public init(createdAt: Date, items: [ShareItem]) {
        self.version = Self.currentVersion
        self.createdAt = createdAt
        self.items = items
    }
}

/// A complete batch the app can take.
public struct ShareBatch: Equatable, Sendable {
    public let id: String
    public let folder: URL
    public let manifest: ShareManifest

    /// The files, in manifest order.
    public var files: [URL] {
        manifest.items.map { folder.appendingPathComponent($0.name, isDirectory: false) }
    }
}

public enum ShareInboxError: Error, Equatable, Sendable {
    case noAppGroup
    case emptyBatch
    case tooManyItems
}

/// The inbox folder in the App Group container.
public struct ShareInbox: Sendable {
    /// The App Group shared by the app and the Share Extension.
    public static let appGroup = "group.io.github.dropumi"
    /// Items one share may hand over (the extension's activation rule allows 100).
    public static let maxItems = 100
    static let manifestName = "manifest.json"
    static let stagingPrefix = ".staging-"

    public let root: URL

    public init(root: URL) {
        self.root = root
    }

    /// The inbox in the App Group container, if the App Group is available.
    public static func shared(fileManager: FileManager = .default) -> ShareInbox? {
        guard let container = fileManager.containerURL(forSecurityApplicationGroupIdentifier: appGroup) else {
            return nil
        }
        return ShareInbox(root: container.appendingPathComponent("Inbox", isDirectory: true))
    }

    // MARK: Extension side

    /// Starts a batch in a staging folder the app never reads.
    public func beginBatch(fileManager: FileManager = .default) throws -> StagedBatch {
        let id = UUID().uuidString.lowercased()
        let staging = root.appendingPathComponent(Self.stagingPrefix + id, isDirectory: true)
        try fileManager.createDirectory(at: staging, withIntermediateDirectories: true)
        return StagedBatch(id: id, staging: staging, final: root.appendingPathComponent(id, isDirectory: true))
    }

    // MARK: App side

    /// Complete batches, oldest first. A batch whose files do not match its
    /// manifest (missing, or another size) is deleted and not returned.
    public func pendingBatches(fileManager: FileManager = .default) -> [ShareBatch] {
        guard let entries = try? fileManager.contentsOfDirectory(
            at: root, includingPropertiesForKeys: [.isDirectoryKey], options: [.skipsHiddenFiles])
        else { return [] }
        var batches: [ShareBatch] = []
        for folder in entries where !folder.lastPathComponent.hasPrefix(Self.stagingPrefix) {
            let manifestURL = folder.appendingPathComponent(Self.manifestName)
            guard let data = try? Data(contentsOf: manifestURL),
                  let manifest = try? JSONDecoder.inbox.decode(ShareManifest.self, from: data),
                  manifest.version == ShareManifest.currentVersion,
                  !manifest.items.isEmpty
            else {
                try? fileManager.removeItem(at: folder)
                continue
            }
            let batch = ShareBatch(id: folder.lastPathComponent, folder: folder, manifest: manifest)
            if Self.matchesManifest(batch, fileManager: fileManager) {
                batches.append(batch)
            } else {
                try? fileManager.removeItem(at: folder)
            }
        }
        return batches.sorted { $0.manifest.createdAt < $1.manifest.createdAt }
    }

    /// Moves a batch out of the shared container into a folder the app
    /// owns; returns the files in their new place.
    public func take(_ batch: ShareBatch, into destination: URL, fileManager: FileManager = .default) throws -> [URL] {
        try fileManager.createDirectory(at: destination.deletingLastPathComponent(), withIntermediateDirectories: true)
        if fileManager.fileExists(atPath: destination.path) {
            try fileManager.removeItem(at: destination)
        }
        try fileManager.moveItem(at: batch.folder, to: destination)
        try? fileManager.removeItem(at: destination.appendingPathComponent(Self.manifestName))
        return batch.manifest.items.map { destination.appendingPathComponent($0.name, isDirectory: false) }
    }

    /// Deletes a batch.
    public func discard(_ batch: ShareBatch, fileManager: FileManager = .default) {
        try? fileManager.removeItem(at: batch.folder)
    }

    /// Removes staging folders older than `staleAfter` (an extension that was
    /// killed) and complete batches older than `expireAfter`.
    public func removeStale(
        now: Date = Date(),
        staleAfter: TimeInterval = 3_600,
        expireAfter: TimeInterval = 86_400,
        fileManager: FileManager = .default
    ) {
        guard let entries = try? fileManager.contentsOfDirectory(
            at: root, includingPropertiesForKeys: [.contentModificationDateKey], options: [])
        else { return }
        for folder in entries {
            let modified = (try? folder.resourceValues(forKeys: [.contentModificationDateKey]))?
                .contentModificationDate ?? .distantPast
            let age = now.timeIntervalSince(modified)
            let staging = folder.lastPathComponent.hasPrefix(Self.stagingPrefix)
            if (staging && age > staleAfter) || (!staging && age > expireAfter) {
                try? fileManager.removeItem(at: folder)
            }
        }
    }

    static func matchesManifest(_ batch: ShareBatch, fileManager: FileManager) -> Bool {
        for item in batch.manifest.items {
            guard SafeName.isSingleComponent(item.name) else { return false }
            let url = batch.folder.appendingPathComponent(item.name, isDirectory: false)
            guard let attributes = try? fileManager.attributesOfItem(atPath: url.path),
                  attributes[.type] as? FileAttributeType == .typeRegular,
                  (attributes[.size] as? NSNumber)?.uint64Value == item.size
            else { return false }
        }
        return true
    }
}

/// A batch being written by the extension.
public final class StagedBatch: @unchecked Sendable {
    public let id: String
    let staging: URL
    let final: URL
    private let lock = NSLock()
    private var items: [ShareItem] = []
    private var names: Set<String> = []

    init(id: String, staging: URL, final: URL) {
        self.id = id
        self.staging = staging
        self.final = final
    }

    /// Copies `source` into the batch under a safe, unique name. Call it
    /// while `source` is still valid (inside the item provider's handler).
    @discardableResult
    public func add(copyOf source: URL, suggestedName: String? = nil, fileManager: FileManager = .default) throws -> ShareItem {
        let name = reserveName(SafeName.make(suggestedName ?? source.lastPathComponent))
        let destination = staging.appendingPathComponent(name, isDirectory: false)
        try fileManager.copyItem(at: source, to: destination)
        let size = (try fileManager.attributesOfItem(atPath: destination.path)[.size] as? NSNumber)?.uint64Value ?? 0
        let item = ShareItem(name: name, size: size)
        lock.lock()
        items.append(item)
        lock.unlock()
        return item
    }

    /// Writes the manifest, then makes the batch visible to the app.
    public func commit(now: Date = Date(), fileManager: FileManager = .default) throws -> ShareManifest {
        lock.lock()
        let snapshot = items
        lock.unlock()
        guard !snapshot.isEmpty else {
            abandon(fileManager: fileManager)
            throw ShareInboxError.emptyBatch
        }
        guard snapshot.count <= ShareInbox.maxItems else {
            abandon(fileManager: fileManager)
            throw ShareInboxError.tooManyItems
        }
        let manifest = ShareManifest(createdAt: now, items: snapshot)
        let data = try JSONEncoder.inbox.encode(manifest)
        try data.write(to: staging.appendingPathComponent(ShareInbox.manifestName), options: .atomic)
        try fileManager.moveItem(at: staging, to: final)
        return manifest
    }

    /// Deletes everything copied so far.
    public func abandon(fileManager: FileManager = .default) {
        try? fileManager.removeItem(at: staging)
    }

    private func reserveName(_ base: String) -> String {
        lock.lock()
        defer { lock.unlock() }
        var candidate = base
        var n = 1
        while names.contains(candidate.lowercased()) {
            candidate = SafeName.withSuffix(base, n)
            n += 1
        }
        names.insert(candidate.lowercased())
        return candidate
    }
}

/// File names that are safe as a single path component in the inbox. The
/// Rust core sanitizes again before anything is offered or stored.
enum SafeName {
    static func make(_ raw: String) -> String {
        let last = raw.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? ""
        let cleaned = last.unicodeScalars
            .filter { !CharacterSet.controlCharacters.contains($0) }
            .map(String.init).joined()
            .trimmingCharacters(in: .whitespaces)
        let trimmed = cleaned.hasPrefix(".") ? String(cleaned.drop(while: { $0 == "." })) : cleaned
        let bounded = String(trimmed.prefix(200))
        return bounded.isEmpty || bounded == ".." ? "shared-item" : bounded
    }

    static func withSuffix(_ name: String, _ n: Int) -> String {
        let ext = (name as NSString).pathExtension
        let stem = (name as NSString).deletingPathExtension
        return ext.isEmpty ? "\(stem) (\(n))" : "\(stem) (\(n)).\(ext)"
    }

    static func isSingleComponent(_ name: String) -> Bool {
        !name.isEmpty && name != "." && name != ".." && !name.contains("/") && !name.hasPrefix(".")
    }
}

extension JSONEncoder {
    static let inbox: JSONEncoder = {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        e.outputFormatting = [.sortedKeys]
        return e
    }()
}

extension JSONDecoder {
    static let inbox: JSONDecoder = {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return d
    }()
}
