// SPDX-License-Identifier: MPL-2.0
//
// Pair a device: compare a six-digit code once; afterwards the devices know
// each other's key (ADR-0004). Both people open this screen, which makes
// each device ready to pair (the model opens a five-minute window, with a
// countdown and Stop), and one of them taps the other device. The code
// itself appears as a prompt (PromptPresenter); trust is stored only if
// both users confirm. Names in the list are claims: shown verbatim, in
// quotes, and a row never says "Paired" (D9).

import DropumiAppState
import DropumiBindings
import DropumiCore
import DropumiDesign
import SwiftUI

/// Where VoiceOver goes when the screen changes.
private enum PairingFocus: Hashable {
    case result, note
}

struct PairingScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.dynamicTypeSize) private var typeSize
    @AccessibilityFocusState private var focus: PairingFocus?

    var body: some View {
        ScreenScroll {
            ScreenHeader("Pair a device", subtitle: Text("Open this screen on both devices. Then tap the other device on one of them."))
            content
        }
        // Showing the screen makes this device ready to pair; leaving it
        // ends that (a code comparison under way is kept).
        .onAppear { model.enterPairingScreen() }
        .onDisappear { model.leavePairingScreen() }
    }

    @ViewBuilder
    private var content: some View {
        let flow = model.pairingFlow
        switch flow.phase {
        case .idle, .opening, .waiting:
            columns {
                ready(flow.phase)
                PairingSteps()
            } right: {
                if let note = flow.note {
                    PairingNote(text: note.noteText ?? note.text.message, focus: $focus)
                }
                PairingDeviceList()
            }
        case let .switching(name), let .asking(name):
            StatusCard(
                title: Text("Asking “\(Text(verbatim: name))”…"),
                line: Text("A code appears on both devices. Check that they match."),
                cancel: { model.cancelPairing() })
        case .comparing:
            StatusCard(
                title: Text("Compare the code"),
                line: Text("Check that both screens show the same code."),
                cancel: nil)
        case let .paused(outcome):
            if case .couldNotWait = outcome {
                // This device can still ask one in the list.
                columns {
                    result(outcome, paused: true)
                } right: {
                    PairingDeviceList()
                }
            } else {
                result(outcome, paused: true)
            }
        case let .finished(outcome):
            result(outcome, paused: false)
        }
    }

    @ViewBuilder
    private func ready(_ phase: PairingFlow.Phase) -> some View {
        switch phase {
        case .opening:
            ReadyCard(expiresAt: nil, stop: stop)
        case let .waiting(expiresAt):
            ReadyCard(expiresAt: expiresAt, stop: stop)
        default:
            EmptyView()
        }
    }

    private func result(_ outcome: PairingOutcome, paused: Bool) -> some View {
        ResultCard(
            outcome: outcome, paused: paused, focus: $focus,
            done: {
                model.dismissPairing()
                dismiss()
            },
            again: { model.startWaiting() })
    }

    /// Stop: this device is no longer ready to pair, and the screen goes.
    private func stop() {
        model.stopPairing()
        dismiss()
    }

    /// Two columns on the Mac and a wide iPad: the ready card and the steps,
    /// then the list. One column on narrow screens and at accessibility
    /// text sizes.
    @ViewBuilder
    private func columns<Left: View, Right: View>(
        @ViewBuilder _ left: () -> Left, @ViewBuilder right: () -> Right
    ) -> some View {
        if wide {
            HStack(alignment: .top, spacing: 40) {
                VStack(alignment: .leading, spacing: DS.Metrics.sectionGap) { left() }
                    .frame(maxWidth: 320)
                VStack(alignment: .leading, spacing: DS.Metrics.sectionGap) { right() }
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        } else {
            left()
            right()
        }
    }

    private var wide: Bool {
        guard !typeSize.isAccessibilitySize else { return false }
        #if os(macOS)
        return true
        #else
        return sizeClass == .regular
        #endif
    }
}

