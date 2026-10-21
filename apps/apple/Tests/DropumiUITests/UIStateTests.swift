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
        XCTAssertTrue(app.buttons["nearby.pair"].exists)
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

    // MARK: Pair a device
    //
    // The pairing scenarios open Pair a device from Settings, on a fake core
    // that pairs like the real one: showing the screen makes this device
    // ready to pair. The screen is long, so each audit runs with it scrolled
    // clear of the tab bar (see testNearbyNeverLabelsADevicePaired).

    /// The Pair button next to a nearby device; VoiceOver reads it "Pair
    /// with ‹name›".
    @MainActor
    private func pairButton(_ app: XCUIApplication, _ name: String) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "identifier == %@ AND label == %@", "pair.peer", "Pair with \(name)"))
            .firstMatch
    }

    /// "Ready to pair · m:ss", shown while this device is ready to pair.
    @MainActor
    private func countdown(_ app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: "pair.countdown").firstMatch
    }

    @MainActor
    private func note(_ app: XCUIApplication) -> XCUIElement {
        app.descendants(matching: .any).matching(identifier: "pair.note").firstMatch
    }

    /// Whether any element's label contains `string`.
    @MainActor
    private func shows(_ app: XCUIApplication, containing string: String) -> Bool {
        app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", string)).firstMatch.exists
    }

    /// Taps Pair next to Maya's iPhone once this device is ready to pair.
    @MainActor
    private func askMaya(_ app: XCUIApplication) {
        let pair = pairButton(app, "Maya's iPhone")
        XCTAssertTrue(pair.waitForExistence(timeout: 10))
        XCTAssertTrue(countdown(app).waitForExistence(timeout: 10))
        pair.tap()
    }

    @MainActor
    func testPairingShowsReadyStateAndNearbyDevices() throws {
        let app = launch("pairing-ready")
        let ready = countdown(app)
        XCTAssertTrue(ready.waitForExistence(timeout: 10))
        // One element: the minutes left are its value.
        XCTAssertEqual(ready.label, "Ready to pair")
        XCTAssertTrue((ready.value as? String ?? "").contains("minute"), String(describing: ready.value))
        // The name and the devices can arrive just after the window opens.
        let name = app.staticTexts["pair.name"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        XCTAssertEqual(name.label, "Other devices see this one as “Dropumi-1A2B”.")
        XCTAssertTrue(pairButton(app, "Maya's iPhone").waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Receiving files is paused until you leave this screen."].exists)
        // The window's port, whatever this device's addresses are.
        for address in app.staticTexts.matching(identifier: "pair.address").allElementsBoundByIndex {
            XCTAssertTrue(address.label.hasSuffix(":47321"), address.label)
        }
        XCTAssertTrue(app.buttons["pair.stop"].exists)
        XCTAssertTrue(app.staticTexts["How to pair"].exists)
        for step in [
            "1. Open Pair a device on the other device too.",
            "2. On one of them, tap Pair next to the other.",
            "3. Check that both screens show the same code. Then tap Codes match on both.",
        ] {
            XCTAssertTrue(shows(app, containing: step), step)
        }
        XCTAssertEqual(app.staticTexts["pair.list"].label, "Nearby devices")
        XCTAssertTrue(pairButton(app, "Maya's iPhone").exists)
        XCTAssertTrue(pairButton(app, "Studio Mac").exists)
        XCTAssertTrue(app.staticTexts["Not paired"].exists)
        XCTAssertTrue(app.staticTexts["Looks like Studio Mac"].exists)
        // A name in the list is a claim: no row says "Paired".
        XCTAssertFalse(app.staticTexts["Paired"].exists)
        XCTAssertTrue(app.buttons["pair.byAddress"].exists)
        app.swipeUp()
        try audit(app)
    }

    @MainActor
    func testPairingWithNoDevicesExplainsWhatToDo() throws {
        let app = launch("pairing-empty")
        XCTAssertTrue(app.staticTexts["Looking for devices…"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts[
            "Open Pair a device on the other device. Both need to be on the same Wi-Fi network."].exists)
        XCTAssertTrue(countdown(app).waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons.matching(identifier: "pair.peer").firstMatch.exists)
        XCTAssertTrue(app.buttons["pair.byAddress"].exists)
        XCTAssertTrue(app.staticTexts[
            "Android phones aren't listed here. Type the address the phone shows on its Pair a device screen."].exists)
        app.swipeUp()
        try audit(app)
    }

    @MainActor
    func testTappingPairAsksThatDevice() throws {
        let app = launch("pairing-ready")
        askMaya(app)
        XCTAssertTrue(app.staticTexts["Asking “Maya's iPhone”…"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["A code appears on both devices. Check that they match."].exists)
        // The list and the ready card make way for the request.
        XCTAssertFalse(pairButton(app, "Studio Mac").exists)
        XCTAssertFalse(app.buttons["pair.stop"].exists)
        app.swipeUp()
        try audit(app)
        // Cancel: ready to pair again, with a note.
        app.buttons["pair.cancel"].tap()
        XCTAssertTrue(note(app).waitForExistence(timeout: 10))
        XCTAssertTrue(note(app).label.contains("Pairing cancelled."), note(app).label)
        XCTAssertTrue(countdown(app).waitForExistence(timeout: 10))
    }

    @MainActor
    func testStopLeavesThePairingScreen() throws {
        let app = launch("pairing-ready")
        let stop = app.buttons["pair.stop"]
        XCTAssertTrue(stop.waitForExistence(timeout: 10))
        XCTAssertTrue(countdown(app).waitForExistence(timeout: 10))
        app.swipeUp()
        try audit(app)
        stop.tap()
        // Back where it was opened from.
        XCTAssertTrue(app.buttons["settings.pair"].waitForExistence(timeout: 10))
        XCTAssertFalse(countdown(app).exists)
    }

    @MainActor
    func testPairedResultNamesTheDeviceAndDoneGoesBack() throws {
        let app = launch("pairing-paired")
        let title = app.staticTexts["pair.result.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(title.label, "Paired with “Maya's iPhone”")
        XCTAssertTrue(app.staticTexts[
            "You can now send files to each other without a code. Each time, Dropumi checks that it's the same device."
        ].exists)
        XCTAssertEqual(app.buttons["pair.again"].label, "Pair another device")
        app.swipeUp()
        try audit(app)
        app.buttons["pair.done"].tap()
        XCTAssertTrue(app.buttons["settings.pair"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["pair.result.title"].exists)
    }

    @MainActor
    func testNotReadySaysWhatToDo() throws {
        let app = launch("pairing-not-ready")
        askMaya(app)
        XCTAssertTrue(note(app).waitForExistence(timeout: 10))
        let expected = "“Maya's iPhone” isn't waiting to pair. Open Pair a device on “Maya's iPhone” too, "
            + "then tap it here again. If you both tapped at once, let only one of you tap."
        XCTAssertTrue(note(app).label.contains(expected), note(app).label)
        // Not the text about sending files.
        XCTAssertFalse(shows(app, containing: "Only paired devices"))
        // This device is ready to pair again.
        XCTAssertTrue(countdown(app).waitForExistence(timeout: 10))
        app.swipeUp()
        try audit(app)
    }

    @MainActor
    func testCodesDontMatchIsAttributedToYou() throws {
        let app = launch("pairing-codes")
        askMaya(app)
        let mismatch = app.buttons["code.mismatch"]
        XCTAssertTrue(mismatch.waitForExistence(timeout: 10))
        XCTAssertTrue(waitUntilEnabled(mismatch))
        mismatch.tap()
        let title = app.staticTexts["pair.result.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(title.label, "You said the codes don't match")
        XCTAssertTrue(app.staticTexts[
            "Nothing was saved. If the codes were different, another device may have answered. "
                + "Pair only when both devices are in front of you."
        ].exists)
        XCTAssertFalse(shows(app, containing: "didn't confirm"))
        XCTAssertTrue(app.buttons["pair.done"].exists)
        XCTAssertEqual(app.buttons["pair.again"].label, "Try again")
        app.swipeUp()
        try audit(app)
    }

    @MainActor
    func testOtherDeviceDecliningIsAttributedToIt() throws {
        let app = launch("pairing-codes")
        askMaya(app)
        let match = app.buttons["code.match"]
        XCTAssertTrue(match.waitForExistence(timeout: 10))
        XCTAssertTrue(waitUntilEnabled(match))
        match.tap()
        let title = app.staticTexts["pair.result.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(title.label, "“Maya's iPhone” didn't confirm the code")
        XCTAssertTrue(app.staticTexts[
            "It said the codes don't match, stopped, or didn't answer in time. Nothing was saved."].exists)
        XCTAssertFalse(shows(app, containing: "You said"))
        XCTAssertEqual(app.buttons["pair.again"].label, "Try again")
        XCTAssertTrue(app.buttons["pair.done"].exists)
        app.swipeUp()
        try audit(app)
    }

    @MainActor
    func testReadyTimeEndingOffersStartAgain() throws {
        let app = launch("pairing-time-up")
        let title = app.staticTexts["pair.result.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(title.label, "Time's up")
        XCTAssertTrue(app.staticTexts["No device asked to pair in 5 minutes. Nothing was saved."].exists)
        XCTAssertFalse(shows(app, containing: "No answer"))
        // One way on: it never gets ready again by itself.
        let again = app.buttons["pair.again"]
        XCTAssertEqual(again.label, "Start again")
        XCTAssertFalse(app.buttons["pair.done"].exists)
        XCTAssertFalse(countdown(app).exists)
        app.swipeUp()
        try audit(app)
        again.tap()
        XCTAssertTrue(countdown(app).waitForExistence(timeout: 10))
    }

    @MainActor
    func testUsedUpReadinessSaysPairingDidNotStart() throws {
        let app = launch("pairing-used-up")
        let title = app.staticTexts["pair.result.title"]
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(title.label, "Pairing didn't start")
        XCTAssertTrue(app.staticTexts["A device connected, but not to pair. Nothing was saved."].exists)
        XCTAssertEqual(app.buttons["pair.again"].label, "Start again")
        XCTAssertFalse(countdown(app).exists)
        // The pairing screen says it; no banner on top.
        XCTAssertFalse(app.staticTexts["A device was refused"].exists)
        app.swipeUp()
        try audit(app)
    }

    @MainActor
    func testRefusalBannerOpensPairing() throws {
        let app = launch("refused-device")
        XCTAssertTrue(app.staticTexts["A device was refused"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts[
            "A device on this network tried to connect and was turned away. "
                + "If it was trying to pair, open Pair a device on both devices."
        ].exists)
        let action = app.buttons["banner.action"]
        XCTAssertEqual(action.label, "Pair a device")
        action.tap()
        XCTAssertTrue(countdown(app).waitForExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["A device was refused"].exists)
        // Audited here: the banner covers the screen's top, whose contrast
        // the audit would measure on the banner.
        app.swipeUp()
        try audit(app)
    }

    @MainActor
    func testEmptyPairedDevicesOffersPairing() throws {
        let app = launch("paired-devices-empty")
        XCTAssertTrue(app.staticTexts["No paired devices"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts[
            "Pair a device to send to each other without a code. You can also add one by its fingerprint."].exists)
        let pair = app.buttons["paired.emptyPair"]
        XCTAssertEqual(pair.label, "Pair a device")
        XCTAssertTrue(app.buttons["paired.emptyAdd"].exists)
        XCTAssertTrue(app.buttons["paired.pair"].exists)
        try audit(app)
        pair.tap()
        XCTAssertTrue(countdown(app).waitForExistence(timeout: 10))
        // Stop goes back to Paired devices.
        app.buttons["pair.stop"].tap()
        XCTAssertTrue(app.buttons["paired.emptyPair"].waitForExistence(timeout: 10))
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
