// SPDX-License-Identifier: MPL-2.0

import Foundation
#if os(iOS)
import UIKit
#else
import AppKit
#endif

enum Platform {
    /// Opens the place where the user turns Local Network access on.
    static func openLocalNetworkSettings() {
        #if os(iOS)
        if let url = URL(string: UIApplication.openSettingsURLString) {
            UIApplication.shared.open(url)
        }
        #else
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork") {
            NSWorkspace.shared.open(url)
        }
        #endif
    }

    /// Shows a received file in Files (iOS) or Finder (macOS). `savedAs`
    /// comes from the core; it must be a single name inside `folder`.
    static func reveal(savedAs: String, in folder: URL) {
        guard !savedAs.isEmpty, !savedAs.contains("/"), savedAs != ".", savedAs != ".." else { return }
        let file = folder.appendingPathComponent(savedAs, isDirectory: false)
        guard file.standardizedFileURL.deletingLastPathComponent().path == folder.standardizedFileURL.path else { return }
        #if os(iOS)
        var components = URLComponents()
        components.scheme = "shareddocuments"
        components.path = file.path
        if let url = components.url {
            UIApplication.shared.open(url)
        }
        #else
        NSWorkspace.shared.activateFileViewerSelecting([file])
        #endif
    }

    /// Shows the receive folder.
    static func reveal(folder: URL) {
        #if os(iOS)
        var components = URLComponents()
        components.scheme = "shareddocuments"
        components.path = folder.path
        if let url = components.url {
            UIApplication.shared.open(url)
        }
        #else
        NSWorkspace.shared.open(folder)
        #endif
    }
}