/// This device is getting ready to pair, or is ready: the time left, the
/// name other devices see, and the address for devices that don't list it.
private struct ReadyCard: View {
    @Environment(AppModel.self) private var model
    /// Nil while getting ready.
    let expiresAt: Date?
    let stop: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .top, spacing: 14) {
                DeviceAvatar(symbol: Icons.pair)
                VStack(alignment: .leading, spacing: 4) {
                    if let expiresAt {
                        Countdown(expiresAt: expiresAt)
                        if let name = model.identity?.name {
                            Text("Other devices see this one as “\(Text(verbatim: name))”.")
                                .font(DSFont.meta)
                                .foregroundStyle(DS.textSecondary)
                                .fixedSize(horizontal: false, vertical: true)
                                .accessibilityIdentifier("pair.name")
                        }
                    } else {
                        HStack(spacing: 10) {
                            ProgressView()
                                .controlSize(.small)
                                .accessibilityHidden(true)
                            Text("Getting ready to pair…")
                                .font(DSFont.rowTitle)
                                .foregroundStyle(DS.textPrimary)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
                Spacer(minLength: 0)
            }
            if expiresAt != nil {
                if model.visibility != .off {
                    Text("Receiving files is paused until you leave this screen.")
                        .font(DSFont.meta)
                        .foregroundStyle(DS.textSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                addressLines
            }
            Button("Stop", action: stop)
                .buttonStyle(OutlinePillButtonStyle())
                #if os(macOS)
                // Never while a prompt is up: there, Escape means no.
                .keyboardShortcut(model.currentPrompt == nil ? .cancelAction : nil)
                #endif
                .accessibilityIdentifier("pair.stop")
                .frame(maxWidth: 260)
                .frame(maxWidth: .infinity)
        }
        .pairingCard()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("pair.ready")
    }

    @ViewBuilder
    private var addressLines: some View {
        let addresses = addresses
        if !addresses.isEmpty {
            VStack(alignment: .leading, spacing: 4) {
                Text("Not listed on the other device? There, choose Pair by address and type:")
                    .font(DSFont.meta)
                    .foregroundStyle(DS.textSecondary)
                    .fixedSize(horizontal: false, vertical: true)
                ForEach(addresses, id: \.text) { address in
                    // The label is on a container: on a Text itself it made
                    // the iOS audit report the text as clipped, and on a
                    // selectable Text it crashed the Mac's label lookup.
                    HStack(spacing: 0) {
                        Text(verbatim: address.text)
                            .font(DSFont.fingerprint)
                            .foregroundStyle(DS.textPrimary)
                    }
                    .accessibilityElement(children: .ignore)
                    .accessibilityAddTraits(.isStaticText)
                    .accessibilityLabel(Text("Address \(Text(verbatim: address.host)), port \(Text(verbatim: address.port))"))
                    .accessibilityIdentifier("pair.address")
                }
            }
        }
    }

    /// The Wi-Fi and Ethernet addresses (en*) with the port this device
    /// waits on. The window's port is not the receiver's.
    private var addresses: [(text: String, host: String, port: String)] {
        guard let pairing = model.pairing, case .window = pairing.role, pairing.result == nil,
              let port = pairing.port
        else { return [] }
        return LocalAddresses.ipv4().filter { $0.interface.hasPrefix("en") }
            .map { (text: "\($0.text):\(port)", host: $0.text, port: String(port)) }
    }
}

/// "Ready to pair · 4:59". VoiceOver reads it as one element whose value
/// changes once a minute, not every second.
private struct Countdown: View {
    let expiresAt: Date

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            let left = max(0, Int(expiresAt.timeIntervalSince(context.date)))
            // One text that wraps at large sizes (a ViewThatFits swap
            // failed the Dynamic Type audit on an iPhone 15).
            Text("Ready to pair · \(Formatting.countdown(left))")
            .fixedSize(horizontal: false, vertical: true)
            .font(DSFont.rowTitle)
            .monospacedDigit()
            .foregroundStyle(DS.textPrimary)
            // A single Text is already one static-text element; making it
            // ignore its children too crashed SwiftUI's label lookup on the
            // Mac (a stack overflow under XCUITest, 2026-10-25).
            .accessibilityLabel(Text("Ready to pair"))
            .accessibilityValue(Self.minutesLeft(left))
            .accessibilityIdentifier("pair.countdown")
        }
    }

    /// Whole minutes, so the value does not change every second.
    private static func minutesLeft(_ seconds: Int) -> Text {
        let minutes = seconds / 60
        guard minutes > 0 else { return Text("Less than a minute left") }
        return Text("^[\(minutes) minute](inflect: true) left")
    }
}

