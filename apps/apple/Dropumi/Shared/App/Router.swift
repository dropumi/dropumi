// SPDX-License-Identifier: MPL-2.0

import Observation

enum AppTab: Hashable {
    case nearby, transfers, settings
}

enum NearbyRoute: Hashable {
    /// The Send screen, optionally with a nearby device chosen.
    case send(preselected: String?)
    case connectByAddress
    case pairing
    case pairByAddress
}

enum SettingsRoute: Hashable {
    case thisDevice
    case trustedDevices
    case pairing
    case connectByAddress
    case pairByAddress
}

/// Where the user is.
@Observable
final class Router {
    var tab: AppTab = .nearby
    var nearbyPath: [NearbyRoute] = []
    var settingsPath: [SettingsRoute] = []
    /// Set by ⌘O; the Send screen opens the file picker and clears it.
    var requestFilePicker = false

    func openSend(preselected: String? = nil) {
        tab = .nearby
        if case .send = nearbyPath.last {
            nearbyPath[nearbyPath.count - 1] = .send(preselected: preselected)
        } else {
            nearbyPath = [.send(preselected: preselected)]
        }
    }

    /// Pair a device, on top of the current tab's screens, so Done goes back
    /// to where it was opened. Already open: back to it. Only one pairing
    /// screen is ever alive: the other tab's goes.
    func openPairing() {
        switch tab {
        case .nearby:
            settingsPath = Self.cut(settingsPath, at: .pairing)
            nearbyPath = Self.cut(nearbyPath, after: .pairing) ?? nearbyPath + [.pairing]
        case .settings:
            nearbyPath = Self.cut(nearbyPath, at: .pairing)
            settingsPath = Self.cut(settingsPath, after: .pairing) ?? settingsPath + [.pairing]
        case .transfers:
            nearbyPath = Self.cut(nearbyPath, at: .pairing)
            tab = .settings
            settingsPath = [.pairing]
        }
    }

    /// Pair by address, from the pairing screen of the current tab.
    func openPairByAddress() {
        switch tab {
        case .nearby:
            nearbyPath.append(.pairByAddress)
        case .settings, .transfers:
            settingsPath.append(.pairByAddress)
        }
    }

    /// Mac: the main window closed. A pairing screen shown again later would
    /// make this device ready to pair without anyone asking.
    func closePairing() {
        nearbyPath = Self.cut(nearbyPath, at: .pairing)
        settingsPath = Self.cut(settingsPath, at: .pairing)
    }

    /// The path without `route` and what follows it.
    private static func cut<Route: Equatable>(_ path: [Route], at route: Route) -> [Route] {
        guard let index = path.firstIndex(of: route) else { return path }
        return Array(path[..<index])
    }

    /// The path up to and including `route`, or nil if it has none.
    private static func cut<Route: Equatable>(_ path: [Route], after route: Route) -> [Route]? {
        guard let index = path.firstIndex(of: route) else { return nil }
        return Array(path[...index])
    }
}
