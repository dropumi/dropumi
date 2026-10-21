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
            // The code digits: the audit reports "Contrast failed" on them on
            // macOS 27.0.1, though they measure 13.00:1 on screen (#F5F8FB on
            // #192E46: `screencapture -R` of the element's frame, WCAG
            // luminance of its darkest and lightest pixels; 2026-10-18). The
            // iPhone's audit checks the same SasCodeView's contrast and passes.
            // XCUITest cannot capture the built-in display here, so the
            // measurement cannot run in this test.
            if issue.auditType == .contrast, issue.element?.identifier == "code.digits" {
                return true
            }
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
            // with no identifier spanning the window's height), the root group
            // of every sheet (no identifier, exactly the sheet's frame) and the
            // Touch Bar are system elements the app cannot describe: labelling
            // the tab view's or the sheet's content left them unchanged and
            // put the label on an inner element (macOS 27.0.1, 2026-10-18).
            if issue.auditType == .sufficientElementDescription, let element = issue.element,
               element.identifier.isEmpty,
               element.elementType == .touchBar
                || (element.elementType == .group && element.frame.minY == windowFrame.minY
                    && element.frame.height == windowFrame.height)
                || (element.elementType == .group && element.frame == sheetFrame) {
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

    /// The first element of any type in `scope` with this identifier.
    @MainActor
    private func element(_ scope: XCUIElement, _ identifier: String) -> XCUIElement {
        scope.descendants(matching: .any).matching(identifier: identifier).element(boundBy: 0)
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
        XCTAssertTrue(window.buttons["nearby.pair"].exists)
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
        XCTAssertTrue(element(window, "paired.pair").exists)
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

    // MARK: Pair a device
    //
    // The pairing scenarios open Pair a device from Settings, on a fake core
    // that pairs like the real one: showing the screen makes this device
    // ready to pair.

    /// The Pair button next to a nearby device; VoiceOver reads it "Pair
    /// with ‹name›".
    @MainActor
    private func pairButton(_ window: XCUIElement, _ name: String) -> XCUIElement {
        window.buttons.matching(NSPredicate(format: "label == %@", "Pair with \(name)")).element(boundBy: 0)
    }

    /// Whether `element`, or a static text in it, says `string`: macOS may
    /// keep a combined element's text in its label, its value or a child.
    @MainActor
    private func says(_ element: XCUIElement, _ string: String) -> Bool {
        spoken(element).contains(string) || text(in: element, containing: string).exists
    }

    /// Whether a static text or an element's label in `scope` contains
    /// `string`. (Only static texts' values are matched: other values need
    /// not be strings.)
    @MainActor
    private func shows(_ scope: XCUIElement, containing string: String) -> Bool {
        text(in: scope, containing: string).exists
            || scope.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", string))
                .element(boundBy: 0).exists
    }

    /// Clicks Pair next to Maya's iPhone once this device is ready to pair.
    @MainActor
    private func askMaya(_ window: XCUIElement) {
        let pair = pairButton(window, "Maya's iPhone")
        XCTAssertTrue(pair.waitForExistence(timeout: 10))
        XCTAssertTrue(element(window, "pair.countdown").waitForExistence(timeout: 10))
        pair.click()
    }

    /// The result card's title, a heading: macOS reads it from its label.
    @MainActor
    private func resultTitle(_ window: XCUIElement) -> XCUIElement {
        element(window, "pair.result.title")
    }

    @MainActor
    func testPairingShowsReadyStateAndNearbyDevices() throws {
        let app = launch("pairing-ready")
        let window = mainWindow(app)
        let ready = element(window, "pair.countdown")
        XCTAssertTrue(ready.waitForExistence(timeout: 10))
        // One element: the minutes left are its value.
        XCTAssertEqual(ready.label, "Ready to pair")
        XCTAssertTrue((ready.value as? String ?? "").contains("minute"), String(describing: ready.value))
        // The name and the devices can arrive just after the window opens.
        XCTAssertTrue(
            text(in: window, containing: "Other devices see this one as “Dropumi-1A2B”.").waitForExistence(timeout: 5))
        XCTAssertTrue(pairButton(window, "Maya's iPhone").waitForExistence(timeout: 5))
        XCTAssertTrue(text(in: window, containing: "Receiving files is paused until you leave this screen.").exists)
        // The window's port, whatever this Mac's addresses are.
        for address in window.staticTexts.matching(identifier: "pair.address").allElementsBoundByIndex {
            XCTAssertTrue(spoken(address).hasSuffix(":47321"), spoken(address))
        }
        XCTAssertTrue(window.buttons["pair.stop"].exists)
        XCTAssertTrue(text(app, "How to pair").exists)
        for step in [
            "1. Open Pair a device on the other device too.",
            "2. On one of them, tap Pair next to the other.",
            "3. Check that both screens show the same code. Then tap Codes match on both.",
        ] {
            XCTAssertTrue(shows(window, containing: step), step)
        }
        XCTAssertTrue(text(app, "Nearby devices").exists)
        XCTAssertTrue(pairButton(window, "Maya's iPhone").exists)
        XCTAssertTrue(pairButton(window, "Studio Mac").exists)
        XCTAssertTrue(text(app, "Not paired").exists)
        XCTAssertTrue(text(app, "Looks like Studio Mac").exists)
        // A name in the list is a claim: nothing says "Paired".
        let parts = (window.buttons.allElementsBoundByIndex.map(\.label)
            + window.staticTexts.allElementsBoundByIndex.map(spoken))
            .flatMap { $0.components(separatedBy: ", ") }
        XCTAssertFalse(parts.contains("Paired"), parts.joined(separator: " | "))
        XCTAssertTrue(window.buttons["pair.byAddress"].exists)
        try audit(app)
    }

    @MainActor
    func testPairingWithNoDevicesExplainsWhatToDo() throws {
        let app = launch("pairing-empty")
        let window = mainWindow(app)
        XCTAssertTrue(text(app, "Looking for devices…").waitForExistence(timeout: 10))
        XCTAssertTrue(
            text(app, "Open Pair a device on the other device. Both need to be on the same Wi-Fi network.").exists)
        XCTAssertTrue(element(window, "pair.countdown").waitForExistence(timeout: 10))
        XCTAssertFalse(window.buttons.matching(identifier: "pair.peer").element(boundBy: 0).exists)
        XCTAssertTrue(window.buttons["pair.byAddress"].exists)
        XCTAssertTrue(text(
            app,
            "Android phones aren't listed here. Type the address the phone shows on its Pair a device screen."
        ).exists)
        try audit(app)
    }

    @MainActor
    func testTappingPairAsksThatDevice() throws {
        let app = launch("pairing-ready")
        let window = mainWindow(app)
        askMaya(window)
        XCTAssertTrue(text(app, "Asking “Maya's iPhone”…").waitForExistence(timeout: 10))
        XCTAssertTrue(text(app, "A code appears on both devices. Check that they match.").exists)
        // The list and the ready card make way for the request.
        XCTAssertFalse(pairButton(window, "Studio Mac").exists)
        XCTAssertFalse(window.buttons["pair.stop"].exists)
        try audit(app)
        // Cancel: ready to pair again, with a note.
        window.buttons["pair.cancel"].click()
        let note = element(window, "pair.note")
        XCTAssertTrue(note.waitForExistence(timeout: 10))
        XCTAssertTrue(says(note, "Pairing cancelled."), spoken(note))
        XCTAssertTrue(element(window, "pair.countdown").waitForExistence(timeout: 10))
    }

    @MainActor
    func testStopLeavesThePairingScreen() throws {
        let app = launch("pairing-ready")
        let window = mainWindow(app)
        let stop = window.buttons["pair.stop"]
        XCTAssertTrue(stop.waitForExistence(timeout: 10))
        XCTAssertTrue(element(window, "pair.countdown").waitForExistence(timeout: 10))
        try audit(app)
        stop.click()
        // Back where it was opened from.
        XCTAssertTrue(element(window, "settings.pair").waitForExistence(timeout: 10))
        XCTAssertFalse(element(window, "pair.countdown").exists)
    }

    /// Escape is Stop while no prompt is up.
    @MainActor
    func testEscapeStopsPairing() throws {
        let app = launch("pairing-ready")
        let window = mainWindow(app)
        XCTAssertTrue(window.buttons["pair.stop"].waitForExistence(timeout: 10))
        XCTAssertTrue(element(window, "pair.countdown").waitForExistence(timeout: 10))
        try audit(app)
        // A text, so the window is key without pressing anything.
        text(app, "How to pair").click()
        app.typeKey(.escape, modifierFlags: [])
        XCTAssertTrue(element(window, "settings.pair").waitForExistence(timeout: 10))
        XCTAssertFalse(window.buttons["pair.stop"].exists)
    }

    @MainActor
    func testPairedResultNamesTheDeviceAndDoneGoesBack() throws {
        let app = launch("pairing-paired")
        let window = mainWindow(app)
        let title = resultTitle(window)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(spoken(title), "Paired with “Maya's iPhone”")
        XCTAssertTrue(text(
            app,
            "You can now send files to each other without a code. Each time, Dropumi checks that it's the same device."
        ).exists)
        XCTAssertEqual(window.buttons["pair.again"].label, "Pair another device")
        try audit(app)
        window.buttons["pair.done"].click()
        XCTAssertTrue(element(window, "settings.pair").waitForExistence(timeout: 10))
        XCTAssertFalse(resultTitle(window).exists)
    }

    @MainActor
    func testNotReadySaysWhatToDo() throws {
        let app = launch("pairing-not-ready")
        let window = mainWindow(app)
        askMaya(window)
        let note = element(window, "pair.note")
        XCTAssertTrue(note.waitForExistence(timeout: 10))
        let expected = "“Maya's iPhone” isn't waiting to pair. Open Pair a device on “Maya's iPhone” too, "
            + "then tap it here again. If you both tapped at once, let only one of you tap."
        XCTAssertTrue(says(note, expected), spoken(note))
        // Not the text about sending files.
        XCTAssertFalse(shows(window, containing: "Only paired devices"))
        // This device is ready to pair again.
        XCTAssertTrue(element(window, "pair.countdown").waitForExistence(timeout: 10))
        try audit(app)
    }

    @MainActor
    func testCodesDontMatchIsAttributedToYou() throws {
        let app = launch("pairing-codes")
        let window = mainWindow(app)
        askMaya(window)
        let mismatch = app.buttons["code.mismatch"]
        XCTAssertTrue(mismatch.waitForExistence(timeout: 10))
        XCTAssertTrue(waitUntilEnabled(mismatch))
        mismatch.click()
        let title = resultTitle(window)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(spoken(title), "You said the codes don't match")
        XCTAssertTrue(text(
            app,
            "Nothing was saved. If the codes were different, another device may have answered. "
                + "Pair only when both devices are in front of you."
        ).exists)
        XCTAssertFalse(shows(window, containing: "didn't confirm"))
        XCTAssertTrue(window.buttons["pair.done"].exists)
        XCTAssertEqual(window.buttons["pair.again"].label, "Try again")
        try audit(app)
    }

    @MainActor
    func testOtherDeviceDecliningIsAttributedToIt() throws {
        let app = launch("pairing-codes")
        let window = mainWindow(app)
        askMaya(window)
        let match = app.buttons["code.match"]
        XCTAssertTrue(match.waitForExistence(timeout: 10))
        XCTAssertTrue(waitUntilEnabled(match))
        match.click()
        let title = resultTitle(window)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(spoken(title), "“Maya's iPhone” didn't confirm the code")
        XCTAssertTrue(
            text(app, "It said the codes don't match, stopped, or didn't answer in time. Nothing was saved.").exists)
        XCTAssertFalse(shows(window, containing: "You said"))
        XCTAssertEqual(window.buttons["pair.again"].label, "Try again")
        XCTAssertTrue(window.buttons["pair.done"].exists)
        try audit(app)
    }

    @MainActor
    func testReadyTimeEndingOffersStartAgain() throws {
        let app = launch("pairing-time-up")
        let window = mainWindow(app)
        let title = resultTitle(window)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(spoken(title), "Time's up")
        XCTAssertTrue(text(app, "No device asked to pair in 5 minutes. Nothing was saved.").exists)
        XCTAssertFalse(shows(window, containing: "No answer"))
        // One way on: it never gets ready again by itself.
        let again = window.buttons["pair.again"]
        XCTAssertEqual(again.label, "Start again")
        XCTAssertFalse(window.buttons["pair.done"].exists)
        XCTAssertFalse(element(window, "pair.countdown").exists)
        try audit(app)
        again.click()
        XCTAssertTrue(element(window, "pair.countdown").waitForExistence(timeout: 10))
    }

    @MainActor
    func testUsedUpReadinessSaysPairingDidNotStart() throws {
        let app = launch("pairing-used-up")
        let window = mainWindow(app)
        let title = resultTitle(window)
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        XCTAssertEqual(spoken(title), "Pairing didn't start")
        XCTAssertTrue(text(app, "A device connected, but not to pair. Nothing was saved.").exists)
        XCTAssertEqual(window.buttons["pair.again"].label, "Start again")
        XCTAssertFalse(element(window, "pair.countdown").exists)
        // The pairing screen says it; no banner on top.
        XCTAssertFalse(text(app, "A device was refused").exists)
        try audit(app)
    }

    @MainActor
    func testRefusalBannerOpensPairing() throws {
        let app = launch("refused-device")
        let window = mainWindow(app)
        XCTAssertTrue(text(app, "A device was refused").waitForExistence(timeout: 10))
        XCTAssertTrue(text(
            app,
            "A device on this network tried to connect and was turned away. "
                + "If it was trying to pair, open Pair a device on both devices."
        ).exists)
        let action = window.buttons["banner.action"]
        XCTAssertEqual(action.label, "Pair a device")
        action.click()
        XCTAssertTrue(element(window, "pair.countdown").waitForExistence(timeout: 10))
        XCTAssertFalse(text(app, "A device was refused").exists)
        // Audited here: the banner covers the screen's top, whose contrast
        // the audit would measure on the banner.
        try audit(app)
    }

    @MainActor
    func testEmptyPairedDevicesOffersPairing() throws {
        let app = launch("paired-devices-empty")
        let window = mainWindow(app)
        XCTAssertTrue(text(app, "No paired devices").waitForExistence(timeout: 10))
        XCTAssertTrue(text(
            app, "Pair a device to send to each other without a code. You can also add one by its fingerprint.").exists)
        let pair = window.buttons["paired.emptyPair"]
        XCTAssertEqual(pair.label, "Pair a device")
        XCTAssertTrue(window.buttons["paired.emptyAdd"].exists)
        XCTAssertTrue(element(window, "paired.pair").exists)
        try audit(app)
        pair.click()
        XCTAssertTrue(element(window, "pair.countdown").waitForExistence(timeout: 10))
        // Stop goes back to Paired devices.
        window.buttons["pair.stop"].click()
        XCTAssertTrue(window.buttons["paired.emptyPair"].waitForExistence(timeout: 10))
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
