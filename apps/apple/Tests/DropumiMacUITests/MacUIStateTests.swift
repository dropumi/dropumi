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

    /// Runs the accessibility audit and fails with every issue, its element
    /// and its frame, so a failure says what to fix.
    @MainActor
    private func audit(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) throws {
        var issues: [String] = []
        let tabBar = app.tabBars.firstMatch.exists ? app.tabBars.firstMatch.frame : .null
        let window = mainWindow(app)
        let windowFrame = window.exists ? window.frame : .null
        let sheet = app.sheets.element(boundBy: 0)
        let sheetFrame = sheet.exists ? sheet.frame : .null
        try app.performAccessibilityAudit { issue in
            // Behind a presented sheet the window's content is inert, and
            // where the sheet covers it the audit measures the sheet's pixels;
            // its contrast there is not its own (the same empty-state text
            // passes with no sheet, testEmptyNearbyExplainsWhatToDo). An
            // element is behind the sheet when it lies outside the sheet's
            // frame, or when no element of the sheet has its identifier.
            // Everything inside the sheet is still audited.
            if issue.auditType == .contrast, let element = issue.element, !sheetFrame.isNull,
               !sheetFrame.contains(element.frame)
                || (!element.identifier.isEmpty
                    && !sheet.descendants(matching: .any).matching(identifier: element.identifier)
                        .element(boundBy: 0).exists) {
                return true
            }
            // Scrolling content passes under the translucent tab bar; its
            // contrast there is the system bar's, not the app's.
            if issue.auditType == .contrast, let element = issue.element, element.frame.intersects(tabBar) {
                return true
            }
            // SwiftUI's own split-view panes (TabView .sidebarAdaptable: groups
            // with no identifier spanning the window's height) and the Touch
            // Bar are system elements the app cannot describe: labelling the
            // tab view's content left them unchanged (macOS 27.0.1, 2026-10-18).
            if issue.auditType == .sufficientElementDescription, let element = issue.element,
               element.identifier.isEmpty,
               element.elementType == .touchBar
                || (element.elementType == .group && element.frame.minY == windowFrame.minY
                    && element.frame.height == windowFrame.height) {
                return true
            }
            let element = issue.element.map {
                "\($0.elementType.rawValue) '\($0.label)' id='\($0.identifier)' frame=\($0.frame)"
            } ?? "? \(issue.detailedDescription)"
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

    /// A static text by its string. macOS exposes a SwiftUI Text's string as
    /// its value (a heading as its label). `firstMatch` on a predicate query
    /// never resolved on macOS 27 (checked by a diagnostic run), so this takes
    /// the first element by index.
    @MainActor
    private func text(_ app: XCUIApplication, _ string: String) -> XCUIElement {
        app.staticTexts.matching(NSPredicate(format: "label == %@ OR value == %@", string, string)).element(boundBy: 0)
    }

    /// The first static text in `scope` whose label or value contains
    /// `string`. macOS merges a row's texts into one element, unlike iOS
    /// ("Unverified device, claims …").
    @MainActor
    private func text(in scope: XCUIElement, containing string: String) -> XCUIElement {
        scope.staticTexts.matching(NSPredicate(format: "label CONTAINS %@ OR value CONTAINS %@", string, string))
            .element(boundBy: 0)
    }

    /// The first button in `scope` whose label contains `string`. A Nearby row
    /// is one button on macOS ("Maya's iPhone, Not paired, Send to …").
    @MainActor
    private func button(in scope: XCUIElement, containing string: String) -> XCUIElement {
        scope.buttons.matching(NSPredicate(format: "label CONTAINS %@", string)).element(boundBy: 0)
    }

    @MainActor
    private func mainWindow(_ app: XCUIApplication) -> XCUIElement {
        app.windows.matching(NSPredicate(format: "title == %@", "Dropumi")).element(boundBy: 0)
    }

    /// What VoiceOver reads for an element: its label, or on macOS its value.
    @MainActor
    private func spoken(_ element: XCUIElement) -> String {
        element.label.isEmpty ? (element.value as? String ?? "") : element.label
    }

    @MainActor
    func testOnboardingEndsOnNearby() {
        let app = launch("onboarding")
        let next = app.buttons["onboarding.next"]
        XCTAssertTrue(next.waitForExistence(timeout: 10))
        next.click()
        XCTAssertTrue(app.textFields["onboarding.name"].waitForExistence(timeout: 5))
        next.click()
        XCTAssertTrue(text(app, "Find devices on your network").waitForExistence(timeout: 5))
        next.click()
        XCTAssertTrue(text(app, "Who can send to this device?").waitForExistence(timeout: 5))
        next.click()
        XCTAssertTrue(text(app, "Nearby").waitForExistence(timeout: 10))
    }

    @MainActor
    func testNearbyNeverLabelsADevicePaired() throws {
        let app = launch("nearby")
        let window = mainWindow(app)
        XCTAssertTrue(button(in: window, containing: "Maya's iPhone").waitForExistence(timeout: 10))
        XCTAssertTrue(button(in: window, containing: "Looks like Studio Mac").exists)
        // No row or text calls a device "Paired": a row's label lists its parts.
        let parts = (window.buttons.allElementsBoundByIndex.map(\.label)
            + window.staticTexts.allElementsBoundByIndex.map(spoken))
            .flatMap { $0.components(separatedBy: ", ") }
        XCTAssertFalse(parts.contains("Paired"), parts.joined(separator: " | "))
        // The audit includes the "…" menu's actions: it must offer AXShowMenu.
        try audit(app)
    }

    @MainActor
    func testPairedDevicesListsTheDeviceWithItsActions() throws {
        let app = launch("paired-devices")
        let window = mainWindow(app)
        let paired = window.descendants(matching: .any)["settings.paired"]
        XCTAssertTrue(paired.waitForExistence(timeout: 10))
        paired.click()
        XCTAssertTrue(text(in: window, containing: "Studio Mac").waitForExistence(timeout: 10))
        XCTAssertTrue(window.menuButtons["Actions"].exists)
        // The audit includes the row's "…" menu: it must offer AXShowMenu.
        try audit(app)
    }

    @MainActor
    func testEmptyNearbyExplainsWhatToDo() throws {
        let app = launch("nearby-empty")
        XCTAssertTrue(text(app, "Looking for devices…").waitForExistence(timeout: 10))
        // With no sheet over the window, so the empty state's contrast is its own.
        try audit(app)
    }

    @MainActor
    func testUnverifiedOfferShowsClaimAndCode() throws {
        let app = launch("offer-unverified")
        let sheet = app.sheets.element(boundBy: 0)
        XCTAssertTrue(text(in: sheet, containing: "Unverified device").waitForExistence(timeout: 10))
        XCTAssertTrue(text(in: sheet, containing: "claims “Maya's iPhone”").exists)
        XCTAssertTrue(sheet.staticTexts["code.digits"].exists)
        XCTAssertEqual(spoken(sheet.staticTexts["code.digits"]), "Verification code: 1 2 3, 4 5 6")
        XCTAssertTrue(sheet.buttons["offer.accept"].exists)
        try audit(app)
        // Declining sends the answer; the prompt stays until the core closes it.
        sheet.buttons["offer.decline"].click()
        XCTAssertTrue(text(in: sheet, containing: "Unverified device").exists)
    }

    @MainActor
    func testPairedOfferShowsAliasWithoutCode() {
        let app = launch("offer-paired")
        let sheet = app.sheets.element(boundBy: 0)
        XCTAssertTrue(text(in: sheet, containing: "Studio Mac").waitForExistence(timeout: 10))
        XCTAssertFalse(sheet.staticTexts["code.digits"].exists)
    }

    @MainActor
    func testPairingCodeHasEqualChoices() throws {
        let app = launch("code-pairing")
        XCTAssertTrue(app.buttons["code.match"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["code.mismatch"].exists)
        XCTAssertEqual(spoken(app.staticTexts["code.digits"]), "Verification code: 6 5 4, 3 2 1")
        try audit(app)
    }

    @MainActor
    func testTransfersShowProgressAndHistory() throws {
        let app = launch("transfers")
        let window = mainWindow(app)
        XCTAssertTrue(text(in: window, containing: "Earlier").waitForExistence(timeout: 10))
        // A history row is one combined element on macOS: its label holds the file name.
        XCTAssertTrue(
            text(in: window, containing: "IMG_2041.heic").exists
                || window.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", "IMG_2041.heic"))
                    .element(boundBy: 0).exists)
        // A SwiftUI container with an identifier is a group on macOS, not an "other" element.
        let card = window.descendants(matching: .any)["transfer.card"].exists || window.buttons["transfer.cancel"].exists
        XCTAssertTrue(card, card ? "" : "identifiers in the window: " + window.descendants(matching: .any)
            .allElementsBoundByIndex.map(\.identifier).filter { !$0.isEmpty && !$0.hasPrefix("_") }.joined(separator: ", "))
        try audit(app)
    }

    @MainActor
    func testLocalNetworkDeniedExplainsHowToFix() {
        let app = launch("local-network-denied")
        XCTAssertTrue(app.staticTexts["Local network access is off"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["Open Settings"].exists)
    }

    // MARK: Prompt arming (PROMPT-1)
    //
    // A click held from before Accept or "Codes match" arms until after it
    // must not answer. In these scenarios the fake core closes a prompt once
    // it is answered, so an answer shows as the prompt going away. The arming
    // delay is made longer (never shorter) so that the click reliably starts
    // inside it; each test has a positive control.

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

    /// A click held in place for `holdPastArming`. It goes through a point in
    /// the button, not the element: XCUITest refused the element-based hold as
    /// "not hittable" on a built-in display left of the main one (negative
    /// coordinates), where a plain click on the same button worked.
    @MainActor
    private func hold(_ element: XCUIElement) {
        let point = element.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
        point.click(forDuration: holdPastArming, thenDragTo: point)
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
        XCTAssertFalse(control.isEnabled, "the click must start before the button arms")
        hold(control)
        XCTAssertTrue(stays(control), "a click that started before arming answered the prompt")
        XCTAssertTrue(control.isEnabled, "the click must end after the button armed")
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
            hold(control)
        } else {
            control.click()
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
