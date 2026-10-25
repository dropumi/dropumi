// SPDX-License-Identifier: MPL-2.0
//
// Files the user picks or drops. Picked files are read in place while the
// app holds their security-scoped access (released after the transfer);
// Photos items and iPad drops are copied to `tmp/outgoing` first and the
// copy is deleted after the transfer (and at the next launch).

import CoreTransferable
import DropumiAppState
import Foundation
import UniformTypeIdentifiers

/// A copy of a dropped or picked item in `tmp/outgoing/<uuid>/`.
struct StagedCopy: Transferable {
    let url: URL

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .item) { received in
            let folder = FileManager.default.temporaryDirectory
                .appendingPathComponent("outgoing", isDirectory: true)
                .appendingPathComponent(UUID().uuidString, isDirectory: true)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let name = received.file.lastPathComponent.isEmpty ? "file" : received.file.lastPathComponent
            let destination = folder.appendingPathComponent(name, isDirectory: false)
            try FileManager.default.copyItem(at: received.file, to: destination)
            return StagedCopy(url: destination)
        }
    }

    var outgoing: OutgoingFile {
        OutgoingFile(url: url, securityScoped: false, staged: true)
    }
}

enum FileImport {
    /// Files from the file importer: read in place, with security-scoped
    /// access held until the transfer ends. Folders are skipped (protocol
    /// v1 sends files only).
    static func picked(_ urls: [URL]) -> [OutgoingFile] {
        urls.compactMap { url in
            let scoped = url.startAccessingSecurityScopedResource()
            let values = try? url.resourceValues(forKeys: [.isRegularFileKey])
            guard values?.isRegularFile == true else {
                if scoped { url.stopAccessingSecurityScopedResource() }
                return nil
            }
            return OutgoingFile(url: url, securityScoped: scoped, staged: false)
        }
    }
}
