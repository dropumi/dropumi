// SPDX-License-Identifier: MPL-2.0

import Foundation
import Testing
@testable import DropumiShare

private func tempDir() throws -> URL {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent("dropumi-inbox-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
}

private func file(_ dir: URL, _ name: String, _ bytes: Int) throws -> URL {
    let url = dir.appendingPathComponent(name)
    try Data(repeating: 7, count: bytes).write(to: url)
    return url
}

@Suite struct ShareInboxTests {
    @Test func committedBatchIsVisibleWithItsFiles() throws {
        let root = try tempDir(), source = try tempDir()
        let inbox = ShareInbox(root: root)
        let batch = try inbox.beginBatch()
        try batch.add(copyOf: try file(source, "photo.heic", 10))
        try batch.add(copyOf: try file(source, "notes.txt", 3))
        _ = try batch.commit()
        let pending = inbox.pendingBatches()
        #expect(pending.count == 1)
        #expect(pending[0].manifest.items.map(\.name) == ["photo.heic", "notes.txt"])
        #expect(pending[0].manifest.items.map(\.size) == [10, 3])
    }

    @Test func stagingBatchIsNeverOffered() throws {
        let root = try tempDir(), source = try tempDir()
        let inbox = ShareInbox(root: root)
        let batch = try inbox.beginBatch()
        try batch.add(copyOf: try file(source, "big.mov", 100))
        // Not committed: the extension was killed mid-copy.
        #expect(inbox.pendingBatches().isEmpty)
    }

    @Test func truncatedFileRejectsTheBatch() throws {
        let root = try tempDir(), source = try tempDir()
        let inbox = ShareInbox(root: root)
        let batch = try inbox.beginBatch()
        try batch.add(copyOf: try file(source, "video.mov", 1_000))
        _ = try batch.commit()
        let pending = try #require(inbox.pendingBatches().first)
        // Shorter than the manifest says.
        try Data(repeating: 1, count: 10).write(to: pending.files[0])
        #expect(inbox.pendingBatches().isEmpty)
        #expect(!FileManager.default.fileExists(atPath: pending.folder.path), "the bad batch is deleted")
    }

    @Test func manifestNamingAnotherFolderIsRejected() throws {
        let root = try tempDir()
        let inbox = ShareInbox(root: root)
        let folder = root.appendingPathComponent("forged")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let manifest = ShareManifest(createdAt: Date(), items: [ShareItem(name: "../escape", size: 1)])
        try JSONEncoder.inbox.encode(manifest).write(to: folder.appendingPathComponent("manifest.json"))
        #expect(inbox.pendingBatches().isEmpty)
    }

    @Test func namesAreSafeAndUnique() throws {
        let root = try tempDir(), source = try tempDir()
        let inbox = ShareInbox(root: root)
        let batch = try inbox.beginBatch()
        let a = try batch.add(copyOf: try file(source, "a.txt", 1), suggestedName: "../../etc/passwd")
        let b = try batch.add(copyOf: try file(source, "b.txt", 1), suggestedName: "Report.PDF")
        let c = try batch.add(copyOf: try file(source, "c.txt", 1), suggestedName: "report.pdf")
        let d = try batch.add(copyOf: try file(source, "d.txt", 1), suggestedName: ".hidden")
        #expect(a.name == "passwd")
        #expect(b.name == "Report.PDF")
        #expect(c.name == "report (1).pdf")
        #expect(d.name == "hidden")
    }

    @Test func takeMovesTheBatchOutOfTheSharedContainer() throws {
        let root = try tempDir(), source = try tempDir(), own = try tempDir()
        let inbox = ShareInbox(root: root)
        let batch = try inbox.beginBatch()
        try batch.add(copyOf: try file(source, "x.bin", 5))
        _ = try batch.commit()
        let pending = try #require(inbox.pendingBatches().first)
        let files = try inbox.take(pending, into: own.appendingPathComponent("outgoing"))
        #expect(files.count == 1)
        #expect(FileManager.default.fileExists(atPath: files[0].path))
        #expect(inbox.pendingBatches().isEmpty)
    }

    @Test func emptyBatchCannotBeCommitted() throws {
        let inbox = ShareInbox(root: try tempDir())
        let batch = try inbox.beginBatch()
        #expect(throws: ShareInboxError.emptyBatch) { try batch.commit() }
    }

    @Test func staleStagingFoldersAreRemoved() throws {
        let root = try tempDir(), source = try tempDir()
        let inbox = ShareInbox(root: root)
        let batch = try inbox.beginBatch()
        try batch.add(copyOf: try file(source, "x.bin", 5))
        inbox.removeStale(now: Date().addingTimeInterval(7_200))
        let left = try FileManager.default.contentsOfDirectory(atPath: root.path)
        #expect(left.isEmpty)
    }
}
