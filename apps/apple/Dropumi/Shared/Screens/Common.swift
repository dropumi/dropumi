// SPDX-License-Identifier: MPL-2.0
//
// Pieces several screens share.

import DropumiAppState
import DropumiBindings
import DropumiDesign
import SwiftUI

/// The big title and subtitle at the top of a screen.
struct ScreenHeader: View {
    let title: LocalizedStringKey
    let subtitle: Text?

    init(_ title: LocalizedStringKey, subtitle: Text? = nil) {
        self.title = title
        self.subtitle = subtitle
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(DSFont.screenTitle)
                .foregroundStyle(DS.textPrimary)
                .accessibilityAddTraits(.isHeader)
            if let subtitle {
                subtitle
                    .font(DSFont.body)
                    .foregroundStyle(DS.textSecondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A scrolling screen with the design's padding and background.
struct ScreenScroll<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: DS.Metrics.sectionGap) {
                content
            }
            .padding(.horizontal, padding)
            .padding(.top, 8)
            .padding(.bottom, 24)
            .frame(maxWidth: 760, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .background(DS.bg)
    }

    private var padding: CGFloat {
        #if os(macOS)
        DS.Metrics.desktopPadding
        #else
        DS.Metrics.screenPadding
        #endif
    }
}

/// design.md segmented control: a `surface-2` track with a `surface`
/// selected segment.
struct DSSegmented<Value: Hashable>: View {
    let options: [(Value, LocalizedStringKey)]
    @Binding var selection: Value

    var body: some View {
        HStack(spacing: 4) {
            ForEach(Array(options.enumerated()), id: \.offset) { _, option in
                let selected = option.0 == selection
                Button {
                    selection = option.0
                } label: {
                    Text(option.1)
                        .font(DSFont.smallButton)
                        .foregroundStyle(selected ? DS.textPrimary : DS.textSecondary)
                        .frame(maxWidth: .infinity, minHeight: 36)
                        .background(
                            RoundedRectangle(cornerRadius: 10)
                                .fill(selected ? DS.surface : .clear)
                                .shadow(color: selected ? DS.navy.opacity(0.08) : .clear, radius: 4, y: 1))
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(selected ? [.isSelected] : [])
            }
        }
        .padding(4)
        .background(RoundedRectangle(cornerRadius: 13).fill(DS.surface2))
    }
}

/// A settings row: icon, title, subtitle, and a chevron or a trailing view.
struct SettingsRow<Trailing: View>: View {
    let symbol: String
    let title: LocalizedStringKey
    let subtitle: Text?
    @ViewBuilder let trailing: Trailing

    var body: some View {
        HStack(spacing: 14) {
            Image(systemName: symbol)
                .font(.system(size: 17, weight: .medium))
                .foregroundStyle(DS.accent)
                .frame(width: 24)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(DSFont.rowTitle)
                    .foregroundStyle(DS.textPrimary)
                if let subtitle {
                    subtitle
                        .font(DSFont.meta)
                        .foregroundStyle(DS.textSecondary)
                }
            }
            Spacer(minLength: 8)
            trailing
        }
        .padding(.vertical, DS.Metrics.rowPadding)
        .contentShape(Rectangle())
        .overlay(alignment: .bottom) { Rectangle().fill(DS.border).frame(height: 1) }
    }
}

extension SettingsRow where Trailing == ChevronView {
    init(symbol: String, title: LocalizedStringKey, subtitle: Text?) {
        self.init(symbol: symbol, title: title, subtitle: subtitle) { ChevronView() }
    }
}

struct ChevronView: View {
    var body: some View {
        Image(systemName: "chevron.right")
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(DS.textSecondary)
            .accessibilityHidden(true)
    }
}

/// The banner at the top of the window.
struct BannerHost: View {
    @Bindable var model: AppModel
    @Environment(Router.self) private var router

    var body: some View {
        if let banner = model.banner {
            BannerView(
                title: banner.title, message: banner.message, danger: banner.style == .danger,
                actionTitle: actionTitle(banner.action), action: banner.action.map { action in { perform(action) } },
                dismiss: { model.banner = nil })
                .padding(.horizontal, 16)
                .padding(.top, 8)
                .frame(maxWidth: 560)
                .transition(.move(edge: .top).combined(with: .opacity))
                .accessibilityIdentifier("banner")
        }
    }

    private func actionTitle(_ action: Banner.Action?) -> String? {
        switch action {
        case .retryReceiving: return String(localized: "Try again")
        case .openEveryone: return String(localized: "Let everyone send for 10 minutes")
        case .openSettings: return String(localized: "Open Settings")
        case nil: return nil
        }
    }

    private func perform(_ action: Banner.Action) {
        model.banner = nil
        switch action {
        case .retryReceiving:
            Task { await model.applyVisibility() }
        case .openEveryone:
            Task { await model.setVisibility(.everyone) }
        case .openSettings:
            router.tab = .settings
        }
    }
}

/// Who may send, as the status pill and the sidebar say it.
struct VisibilityText {
    static func label(_ model: AppModel, now: Date) -> Text {
        guard model.receiver != nil else {
            return Text(model.visibility == .off ? "Not receiving" : "Starting…")
        }
        switch model.visibility {
        case .off:
            return Text("Not receiving")
        case .pairedOnly:
            return Text("Paired devices only")
        case .everyone:
            if let left = everyoneSecondsLeft(model, now: now) {
                return Text("Everyone · \(Formatting.countdown(left))")
            }
            return Text("Everyone")
        }
    }

    static func everyoneSecondsLeft(_ model: AppModel, now: Date) -> Int? {
        guard let until = model.receiver?.everyoneUntil else { return nil }
        return max(0, Int((Double(until) / 1000 - now.timeIntervalSince1970).rounded(.down)))
    }
}

/// The status pill: who may send to this device right now.
struct VisibilityPill: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            HStack(spacing: 8) {
                Circle()
                    .fill(model.receiver != nil ? DS.success : DS.textSecondary)
                    .frame(width: 8, height: 8)
                VisibilityText.label(model, now: context.date)
                    .font(DSFont.meta)
                    .monospacedDigit()
                    .foregroundStyle(DS.textPrimary)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 8)
            .background(Capsule().fill(DS.surface2))
        }
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("visibility.pill")
    }
}

