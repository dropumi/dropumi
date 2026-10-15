// SPDX-License-Identifier: MPL-2.0
//
// The Mac app: one window plus a menu-bar receiver. Closing the window does
// not quit; Dropumi keeps receiving from the menu bar (the Dock icon goes
// away). "Quit Dropumi completely" in the menu bar stops receiving and
// withdraws the advertisement.

import AppKit
import DropumiAppState
import DropumiDesign
import SwiftUI
import UserNotifications

@main
struct DropumiMacApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    init() {
        DSFont.register()
    }

    var body: some Scene {
        Window("Dropumi", id: MainWindow.id) {
            RootView(loader: appDelegate.loader)
                .frame(minWidth: 860, minHeight: 600)
                .modifier(MainWindowLifecycle())
        }
        .defaultSize(width: 1120, height: 720)
        .commands { DropumiCommands(router: appDelegate.loader.router) }

        MenuBarExtra {
            MenuBarContent(loader: appDelegate.loader)
        } label: {
            MenuBarLabel()
        }
        .menuBarExtraStyle(.window)
    }
}

enum MainWindow {
    static let id = "main"
    /// Opens the window from places without a SwiftUI environment
    /// (notification clicks). Set by the menu-bar label, which always exists.
    static var open: OpenWindowAction?

    static func show() {
        NSApp.setActivationPolicy(.regular)
        open?(id: id)
        NSApp.activate()
    }
}

/// Dock icon while the window is open; menu bar only while it is closed.
private struct MainWindowLifecycle: ViewModifier {
    @AppStorage("explainedMenuBar") private var explained = false

    func body(content: Content) -> some View {
        content
            .onAppear { NSApp.setActivationPolicy(.regular) }
            .onDisappear {
                NSApp.setActivationPolicy(.accessory)
                if !explained {
                    explained = true
                    Notifier.shared.explainMenuBar()
                }
            }
    }
}

private struct MenuBarLabel: View {
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        Image("MenuBarIcon")
            .accessibilityLabel(Text("Dropumi"))
            .onAppear { MainWindow.open = openWindow }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    /// Owned here, not by the window: the core opens at launch and keeps
    /// receiving while the window is closed. A launch at login may show no
    /// window at all.
    let loader = AppLoader()
    let notifications = ReceiverNotifications()
    /// The load started at launch (tests wait for it).
    private(set) var launch: Task<Void, Never>?

    func applicationDidFinishLaunching(_ notification: Notification) {
        UNUserNotificationCenter.current().delegate = Notifier.shared
        let loader = loader
        notifications.follow { loader.model }
        launch = Task { await loader.load() }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        if !flag { MainWindow.show() }
        return true
    }

    /// Quitting stops the receiver and withdraws the advertisement first
    /// (bounded by the core's shutdown).
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let model = loader.model else { return .terminateNow }
        Task { @MainActor in
            await model.shutdownAndWait()
            NSApp.reply(toApplicationShouldTerminate: true)
        }
        return .terminateLater
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        for url in urls {
            loader.open(url)
        }
        MainWindow.show()
    }
}
