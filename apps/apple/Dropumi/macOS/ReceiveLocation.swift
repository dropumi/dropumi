// SPDX-License-Identifier: MPL-2.0
//
// The receive folder on the Mac: ~/Downloads/Dropumi by default, or a folder
// the user chose, kept as a security-scoped bookmark so the sandboxed app
// can write there after a relaunch.

import AppKit
import Foundation

enum ReceiveLocation {
    private static let key = "receiveFolderBookmark"

    /// The chosen folder, with access started; nil for the default.
    static func restore() -> URL? {
        guard let data = UserDefaults.standard.data(forKey: key) else { return nil }
        var stale = false
        guard let url = try? URL(
            resolvingBookmarkData: data, options: [.withSecurityScope], relativeTo: nil, bookmarkDataIsStale: &stale),
            url.startAccessingSecurityScopedResource()
        else {
            UserDefaults.standard.removeObject(forKey: key)
            return nil
        }
        if stale, let fresh = try? url.bookmarkData(options: [.withSecurityScope]) {
            UserDefaults.standard.set(fresh, forKey: key)
        }
        return url
    }

    /// Asks for a folder and remembers it. Returns nil if cancelled.
    @MainActor
    static func choose() -> URL? {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = String(localized: "Use this folder")
        panel.message = String(localized: "Choose where Dropumi saves received files.")
        guard panel.runModal() == .OK, let url = panel.url,
              let data = try? url.bookmarkData(options: [.withSecurityScope])
        else { return nil }
        UserDefaults.standard.set(data, forKey: key)
        return url
    }

    /// Back to ~/Downloads/Dropumi.
    static func reset() {
        UserDefaults.standard.removeObject(forKey: key)
    }

    /// The user's real home folder (not the sandbox container).
    static var realHome: String {
        guard let entry = getpwuid(getuid()), let dir = entry.pointee.pw_dir else { return NSHomeDirectory() }
        return String(cString: dir)
    }
}

extension URL {
    /// The path with the user's home folder shown as `~`.
    var abbreviatingWithTildeInPath: String {
        let home = ReceiveLocation.realHome
        let path = standardizedFileURL.path
        if path == home { return "~" }
        if path.hasPrefix(home + "/") { return "~" + path.dropFirst(home.count) }
        return path
    }
}