/// "How to pair": three numbered steps, each read as one sentence.
private struct PairingSteps: View {
    @ScaledMetric(relativeTo: .body) private var badge: CGFloat = 26

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("How to pair")
                .font(DSFont.sectionHeading)
                .foregroundStyle(DS.textPrimary)
                .accessibilityAddTraits(.isHeader)
            step(1, String(localized: "Open Pair a device on the other device too."))
            step(2, String(localized: "On one of them, tap Pair next to the other."))
            step(3, String(localized: "Check that both screens show the same code. Then tap Codes match on both."))
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("pair.steps")
    }

    private func step(_ number: Int, _ text: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(number, format: .number)
                .font(DSFont.smallButton)
                .foregroundStyle(DS.accentText)
                .frame(width: badge, height: badge)
                .background(Circle().fill(DS.accentSoft))
            Text(verbatim: text)
                .font(DSFont.body)
                .foregroundStyle(DS.textPrimary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits(.isStaticText)
        .accessibilityLabel(Text(verbatim: "\(number.formatted()). \(text)"))
    }
}

/// What happened to the last request, while this device is ready to pair
/// again.
private struct PairingNote: View {
    let text: String
    let focus: AccessibilityFocusState<PairingFocus?>.Binding

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: Icons.info)
                .foregroundStyle(DS.accent)
                .accessibilityHidden(true)
            Text(verbatim: text)
                .font(DSFont.body)
                .foregroundStyle(DS.textPrimary)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
        .padding(16)
        .background(RoundedRectangle(cornerRadius: 18).fill(DS.surface))
        .overlay(RoundedRectangle(cornerRadius: 18).stroke(DS.border))
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("pair.note")
        .accessibilityFocused(focus, equals: .note)
        .task(id: text) { focus.wrappedValue = .note }
    }
}

/// Nearby devices, each with Pair, and the way to pair by address.
private struct PairingDeviceList: View {
    @Environment(AppModel.self) private var model
    @Environment(Router.self) private var router

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Nearby devices")
                .font(DSFont.sectionHeading)
                .foregroundStyle(DS.textPrimary)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("pair.list")
            if model.nearby.isEmpty {
                EmptyStateView(
                    symbol: Icons.nearby, title: String(localized: "Looking for devices…"),
                    message: String(localized: "Open Pair a device on the other device. Both need to be on the same Wi-Fi network."))
                    .accessibilityIdentifier("pair.empty")
            } else {
                ForEach(model.nearby, id: \.instance) { peer in
                    DSRow(title: peer.name, subtitle: nearbySubtitle(peer), titleIsPeerText: true) {
                        DeviceAvatar()
                    } trailing: {
                        Button("Pair") {
                            model.startPairing(with: .nearby(instance: peer.instance), name: peer.name)
                        }
                        .buttonStyle(SoftPillButtonStyle())
                        .disabled(!enabled || !peer.compatible)
                        .accessibilityLabel(Text("Pair with \(Text(verbatim: peer.name))"))
                        .accessibilityIdentifier("pair.peer")
                    }
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("pair.row")
                }
            }
            byAddress
        }
    }

    /// Not while this device is asking or comparing a code (the list is
    /// hidden then too).
    private var enabled: Bool {
        switch model.pairingFlow.phase {
        case .switching, .asking, .comparing: false
        case .idle, .opening, .waiting, .paused, .finished: true
        }
    }

    /// Android phones are not listed here: their Pair a device screen
    /// shows an address to type instead.
    private var byAddress: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button {
                router.openPairByAddress()
            } label: {
                HStack(spacing: 4) {
                    Text("Not listed? Pair by address")
                    Image(systemName: "chevron.right")
                        .font(.system(size: 11, weight: .semibold))
                        .accessibilityHidden(true)
                }
            }
            .buttonStyle(TextLinkButtonStyle())
            .accessibilityIdentifier("pair.byAddress")
            Text("Android phones aren't listed here. Type the address the phone shows on its Pair a device screen.")
                .font(DSFont.meta)
                .foregroundStyle(DS.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.top, 8)
    }
}

/// Asking a device, or comparing the code.
private struct StatusCard: View {
    let title: Text
    let line: Text
    let cancel: (() -> Void)?

