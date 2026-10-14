// SPDX-License-Identifier: MPL-2.0
//
// This device: its name, its fingerprint, the addresses other devices can
// type, and "Reset identity".

import DropumiAppState
import DropumiDesign
import SwiftUI

struct ThisDeviceScreen: View {
    @Environment(AppModel.self) private var model
    @State private var name = ""
    @State private var confirmingReset = false

    var body: some View {
        ScreenScroll {
            ScreenHeader("This device")
            VStack(alignment: .leading, spacing: 8) {
                SectionLabel(String(localized: "Name"))
                HStack {
                    TextField("Device name", text: $name)
                        .textFieldStyle(.roundedBorder)
                        .autocorrectionDisabled()
                        .onSubmit(save)
                        .accessibilityIdentifier("device.name")
                    Button("Save", action: save)
                        .buttonStyle(SoftPillButtonStyle())
                        .disabled(trimmed.isEmpty || trimmed == model.identity?.name)
                }
                Text("Nearby devices see this name. It is only a label: devices recognise each other by their key.")
                    .font(DSFont.meta)
                    .foregroundStyle(DS.textSecondary)
            }
            if let identity = model.identity {
                VStack(alignment: .leading, spacing: 8) {
                    SectionLabel(String(localized: "Fingerprint"))
                    Text(verbatim: identity.fingerprintDisplay)
                        .font(DSFont.fingerprint)
                        .foregroundStyle(DS.textPrimary)
                        .textSelection(.enabled)
                        .accessibilityIdentifier("device.fingerprint")
                    Text("Another device can add this one by typing this fingerprint. Compare it in person.")
                        .font(DSFont.meta)
                        .foregroundStyle(DS.textSecondary)
                }
            }
            VStack(alignment: .leading, spacing: 8) {
                SectionLabel(String(localized: "Address"))
                if model.connectionDetails.isEmpty {
                    Text("Not receiving right now.")
                        .font(DSFont.body)
                        .foregroundStyle(DS.textSecondary)
                } else {
                    ForEach(model.connectionDetails, id: \.self) { address in
                        Text(verbatim: address)
                            .font(DSFont.fingerprint)
                            .foregroundStyle(DS.textPrimary)
                            .textSelection(.enabled)
                    }
                    Text("If this device doesn't show up nearby, type this address on the other device (Connect by address).")
                        .font(DSFont.meta)
                        .foregroundStyle(DS.textSecondary)
                }
            }
            Button("Reset identity…", role: .destructive) { confirmingReset = true }
                .buttonStyle(OutlinePillButtonStyle(foreground: DS.danger, outline: DS.danger))
                .frame(maxWidth: 320)
                .accessibilityIdentifier("device.reset")
        }
        .onAppear { name = model.identity?.name ?? "" }
        .confirmationDialog("Reset this device's identity?", isPresented: $confirmingReset) {
            Button("Reset identity", role: .destructive) {
                Task { await model.resetIdentity() }
            }
        } message: {
            Text("This device gets a new key and fingerprint. Every device you paired with must pair again.")
        }
    }

    private var trimmed: String {
        name.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func save() {
        guard !trimmed.isEmpty, trimmed != model.identity?.name else { return }
        let value = trimmed
        Task {
            await model.renameDevice(value)
            name = model.identity?.name ?? value
        }
    }
}
