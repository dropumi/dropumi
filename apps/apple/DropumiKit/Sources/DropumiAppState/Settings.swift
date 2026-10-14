// SPDX-License-Identifier: MPL-2.0

import DropumiBindings
import Foundation

/// Who may send to this device ("Who can send to this device").
public enum ReceiveVisibility: String, CaseIterable, Codable, Sendable {
    /// Not receiving: no listener, no advertisement.
    case off
    /// Paired devices only (`TrustedOnly`). The default.
    case pairedOnly
    /// Any device may ask, for 10 minutes; unverified offers show a code.
    case everyone

    /// The Everyone window length.
    public static let everyoneMinutes: UInt32 = 10

    public var policy: ReceivePolicy? {
        switch self {
        case .off: return nil
        case .pairedOnly: return .trustedOnly
        case .everyone: return .everyone(minutes: Self.everyoneMinutes)
        }
    }
}

/// The colour scheme the user picked.
public enum Appearance: String, CaseIterable, Codable, Sendable {
    case system, light, dark
}

/// Settings stored in `UserDefaults`.
public struct SettingsStore {
    private let defaults: UserDefaults
    private enum Key {
        static let visibility = "visibility"
        static let autoAccept = "autoAcceptPaired"
        static let onboarded = "onboarded"
        static let appearance = "appearance"
    }

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    /// Paired only by default. "Everyone" is never restored at launch: its
    /// window is short and must be opened on purpose.
    public var visibility: ReceiveVisibility {
        get {
            let stored = defaults.string(forKey: Key.visibility).flatMap(ReceiveVisibility.init(rawValue:))
            return stored == .everyone ? .pairedOnly : (stored ?? .pairedOnly)
        }
        nonmutating set { defaults.set(newValue.rawValue, forKey: Key.visibility) }
    }

    /// Off by default.
    public var autoAcceptPaired: Bool {
        get { defaults.bool(forKey: Key.autoAccept) }
        nonmutating set { defaults.set(newValue, forKey: Key.autoAccept) }
    }

    public var onboarded: Bool {
        get { defaults.bool(forKey: Key.onboarded) }
        nonmutating set { defaults.set(newValue, forKey: Key.onboarded) }
    }

    public var appearance: Appearance {
        get { defaults.string(forKey: Key.appearance).flatMap(Appearance.init(rawValue:)) ?? .system }
        nonmutating set { defaults.set(newValue.rawValue, forKey: Key.appearance) }
    }
}