    var body: some View {
        VStack(spacing: 12) {
            ProgressView()
                .accessibilityHidden(true)
            title
                .font(DSFont.sectionHeading)
                .foregroundStyle(DS.textPrimary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityAddTraits(.isHeader)
            line
                .font(DSFont.body)
                .foregroundStyle(DS.textSecondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
            if let cancel {
                Button("Cancel", action: cancel)
                    .buttonStyle(OutlinePillButtonStyle())
                    .frame(maxWidth: 260)
                    .accessibilityIdentifier("pair.cancel")
            }
        }
        .frame(maxWidth: .infinity)
        .pairingCard()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("pair.status")
    }
}

/// How an attempt ended (Done and Try again), or why this device is not
/// ready to pair (one button). Its title takes VoiceOver's focus.
private struct ResultCard: View {
    @Environment(AppModel.self) private var model
    let outcome: PairingOutcome
    let paused: Bool
    let focus: AccessibilityFocusState<PairingFocus?>.Binding
    let done: () -> Void
    let again: () -> Void

    var body: some View {
        let text = outcome.text
        VStack(spacing: 12) {
            Image(systemName: symbol(text))
                .font(.system(size: 34, weight: .medium))
                .foregroundStyle(color(text))
                .accessibilityHidden(true)
            Text(verbatim: text.title)
                .font(DSFont.sectionHeading)
                .foregroundStyle(DS.textPrimary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityAddTraits(.isHeader)
                .accessibilityIdentifier("pair.result.title")
                .accessibilityFocused(focus, equals: .result)
            Text(verbatim: text.message)
                .font(DSFont.body)
                .foregroundStyle(DS.textSecondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
            VStack(spacing: 10) { buttons }
                .frame(maxWidth: 260)
        }
        .frame(maxWidth: .infinity)
        .pairingCard()
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("pair.result")
        .task(id: outcome) { focus.wrappedValue = .result }
    }

    @ViewBuilder
    private var buttons: some View {
        if paused {
            againButton(primary: true)
        } else {
            switch outcome {
            case .paired, .youDeclined:
                doneButton(primary: true)
                againButton(primary: false)
            default:
                againButton(primary: true)
                doneButton(primary: false)
            }
        }
    }

    @ViewBuilder
    private func doneButton(primary: Bool) -> some View {
        let button = Button("Done", action: done)
            #if os(macOS)
            // Never while a prompt is up: there, Escape means no.
            .keyboardShortcut(model.currentPrompt == nil ? .cancelAction : nil)
            #endif
            .accessibilityIdentifier("pair.done")
        if primary {
            button.buttonStyle(PrimaryPillButtonStyle())
        } else {
            button.buttonStyle(OutlinePillButtonStyle())
        }
    }

    @ViewBuilder
    private func againButton(primary: Bool) -> some View {
        let button = Button(againTitle, action: again)
            .accessibilityIdentifier("pair.again")
        if primary {
            button.buttonStyle(PrimaryPillButtonStyle())
        } else {
            button.buttonStyle(OutlinePillButtonStyle())
        }
    }

    private var againTitle: LocalizedStringKey {
        switch outcome {
        case .paired: "Pair another device"
        case .timeUp, .usedUp: "Start again"
        default: "Try again"
        }
    }

    private func symbol(_ text: ErrorText) -> String {
        if text.isSecurity { return Icons.warning }
        switch outcome {
        case .paired: return Icons.check
        case .codeExpired, .timeUp: return Icons.clock
        case .youDeclined, .otherDeclined, .cancelled: return Icons.failed
        case .connectionLost, .failed: return Icons.warning
        default: return Icons.info
        }
    }

    /// `danger` only for a possible attack.
    private func color(_ text: ErrorText) -> Color {
        if text.isSecurity { return DS.danger }
        if case .paired = outcome { return DS.success }
        return DS.textSecondary
    }
}

private extension View {
    /// A pairing screen card: `surface`, rounded, with a border.
    func pairingCard() -> some View {
        padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(RoundedRectangle(cornerRadius: DS.Metrics.cardRadius).fill(DS.surface))
            .overlay(RoundedRectangle(cornerRadius: DS.Metrics.cardRadius).stroke(DS.border))
    }
}
