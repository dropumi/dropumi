// SPDX-License-Identifier: MPL-2.0
//
// Offers and codes, one at a time (ADR-0004, ADR-0009):
// - Nothing is answered without a tap; at zero the prompt reads "Expired"
//   and stays until the core closes it.
// - No default button: Return never accepts. Escape declines.
// - Accept and "Codes match" are enabled 600 ms after the prompt appears,
//   so typing or tapping ahead cannot accept by accident.
// - The prompt goes away only when the core sends PromptClosed.

import DropumiAppState
import DropumiBindings
import DropumiDesign
import SwiftUI

/// The prompt's content scrolls (large text never clips); its two buttons
/// stay at the bottom, side by side, or stacked at accessibility sizes.
private struct PromptLayout<Content: View, Buttons: View>: View {
    @Environment(\.dynamicTypeSize) private var typeSize
    @ViewBuilder let content: Content
    @ViewBuilder let buttons: Buttons

    var body: some View {
        VStack(spacing: 12) {
            ScrollView {
                content
                    .padding(.horizontal, DS.Metrics.screenPadding)
                    .padding(.vertical, 16)
                    .frame(maxWidth: 520)
                    .frame(maxWidth: .infinity)
            }
            .scrollBounceBehavior(.basedOnSize)
            // Side by side at the default text sizes, stacked above them.
            let layout = typeSize > .large
                ? AnyLayout(VStackLayout(spacing: 12)) : AnyLayout(HStackLayout(spacing: 12))
            layout { buttons }
                .frame(maxWidth: 520)
                .padding(.horizontal, DS.Metrics.screenPadding)
                .padding(.bottom, DS.Metrics.screenPadding)
        }
    }
}

struct PromptItem: Identifiable {
    let prompt: Prompt
    var id: UInt64 { prompt.promptId }
}

struct PromptPresenter: ViewModifier {
    let model: AppModel

    func body(content: Content) -> some View {
        let item = Binding<PromptItem?>(
            get: { model.currentPrompt.map(PromptItem.init) },
            // Only the core closes a prompt.
            set: { _ in })
        #if os(iOS)
        content.fullScreenCover(item: item) { item in
            PromptScreen(model: model, prompt: item.prompt)
                .interactiveDismissDisabled()
        }
        #else
        content.sheet(item: item) { item in
            PromptScreen(model: model, prompt: item.prompt)
                .interactiveDismissDisabled()
                .frame(width: 440)
        }
        #endif
    }
}

/// How long Accept stays disabled after a prompt appears.
let promptArmDelay: Duration = .milliseconds(600)

/// The arming delay in force. UI tests (DEBUG builds only) may make it
/// longer, never shorter.
private var armDelay: Duration {
    #if DEBUG
    if let longer = UITestScenario.armDelayOverride { return longer }
    #endif
    return promptArmDelay
}

struct PromptScreen: View {
    let model: AppModel
    let prompt: Prompt
    @State private var armed = false

    #if os(iOS)
    private let onNavy = true
    #else
    private let onNavy = false
    #endif

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            content(secondsLeft: secondsLeft(at: context.date))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(onNavy ? DS.Receive.background : DS.surface)
        .environment(\.colorScheme, onNavy ? .dark : colorScheme)
        .task(id: prompt.promptId) {
            armed = false
            try? await Task.sleep(for: armDelay)
            armed = true
        }
    }

    @Environment(\.colorScheme) private var colorScheme

    private var answering: Bool { model.answering.contains(prompt.promptId) }

    private func secondsLeft(at date: Date) -> Int {
        let expires: UInt64
        switch prompt {
        case let .offer(_, _, _, _, _, _, _, expiresAtMs): expires = expiresAtMs
        case let .code(_, _, _, _, _, expiresAtMs): expires = expiresAtMs
        }
        let left = Double(expires) / 1000 - date.timeIntervalSince1970
        return max(0, Int(left.rounded(.down)))
    }

    @ViewBuilder
    private func content(secondsLeft: Int) -> some View {
        switch prompt {
        case let .offer(_, _, peer, code, files, fileCount, totalBytes, _):
            OfferContent(
                peer: peer, code: code, files: files, fileCount: Int(fileCount), totalBytes: totalBytes,
                onNavy: onNavy, secondsLeft: secondsLeft, armed: armed, answering: answering,
                answer: answer)
        case let .code(_, _, peer, code, purpose, _):
            CodeContent(
                peer: peer, code: code, purpose: purpose, onNavy: onNavy, secondsLeft: secondsLeft,
                armed: armed, answering: answering, answer: answer)
        }
    }

    private func answer(_ yes: Bool) {
        Task { await model.answer(prompt, yes: yes) }
    }
}

private struct OfferContent: View {
    @Environment(\.dynamicTypeSize) private var typeSize
    let peer: PeerInfo
    let code: String?
    let files: [OfferedFile]
    let fileCount: Int
    let totalBytes: UInt64
    let onNavy: Bool
    let secondsLeft: Int
    let armed: Bool
    let answering: Bool
    let answer: (Bool) -> Void

