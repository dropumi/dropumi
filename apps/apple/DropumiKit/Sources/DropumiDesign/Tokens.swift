// SPDX-License-Identifier: MPL-2.0
//
// The design.md tokens as SwiftUI colours that follow light and dark mode.
// Use them by name; never use the raw values in views.

import SwiftUI
#if canImport(UIKit)
import UIKit
#elseif canImport(AppKit)
import AppKit
#endif

extension Color {
    /// A colour with a light and a dark value (`0xRRGGBB`, optional alpha).
    init(light: UInt32, dark: UInt32, alpha: Double = 1) {
        #if canImport(UIKit)
        self.init(uiColor: UIColor { trait in
            UIColor(rgb: trait.userInterfaceStyle == .dark ? dark : light, alpha: alpha)
        })
        #else
        self.init(nsColor: NSColor(name: nil) { appearance in
            let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            return NSColor(rgb: isDark ? dark : light, alpha: alpha)
        })
        #endif
    }

    init(fixed: UInt32) {
        self.init(
            .sRGB,
            red: Double((fixed >> 16) & 0xFF) / 255,
            green: Double((fixed >> 8) & 0xFF) / 255,
            blue: Double(fixed & 0xFF) / 255,
            opacity: 1)
    }
}

#if canImport(UIKit)
extension UIColor {
    convenience init(rgb: UInt32, alpha: Double) {
        self.init(
            red: CGFloat((rgb >> 16) & 0xFF) / 255, green: CGFloat((rgb >> 8) & 0xFF) / 255,
            blue: CGFloat(rgb & 0xFF) / 255, alpha: CGFloat(alpha))
    }
}
#else
extension NSColor {
    convenience init(rgb: UInt32, alpha: Double) {
        self.init(
            srgbRed: CGFloat((rgb >> 16) & 0xFF) / 255, green: CGFloat((rgb >> 8) & 0xFF) / 255,
            blue: CGFloat(rgb & 0xFF) / 255, alpha: CGFloat(alpha))
    }
}
#endif

/// design.md colour tokens.
public enum DS {
    public static let bg = Color(light: 0xF4F8FA, dark: 0x07121F)
    public static let surface = Color(light: 0xFFFFFF, dark: 0x0F2238)
    public static let surface2 = Color(light: 0xE8F0F4, dark: 0x162C45)
    public static let textPrimary = Color(light: 0x0B1F3A, dark: 0xF2F7FA)
    public static let textSecondary = Color(light: 0x5B6B7D, dark: 0x9DB0C2)
    public static let border = Color(light: 0xDCE5EB, dark: 0x1F3652)
    /// Light accent is about 4.1:1 on white: use it for bold or large text
    /// and controls, not for small body copy (design.md contrast notes).
    public static let accent = Color(light: 0x0E8A9B, dark: 0x3CC4D4)
    public static let accentSoft = Color(light: 0xD6EEF2, dark: 0x123A4C)
    /// Accent for small text (pills, links, captions). Light `accent` is
    /// below 4.5:1 on white and on `accent-soft` (design.md contrast notes),
    /// so small light-mode text uses this darker teal (about 4.9:1 on
    /// `accent-soft`). Added for the apps; for @santoadji21 to review.
    public static let accentText = Color(light: 0x0B6E7C, dark: 0x3CC4D4)
    public static let ring = Color(light: 0x0E94A6, dark: 0x3CC4D4, alpha: 0.25)
    public static let onAccent = Color(light: 0xFFFFFF, dark: 0x06111F)
    public static let success = Color(light: 0x1E9E6A, dark: 0x4FD3A0)
    public static let danger = Color(light: 0xC93B3B, dark: 0xFF8080)
    public static let navy = Color(fixed: 0x0B1F3A)

    /// The full-screen Receive screen (always navy).
    public enum Receive {
        public static let background = Color(fixed: 0x0B1F3A)
        public static let card = Color(fixed: 0x12294A)
        public static let divider = Color(fixed: 0x25405F)
        public static let muted = Color(fixed: 0x9FB4C7)
        public static let outline = Color(fixed: 0x3A5573)
        public static let accent = Color(fixed: 0x3CC4D4)
        public static let onAccent = Color(fixed: 0x06111F)
    }

    /// Spacing and radii.
    public enum Metrics {
        public static let screenPadding: CGFloat = 20
        public static let desktopPadding: CGFloat = 40
        public static let sectionGap: CGFloat = 24
        public static let rowPadding: CGFloat = 12
        public static let cardRadius: CGFloat = 22
        public static let tileRadius: CGFloat = 12
    }
}
