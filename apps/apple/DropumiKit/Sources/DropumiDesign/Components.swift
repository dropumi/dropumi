// SPDX-License-Identifier: MPL-2.0
//
// Components from design.md. Peer-supplied strings are always rendered with
// `Text(verbatim:)` (never Markdown, never a localization key), so `**x**`
// or `[a](b)` in a device name shows literally.

import SwiftUI
import UniformTypeIdentifiers

// MARK: Buttons

/// Accent fill, pill shape: the main action of a screen.
public struct PrimaryPillButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled
    public init() {}
    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(DSFont.button)
            .multilineTextAlignment(.center)
            .foregroundStyle(DS.onAccent)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, minHeight: 52)
            .padding(.horizontal, 14)
            .background(Capsule().fill(DS.accent.opacity(isEnabled ? 1 : 0.4)))
            .opacity(configuration.isPressed ? 0.85 : 1)
    }
}

/// A text link ("Pair a device", "Add by fingerprint"): accent text with a
/// 44 pt tall hit area, the minimum the accessibility audit accepts.
public struct TextLinkButtonStyle: ButtonStyle {
    public init() {}
    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(DSFont.smallButton)
            .foregroundStyle(DS.accentText)
            .frame(minHeight: 44)
            .contentShape(Rectangle())
            .opacity(configuration.isPressed ? 0.6 : 1)
    }
}

/// `accent-soft` fill with `accent` text: "Send" pills and secondary actions.
public struct SoftPillButtonStyle: ButtonStyle {
    public init() {}
    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(DSFont.smallButton)
            .foregroundStyle(DS.accentText)
            .padding(.horizontal, 16)
            .frame(minHeight: 34)
            .background(Capsule().fill(DS.accentSoft))
            .frame(minHeight: 44)
            .contentShape(Rectangle())
            .opacity(configuration.isPressed ? 0.8 : 1)
    }
}

/// An outlined pill, used for the second of two equal choices.
public struct OutlinePillButtonStyle: ButtonStyle {
    let foreground: Color
    let outline: Color
    public init(foreground: Color = DS.textPrimary, outline: Color = DS.border) {
        self.foreground = foreground
        self.outline = outline
    }
    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(DSFont.button)
            .multilineTextAlignment(.center)
            .foregroundStyle(foreground)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity, minHeight: 52)
            .padding(.horizontal, 14)
            .background(Capsule().stroke(outline, lineWidth: 1.5))
            .contentShape(Capsule())
            .opacity(configuration.isPressed ? 0.8 : 1)
    }
}

// MARK: Status and labels

/// The status pill at the top of Nearby ("Paired devices only").
public struct StatusPill: View {
    let text: String
    let active: Bool
    public init(_ text: String, active: Bool) {
        self.text = text
        self.active = active
    }
    public var body: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(active ? DS.success : DS.textSecondary)
                .frame(width: 8, height: 8)
            Text(text)
                .font(DSFont.meta)
                .foregroundStyle(DS.textPrimary)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(Capsule().fill(DS.surface2))
        .accessibilityElement(children: .combine)
    }
}

/// An uppercase section label in `text-secondary`.
public struct SectionLabel: View {
    let text: String
    public init(_ text: String) { self.text = text }
    public var body: some View {
        Text(text.uppercased())
            .font(DSFont.sectionLabel)
            .foregroundStyle(DS.textSecondary)
            .accessibilityAddTraits(.isHeader)
    }
}

// MARK: Rows and tiles

/// A tinted disc with a device symbol.
public struct DeviceAvatar: View {
    let symbol: String
    let size: CGFloat
    public init(symbol: String = Icons.device, size: CGFloat = 44) {
        self.symbol = symbol
        self.size = size
    }
    public var body: some View {
        Image(systemName: symbol)
            .font(.system(size: size * 0.42, weight: .medium))
            .foregroundStyle(DS.accentText)
            .frame(width: size, height: size)
            .background(Circle().fill(DS.accentSoft))
            .accessibilityHidden(true)
    }
}

/// A rounded tile with a file-type symbol.
public struct FileTile: View {
    let symbol: String
    let muted: Bool
    public init(symbol: String, muted: Bool = false) {
        self.symbol = symbol
        self.muted = muted
    }
    public var body: some View {
        Image(systemName: symbol)
            .font(.system(size: 18, weight: .medium))
            .foregroundStyle(muted ? DS.textSecondary : DS.accentText)
            .frame(width: 44, height: 44)
            .background(RoundedRectangle(cornerRadius: DS.Metrics.tileRadius).fill(muted ? DS.surface2 : DS.accentSoft))
            .accessibilityHidden(true)
    }
}

