// SPDX-License-Identifier: MPL-2.0
//
// Plus Jakarta Sans (SIL Open Font License 1.1, Resources/Fonts/OFL.txt),
// bundled so no font is ever downloaded. Every style scales with Dynamic
// Type (`relativeTo:`). Codes and fingerprints use the system monospaced
// font so digits are easy to compare.

import CoreText
import SwiftUI

public enum DSFont {
    static let family = "Plus Jakarta Sans"
    nonisolated(unsafe) private static var registered = false
    private static let lock = NSLock()

    /// Registers the bundled font once per process. Safe to call often.
    public static func register() {
        lock.lock()
        defer { lock.unlock() }
        guard !registered else { return }
        registered = true
        guard let url = Bundle.module.url(
            forResource: "PlusJakartaSans-Variable", withExtension: "ttf", subdirectory: "Fonts")
        else { return }
        CTFontManagerRegisterFontsForURL(url as CFURL, .process, nil)
    }

    static func custom(_ size: CGFloat, _ weight: Font.Weight, relativeTo style: Font.TextStyle) -> Font {
        register()
        return Font.custom(family, size: size, relativeTo: style).weight(weight)
    }

    /// 32 / 700: the title of every screen.
    public static let screenTitle = custom(32, .bold, relativeTo: .largeTitle)
    /// 28 / 700: the sender's name on the Receive screen.
    public static let dialogName = custom(28, .bold, relativeTo: .title)
    /// 18 / 600.
    public static let sectionHeading = custom(18, .semibold, relativeTo: .title3)
    /// 16 / 600: file and device names.
    public static let rowTitle = custom(16, .semibold, relativeTo: .headline)
    /// 15 / 400.
    public static let body = custom(15, .regular, relativeTo: .body)
    /// 13 / 400: sizes, devices, times.
    public static let meta = custom(13, .regular, relativeTo: .subheadline)
    /// 16 / 600: primary buttons.
    public static let button = custom(16, .semibold, relativeTo: .headline)
    /// 13 / 600: small buttons.
    public static let smallButton = custom(13, .semibold, relativeTo: .subheadline)
    /// 12 / 700, uppercase.
    public static let sectionLabel = custom(12, .bold, relativeTo: .caption)
    /// 10 / 500: tab labels.
    public static let tabLabel = custom(10, .medium, relativeTo: .caption2)
    /// A large monospaced code ("123 456"). `SasCodeView` scales it with
    /// Dynamic Type.
    public static let code = Font.system(.largeTitle, design: .monospaced, weight: .bold)
    /// Fingerprints.
    public static let fingerprint = Font.system(.footnote, design: .monospaced)
}
