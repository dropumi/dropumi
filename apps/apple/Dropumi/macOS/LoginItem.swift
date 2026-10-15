// SPDX-License-Identifier: MPL-2.0
//
// "Open at login" (off by default), through the system's login items.

import ServiceManagement

enum LoginItem {
    static var isEnabled: Bool {
        SMAppService.mainApp.status == .enabled
    }

    /// Turns opening at login on or off and returns the resulting state.
    static func set(_ enabled: Bool) -> Bool {
        do {
            if enabled {
                try SMAppService.mainApp.register()
            } else {
                try SMAppService.mainApp.unregister()
            }
        } catch {
            // The state below says what actually happened.
        }
        return isEnabled
    }
}
