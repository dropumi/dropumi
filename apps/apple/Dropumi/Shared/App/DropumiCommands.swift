// SPDX-License-Identifier: MPL-2.0
//
// Keyboard shortcuts (Mac menus, iPad hardware keyboards): ⌘1/⌘2/⌘3 for
// the tabs, ⌘O to choose files, ⇧⌘P to pair a device, ⌘, for Settings.

import SwiftUI

struct DropumiCommands: Commands {
    let router: Router

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("Send Files…") {
                router.openSend()
                router.requestFilePicker = true
            }
            .keyboardShortcut("o")
            Button("Pair a Device…") { router.openPairing() }
                .keyboardShortcut("p", modifiers: [.command, .shift])
        }
        // Nothing to print, and ⇧⌘P is Pair a Device….
        CommandGroup(replacing: .printItem) {}
        CommandGroup(replacing: .appSettings) {
            Button("Settings…") { router.tab = .settings }
                .keyboardShortcut(",")
        }
        CommandMenu("Go") {
            Button("Nearby") { router.tab = .nearby }
                .keyboardShortcut("1")
            Button("Transfers") { router.tab = .transfers }
                .keyboardShortcut("2")
            Button("Settings") { router.tab = .settings }
                .keyboardShortcut("3")
        }
    }
}