/// A list row with a leading view, a title, a subtitle and a trailing view.
public struct DSRow<Leading: View, Trailing: View>: View {
    let title: String
    let subtitle: String?
    let titleIsPeerText: Bool
    let leading: Leading
    let trailing: Trailing

    /// `titleIsPeerText`: the title came from another device and is shown
    /// verbatim.
    public init(
        title: String, subtitle: String?, titleIsPeerText: Bool = false,
        @ViewBuilder leading: () -> Leading, @ViewBuilder trailing: () -> Trailing
    ) {
        self.title = title
        self.subtitle = subtitle
        self.titleIsPeerText = titleIsPeerText
        self.leading = leading()
        self.trailing = trailing()
    }

    @Environment(\.dynamicTypeSize) private var typeSize

    public var body: some View {
        Group {
            if typeSize.isAccessibilitySize {
                // Large text: the trailing control goes below, and nothing
                // is cut off.
                VStack(alignment: .leading, spacing: 8) {
                    HStack(alignment: .top, spacing: 14) {
                        leading
                        texts(lineLimit: nil)
                    }
                    trailing
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                HStack(spacing: 14) {
                    leading
                    texts(lineLimit: 1)
                    Spacer(minLength: 8)
                    trailing
                }
            }
        }
        .padding(.vertical, DS.Metrics.rowPadding)
        .overlay(alignment: .bottom) { Rectangle().fill(DS.border).frame(height: 1) }
    }

    private func texts(lineLimit: Int?) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(verbatim: title)
                .font(DSFont.rowTitle)
                .foregroundStyle(DS.textPrimary)
                .lineLimit(lineLimit)
                .truncationMode(.middle)
            if let subtitle {
                Text(verbatim: subtitle)
                    .font(DSFont.meta)
                    .foregroundStyle(DS.textSecondary)
                    .lineLimit(lineLimit.map { $0 + 1 })
            }
        }
        .fixedSize(horizontal: false, vertical: true)
    }
}

/// An 8 pt track with an accent fill.
public struct DSProgressBar: View {
    let fraction: Double?
    public init(_ fraction: Double?) { self.fraction = fraction }
    public var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(DS.surface2)
                Capsule().fill(DS.accent)
                    .frame(width: geo.size.width * CGFloat(min(1, max(0, fraction ?? 0))))
            }
        }
        .frame(height: 8)
        .accessibilityHidden(true)
    }
}

// MARK: Codes and identity

/// The six-digit code, large and monospaced, read digit by digit.
public struct SasCodeView: View {
    let digits: String
    let foreground: Color
    /// 40 pt at the default text size; follows Dynamic Type.
    @ScaledMetric(relativeTo: .largeTitle) private var size: CGFloat = 40
    public init(_ digits: String, foreground: Color = DS.textPrimary) {
        self.digits = digits
        self.foreground = foreground
    }
    public var body: some View {
        Text(verbatim: grouped)
            .font(.system(size: size, weight: .bold, design: .monospaced))
            .minimumScaleFactor(0.5)
            .lineLimit(1)
            .monospacedDigit()
            .foregroundStyle(foreground)
            .tracking(2)
            .accessibilityLabel(Text("Verification code: \(spoken)"))
            .accessibilityIdentifier("code.digits")
    }
    private var grouped: String {
        digits.count == 6 ? "\(digits.prefix(3)) \(digits.suffix(3))" : digits
    }
    private var spoken: String {
        guard digits.count == 6 else { return digits }
        return digits.prefix(3).map(String.init).joined(separator: " ") + ", "
            + digits.suffix(3).map(String.init).joined(separator: " ")
    }
}

/// How a peer is shown (ADR-0004): the local alias first and "Paired", or
/// "Unverified device" with the name it claims and its fingerprint.
public struct PeerIdentityView: View {
    let title: String
    let claim: String?
    let verified: Bool
    let fingerprint: String?
    let onNavy: Bool

    public init(title: String, claim: String?, verified: Bool, fingerprint: String?, onNavy: Bool = false) {
        self.title = title
        self.claim = claim
        self.verified = verified
        self.fingerprint = fingerprint
        self.onNavy = onNavy
    }

