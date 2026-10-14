// SPDX-License-Identifier: MPL-2.0
//
// The menu-bar receiver: status, who may send, running transfers, open the
// window, and quit completely.

import AppKit
import DropumiAppState
import DropumiDesign
import SwiftUI

struct MenuBarContent: View {
    let loader: AppLoader

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            if let model = loader.model {
                MenuBarStatus(model: model)
            } else {
                Text("Starting…")
                    .font(DSFont.body)
                    .foregroundStyle(DS.textSecondary)
            }
            Divider()
            Button {
                MainWindow.show()
            } label: {
                Label("Open Dropumi", systemImage: "macwindow")
            }
            .buttonStyle(.plain)
            Button {
                NSApp.terminate(nil)
            } label: {
                Label("Quit Dropumi completely", systemImage: "power")
            }
            .buttonStyle(.plain)
            .keyboardShortcut("q")
        }
        .font(DSFont.body)
        .padding(16)
        .frame(width: 320)
    }
}

private struct MenuBarStatus: View {
    @Bindable var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 10) {
                DeviceAvatar(symbol: Icons.device, size: 34)
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: model.identity?.name ?? "")
                        .font(DSFont.rowTitle)
                        .foregroundStyle(DS.textPrimary)
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        VisibilityText.label(model, now: context.date)
                            .font(DSFont.meta)
                            .monospacedDigit()
                            .foregroundStyle(DS.textSecondary)
                    }
                }
            }
            if model.onboarded {
                DSSegmented(
                    options: [(ReceiveVisibility.pairedOnly, LocalizedStringKey("Paired only")), (.everyone, LocalizedStringKey("Everyone")), (.off, LocalizedStringKey("Off"))],
                    selection: Binding(
                        get: { model.visibility },
                        set: { value in Task { await model.setVisibility(value) } }))
            } else {
                Text("Open Dropumi to finish setting up.")
                    .font(DSFont.meta)
                    .foregroundStyle(DS.textSecondary)
            }
            if !model.prompts.isEmpty {
                Button {
                    MainWindow.show()
                } label: {
                    Label("Someone wants to send you files", systemImage: Icons.info)
                }
                .buttonStyle(SoftPillButtonStyle())
            }
            ForEach(model.active) { record in
                MenuBarTransfer(record: record) { model.cancel(record) }
            }
        }
    }
}

private struct MenuBarTransfer: View {
    let record: TransferRecord
    let cancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(verbatim: record.peerLabel.title)
                    .font(DSFont.smallButton)
                    .lineLimit(1)
                Spacer()
                Button(action: cancel) {
                    Image(systemName: Icons.close)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Cancel"))
            }
            DSProgressBar(record.fraction)
        }
    }
}
