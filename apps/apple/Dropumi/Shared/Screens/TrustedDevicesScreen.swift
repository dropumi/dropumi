// SPDX-License-Identifier: MPL-2.0
//
// Paired devices: rename, remove, or add one by its full 52-character
// fingerprint. There is no Send button here on purpose: sending goes through
// a nearby device or an address, where the key is checked (D10).

import DropumiAppState
import DropumiBindings
import DropumiDesign
import SwiftUI

struct TrustedDevicesScreen: View {
    @Environment(AppModel.self) private var model
    @State private var renaming: TrustedDeviceInfo?
    @State private var removing: TrustedDeviceInfo?
    @State private var adding = false

    var body: some View {
        ScreenScroll {
            ScreenHeader("Paired devices", subtitle: Text("These devices can send to this one when you choose Paired only."))
            if model.trusted.isEmpty {
                EmptyStateView(
                    symbol: Icons.shield, title: String(localized: "No paired devices"),
                    message: String(localized: "Pair a device by comparing a code, or add one by its fingerprint."))
            }
            VStack(spacing: 0) {
                ForEach(model.trusted, id: \.fingerprint) { device in
                    DSRow(title: device.alias ?? String(localized: "Unnamed device"), subtitle: subtitle(device)) {
                        DeviceAvatar()
                    } trailing: {
                        Menu {
                            Button("Rename…") { renaming = device }
                            Button("Remove…", role: .destructive) { removing = device }
                        } label: {
                            Image(systemName: "ellipsis")
                                .foregroundStyle(DS.textSecondary)
                                .frame(width: 44, height: 44)
                        }
                        .menuIndicator(.hidden)
                        .fixedSize()
                        .accessibilityLabel(Text("Actions"))
                    }
                }
            }
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    adding = true
                } label: {
                    Label("Add by fingerprint", systemImage: Icons.add)
                }
            }
        }
        .sheet(item: $renaming) { device in
            RenameSheet(device: device)
        }
        .sheet(isPresented: $adding) {
            AddByFingerprintSheet()
        }
        .confirmationDialog(
            "Remove this device?", isPresented: Binding(get: { removing != nil }, set: { if !$0 { removing = nil } }),
            presenting: removing
        ) { device in
            Button("Remove", role: .destructive) {
                Task { await model.forget(device) }
            }
        } message: { _ in
            Text("It can no longer send to this device while Paired only is on. You can pair again later.")
        }
    }

    private func subtitle(_ device: TrustedDeviceInfo) -> String {
        let method = device.method == .code
            ? String(localized: "Paired with a code")
            : String(localized: "Added by fingerprint")
        return "\(method) · \(device.fingerprintDisplay)"
    }
}

extension TrustedDeviceInfo: @retroactive Identifiable {
    public var id: String { fingerprint }
}

private struct RenameSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let device: TrustedDeviceInfo
    @State private var name = ""

    var body: some View {
        NavigationStack {
            Form {
                TextField("Name", text: $name)
                    .autocorrectionDisabled()
                Text(verbatim: device.fingerprintDisplay)
                    .font(DSFont.fingerprint)
                    .foregroundStyle(DS.textSecondary)
            }
            .navigationTitle("Rename")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        let value = name.trimmingCharacters(in: .whitespacesAndNewlines)
                        Task {
                            await model.rename(device, to: value)
                            dismiss()
                        }
                    }
                    .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
        }
        .onAppear { name = device.alias ?? "" }
        .frame(minWidth: 360, minHeight: 200)
    }
}

private struct AddByFingerprintSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var fingerprint = ""
    @State private var alias = ""

    /// The fingerprint without spaces or dashes.
    private var compact: String {
        fingerprint.filter { !$0.isWhitespace && $0 != "-" }
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Fingerprint (52 characters)", text: $fingerprint, axis: .vertical)
                        .font(DSFont.fingerprint)
                        .autocorrectionDisabled()
                        #if os(iOS)
                        .textInputAutocapitalization(.characters)
                        #endif
                    TextField("Name (optional)", text: $alias)
                } footer: {
                    Text("Read the full fingerprint on the other device: Settings › This device. Only add a fingerprint you checked yourself.")
                }
            }
            .navigationTitle("Add by fingerprint")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add") {
                        let name = alias.trimmingCharacters(in: .whitespacesAndNewlines)
                        Task {
                            if await model.trust(fingerprint: compact, alias: name.isEmpty ? nil : name) {
                                dismiss()
                            }
                        }
                    }
                    .disabled(compact.count != 52)
                }
            }
        }
        .frame(minWidth: 420, minHeight: 260)
    }
}