    public var body: some View {
        VStack(spacing: 6) {
            Text(verbatim: title)
                .font(DSFont.dialogName)
                .foregroundStyle(onNavy ? Color.white : DS.textPrimary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
            if let claim {
                (Text("claims ") + Text(verbatim: "“\(claim)”"))
                    .font(DSFont.body)
                    .foregroundStyle(onNavy ? DS.Receive.muted : DS.textSecondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Label(verified ? "Paired" : "Not paired", systemImage: verified ? Icons.shield : Icons.warning)
                .font(DSFont.smallButton)
                .foregroundStyle(verified ? (onNavy ? DS.Receive.accent : DS.success) : (onNavy ? DS.Receive.muted : DS.textSecondary))
            if let fingerprint {
                Text(verbatim: fingerprint)
                    .font(DSFont.fingerprint)
                    .foregroundStyle(onNavy ? DS.Receive.muted : DS.textSecondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityLabel(Text("Fingerprint \(fingerprint)"))
            }
        }
        .accessibilityElement(children: .combine)
    }
}

// MARK: Banners and empty states

public struct BannerView: View {
    let title: String
    let message: String
    let danger: Bool
    let actionTitle: String?
    let action: (() -> Void)?
    let dismiss: () -> Void

    public init(title: String, message: String, danger: Bool, actionTitle: String?, action: (() -> Void)?, dismiss: @escaping () -> Void) {
        self.title = title
        self.message = message
        self.danger = danger
        self.actionTitle = actionTitle
        self.action = action
        self.dismiss = dismiss
    }

    public var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: danger ? Icons.warning : Icons.info)
                .foregroundStyle(danger ? DS.danger : DS.accent)
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(DSFont.rowTitle).foregroundStyle(DS.textPrimary)
                Text(message).font(DSFont.meta).foregroundStyle(DS.textSecondary)
                if let actionTitle, let action {
                    Button(actionTitle, action: action)
                        .font(DSFont.smallButton)
                        .foregroundStyle(DS.accentText)
                        .frame(minHeight: 44)
                        .accessibilityIdentifier("banner.action")
                }
            }
            Spacer()
            Button(action: dismiss) {
                Image(systemName: "xmark")
                    .foregroundStyle(DS.textSecondary)
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(Text("Dismiss"))
        }
        .padding(14)
        .background(
            RoundedRectangle(cornerRadius: 16)
                .fill(DS.surface)
                .shadow(color: DS.navy.opacity(0.08), radius: 12, y: 4))
        .overlay(RoundedRectangle(cornerRadius: 16).stroke(DS.border))
    }
}

public struct EmptyStateView: View {
    let symbol: String
    let title: String
    let message: String
    public init(symbol: String, title: String, message: String) {
        self.symbol = symbol
        self.title = title
        self.message = message
    }
    public var body: some View {
        VStack(spacing: 10) {
            Image(systemName: symbol)
                .font(.system(size: 30, weight: .medium))
                .foregroundStyle(DS.accent)
                .accessibilityHidden(true)
            Text(title).font(DSFont.rowTitle).foregroundStyle(DS.textPrimary)
            Text(message)
                .font(DSFont.meta)
                .foregroundStyle(DS.textSecondary)
                .multilineTextAlignment(.center)
        }
        .padding(24)
        .frame(maxWidth: .infinity)
    }
}

// MARK: Icons

/// SF Symbols for the Lucide icons named in design.md.
public enum Icons {
    public static let nearby = "dot.radiowaves.left.and.right"
    public static let transfers = "arrow.left.arrow.right"
    public static let settings = "gearshape"
    public static let device = "laptopcomputer.and.iphone"
    public static let upload = "square.and.arrow.up"
    public static let send = "paperplane"
    public static let check = "checkmark.circle"
    public static let shield = "checkmark.shield"
    public static let warning = "exclamationmark.triangle"
    public static let info = "info.circle"
    public static let folder = "folder"
    public static let receiveFolder = "tray.and.arrow.down"
    public static let code = "chevron.left.forwardslash.chevron.right"
    public static let pair = "link"
    public static let add = "plus"
    public static let close = "xmark"
    public static let failed = "xmark.circle"
    public static let partial = "exclamationmark.circle"
    public static let clock = "clock"

    /// A symbol for a file name, by its type.
    public static func file(named name: String) -> String {
        let ext = (name as NSString).pathExtension
        guard let type = UTType(filenameExtension: ext) else { return "doc" }
        if type.conforms(to: .image) { return "photo" }
        if type.conforms(to: .movie) || type.conforms(to: .video) { return "video" }
        if type.conforms(to: .audio) { return "music.note" }
        if type.conforms(to: .archive) { return "archivebox" }
        if type.conforms(to: .text) || type.conforms(to: .pdf) { return "doc.text" }
        return "doc"
    }
}