/// This device, at the bottom of the Mac sidebar.
struct ThisDeviceSummary: View {
    @Environment(AppModel.self) private var model
    @Environment(Router.self) private var router

    var body: some View {
        Button {
            router.tab = .settings
            router.settingsPath = [.thisDevice]
        } label: {
            HStack(spacing: 10) {
                DeviceAvatar(symbol: Icons.device, size: 34)
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: model.identity?.name ?? "")
                        .font(DSFont.smallButton)
                        .foregroundStyle(DS.textPrimary)
                        .lineLimit(1)
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        VisibilityText.label(model, now: context.date)
                            .font(DSFont.meta)
                            .monospacedDigit()
                            .foregroundStyle(DS.textSecondary)
                    }
                }
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.vertical, 8)
    }
}

/// The text of a nearby device's second line. A hint that matches a paired
/// device only means "looks like" until the handshake checks the key (D9).
func nearbySubtitle(_ peer: NearbyPeer) -> String {
    if !peer.compatible {
        return String(localized: "Needs a newer version of Dropumi")
    }
    if let alias = peer.hintAlias {
        return String(localized: "Looks like \(alias)")
    }
    return String(localized: "Not paired")
}

extension View {
    /// An icon-only "…" menu. On macOS a Menu with a custom label offers only
    /// AXPress, not AXShowMenu, so VoiceOver's Open Menu has nothing to call
    /// (the accessibility audit's "Action is missing"; macOS 27.0.1,
    /// 2026-10-18). The button menu style with a plain button style offers
    /// both and draws the label as it is.
    @ViewBuilder
    func iconMenu() -> some View {
        #if os(macOS)
        menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden)
        #else
        self
        #endif
    }
}
