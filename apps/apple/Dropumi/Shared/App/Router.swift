// SPDX-License-Identifier: MPL-2.0

import Observation

enum AppTab: Hashable {
    case nearby, transfers, settings
}

enum NearbyRoute: Hashable {
    /// The Send screen, optionally with a nearby device chosen.
    case send(preselected: String?)
    case connectByAddress
}

enum SettingsRoute: Hashable {
    case thisDevice
    case trustedDevices
    case pairing
    case connectByAddress
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

    func openPairing() {
        tab = .settings
        settingsPath = [.pairing]
    }
}
