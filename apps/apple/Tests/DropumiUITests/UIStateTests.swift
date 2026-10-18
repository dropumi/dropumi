// SPDX-License-Identifier: MPL-2.0
//
// UI state tests (fake core): every screen and state, with an accessibility
// audit. They run the app on the fake core (DEBUG only) and say nothing
// about the real core, the network or OS permissions.

import XCTest

final class UIStateTests: XCTestCase {
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
            // Likewise content scrolled partly above the screen, under the status
            // bar (measured on an iPhone 15: the status pill at y = -19 after a swipe).
            if issue.auditType == .contrast, let element = issue.element, element.frame.minY < 0 {
                return true
            }
            let element = issue.element.map {
                "\($0.elementType.rawValue) '\($0.label)' id='\($0.identifier)' frame=\($0.frame)"
            } ?? "? \(issue.detailedDescription)"
            issues.append("\(issue.compactDescription) [type \(issue.auditType.rawValue)]: \(element)")
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
        // Audit with the list scrolled clear of the translucent tab bar. At rest,
        // the audit reports rows under the tab bar without an element, so they
        // cannot be told apart from the app's own; the status pill is audited
        // at rest in testEmptyNearbyExplainsWhatToDo instead.
        app.swipeUp()
        try audit(app)
    }

    @MainActor
    func testEmptyNearbyExplainsWhatToDo() throws {
        let app = launch("nearby-empty")
        XCTAssertTrue(app.staticTexts["Looking for devices…"].waitForExistence(timeout: 10))
        // At rest, with the status pill fully on screen.
        XCTAssertTrue(app.staticTexts["visibility.pill"].exists)
        try audit(app)
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

    /// "Add by fingerprint" needs all 52 characters and points here, so
    /// "This device" shows the full fingerprint (fake core's fixture).
    @MainActor
    func testThisDeviceShowsTheFullFingerprint() {
        let app = launch("nearby-empty")
        let settings = app.tabBars.buttons["Settings"]
        XCTAssertTrue(settings.waitForExistence(timeout: 10))
        settings.tap()
        let thisDevice = app.buttons["settings.thisDevice"]
        XCTAssertTrue(thisDevice.waitForExistence(timeout: 5))
        thisDevice.tap()
        let fingerprint = app.staticTexts["device.fingerprint"]
        XCTAssertTrue(fingerprint.waitForExistence(timeout: 5))
        XCTAssertEqual(
            fingerprint.label,
            "ABCD-EFGH-IJKL-MNOP-QRST-UVWX-YZ23-4567-ABCD-EFGH-IJKL-MNOP-QRST")
    }

    @MainActor
    func testLocalNetworkDeniedExplainsHowToFix() {
        let app = launch("local-network-denied")
        XCTAssertTrue(app.staticTexts["Local network access is off"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["Open Settings"].exists)
    }

    // MARK: Prompt arming (PROMPT-1)
    //
    // A press that starts while Accept or "Codes match" is still disabled,
    // and is held until after the button arms, must not answer. In these
    // scenarios the fake core closes a prompt once it is answered, so an
    // answer shows as the prompt going away. The arming delay is made longer
    // (never shorter) so that the press reliably starts inside it; each test
    // has a positive control showing that an answer does close the prompt.

    private let armDelayMs = 4_000
    private let holdPastArming: TimeInterval = 6

    @MainActor
    private func launchArming(_ scenario: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = [
            "-dropumi-ui-test", "-dropumi-scenario", scenario, "-dropumi-arm-delay-ms", String(armDelayMs),
        ]
        app.launch()
        return app
    }

    /// True when `element` is still there after `seconds`, the window in which
    /// the fake core's PromptClosed would arrive.
    @MainActor
    private func stays(_ element: XCUIElement, seconds: TimeInterval = 3) -> Bool {
        let gone = XCTNSPredicateExpectation(predicate: NSPredicate(format: "exists == false"), object: element)
        return XCTWaiter().wait(for: [gone], timeout: seconds) == .timedOut
    }

    @MainActor
    private func waitUntilEnabled(_ element: XCUIElement) -> Bool {
        let enabled = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isEnabled == true"), object: element)
        return XCTWaiter().wait(for: [enabled], timeout: TimeInterval(armDelayMs) / 1000 + 5) == .completed
    }

    @MainActor
    private func pressedBeforeArmingNeverAnswers(scenario: String, button: String) {
        let app = launchArming(scenario)
        let control = app.buttons[button]
        XCTAssertTrue(control.waitForExistence(timeout: 10))
        XCTAssertFalse(control.isEnabled, "the press must start before the button arms")
        control.press(forDuration: holdPastArming)
        XCTAssertTrue(stays(control), "a press that started before arming answered the prompt")
        XCTAssertTrue(control.isEnabled, "the press must end after the button armed")
    }

    /// The positive control: a press after arming answers. `held` uses the same
    /// long hold as the negative test, so that test cannot pass only because a
    /// long hold never fires a button.
    @MainActor
    private func pressAfterArmingAnswers(scenario: String, button: String, held: Bool = false) {
        let app = launchArming(scenario)
        let control = app.buttons[button]
        XCTAssertTrue(control.waitForExistence(timeout: 10))
        XCTAssertTrue(waitUntilEnabled(control))
        if held {
            control.press(forDuration: holdPastArming)
        } else {
            control.tap()
        }
        XCTAssertFalse(stays(control), "an armed button did not answer the prompt")
    }

    @MainActor
    func testAcceptPressedBeforeArmingNeverAccepts() {
        pressedBeforeArmingNeverAnswers(scenario: "offer-closes-on-answer", button: "offer.accept")
    }

    @MainActor
    func testAcceptAfterArmingAnswers() {
        pressAfterArmingAnswers(scenario: "offer-closes-on-answer", button: "offer.accept")
    }

    @MainActor
    func testAcceptHeldAfterArmingAnswers() {
        pressAfterArmingAnswers(scenario: "offer-closes-on-answer", button: "offer.accept", held: true)
    }

    @MainActor
    func testCodesMatchPressedBeforeArmingNeverConfirms() {
        pressedBeforeArmingNeverAnswers(scenario: "code-closes-on-answer", button: "code.match")
    }

    @MainActor
    func testCodesMatchAfterArmingConfirms() {
        pressAfterArmingAnswers(scenario: "code-closes-on-answer", button: "code.match")
    }

    @MainActor
    func testCodesMatchHeldAfterArmingConfirms() {
        pressAfterArmingAnswers(scenario: "code-closes-on-answer", button: "code.match", held: true)
    }
}