    var body: some View {
        let label = PeerLabel(peer: peer)
        PromptLayout {
            VStack(spacing: 18) {
                Text("Incoming files")
                    .font(DSFont.smallButton)
                    .foregroundStyle(onNavy ? DS.Receive.accent : DS.accentText)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 6)
                    .background(Capsule().fill(onNavy ? DS.Receive.card : DS.accentSoft))
                DeviceAvatar(symbol: Icons.device, size: 72)
                PeerIdentityView(
                    title: label.title, claim: label.claim, verified: label.verified,
                    fingerprint: label.verified ? nil : peer.fingerprintDisplay, onNavy: onNavy)
                Text("wants to send you ^[\(fileCount) file](inflect: true)")
                    .font(DSFont.body)
                    .foregroundStyle(secondary)
                if let code {
                    VStack(spacing: 6) {
                        Text("Compare this code with the sender's screen")
                            .font(DSFont.meta)
                            .foregroundStyle(secondary)
                        SasCodeView(code, foreground: onNavy ? .white : DS.textPrimary)
                        Text("Accept only if both screens show the same code.")
                            .font(DSFont.meta)
                            .foregroundStyle(secondary)
                    }
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                }
                fileList
                Text(secondsLeft > 0 ? "Expires in \(Formatting.countdown(secondsLeft))" : "Expired")
                    .font(DSFont.meta)
                    .monospacedDigit()
                    .foregroundStyle(secondary)
            }
        } buttons: {
            Button("Decline") { answer(false) }
                .buttonStyle(OutlinePillButtonStyle(
                    foreground: onNavy ? .white : DS.textPrimary,
                    outline: onNavy ? DS.Receive.outline : DS.border))
                .keyboardShortcut(.cancelAction)
                .disabled(answering)
                .accessibilityIdentifier("offer.decline")
            Button {
                answer(true)
            } label: {
                Label("Accept", systemImage: "checkmark")
            }
            .buttonStyle(PrimaryPillButtonStyle())
            .disabled(answering || !armed || secondsLeft == 0)
            .accessibilityIdentifier("offer.accept")
        }
    }

    private var secondary: Color { onNavy ? DS.Receive.muted : DS.textSecondary }

    private var fileList: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(Array(files.enumerated()), id: \.offset) { _, file in
                        HStack(spacing: 10) {
                            Image(systemName: Icons.file(named: file.name))
                                .foregroundStyle(onNavy ? DS.Receive.accent : DS.accent)
                                .accessibilityHidden(true)
                            Text(verbatim: file.name)
                                .font(DSFont.body)
                                .foregroundStyle(onNavy ? .white : DS.textPrimary)
                                .lineLimit(1)
                                .truncationMode(.middle)
                            Spacer()
                            Text(Formatting.bytes(file.size))
                                .font(DSFont.meta)
                                .foregroundStyle(secondary)
                        }
                        .padding(.vertical, 9)
                        .accessibilityElement(children: .combine)
                    }
                    if fileCount > files.count {
                        Text("and \(fileCount - files.count) more")
                            .font(DSFont.meta)
                            .foregroundStyle(secondary)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(.vertical, 9)
                    }
                }
            }
            .frame(maxHeight: typeSize.isAccessibilitySize ? nil : 220)
            Rectangle().fill(onNavy ? DS.Receive.divider : DS.border).frame(height: 1)
            HStack {
                Text("Total").font(DSFont.meta).foregroundStyle(secondary)
                Spacer()
                Text(Formatting.bytes(totalBytes))
                    .font(DSFont.rowTitle)
                    .foregroundStyle(onNavy ? .white : DS.textPrimary)
            }
            .padding(.top, 10)
        }
        .padding(16)
        .background(RoundedRectangle(cornerRadius: 18).fill(onNavy ? DS.Receive.card : DS.surface2))
    }
}

private struct CodeContent: View {
    let peer: PeerInfo
    let code: String
    let purpose: CodePurpose
    let onNavy: Bool
    let secondsLeft: Int
    let armed: Bool
    let answering: Bool
    let answer: (Bool) -> Void

    var body: some View {
        let label = PeerLabel(peer: peer)
        PromptLayout {
            VStack(spacing: 20) {
                Image(systemName: Icons.shield)
                    .font(.system(size: 34, weight: .medium))
                    .foregroundStyle(onNavy ? DS.Receive.accent : DS.accent)
                    .accessibilityHidden(true)
                    .padding(.top, 40)
                Text(purpose == .pairing ? "Pair with this device?" : "Check the code")
                    .font(DSFont.sectionHeading)
                    .foregroundStyle(onNavy ? .white : DS.textPrimary)
                PeerIdentityView(
                    title: label.title, claim: label.claim, verified: label.verified,
                    fingerprint: peer.fingerprintDisplay, onNavy: onNavy)
                SasCodeView(code, foreground: onNavy ? .white : DS.textPrimary)
                Text(purpose == .pairing
                     ? "Check that the other device shows the same code. Only then tap Codes match."
                     : "This device isn't paired. Check that its screen shows the same code before anything is sent.")
                    .font(DSFont.body)
                    .foregroundStyle(secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                Text(secondsLeft > 0 ? "Expires in \(Formatting.countdown(secondsLeft))" : "Expired")
                    .font(DSFont.meta)
                    .monospacedDigit()
                    .foregroundStyle(secondary)
            }
        } buttons: {
            Button("Codes don't match") { answer(false) }
                .buttonStyle(OutlinePillButtonStyle(
                    foreground: onNavy ? .white : DS.textPrimary,
                    outline: onNavy ? DS.Receive.outline : DS.border))
                .keyboardShortcut(.cancelAction)
                .disabled(answering)
                .accessibilityIdentifier("code.mismatch")
            Button("Codes match") { answer(true) }
                .buttonStyle(OutlinePillButtonStyle(
                    foreground: onNavy ? DS.Receive.accent : DS.accent,
                    outline: onNavy ? DS.Receive.accent : DS.accent))
                .disabled(answering || !armed || secondsLeft == 0)
                .accessibilityIdentifier("code.match")
        }
    }

    private var secondary: Color { onNavy ? DS.Receive.muted : DS.textSecondary }
}
