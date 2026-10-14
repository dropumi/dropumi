// SPDX-License-Identifier: MPL-2.0
//
// Pairing: compare a six-digit code once; afterwards the devices know each
// other's key (ADR-0004). Either this device asks a nearby one, or it opens
// a five-minute window for another device to ask. The code itself appears
// as a prompt (PromptPresenter); trust is stored only if both users confirm.

import DropumiAppState
import DropumiBindings
import DropumiDesign
import SwiftUI

struct PairingScreen: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ScreenScroll {
            ScreenHeader("Pair a device", subtitle: Text("You compare a code once. After that, the devices recognise each other."))
            if let pairing = model.pairing {
                PairingStatus(pairing: pairing)
            } else {
                choices
            }
        }
        .onAppear { model.beginBrowsing() }
        .onDisappear { model.endBrowsing() }
    }

    private var choices: some View {
        VStack(alignment: .leading, spacing: DS.Metrics.sectionGap) {
            VStack(alignment: .leading, spacing: 10) {
                Text("Let another device pair with this one")
                    .font(DSFont.sectionHeading)
                    .foregroundStyle(DS.textPrimary)
                Text("For five minutes, a device can ask to pair with this one. You both check a code.")
                    .font(DSFont.meta)
                    .foregroundStyle(DS.textSecondary)
                Button("Wait for a device") {
                    Task { await model.openPairingWindow() }
                }
                .buttonStyle(PrimaryPillButtonStyle())
                .accessibilityIdentifier("pair.window")
            }
            VStack(alignment: .leading, spacing: 4) {
                Text("Pair with a nearby device")
                    .font(DSFont.sectionHeading)
                    .foregroundStyle(DS.textPrimary)
                Text("On the other device, open Dropumi and choose to wait for a device.")
                    .font(DSFont.meta)
                    .foregroundStyle(DS.textSecondary)
                if model.nearby.isEmpty {
                    EmptyStateView(
                        symbol: Icons.nearby, title: String(localized: "Looking for devices…"),
                        message: String(localized: "Make sure the other device is on the same Wi-Fi network."))
                }
                ForEach(model.nearby, id: \.instance) { peer in
                    DSRow(title: peer.name, subtitle: nearbySubtitle(peer), titleIsPeerText: true) {
                        DeviceAvatar()
                    } trailing: {
                        Button("Pair") {
                            Task { await model.pair(with: .nearby(instance: peer.instance), name: peer.name) }
                        }
                        .buttonStyle(SoftPillButtonStyle())
                        .disabled(!peer.compatible)
                        .accessibilityLabel(Text("Pair with \(Text(verbatim: peer.name))"))
                    }
                }
            }
        }
    }
}

private struct PairingStatus: View {
    @Environment(AppModel.self) private var model
    let pairing: PairingSession

    var body: some View {
        VStack(spacing: 18) {
            switch pairing.result {
            case nil:
                waiting
            case let .paired(peer):
                result(
                    symbol: Icons.check, color: DS.success, title: Text("Paired"),
                    message: Text("\(Text(verbatim: PeerLabel(peer: peer).title)) can now send to this device, and you can send to it without a code."))
            case let .failed(error):
                let text = ErrorCatalog.text(for: error)
                result(symbol: Icons.warning, color: DS.danger, title: Text(verbatim: text.title), message: Text(verbatim: text.message))
            case .cancelled:
                result(symbol: Icons.failed, color: DS.textSecondary, title: Text("Pairing cancelled"), message: Text("Nothing was saved."))
            }
        }
        .padding(20)
        .frame(maxWidth: .infinity)
        .background(RoundedRectangle(cornerRadius: DS.Metrics.cardRadius).fill(DS.surface))
        .overlay(RoundedRectangle(cornerRadius: DS.Metrics.cardRadius).stroke(DS.border))
    }

    @ViewBuilder
    private var waiting: some View {
        ProgressView()
        switch pairing.role {
        case let .initiator(name):
            Text("Asking \(Text(verbatim: name))…")
                .font(DSFont.rowTitle)
                .foregroundStyle(DS.textPrimary)
            Text("A code appears on both devices. Check that they match.")
                .font(DSFont.meta)
                .foregroundStyle(DS.textSecondary)
                .multilineTextAlignment(.center)
        case .window:
            TimelineView(.periodic(from: .now, by: 1)) { context in
                let left = pairing.expiresAt.map { max(0, Int($0.timeIntervalSince(context.date))) } ?? 0
                VStack(spacing: 8) {
                    Text("Waiting for a device · \(Formatting.countdown(left))")
                        .font(DSFont.rowTitle)
                        .monospacedDigit()
                        .foregroundStyle(DS.textPrimary)
                    Text("On the other device, choose Pair a device, then this one:")
                        .font(DSFont.meta)
                        .foregroundStyle(DS.textSecondary)
                        .multilineTextAlignment(.center)
                    Text(verbatim: model.identity?.name ?? "")
                        .font(DSFont.sectionHeading)
                        .foregroundStyle(DS.textPrimary)
                }
            }
        }
        Button("Cancel") { model.cancelPairing() }
            .buttonStyle(OutlinePillButtonStyle())
            .frame(maxWidth: 260)
            .accessibilityIdentifier("pair.cancel")
    }

    @ViewBuilder
    private func result(symbol: String, color: Color, title: Text, message: Text) -> some View {
        Image(systemName: symbol)
            .font(.system(size: 34, weight: .medium))
            .foregroundStyle(color)
            .accessibilityHidden(true)
        title
            .font(DSFont.sectionHeading)
            .foregroundStyle(DS.textPrimary)
        message
            .font(DSFont.body)
            .foregroundStyle(DS.textSecondary)
            .multilineTextAlignment(.center)
        Button("Done") { model.dismissPairing() }
            .buttonStyle(PrimaryPillButtonStyle())
            .frame(maxWidth: 260)
            .accessibilityIdentifier("pair.done")
    }
}
