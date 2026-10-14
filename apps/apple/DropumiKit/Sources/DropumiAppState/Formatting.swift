// SPDX-License-Identifier: MPL-2.0

import Foundation

public enum Formatting {
    /// File sizes as people read them ("18.2 MB").
    public static func bytes(_ n: UInt64) -> String {
        Int64(clamping: n).formatted(.byteCount(style: .file))
    }

    /// "28 MB/s".
    public static func speed(_ bytesPerSecond: Double) -> String {
        Int64(bytesPerSecond.rounded()).formatted(.byteCount(style: .file)) + "/s"
    }

    /// "20 s left", "3 min left".
    public static func timeLeft(_ seconds: Double) -> String {
        let s = max(1, Int(seconds.rounded()))
        if s < 60 {
            return String(localized: "\(s) s left")
        }
        let minutes = Int((Double(s) / 60).rounded(.up))
        return String(localized: "\(minutes) min left")
    }

    /// "1 file", "3 files".
    public static func fileCount(_ n: Int) -> String {
        String(AttributedString(localized: "^[\(n) file](inflect: true)").characters)
    }

    /// "123 456": six digits in two groups, for reading aloud.
    public static func code(_ digits: String) -> String {
        guard digits.count == 6 else { return digits }
        return "\(digits.prefix(3)) \(digits.suffix(3))"
    }

    /// The code spelled digit by digit for VoiceOver: "1 2 3, 4 5 6".
    public static func spokenCode(_ digits: String) -> String {
        guard digits.count == 6 else { return digits }
        let first = digits.prefix(3).map(String.init).joined(separator: " ")
        let second = digits.suffix(3).map(String.init).joined(separator: " ")
        return "\(first), \(second)"
    }

    /// "m:ss" for countdowns.
    public static func countdown(_ seconds: Int) -> String {
        let s = max(0, seconds)
        return String(format: "%d:%02d", s / 60, s % 60)
    }
}
