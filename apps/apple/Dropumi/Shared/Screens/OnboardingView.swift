// SPDX-License-Identifier: MPL-2.0
//
// First launch: what Dropumi is, this device's name, a word about Local
// Network access before the system asks, and who may send. Receiving starts
// only after the last step. The suggested name is the core's random
// "Dropumi-XXXX", never the owner's device name.

import DropumiAppState
import DropumiDesign
import SwiftUI

struct OnboardingView: View {
    @Environment(AppModel.self) private var model
    @State private var step = 0
    @State private var name = ""
    @State private var choice: ReceiveVisibility = .pairedOnly
    @State private var working = false

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    switch step {
                    case 0: welcome
                    case 1: deviceName
                    case 2: localNetwork
                    default: whoCanSend
                    }
                }
                .padding(DS.Metrics.screenPadding)
                .frame(maxWidth: 560, alignment: .leading)
                .frame(maxWidth: .infinity)
            }
            VStack(spacing: 12) {
                Button(step == 3 ? "Start" : "Continue") { next() }
                    .buttonStyle(PrimaryPillButtonStyle())
                    .disabled(working || (step == 1 && trimmedName.isEmpty))
                    .accessibilityIdentifier("onboarding.next")
                pageDots
            }
            .padding(DS.Metrics.screenPadding)
            .frame(maxWidth: 560)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(DS.bg)
        .onAppear { name = model.identity?.name ?? "" }
        .onChange(of: model.identity?.name) { _, value in
            if name.isEmpty { name = value ?? "" }
        }
    }

    private var trimmedName: String { name.trimmingCharacters(in: .whitespacesAndNewlines) }

    private var pageDots: some View {
        HStack(spacing: 6) {
            ForEach(0..<4, id: \.self) { index in
                Capsule()
                    .fill(index == step ? DS.accent : DS.border)
                    .frame(width: index == step ? 18 : 6, height: 6)
            }
        }
        .accessibilityElement()
        .accessibilityLabel(Text("Step \(step + 1) of 4"))
    }

    private var welcome: some View {
        VStack(alignment: .leading, spacing: 20) {
            Image("Wordmark")
                .resizable()
                .scaledToFit()
                .frame(height: 36)
                .accessibilityLabel(Text("Dropumi"))
                .padding(.top, 40)
            Text("Just drop it nearby.")
                .font(DSFont.screenTitle)
                .foregroundStyle(DS.textPrimary)
            point(Icons.nearby, "Files go straight to the other device over your local network.")
            point("person.crop.circle.badge.xmark", "No account, no cloud, no internet needed.")
            point("lock", "Every connection is encrypted, and you decide who can send to you.")
            Text("Dropumi is a pre-release and has not had a security audit yet.")
                .font(DSFont.meta)
                .foregroundStyle(DS.textSecondary)
        }
    }

    private var deviceName: some View {
        VStack(alignment: .leading, spacing: 16) {
            ScreenHeader("Name this device", subtitle: Text("Nearby devices see this name when they look for devices."))
                .padding(.top, 40)
            TextField("Device name", text: $name)
                .textFieldStyle(.roundedBorder)
                .font(DSFont.body)
                .autocorrectionDisabled()
                .accessibilityIdentifier("onboarding.name")
            Text("Anyone on the same network can see it, so avoid your full name.")
                .font(DSFont.meta)
                .foregroundStyle(DS.textSecondary)
        }
    }

    private var localNetwork: some View {
        VStack(alignment: .leading, spacing: 16) {
            ScreenHeader("Find devices on your network", subtitle: Text("Dropumi looks for nearby devices on your local network, and they find it the same way."))
                .padding(.top, 40)
            #if os(iOS)
            point(Icons.nearby, "Next, iOS asks whether Dropumi may find devices on your local network. Choose Allow, or Dropumi can't see other devices.")
            #else
            point(Icons.nearby, "macOS may ask whether Dropumi may find devices on your local network. Choose Allow, or Dropumi can't see other devices.")
            #endif
            point("wifi", "Use the same Wi-Fi network on both devices. Nothing goes through the internet.")
        }
    }

    private var whoCanSend: some View {
        VStack(alignment: .leading, spacing: 16) {
            ScreenHeader("Who can send to this device?", subtitle: Text("You can change this at any time in Settings."))
                .padding(.top, 40)
            option(.pairedOnly, "Paired devices only", "Recommended. Devices you paired with by comparing a code. You still answer each request.")
            option(.everyone, "Everyone, for 10 minutes", "Anyone nearby can ask. You compare a code before accepting. Then it goes back to paired devices only.")
            option(.off, "No one for now", "This device doesn't receive. You can still send.")
        }
    }

    private func point(_ symbol: String, _ text: LocalizedStringKey) -> some View {
        HStack(alignment: .top, spacing: 14) {
            Image(systemName: symbol)
                .font(.system(size: 18, weight: .medium))
                .foregroundStyle(DS.accent)
                .frame(width: 28)
                .accessibilityHidden(true)
            Text(text)
                .font(DSFont.body)
                .foregroundStyle(DS.textPrimary)
        }
    }

    private func option(_ value: ReceiveVisibility, _ title: LocalizedStringKey, _ detail: LocalizedStringKey) -> some View {
        Button {
            choice = value
        } label: {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: choice == value ? "largecircle.fill.circle" : "circle")
                    .foregroundStyle(choice == value ? DS.accent : DS.textSecondary)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 4) {
                    Text(title).font(DSFont.rowTitle).foregroundStyle(DS.textPrimary)
                    Text(detail).font(DSFont.meta).foregroundStyle(DS.textSecondary)
                }
                Spacer(minLength: 0)
            }
            .padding(14)
            .background(RoundedRectangle(cornerRadius: 16).fill(DS.surface))
            .overlay(RoundedRectangle(cornerRadius: 16).stroke(choice == value ? DS.accent : DS.border, lineWidth: choice == value ? 2 : 1))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(choice == value ? [.isSelected] : [])
    }

    private func next() {
        switch step {
        case 1:
            let value = trimmedName
            working = true
            Task {
                if value != model.identity?.name {
                    await model.renameDevice(value)
                }
                working = false
                step = 2
            }
        case 3:
            // Receiving (and with it the system's Local Network question)
            // starts only now.
            working = true
            let value = choice
            Task {
                await model.finishOnboarding(visibility: value)
                working = false
            }
        default:
            step += 1
        }
    }
}
