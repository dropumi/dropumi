// SPDX-License-Identifier: MPL-2.0
//
// UI state tests (fake core): every screen and state, with an accessibility
// audit. They run the app on the fake core (DEBUG only) and say nothing
// about the real core, the network or OS permissions.

import XCTest

final class MacUIStateTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    /// Runs the accessibility audit and fails with every issue and its
    /// element, so a failure says what to fix.
    @MainActor
    private func audit(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) throws {
        var issues: [String] = []
        let tabBar = app.tabBars.firstMatch.exists ? app.tabBars.firstMatch.frame : .null
        try app.performAccessibilityAudit { issue in
            // Scrolling content passes under the translucent tab bar; its
            // contrast there is the system bar's, not the app's.
            if issue.auditType == .contrast, let element = issue.element, element.frame.intersects(tabBar) {
                return true
            }
            let element = issue.element.map { "\($0.elementType.rawValue) '\($0.label)' id='\($0.identifier)'" } ?? "?"
            issues.append("\(issue.compactDescription): \(element)")
            return true
        }
        XCTAssertTrue(issues.isEmpty, issues.joined(separator: "\n"), file: file, line: line)
    }

    @MainActor
    private func launch(_ scenario: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-dropumi-ui-test", "-dropumi-scenario", scenario]
        app.launch()
        return app
    }

    @MainActor
    func testOnboardingEndsOnNearby() {
        let app = launch("onboarding")
        let next = app.buttons["onboarding.next"]
        XCTAssertTrue(next.waitForExistence(timeout: 10))
        next.tap()
        XCTAssertTrue(app.textFields["onboarding.name"].waitForExistence(timeout: 5))
        next.tap()
        XCTAssertTrue(app.staticTexts["Find devices on your network"].waitForExistence(timeout: 5))
        next.tap()
        XCTAssertTrue(app.staticTexts["Who can send to this device?"].waitForExistence(timeout: 5))
        next.tap()
        XCTAssertTrue(app.staticTexts["Nearby"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testNearbyNeverLabelsADevicePaired() throws {
        let app = launch("nearby")
        XCTAssertTrue(app.staticTexts["Maya's iPhone"].firstMatch.waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["Looks like Studio Mac"].exists)
        XCTAssertFalse(app.staticTexts["Paired"].exists)
        try audit(app)
    }

    @MainActor
    func testEmptyNearbyExplainsWhatToDo() {
        let app = launch("nearby-empty")
        XCTAssertTrue(app.staticTexts["Looking for devices…"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testUnverifiedOfferShowsClaimAndCode() throws {
        let app = launch("offer-unverified")
        XCTAssertTrue(app.staticTexts["Unverified device"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["claims “Maya's iPhone”"].exists)
        XCTAssertTrue(app.staticTexts["code.digits"].exists)
        XCTAssertEqual(app.staticTexts["code.digits"].label, "Verification code: 1 2 3, 4 5 6")
        XCTAssertTrue(app.buttons["offer.accept"].exists)
        try audit(app)
        // Declining sends the answer; the prompt stays until the core closes it.
        app.buttons["offer.decline"].tap()
        XCTAssertTrue(app.staticTexts["Unverified device"].exists)
    }

    @MainActor
    func testPairedOfferShowsAliasWithoutCode() {
        let app = launch("offer-paired")
        XCTAssertTrue(app.staticTexts["Studio Mac"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["code.digits"].exists)
    }

    @MainActor
    func testPairingCodeHasEqualChoices() throws {
        let app = launch("code-pairing")
        XCTAssertTrue(app.buttons["code.match"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["code.mismatch"].exists)
        XCTAssertEqual(app.staticTexts["code.digits"].label, "Verification code: 6 5 4, 3 2 1")
        try audit(app)
    }

    @MainActor
    func testTransfersShowProgressAndHistory() throws {
        let app = launch("transfers")
        XCTAssertTrue(app.staticTexts["Earlier"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["IMG_2041.heic"].exists)
        XCTAssertTrue(app.otherElements["transfer.card"].exists || app.buttons["transfer.cancel"].exists)
        try audit(app)
    }

    @MainActor
    func testLocalNetworkDeniedExplainsHowToFix() {
        let app = launch("local-network-denied")
        XCTAssertTrue(app.staticTexts["Local network access is off"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["Open Settings"].exists)
    }
}
