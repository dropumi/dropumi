// SPDX-License-Identifier: MPL-2.0
//
// Nearby: this device on the radar, nearby devices, a Send button per
// device, and the way to Pair a device. Names come from the network and are
// shown verbatim; a device is never labelled "Paired" here, because nothing
// has checked its key yet (D9). Browsing runs only while this screen is
// visible.

import DropumiAppState
import DropumiBindings
import DropumiDesign
import SwiftUI
import UniformTypeIdentifiers

struct NearbyScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(Router.self) private var router
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var importing = false

    var body: some View {
        ScreenScroll {
            #if os(iOS)
            HStack(spacing: 10) {
                Image("Wordmark")
                    .resizable()
                    .scaledToFit()
                    .frame(height: 26)
                    .accessibilityLabel(Text("Dropumi"))
                Spacer()
                VisibilityPill()
                moreMenu
            }
            #endif
            HStack(alignment: .top) {
                ScreenHeader("Nearby", subtitle: subtitle)
                #if os(macOS)
                VisibilityPill()
                #endif
            }
            problem
            if wide {
                HStack(alignment: .top, spacing: 40) {
                    radar.frame(maxWidth: 420)
                    VStack(alignment: .leading, spacing: DS.Metrics.sectionGap) {
                        DropZone(importing: $importing)
                        deviceList(title: "Send to")
                    }
                }
            } else {
                radar.frame(maxWidth: 330).frame(maxWidth: .infinity)
                deviceList(title: "Tap a device to send")
            }
        }
        #if os(iOS)
        .toolbar(.hidden, for: .navigationBar)
        #else
        .toolbar {
            ToolbarItem { moreMenu }
        }
        #endif
        .fileImporter(isPresented: $importing, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            if case let .success(urls) = result {
                model.addToDraft(FileImport.picked(urls))
                router.openSend()
            }
        }
        .onAppear { model.beginBrowsing() }
        .onDisappear { model.endBrowsing() }
    }

    private var moreMenu: some View {
        Menu {
            Button {
                router.openSend()
            } label: {
                Label("Choose files to send", systemImage: Icons.upload)
            }
            Button {
                router.nearbyPath.append(.connectByAddress)
            } label: {
                Label("Connect by address", systemImage: Icons.code)
            }
            Button {
                router.openPairing()
            } label: {
                Label("Pair a device", systemImage: Icons.pair)
            }
        } label: {
            Image(systemName: "ellipsis")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(DS.textPrimary)
                .frame(width: 44, height: 44)
                .background(Circle().fill(DS.surface2))
        }
        .iconMenu()
        .accessibilityLabel(Text("More"))
        .accessibilityIdentifier("nearby.more")
    }

    private var wide: Bool {
        #if os(macOS)
        true
        #else
        sizeClass == .regular
        #endif
    }

    private var subtitle: Text {
        let count = model.nearby.count
        return Text("^[\(count) device](inflect: true) found · works without internet")
    }

    @ViewBuilder
    private var problem: some View {
        if model.localNetworkDenied {
            ProblemCard(
                title: "Local network access is off",
                message: "Dropumi needs it to find and reach nearby devices. Turn on Local Network for Dropumi in Settings.",
                actionTitle: "Open Settings",
                action: Platform.openLocalNetworkSettings)
        } else if model.visibility == .off {
            ProblemCard(
                title: "Not receiving",
                message: "Other devices can't find or send to this one. You can still send.",
                actionTitle: "Turn on for paired devices",
                action: { Task { await model.setVisibility(.pairedOnly) } })
        } else if let problem = model.discoveryProblem {
            ProblemCard(title: "Nearby devices can't be found", detail: problem, actionTitle: nil, action: nil)
        }
    }

    private var radar: some View {
        RadarView(items: model.nearby.map { RadarItem(id: $0.instance, name: $0.name) }) {
            Circle()
                .fill(DS.navy)
                .overlay {
                    Image("SymbolReversed")
                        .resizable()
                        .scaledToFit()
                        .padding(14)
                }
                .shadow(color: DS.navy.opacity(0.25), radius: 14, y: 6)
        }
    }

    @ViewBuilder
    private func deviceList(title: LocalizedStringKey) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(DSFont.sectionHeading)
                .foregroundStyle(DS.textPrimary)
                .accessibilityAddTraits(.isHeader)
            if model.nearby.isEmpty {
                EmptyStateView(
                    symbol: Icons.nearby, title: String(localized: "Looking for devices…"),
                    message: String(localized: "Open Dropumi on the other device and use the same Wi-Fi network. Devices that only accept paired devices still show up here."))
                    .accessibilityIdentifier("nearby.empty")
            } else {
                ForEach(model.nearby, id: \.instance) { peer in
                    DSRow(title: peer.name, subtitle: nearbySubtitle(peer), titleIsPeerText: true) {
                        DeviceAvatar()
                    } trailing: {
                        Button("Send") { router.openSend(preselected: peer.instance) }
                            .buttonStyle(SoftPillButtonStyle())
                            .disabled(!peer.compatible)
                            .accessibilityLabel(Text("Send to \(Text(verbatim: peer.name))"))
                    }
                    .accessibilityElement(children: .combine)
                    .contextMenu {
                        Button {
                            // Asks at once, so the pairing screen shows the
                            // request instead of getting ready to pair.
                            model.startPairing(with: .nearby(instance: peer.instance), name: peer.name)
                            router.openPairing()
                        } label: {
                            Label("Pair with this device", systemImage: Icons.pair)
                        }
                        .disabled(!peer.compatible)
                    }
                }
            }
            Button {
                router.openPairing()
            } label: {
                Label("Pair a device", systemImage: Icons.pair)
            }
            .buttonStyle(TextLinkButtonStyle())
            .accessibilityIdentifier("nearby.pair")
        }
    }
}

