// SPDX-License-Identifier: MPL-2.0
//
// Connect by address, for networks where discovery does not work (client
// isolation, mDNS blocked). An optional fingerprint pin (at least 24
// characters) is checked before anything is sent.

import DropumiAppState
import DropumiBindings
import DropumiDesign
import SwiftUI

struct ConnectByAddressScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(Router.self) private var router
    @State private var address = ""
    @State private var pin = ""
    @State private var importing = false

    var body: some View {
        ScreenScroll {
            ScreenHeader("Connect by address", subtitle: Text("Type the address shown on the other device under Settings › This device."))
            VStack(alignment: .leading, spacing: 8) {
                SectionLabel(String(localized: "Address"))
                TextField("192.168.1.20:41234", text: $address)
                    .textFieldStyle(.roundedBorder)
                    .font(DSFont.fingerprint)
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .keyboardType(.numbersAndPunctuation)
                    .textInputAutocapitalization(.never)
                    #endif
                    .accessibilityIdentifier("address.field")
                SectionLabel(String(localized: "Fingerprint (optional)"))
                TextField("At least 24 characters", text: $pin)
                    .textFieldStyle(.roundedBorder)
                    .font(DSFont.fingerprint)
                    .autocorrectionDisabled()
                    #if os(iOS)
                    .textInputAutocapitalization(.characters)
                    #endif
                Text(pinHint)
                    .font(DSFont.meta)
                    .foregroundStyle(pinValid ? DS.textSecondary : DS.danger)
            }
            VStack(alignment: .leading, spacing: 12) {
                Text(model.draft.isEmpty
                     ? "No files chosen yet."
                     : "^[\(model.draft.count) file](inflect: true) ready to send.")
                    .font(DSFont.meta)
                    .foregroundStyle(DS.textSecondary)
                if model.draft.isEmpty {
                    Button("Choose files") { importing = true }
                        .buttonStyle(SoftPillButtonStyle())
                }
                Button("Send files") { send() }
                    .buttonStyle(PrimaryPillButtonStyle())
                    .disabled(!ready || model.draft.isEmpty)
                    .accessibilityIdentifier("address.send")
                Button("Pair with this address") { pair() }
                    .buttonStyle(OutlinePillButtonStyle())
                    .disabled(!ready)
            }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            if case let .success(urls) = result {
                model.addToDraft(FileImport.picked(urls))
            }
        }
    }

    private var trimmedAddress: String { address.trimmingCharacters(in: .whitespacesAndNewlines) }
    private var compactPin: String { pin.filter { !$0.isWhitespace && $0 != "-" } }
    private var pinValid: Bool { compactPin.isEmpty || compactPin.count >= 24 }
    private var ready: Bool { trimmedAddress.contains(":") && pinValid }

    private var pinHint: LocalizedStringKey {
        if compactPin.isEmpty {
            return "Without a fingerprint, you compare a code with the other device first."
        }
        return pinValid
            ? "Dropumi checks this fingerprint before anything is sent."
            : "Type at least 24 characters of the fingerprint."
    }

    private var target: SendTarget {
        .address(address: trimmedAddress, expectFingerprint: compactPin.isEmpty ? nil : compactPin)
    }

    private func send() {
        let target = target
        let name = trimmedAddress
        Task {
            await model.sendDraft(to: target, targetName: name, allowUnverified: true)
            if model.draft.isEmpty {
                router.nearbyPath = []
                router.settingsPath = []
                router.tab = .transfers
            }
        }
    }

    private func pair() {
        let target = target
        let name = trimmedAddress
        Task { await model.pair(with: target, name: name) }
        router.openPairing()
    }
}
