// SPDX-License-Identifier: MPL-2.0
//
// Test isolation (DEBUG only). When XCTest hosts the app, or a UI test
// launches it, the app uses fresh temporary folders and its own settings,
// never the real identity, trust store, history or share inbox. On macOS the
// test host shares the installed app's sandbox container, so this matters.

import Foundation

enum TestHost {
    /// Launch argument UI tests pass.
    static let uiTestArgument = "-dropumi-ui-test"
    private static let defaultsSuite = "io.github.dropumi.tests"

    /// Whether the app runs under a test. Always false in Release.
    static var isActive: Bool {
        #if DEBUG
        let environment = ProcessInfo.processInfo.environment
        return environment["XCTestConfigurationFilePath"] != nil
            || environment["XCTestBundlePath"] != nil
            || environment["XCTestSessionIdentifier"] != nil
            || CommandLine.arguments.contains(uiTestArgument)
        #else
        return false
        #endif
    }

    /// Settings for a test run, cleared at every launch.
    static func defaults() -> UserDefaults {
        let defaults = UserDefaults(suiteName: defaultsSuite) ?? .standard
        defaults.removePersistentDomain(forName: defaultsSuite)
        return defaults
    }
}