/// Drop files to send (iPad and Mac).
private struct DropZone: View {
    @Environment(AppModel.self) private var model
    @Environment(Router.self) private var router
    @Binding var importing: Bool
    @State private var targeted = false

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: Icons.upload)
                .font(.system(size: 22, weight: .medium))
                .foregroundStyle(DS.accent)
                .frame(width: 56, height: 56)
                .background(Circle().fill(DS.surface))
                .accessibilityHidden(true)
            Text("Drop files here to send")
                .font(DSFont.sectionHeading)
                .foregroundStyle(DS.textPrimary)
            Button("or browse files") { importing = true }
                .buttonStyle(.plain)
                .font(DSFont.smallButton)
                .foregroundStyle(DS.accentText)
                .frame(minHeight: 44)
        }
        .frame(maxWidth: .infinity, minHeight: 180)
        .background(RoundedRectangle(cornerRadius: DS.Metrics.cardRadius).fill(DS.accentSoft))
        .overlay(
            RoundedRectangle(cornerRadius: DS.Metrics.cardRadius)
                .stroke(targeted ? DS.accent : DS.ring, lineWidth: targeted ? 2 : 1.5))
        #if os(macOS)
        .dropDestination(for: URL.self) { urls, _ in
            let files = urls.filter(\.isFileURL).map { OutgoingFile(url: $0, securityScoped: false, staged: false) }
            guard !files.isEmpty else { return false }
            model.addToDraft(files)
            router.openSend()
            return true
        } isTargeted: { targeted = $0 }
        #else
        .dropDestination(for: StagedCopy.self) { copies, _ in
            guard !copies.isEmpty else { return false }
            model.addToDraft(copies.map(\.outgoing))
            router.openSend()
            return true
        } isTargeted: { targeted = $0 }
        #endif
    }
}

/// A notice inside a screen, with an optional action.
struct ProblemCard: View {
    let title: LocalizedStringKey
    let message: Text
    let actionTitle: LocalizedStringKey?
    let action: (() -> Void)?

    init(title: LocalizedStringKey, message: LocalizedStringKey, actionTitle: LocalizedStringKey?, action: (() -> Void)?) {
        self.title = title
        self.message = Text(message)
        self.actionTitle = actionTitle
        self.action = action
    }

    /// `detail` is local text (an error description), shown as is.
    init(title: LocalizedStringKey, detail: String, actionTitle: LocalizedStringKey?, action: (() -> Void)?) {
        self.title = title
        self.message = Text(verbatim: detail)
        self.actionTitle = actionTitle
        self.action = action
    }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: Icons.info)
                .foregroundStyle(DS.accent)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 6) {
                Text(title).font(DSFont.rowTitle).foregroundStyle(DS.textPrimary)
                message.font(DSFont.meta).foregroundStyle(DS.textSecondary)
                if let actionTitle, let action {
                    Button(actionTitle, action: action)
                        .buttonStyle(SoftPillButtonStyle())
                }
            }
            Spacer(minLength: 0)
        }
        .padding(16)
        .background(RoundedRectangle(cornerRadius: 18).fill(DS.surface))
        .overlay(RoundedRectangle(cornerRadius: 18).stroke(DS.border))
        .accessibilityElement(children: .contain)
    }
}
