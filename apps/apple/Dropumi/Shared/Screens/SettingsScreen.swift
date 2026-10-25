// SPDX-License-Identifier: MPL-2.0

import DropumiAppState
import DropumiDesign
import SwiftUI

struct SettingsScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(Router.self) private var router
    #if os(macOS)
    @Environment(AppLoader.self) private var loader
    @State private var launchAtLogin = LoginItem.isEnabled
    #endif

    var body: some View {
        ScreenScroll {
            ScreenHeader("Settings", subtitle: Text("Your device and how it's found"))
            thisDevice
            whoCanSend
            VStack(spacing: 0) {
                NavigationLink(value: SettingsRoute.trustedDevices) {
                    SettingsRow(
                        symbol: Icons.shield, title: "Paired devices",
                        subtitle: Text("^[\(model.trusted.count) device](inflect: true)"))
                }
                .accessibilityIdentifier("settings.paired")
                // Through the router: only one pairing screen is ever alive.
                Button {
                    router.openPairing()
                } label: {
                    SettingsRow(symbol: Icons.pair, title: "Pair a device", subtitle: Text("Compare a code once. Then send without a code."))
                }
                .accessibilityIdentifier("settings.pair")
                NavigationLink(value: SettingsRoute.connectByAddress) {
                    SettingsRow(symbol: Icons.code, title: "Connect by address", subtitle: Text("When a device doesn't show up nearby"))
                }
                receiveFolder
                SettingsRow(
                    symbol: Icons.shield, title: "Auto-accept paired devices",
                    subtitle: Text("Skip the question for devices you paired")
                ) {
                    Toggle("", isOn: Binding(
                        get: { model.autoAcceptPaired },
                        set: { value in Task { await model.setAutoAcceptPaired(value) } }))
                        .labelsHidden()
                        .accessibilityLabel(Text("Auto-accept paired devices"))
                        .accessibilityIdentifier("settings.autoAccept")
                }
                #if os(macOS)
                SettingsRow(symbol: "power", title: "Open at login", subtitle: Text("Receive from the menu bar after you log in")) {
                    Toggle("", isOn: $launchAtLogin)
                        .labelsHidden()
                        .accessibilityLabel(Text("Open at login"))
                        .onChange(of: launchAtLogin) { _, value in
                            launchAtLogin = LoginItem.set(value)
                        }
                }
                #endif
                appearance
                Link(destination: URL(string: "https://github.com/dropumi/dropumi")!) {
                    SettingsRow(symbol: "chevron.left.forwardslash.chevron.right", title: "Open source", subtitle: Text("View the code and contribute"))
                }
            }
            .buttonStyle(.plain)
            Text("Free · No account · No internet needed · Version \(version)")
                .font(DSFont.meta)
                .foregroundStyle(DS.textSecondary)
                .frame(maxWidth: .infinity)
        }
        .task { await model.refreshTrusted() }
    }

    private var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? ""
    }

    private var thisDevice: some View {
        NavigationLink(value: SettingsRoute.thisDevice) {
            HStack(spacing: 14) {
                Circle()
                    .fill(DS.navy)
                    .frame(width: 52, height: 52)
                    .overlay {
                        Image("SymbolReversed").resizable().scaledToFit().padding(9)
                    }
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: model.identity?.name ?? "")
                        .font(DSFont.rowTitle)
                        .foregroundStyle(DS.textPrimary)
                    Text("Nearby devices see this name")
                        .font(DSFont.meta)
                        .foregroundStyle(DS.textSecondary)
                }
                Spacer()
                Image(systemName: "pencil")
                    .foregroundStyle(DS.textSecondary)
                    .accessibilityHidden(true)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("settings.thisDevice")
    }

    private var whoCanSend: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Who can send to this device")
                .font(DSFont.meta)
                .foregroundStyle(DS.textSecondary)
            DSSegmented(
                options: [(ReceiveVisibility.pairedOnly, LocalizedStringKey("Paired only")), (.everyone, LocalizedStringKey("Everyone · 10 min")), (.off, LocalizedStringKey("Off"))],
                selection: Binding(
                    get: { model.visibility },
                    set: { value in Task { await model.setVisibility(value) } }))
                .accessibilityIdentifier("settings.visibility")
            TimelineView(.periodic(from: .now, by: 1)) { context in
                caption(now: context.date)
                    .font(DSFont.meta)
                    .foregroundStyle(DS.textSecondary)
                    .monospacedDigit()
            }
        }
    }

    private func caption(now: Date) -> Text {
        #if os(iOS)
        let whileOpen = Text(" Only while Dropumi is open.")
        #else
        let whileOpen = Text("")
        #endif
        switch model.visibility {
        case .off:
            return Text("Not receiving. Other devices can't find or send to this one.")
        case .pairedOnly:
            return Text("Only paired devices can send. Others on the network can still see this device's name.\(whileOpen)")
        case .everyone:
            if let left = VisibilityText.everyoneSecondsLeft(model, now: now) {
                return Text("Anyone nearby can ask to send for \(Formatting.countdown(left)). You compare a code before accepting.\(whileOpen)")
            }
            return Text("Anyone nearby can ask to send for 10 minutes. You compare a code before accepting.\(whileOpen)")
        }
    }

    @ViewBuilder
    private var receiveFolder: some View {
        #if os(macOS)
        SettingsRow(
            symbol: Icons.receiveFolder, title: "Save received files to",
            subtitle: Text(verbatim: model.paths.receiveDir.abbreviatingWithTildeInPath)
        ) {
            HStack(spacing: 8) {
                Button("Show") { Platform.reveal(folder: model.paths.receiveDir) }
                Button("Change…") {
                    guard ReceiveLocation.choose() != nil else { return }
                    Task { await loader.reload() }
                }
            }
            .buttonStyle(SoftPillButtonStyle())
        }
        #else
        Button {
            Platform.reveal(folder: model.paths.receiveDir)
        } label: {
            SettingsRow(
                symbol: Icons.receiveFolder, title: "Received files",
                subtitle: Text("In Files: On My \(Text(deviceKind)) › Dropumi › Received"))
        }
        #endif
    }

    #if os(iOS)
    private var deviceKind: LocalizedStringKey {
        UIDevice.current.userInterfaceIdiom == .pad ? "iPad" : "iPhone"
    }
    #endif

    private var appearance: some View {
        SettingsRow(symbol: "circle.lefthalf.filled", title: "Appearance", subtitle: nil) {
            Picker("Appearance", selection: Binding(get: { model.appearance }, set: { model.appearance = $0 })) {
                Text("Match system").tag(Appearance.system)
                Text("Light").tag(Appearance.light)
                Text("Dark").tag(Appearance.dark)
            }
            .labelsHidden()
            .pickerStyle(.menu)
            .fixedSize()
        }
    }
}
