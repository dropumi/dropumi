// SPDX-License-Identifier: MPL-2.0
//
// Pairing as the app drives it, against a fake core that pairs like the real
// one (scriptPairingLikeTheCore). Timers fire only when a test fires them
// (ManualTimer), and tests wait for the model with settle() and idle(),
// never with sleeps.

import DropumiBindings
import DropumiCore
import DropumiTestSupport
import Foundation
import Testing
@testable import DropumiAppState

/// `likeTheCore: false` keeps the fake's plain answers: a window that never
/// reports anything by itself.
@MainActor
private func makeModel(clock: TestClock = TestClock(), likeTheCore: Bool = true) throws
    -> (AppModel, FakeCoreClient, ManualTimer)
{
    let core = FakeCoreClient()
    if likeTheCore { core.scriptPairingLikeTheCore() }
    let paths = try AppPaths.temporary()
    try paths.prepare()
    let defaults = UserDefaults(suiteName: "dropumi-tests-\(UUID().uuidString)")!
    let timer = ManualTimer()
    let model = AppModel(
        core: core, paths: paths, settings: SettingsStore(defaults: defaults), clock: { clock.now }, timer: timer)
    return (model, core, timer)
}

private func starts(_ core: FakeCoreClient) -> Int {
    core.calls.filter { if case .startReceiving = $0 { return true } else { return false } }.count
}

private func windowOpens(_ core: FakeCoreClient) -> Int {
    core.calls.filter { $0 == .openPairingWindow }.count
}

private func requests(_ core: FakeCoreClient) -> [SendTarget] {
    core.calls.compactMap { if case let .pair(target) = $0 { return target } else { return nil } }
}

private func cancels(_ core: FakeCoreClient) -> [UInt64] {
    core.calls.compactMap { if case let .cancel(operation) = $0 { return operation } else { return nil } }
}

private let maya = SendTarget.nearby(instance: "dropumi-00000001")
private let mayaName = "Maya's iPhone"
private let studio = SendTarget.nearby(instance: "dropumi-00000002")

/// A model on the pairing screen, ready to pair: its window listens.
@MainActor
private struct OnThePairScreen {
    let model: AppModel
    let core: FakeCoreClient
    let timer: ManualTimer
    let clock: TestClock
    /// The window's operation.
    let window: UInt64

    /// Waits until the model has applied what the core delivered.
    func settle() async {
        await model.settle { core.emittedBatches }
    }

    /// Whether the screen shows this device ready to pair.
    var isWaiting: Bool {
        if case .waiting = model.pairingFlow.phase { return true }
        return false
    }

    /// Taps Pair next to Maya's iPhone and returns the request once it is
    /// under way.
    func askMaya() async throws -> UInt64 {
        model.startPairing(with: maya, name: mayaName)
        await settle()
        #expect(model.pairingFlow.phase == .asking(name: mayaName))
        let request = try #require(model.pairing?.operation)
        #expect(requests(core).last == maya)
        return request
    }
}

@MainActor
private func onThePairScreen(_ visibility: ReceiveVisibility = .pairedOnly) async throws -> OnThePairScreen {
    let clock = TestClock()
    let (model, core, timer) = try makeModel(clock: clock)
    await model.start()
    await model.finishOnboarding(visibility: visibility)
    model.enterPairingScreen()
    await model.settle { core.emittedBatches }
    let window = try #require(core.windowOperation)
    #expect(model.pairingFlow == PairingFlow(phase: .waiting(expiresAt: clock.now.addingTimeInterval(300))))
    return OnThePairScreen(model: model, core: core, timer: timer, clock: clock, window: window)
}

/// A model that receives (Paired only, the default) and has opened a pairing
/// window, which paused the receiver as the core does.
@MainActor
private struct PausedForPairing {
    let model: AppModel
    let core: FakeCoreClient
    let timer: ManualTimer
    /// The window's operation.
    let window: UInt64
    /// The receiver the window paused: the core resumes it with these
    /// settings when the window ends.
    let paused: ReceiverInfo

    /// Waits until the model has applied what the core delivered.
    func settle() async {
        await model.settle { core.emittedBatches }
    }
}

@MainActor
private func receivingWithAPairingWindow() async throws -> PausedForPairing {
    let (model, core, timer) = try makeModel()
    await model.start()
    await model.finishOnboarding()
    let paused = try #require(model.receiver)
    #expect(starts(core) == 1)
    await model.openPairingWindow()
    await model.settle { core.emittedBatches }
    let window = try #require(core.windowOperation)
    #expect(model.receiver == nil)
    #expect(model.receivingPausedForPairing)
    return PausedForPairing(model: model, core: core, timer: timer, window: window, paused: paused)
}

@MainActor
@Suite struct PairingFlowTests {
    // MARK: Receiving hold

    /// Opening a pairing window pauses the receiver (ReceiverStopped(.stopped),
    /// reported before the call returns) and the core resumes it when the
    /// window ends. The model must not start receiving meanwhile: the core
    /// refuses, and the user saw "Receiving could not start" each time they
    /// paired.
    @Test func pairingWindowPauseDoesNotRestartReceiving() async throws {
        let (model, core, _) = try makeModel()
        await model.start()
        await model.finishOnboarding()
        #expect(starts(core) == 1)
        await model.openPairingWindow()
        // The pause can also be applied after the call returns.
        model.apply([.receiverStopped(reason: .stopped)])
        await model.settle { core.emittedBatches }
        // Launch and "became active" ask for the receiver too.
        await model.applyVisibility()
        #expect(starts(core) == 1)
        #expect(model.banner == nil)
        #expect(model.receiver == nil)
        #expect(model.receivingPausedForPairing)
    }

    /// Settings chosen during a pairing window are kept, and applied when the
    /// core resumes the paused receiver (with the settings it had then).
    @Test func receivingSettingsChosenDuringAPairingWindowApplyWhenItEnds() async throws {
        let s = try await receivingWithAPairingWindow()
        await s.model.setVisibility(.everyone)
        await s.model.setAutoAcceptPaired(true)
        #expect(s.model.visibility == .everyone)
        #expect(s.model.autoAcceptPaired)
        #expect(s.model.banner == nil, "nothing refused, nothing failed")
        #expect(starts(s.core) == 1, "the core refuses receiver changes during the window")
        #expect(!s.core.calls.contains { if case .setReceivePolicy = $0 { return true } else { return false } })
        s.core.endWindow(s.window, with: .cancelled)
        await s.settle()
        #expect(!s.model.receivingPausedForPairing)
        #expect(s.core.calls.last == .setReceivePolicy(.everyone(minutes: ReceiveVisibility.everyoneMinutes), .autoTrusted))
        #expect(starts(s.core) == 1)
        #expect(s.model.banner == nil)
    }

    /// Off chosen during a window is kept; the core resumes the receiver with
    /// Paired only, and the model then stops it.
    @Test func offChosenDuringAWindowStopsTheResumedReceiver() async throws {
        let s = try await receivingWithAPairingWindow()
        await s.model.setVisibility(.off)
        #expect(s.model.visibility == .off)
        #expect(!s.core.calls.contains(.stopReceiving), "the core holds no running receiver to stop")
        s.core.endWindow(s.window, with: .cancelled)
        await s.settle()
        #expect(s.core.calls.contains(.stopReceiving), "Off stops the resumed receiver")
        #expect(s.model.receiver == nil)
        #expect(starts(s.core) == 1)
        #expect(!s.model.receivingPausedForPairing)
    }

    /// The core reports the window's end before it resumes the receiver. Off
    /// chosen in between is applied to the resumed receiver.
    @Test func offChosenBetweenTheWindowsEndAndTheResumeStopsTheReceiver() async throws {
        let s = try await receivingWithAPairingWindow()
        s.core.endWindow(s.window, with: .paired(peer: Fixtures.peer(trusted: true)), resume: false)
        await s.settle()
        #expect(s.model.receivingPausedForPairing, "the core is resuming the receiver")
        await s.model.setVisibility(.off)
        #expect(!s.core.calls.contains(.stopReceiving))
        s.core.emit(.receiverStarted(info: s.paused))
        await s.settle()
        #expect(s.core.calls.contains(.stopReceiving))
        #expect(s.model.receiver == nil)
        #expect(!s.model.receivingPausedForPairing)
    }

    /// Everyone chosen between the window's end and the resume would start a
    /// second receiver, which the core refuses. It is applied to the resumed
    /// one.
    @Test func everyoneChosenBeforeTheResumeIsAppliedToTheResumedReceiver() async throws {
        let s = try await receivingWithAPairingWindow()
        s.core.endWindow(s.window, with: .cancelled, resume: false)
        await s.settle()
        await s.model.setVisibility(.everyone)
        #expect(starts(s.core) == 1)
        #expect(s.model.banner == nil)
        s.core.emit(.receiverStarted(info: s.paused))
        await s.settle()
        #expect(s.core.calls.last == .setReceivePolicy(.everyone(minutes: ReceiveVisibility.everyoneMinutes), .prompt))
        #expect(starts(s.core) == 1)
        #expect(s.model.banner == nil)
    }

    /// A receiver the core resumes while the app is in the background stops
    /// accepting at once. (The background keeps only a window a device is
    /// pairing through.)
    @Test func receiverResumedInTheBackgroundStopsAccepting() async throws {
        let s = try await receivingWithAPairingWindow()
        try #require(s.core.attemptOnWindow() != nil)
        await s.settle()
        s.model.enterBackground()
        await s.model.idle()
        #expect(!s.core.calls.contains(.stopAccepting), "nothing is accepting during the window")
        s.core.endWindow(s.window, with: .cancelled)
        await s.settle()
        #expect(s.core.calls.contains(.stopAccepting))
        #expect(!s.core.calls.contains(.stopReceiving))
    }

    /// With receiving Off the window still opens (it is an explicit act), and
    /// nothing resumes after it; a choice made during it applies when it ends.
    @Test func windowWithReceivingOffStillOpensAndAppliesAChoiceWhenItEnds() async throws {
        let (model, core, _) = try makeModel()
        await model.start()
        await model.finishOnboarding(visibility: .off)
        await model.openPairingWindow()
        await model.settle { core.emittedBatches }
        let window = try #require(core.windowOperation)
        #expect(model.receivingPausedForPairing)
        await model.setVisibility(.pairedOnly)
        #expect(starts(core) == 0, "the core refuses a start during the window")
        core.endWindow(window, with: .cancelled)
        await model.settle { core.emittedBatches }
        #expect(!model.receivingPausedForPairing)
        #expect(starts(core) == 1)
        #expect(model.receiver != nil)
        #expect(model.banner == nil)
    }

    /// If the core's resume is never reported, the hold still ends after a
    /// bounded wait, and the settings apply.
    @Test func pairingHoldEndsWhenTheResumeIsNeverReported() async throws {
        let s = try await receivingWithAPairingWindow()
        s.core.endWindow(s.window, with: .cancelled, resume: false)
        await s.settle()
        #expect(s.model.receivingPausedForPairing)
        #expect(s.timer.pending == [PairingTiming().resumeWait])
        s.timer.fireAll()
        await s.model.idle()
        #expect(!s.model.receivingPausedForPairing)
        #expect(starts(s.core) == 2, "receiving starts again")
        #expect(s.model.receiver != nil)
    }

    /// A resume that fails ends the hold and says so like any other stop; the
    /// bounded wait then does nothing.
    @Test func resumeThatFailsShowsReceivingStopped() async throws {
        let s = try await receivingWithAPairingWindow()
        s.core.endWindow(s.window, with: .cancelled, resume: false)
        await s.settle()
        s.core.emit(.receiverStopped(reason: .internal))
        await s.settle()
        #expect(!s.model.receivingPausedForPairing)
        let banner = try #require(s.model.banner)
        #expect(banner.title == "Receiving stopped")
        #expect(banner.action == .retryReceiving)
        s.timer.fireAll()
        await s.model.idle()
        #expect(starts(s.core) == 1, "nothing restarts by itself")
    }

    /// A window's end dropped with other events (ResyncRequired) cannot keep
    /// the settings on hold.
    @Test func resyncEndsThePairingHoldWhenTheWindowsEndWasDropped() async throws {
        let (model, core, _) = try makeModel()
        // The model does not read the core's events in this test; it gets
        // only those applied here, so the window's end can be lost.
        await model.finishOnboarding()
        await model.openPairingWindow()
        let window = try #require(core.windowOperation)
        model.apply([.receiverStopped(reason: .stopped)])
        #expect(model.receivingPausedForPairing)
        // The window ends and the receiver resumes; the model hears nothing.
        core.endWindow(window, with: .cancelled)
        model.apply([.eventsDropped(count: 2), .resyncRequired])
        await model.idle()
        #expect(!model.receivingPausedForPairing)
        #expect(model.receiver != nil)
        await model.setVisibility(.off)
        #expect(core.calls.contains(.stopReceiving))
    }

    /// A window the core refuses ends the hold at once, says why on the
    /// pairing screen, and a change made then applies.
    @Test func refusedWindowEndsTheHoldAndSaysWhyInline() async throws {
        let (model, core, _) = try makeModel()
        await model.start()
        await model.finishOnboarding()
        let refusal = DropumiError.Busy(detail: "a transfer is running")
        core.failures["openPairingWindow"] = refusal
        await model.openPairingWindow()
        #expect(!model.receivingPausedForPairing)
        #expect(model.pairingFlow.phase == .paused(PairingClassifier.classify(windowError: refusal)))
        await model.setVisibility(.off)
        #expect(core.calls.contains(.stopReceiving))
    }

    // MARK: The pairing screen

    /// Showing the pairing screen makes this device ready to pair at once
    /// and lists nearby devices; receiving pauses without a banner.
    @Test func enteringThePairScreenOpensAWindowAndBrowses() async throws {
        let s = try await onThePairScreen()
        #expect(s.core.calls.contains(.startDiscovery))
        #expect(windowOpens(s.core) == 1)
        #expect(s.model.pairing?.role == .window)
        #expect(s.model.pairing?.port == 47321)
        #expect(s.model.receivingPausedForPairing)
        #expect(s.model.banner == nil)
    }

    /// Leaving closes this device's window and stops browsing; receiving
    /// resumes.
    @Test func leavingThePairScreenClosesTheWindowAndStopsBrowsing() async throws {
        let s = try await onThePairScreen()
        s.model.leavePairingScreen()
        await s.settle()
        #expect(cancels(s.core) == [s.window])
        #expect(s.core.calls.contains(.stopDiscovery))
        #expect(s.core.windowOperation == nil)
        #expect(s.model.pairingFlow == PairingFlow())
        #expect(!s.model.receivingPausedForPairing)
        #expect(s.model.receiver != nil)
        #expect(s.model.banner == nil)
    }

    /// Two pairing screens on show share one window; it closes when the
    /// last goes.
    @Test func twoVisibleScreensShareOneWindow() async throws {
        let s = try await onThePairScreen()
        s.model.enterPairingScreen()
        await s.settle()
        #expect(windowOpens(s.core) == 1)
        s.model.leavePairingScreen()
        await s.settle()
        #expect(cancels(s.core).isEmpty)
        #expect(s.isWaiting)
        s.model.leavePairingScreen()
        await s.settle()
        #expect(cancels(s.core) == [s.window])
        #expect(s.core.calls.contains(.stopDiscovery))
    }

    /// Leaving and coming straight back: the next window opens only once
    /// the one before has ended (the core allows one at a time).
    @Test func quickLeaveAndReturnOpensTheNextWindowOnlyAfterTheLastEnds() async throws {
        let s = try await onThePairScreen()
        s.model.leavePairingScreen()
        s.model.enterPairingScreen()
        #expect(s.model.pairingFlow.phase == .opening)
        await s.settle()
        let calls = s.core.calls
        let closed = try #require(calls.firstIndex(of: .cancel(s.window)))
        let reopened = try #require(calls.lastIndex(of: .openPairingWindow))
        #expect(closed < reopened)
        #expect(windowOpens(s.core) == 2)
        let next = try #require(s.core.windowOperation)
        #expect(next != s.window)
        #expect(s.model.pairing?.operation == next)
        #expect(s.isWaiting)
        #expect(s.model.banner == nil)
    }

    /// A request started just before the screen appeared (Nearby's "Pair
    /// with this device"): the screen shows it, and a window opens only if
    /// it ends before a code.
    @Test func requestStartedBeforeTheScreenAppearsOpensNoWindow() async throws {
        let (model, core, _) = try makeModel()
        await model.start()
        await model.finishOnboarding()
        model.startPairing(with: maya, name: mayaName)
        model.enterPairingScreen()
        await model.settle { core.emittedBatches }
        #expect(windowOpens(core) == 0)
        #expect(model.pairingFlow.phase == .asking(name: mayaName))
        let request = try #require(model.pairing?.operation)
        core.emit(.pairingFinished(operation: request, result: .failed(error: .notAuthorized)))
        await model.settle { core.emittedBatches }
        #expect(windowOpens(core) == 1)
        #expect(model.pairingFlow.note == .notWaiting(name: mayaName))
        guard case .waiting = model.pairingFlow.phase else {
            Issue.record("not ready to pair: \(model.pairingFlow.phase)")
            return
        }
    }

    /// The iOS code prompt covers the pairing screen, which then goes away:
    /// the comparison goes on, and its outcome is shown when the screen is
    /// back.
    @Test func leavingDuringACodeComparisonKeepsThePairing() async throws {
        let s = try await onThePairScreen()
        try #require(s.core.attemptOnWindow() != nil)
        await s.settle()
        #expect(s.model.pairingFlow.phase == .comparing)
        s.model.leavePairingScreen()
        await s.settle()
        #expect(cancels(s.core).isEmpty)
        #expect(s.model.pairingFlow.phase == .comparing)
        let paired = Fixtures.peer(trusted: true, alias: mayaName)
        s.core.endWindow(s.window, with: .paired(peer: paired))
        await s.settle()
        s.model.enterPairingScreen()
        await s.settle()
        #expect(s.model.pairingFlow.phase == .finished(.paired(paired)))
        #expect(windowOpens(s.core) == 1)
    }

    /// An outcome seen on the screen is not shown again: back on the screen
    /// later, this device is ready to pair again.
    @Test func returningAfterASeenOutcomeWaitsAgain() async throws {
        let s = try await onThePairScreen()
        s.core.endWindow(s.window, with: .failed(error: .notAuthorized))
        await s.settle()
        #expect(s.model.pairingFlow.phase == .paused(.usedUp))
        s.model.leavePairingScreen()
        #expect(s.model.pairingFlow == PairingFlow())
        s.model.enterPairingScreen()
        await s.settle()
        #expect(windowOpens(s.core) == 2)
        #expect(s.isWaiting)
    }

    /// iOS: an offer from the device just paired covers the pairing screen,
    /// which then goes away. When the offer closes, "Paired with" is still
    /// there and this device is not ready to pair without a tap.
    @Test func promptOverAResultKeepsItAndOpensNoWindow() async throws {
        let s = try await onThePairScreen()
        try #require(s.core.attemptOnWindow() != nil)
        await s.settle()
        let paired = Fixtures.peer(trusted: true, alias: mayaName)
        s.core.emit(.promptClosed(id: 2, reason: .answered))
        s.core.endWindow(s.window, with: .paired(peer: paired))
        await s.settle()
        #expect(s.model.pairingFlow.phase == .finished(.paired(paired)))
        let offer = Fixtures.offer(id: 3, peer: paired)
        s.core.emit(.promptOpened(prompt: offer))
        await s.settle()
        s.model.leavePairingScreen()
        await s.settle()
        s.core.emit(.promptClosed(id: offer.promptId, reason: .answered))
        await s.settle()
        s.model.enterPairingScreen()
        await s.settle()
        #expect(s.model.pairingFlow.phase == .finished(.paired(paired)))
        s.timer.fireAll()
        await s.model.idle()
        #expect(windowOpens(s.core) == 1)
        #expect(!s.model.receivingPausedForPairing)
    }

    /// iOS: a prompt covers "Time's up": it is still there when the prompt
    /// closes, and only Start again makes this device ready.
    @Test func promptOverTimeUpKeepsStartAgain() async throws {
        let s = try await onThePairScreen()
        s.clock.advance(by: 300)
        s.core.endWindow(s.window, with: .failed(error: .timeout))
        await s.settle()
        #expect(s.model.pairingFlow.phase == .paused(.timeUp))
        let offer = Fixtures.offer(id: 3)
        s.core.emit(.promptOpened(prompt: offer))
        await s.settle()
        s.model.leavePairingScreen()
        s.core.emit(.promptClosed(id: offer.promptId, reason: .answered))
        await s.settle()
        s.model.enterPairingScreen()
        await s.settle()
        #expect(s.model.pairingFlow.phase == .paused(.timeUp))
        #expect(windowOpens(s.core) == 1)
        s.model.startWaiting()
        await s.settle()
        #expect(windowOpens(s.core) == 2)
        #expect(s.isWaiting)
    }

    /// Stop ends being ready without a card or a banner, and nothing opens
    /// the window again.
    @Test func stopEndsBeingReadyQuietly() async throws {
        let s = try await onThePairScreen()
        s.model.stopPairing()
        // The screen then goes away.
        s.model.leavePairingScreen()
        await s.settle()
        #expect(cancels(s.core) == [s.window])
        #expect(s.model.pairingFlow == PairingFlow())
        #expect(s.model.banner == nil)
        #expect(!s.model.receivingPausedForPairing)
        s.timer.fireAll()
        await s.model.idle()
        #expect(windowOpens(s.core) == 1)
    }

    // MARK: Taps

    /// Tapping a device closes this device's window first, then asks it.
    @Test func tappingADeviceClosesThisDevicesWindowBeforeAsking() async throws {
        let s = try await onThePairScreen()
        s.model.startPairing(with: maya, name: mayaName)
        #expect(s.model.pairingFlow.phase == .switching(name: mayaName))
        await s.settle()
        let calls = s.core.calls
        let closed = try #require(calls.firstIndex(of: .cancel(s.window)))
        let asked = try #require(calls.firstIndex(of: .pair(maya)))
        #expect(closed < asked)
        #expect(!s.core.pairCalledWhileWindowOpen)
        #expect(s.model.pairingFlow == PairingFlow(phase: .asking(name: mayaName)))
        #expect(windowOpens(s.core) == 1)
        #expect(s.model.banner == nil)
    }

    /// A second tap, on the same device or another, starts nothing more.
    @Test func doubleTapStartsOneRequest() async throws {
        let s = try await onThePairScreen()
        s.model.startPairing(with: maya, name: mayaName)
        s.model.startPairing(with: maya, name: mayaName)
        await s.settle()
        s.model.startPairing(with: studio, name: "Studio Mac")
        await s.settle()
        #expect(requests(s.core) == [maya])
        #expect(cancels(s.core) == [s.window])
        #expect(s.model.pairingFlow.phase == .asking(name: mayaName))
    }

    /// A tap while this device's window is still opening: the window closes
    /// as soon as the core has opened it, then the device is asked.
    @Test func tapWhileTheWindowIsOpeningCancelsItOnceRegistered() async throws {
        let (model, core, _) = try makeModel()
        await model.start()
        await model.finishOnboarding()
        let gate = core.holdNext("openPairingWindow")
        model.enterPairingScreen()
        await gate.reached()
        model.startPairing(with: maya, name: mayaName)
        #expect(model.pairingFlow.phase == .switching(name: mayaName))
        gate.open()
        await model.settle { core.emittedBatches }
        #expect(windowOpens(core) == 1)
        #expect(cancels(core).count == 1)
        #expect(core.windowOperation == nil)
        #expect(requests(core) == [maya])
        #expect(!core.pairCalledWhileWindowOpen)
        #expect(model.pairingFlow.phase == .asking(name: mayaName))
    }

    /// Pair another device, then a tap while the next window still waits for
    /// the receiver the last one paused: the window never opens, and the
    /// device is asked. How the last window ended (paired) does not count
    /// for this one.
    @Test func tapWhileTheNextWindowWaitsForTheResumeStillAsks() async throws {
        let (model, core, timer) = try makeModel()
        await model.start()
        await model.finishOnboarding()
        let paused = try #require(model.receiver)
        model.enterPairingScreen()
        await model.settle { core.emittedBatches }
        let window = try #require(core.windowOperation)
        let paired = Fixtures.peer(trusted: true, alias: "Studio Mac")
        core.endWindow(window, with: .paired(peer: paired), resume: false)
        await model.settle { core.emittedBatches }
        #expect(model.pairingFlow.phase == .finished(.paired(paired)))
        model.startWaiting()
        model.startPairing(with: maya, name: mayaName)
        #expect(model.pairingFlow.phase == .switching(name: mayaName))
        await timer.waitForPending(PairingTiming().windowCloseWait)
        core.emit(.receiverStarted(info: paused))
        await model.settle { core.emittedBatches }
        #expect(windowOpens(core) == 1)
        #expect(requests(core) == [maya])
        #expect(model.pairingFlow.phase == .asking(name: mayaName))
    }

    /// A tap is ignored while a device is pairing with this one: that
    /// comparison goes on.
    @Test func tapIsIgnoredWhileAnotherDeviceIsPairingWithThisOne() async throws {
        let s = try await onThePairScreen()
        s.core.emit(.connected(operation: s.window, peer: Fixtures.peer(name: "Studio Mac")))
        await s.settle()
        #expect(s.model.pairingFlow.phase == .comparing)
        s.model.startPairing(with: maya, name: mayaName)
        await s.settle()
        #expect(requests(s.core).isEmpty)
        #expect(cancels(s.core).isEmpty)
        #expect(s.model.pairingFlow.phase == .comparing)
    }

    /// A window that never reports its end does not block the request: it
    /// goes ahead after a bounded wait, and not before.
    @Test func windowThatNeverReportsItsEndStillLetsTheRequestGo() async throws {
        let (model, core, timer) = try makeModel(likeTheCore: false)
        await model.start()
        await model.finishOnboarding()
        model.enterPairingScreen()
        await model.settle { core.emittedBatches }
        let window = try #require(model.pairing?.operation)
        core.emit(.pairingWindowOpened(operation: window, port: 47321))
        await model.settle { core.emittedBatches }
        model.startPairing(with: maya, name: mayaName)
        await timer.waitForPending(PairingTiming().windowCloseWait)
        #expect(requests(core).isEmpty, "not before the wait is over")
        timer.fireAll()
        await model.idle()
        #expect(cancels(core) == [window])
        #expect(requests(core) == [maya])
        #expect(model.pairingFlow.phase == .asking(name: mayaName))
    }

    /// Cancel while this device's window closes: the device is not asked,
    /// and this one is ready to pair again, with a note.
    @Test func cancelWhileSwitchingDoesNotDial() async throws {
        let s = try await onThePairScreen()
        s.model.startPairing(with: maya, name: mayaName)
        s.model.cancelPairing()
        await s.settle()
        #expect(requests(s.core).isEmpty)
        #expect(s.model.pairingFlow.note == .cancelled)
        #expect(windowOpens(s.core) == 2)
        #expect(s.isWaiting)
    }

    /// Cancel while asking: the request is cancelled, and this device is
    /// ready to pair again with "Pairing cancelled.".
    @Test func cancelWhileAskingGoesBackToReadyWithANote() async throws {
        let s = try await onThePairScreen()
        let request = try await s.askMaya()
        s.model.cancelPairing()
        await s.settle()
        #expect(cancels(s.core) == [s.window, request])
        #expect(s.model.pairingFlow.note == .cancelled)
        #expect(s.model.pairingFlow.note?.noteText == "Pairing cancelled.")
        #expect(windowOpens(s.core) == 2)
        #expect(s.isWaiting)
    }

    /// The device tapped changed in the list (it may have just got ready to
    /// pair): a note, and this device is ready again.
    @Test func vanishedDeviceGetsANoteAndThisDeviceWaitsAgain() async throws {
        let s = try await onThePairScreen()
        s.core.failures["pair"] = .NotFound(detail: "that device is no longer nearby")
        s.model.startPairing(with: maya, name: mayaName)
        await s.settle()
        #expect(s.model.pairingFlow.note == .noLongerNearby(name: mayaName))
        #expect(windowOpens(s.core) == 2)
        #expect(s.isWaiting)
        #expect(s.model.banner == nil)
    }

    /// Without the pairing screen (the interop test, older call sites) a
    /// request that cannot start shows the banner as before.
    @Test func pairWithoutTheScreenKeepsTheBannerOnFailure() async throws {
        let (model, core, _) = try makeModel()
        await model.start()
        await model.finishOnboarding()
        core.failures["pair"] = .Busy(detail: "another operation is running")
        await model.pair(with: maya, name: mayaName)
        #expect(model.banner?.title == "Pairing could not start")
        #expect(model.pairingFlow == PairingFlow())
        #expect(windowOpens(core) == 0)
    }

    /// Without the pairing screen, the request's result is kept on the
    /// session (the interop test reads it).
    @Test func pairWithoutTheScreenKeepsItsResult() async throws {
        let (model, core, _) = try makeModel()
        await model.start()
        await model.finishOnboarding()
        await model.pair(with: .address(address: "192.168.1.20:47321", expectFingerprint: nil), name: "CLI")
        let request = try #require(model.pairing?.operation)
        let peer = Fixtures.peer(name: "CLI", trusted: true)
        core.emit(.pairingFinished(operation: request, result: .paired(peer: peer)))
        await model.settle { core.emittedBatches }
        #expect(model.pairing?.result == .paired(peer: peer))
        #expect(windowOpens(core) == 0)
    }

    // MARK: Outcomes

    /// The device asked was not waiting to pair: a note that says what to
    /// do (never "Only paired devices"), and this device is ready again.
    @Test func notWaitingDeviceGetsAClearNoteAndThisDeviceWaitsAgain() async throws {
        let s = try await onThePairScreen()
        let request = try await s.askMaya()
        s.core.emit(.pairingFinished(operation: request, result: .failed(error: .notAuthorized)))
        await s.settle()
        let note = try #require(s.model.pairingFlow.note)
        #expect(note == .notWaiting(name: mayaName))
        let text = try #require(note.noteText)
        #expect(text.contains("isn't waiting to pair"))
        #expect(!text.contains("Only paired devices"))
        #expect(windowOpens(s.core) == 2)
        #expect(s.isWaiting)
    }

    /// A connection that is not a pairing request uses up the window: it
    /// ends as "Pairing didn't start", with Start again, and does not open
    /// again by itself.
    @Test func strayConnectionEndsWithStartAgainAndNoReopen() async throws {
        let s = try await onThePairScreen()
        s.core.emit(.sessionClosed(peer: nil, code: "not_authorized"))
        s.core.endWindow(s.window, with: .failed(error: .notAuthorized))
        await s.settle()
        #expect(s.model.pairingFlow == PairingFlow(phase: .paused(.usedUp)))
        #expect(s.model.banner == nil)
        s.timer.fireAll()
        await s.model.idle()
        #expect(windowOpens(s.core) == 1)
        s.model.startWaiting()
        await s.settle()
        #expect(windowOpens(s.core) == 2)
        #expect(s.isWaiting)
    }

    /// The screen left and came back while its window was closing, and a
    /// stray connection used that window up before the close landed: the
    /// result shows, and the window asked for on return never opens behind
    /// it.
    @Test func strayWhileTheNextWindowWaitsOpensNoWindowBehindTheResult() async throws {
        let s = try await onThePairScreen()
        s.model.leavePairingScreen()
        s.model.enterPairingScreen()
        s.core.emit(.sessionClosed(peer: nil, code: "not_authorized"))
        s.core.endWindow(s.window, with: .failed(error: .notAuthorized))
        await s.settle()
        s.timer.fireAll()
        await s.model.idle()
        #expect(s.model.pairingFlow == PairingFlow(phase: .paused(.usedUp)))
        #expect(windowOpens(s.core) == 1)
        #expect(s.core.windowOperation == nil)
        #expect(!s.model.receivingPausedForPairing)
        s.model.startWaiting()
        await s.settle()
        #expect(windowOpens(s.core) == 2)
        #expect(s.isWaiting)
    }

    /// An attempt that reached a code never makes this device ready again
    /// by itself, on either side.
    @Test func attemptThatShowedACodeNeverReopensTheWindow() async throws {
        let s = try await onThePairScreen()
        try #require(s.core.attemptOnWindow() != nil)
        await s.settle()
        s.core.endWindow(s.window, with: .failed(error: .connectionLost))
        await s.settle()
        #expect(s.model.pairingFlow.phase == .finished(.connectionLost(name: nil)))
        s.timer.fireAll()
        await s.model.idle()
        #expect(windowOpens(s.core) == 1)
        // Try again, then this device asks; a refusal after the code is a
        // result too.
        s.model.startWaiting()
        await s.settle()
        let request = try await s.askMaya()
        s.core.emit(.sessionCode(operation: request, peer: Fixtures.peer(), code: "123456"))
        s.core.emit(.pairingFinished(operation: request, result: .failed(error: .notAuthorized)))
        await s.settle()
        #expect(s.model.pairingFlow.phase == .finished(.failed(.notAuthorized, name: mayaName)))
        s.timer.fireAll()
        await s.model.idle()
        #expect(windowOpens(s.core) == 2)
    }

    /// "You said the codes don't match" is for the device whose user said
    /// no; the other device reads that its partner didn't confirm. The core
    /// reports both as the same rejection.
    @Test func youDeclinedIsToldApartFromTheyDeclined() async throws {
        // This device asked, and its user said no.
        let asker = try await onThePairScreen()
        let request = try await asker.askMaya()
        let prompt = Fixtures.code(id: 7, operation: request)
        asker.core.emit(.promptOpened(prompt: prompt))
        await asker.settle()
        #expect(asker.model.pairingFlow.phase == .comparing)
        await asker.model.answer(prompt, yes: false)
        asker.core.emit(.pairingFinished(operation: request, result: .failed(error: .pairingRejected)))
        await asker.settle()
        #expect(asker.model.pairingFlow.phase == .finished(.youDeclined))

        // The device asked: its user said the codes match.
        let asked = try await onThePairScreen()
        let theirs = try #require(asked.core.attemptOnWindow())
        await asked.settle()
        await asked.model.answer(theirs, yes: true)
        asked.core.endWindow(asked.window, with: .failed(error: .pairingRejected))
        await asked.settle()
        #expect(asked.model.pairingFlow.phase == .finished(.otherDeclined(name: nil)))
        #expect(PairingOutcome.otherDeclined(name: nil).text.title == "The other device didn't confirm the code")
    }

    /// The result can arrive before the answer's call returns; the answer
    /// still counts.
    @Test func answerCountsEvenIfTheResultArrivesFirst() async throws {
        let s = try await onThePairScreen()
        let prompt = try #require(s.core.attemptOnWindow())
        await s.settle()
        let gate = s.core.holdNext("answerCode")
        let answering = Task { await s.model.answer(prompt, yes: false) }
        await gate.reached()
        s.core.endWindow(s.window, with: .failed(error: .pairingRejected))
        await s.settle()
        #expect(s.model.pairingFlow.phase == .finished(.youDeclined))
        gate.open()
        await answering.value
        #expect(s.model.pairingFlow.phase == .finished(.youDeclined))
        #expect(s.model.banner == nil)
    }

    /// Nobody confirmed the code before it ran out here: "The code expired".
    @Test func expiredCodeIsCodeExpired() async throws {
        let s = try await onThePairScreen()
        let request = try await s.askMaya()
        let prompt = Fixtures.code(id: 7, operation: request, expiresAt: s.clock.now.addingTimeInterval(60))
        s.core.emit(.promptOpened(prompt: prompt))
        await s.settle()
        s.clock.advance(by: 60)
        s.core.emit(.pairingFinished(operation: request, result: .failed(error: .pairingRejected)))
        await s.settle()
        #expect(s.model.pairingFlow.phase == .finished(.codeExpired))
    }

    /// The five minutes ran out with nobody asking: "Time's up", with Start
    /// again; the window does not open again by itself.
    @Test func windowExpiryOffersStartAgainAndDoesNotReopen() async throws {
        let s = try await onThePairScreen()
        s.clock.advance(by: 300)
        s.core.endWindow(s.window, with: .failed(error: .timeout))
        await s.settle()
        #expect(s.model.pairingFlow.phase == .paused(.timeUp))
        #expect(s.model.banner == nil)
        s.timer.fireAll()
        await s.model.idle()
        #expect(windowOpens(s.core) == 1)
        #expect(!s.model.receivingPausedForPairing)
    }

    /// Events of a request reported before its call returned are kept and
    /// applied once it has: here the code was shown, so a rejection is a
    /// result, not a note.
    @Test func pairingResultBeforeTheCallReturnsIsKept() async throws {
        let s = try await onThePairScreen()
        let gate = s.core.holdNext("pair")
        s.model.startPairing(with: maya, name: mayaName)
        await gate.reached()
        // The fake numbers operations in order: the request comes next.
        let request = s.window + 1
        let peer = Fixtures.peer()
        s.model.apply([
            .connected(operation: request, peer: peer),
            .sessionCode(operation: request, peer: peer, code: "123456"),
            .pairingFinished(operation: request, result: .failed(error: .pairingRejected)),
        ])
        #expect(s.model.pairingFlow.phase == .asking(name: mayaName))
        gate.open()
        await s.settle()
        #expect(s.model.pairing?.operation == request)
        #expect(s.model.pairing?.result == .failed(error: .pairingRejected))
        #expect(s.model.pairingFlow.phase == .finished(.otherDeclined(name: mayaName)))
        #expect(s.model.operationCount == 0)
        #expect(windowOpens(s.core) == 1)
    }

    /// A code prompt opened before the request's call returned counts for
    /// an end kept meanwhile: that end is a result, and this device does
    /// not get ready again by itself.
    @Test func codePromptBeforeTheCallReturnsCountsForAKeptEnd() async throws {
        let s = try await onThePairScreen()
        let gate = s.core.holdNext("pair")
        s.model.startPairing(with: maya, name: mayaName)
        await gate.reached()
        // The fake numbers operations in order: the request comes next.
        let request = s.window + 1
        s.model.apply([
            .promptOpened(prompt: Fixtures.code(id: 7, operation: request)),
            .pairingFinished(operation: request, result: .failed(error: .connectionLost)),
        ])
        gate.open()
        await s.settle()
        #expect(s.model.pairingFlow.phase == .finished(.connectionLost(name: mayaName)))
        s.timer.fireAll()
        await s.model.idle()
        #expect(windowOpens(s.core) == 1)
    }

    /// The window's port reported before its call returned is kept.
    @Test func windowPortBeforeTheCallReturnsIsKept() async throws {
        let clock = TestClock()
        let (model, core, _) = try makeModel(clock: clock, likeTheCore: false)
        await model.start()
        await model.finishOnboarding()
        let gate = core.holdNext("openPairingWindow")
        model.enterPairingScreen()
        await gate.reached()
        // The fake numbers operations from 1: the window is the first.
        model.apply([.pairingWindowOpened(operation: 1, port: 47000)])
        gate.open()
        await model.settle { core.emittedBatches }
        #expect(model.pairing?.operation == 1)
        #expect(model.pairing?.port == 47000)
        #expect(model.pairingFlow.phase == .waiting(expiresAt: clock.now.addingTimeInterval(300)))
    }

    // MARK: Background, notices and security

    /// Going to the background closes a window nobody is pairing through;
    /// back in front on the pairing screen, this device is ready again.
    @Test func backgroundClosesTheWindowAndReturningReopensIt() async throws {
        let s = try await onThePairScreen()
        s.model.enterBackground()
        await s.settle()
        #expect(cancels(s.core) == [s.window])
        #expect(s.core.windowOperation == nil)
        #expect(windowOpens(s.core) == 1)
        await s.model.becomeActive()
        await s.settle()
        #expect(windowOpens(s.core) == 2)
        #expect(s.isWaiting)
        #expect(s.model.banner == nil)
    }

    /// A code comparison under way is kept in the background.
    @Test func backgroundDuringACodeComparisonKeepsIt() async throws {
        let s = try await onThePairScreen()
        try #require(s.core.attemptOnWindow() != nil)
        await s.settle()
        s.model.enterBackground()
        await s.settle()
        #expect(cancels(s.core).isEmpty)
        #expect(s.model.pairingFlow.phase == .comparing)
        await s.model.becomeActive()
        await s.settle()
        #expect(windowOpens(s.core) == 1)
        #expect(s.model.pairingFlow.phase == .comparing)
    }

    /// A device that is not pairing uses the window up before the core has
    /// the background's close: back in front, "Pairing didn't start" stays
    /// and waits for Start again.
    @Test func strayBeforeTheBackgroundCloseStillNeedsStartAgain() async throws {
        let s = try await onThePairScreen()
        s.model.enterBackground()
        s.core.endWindow(s.window, with: .failed(error: .notAuthorized))
        await s.settle()
        #expect(s.model.pairingFlow.phase == .paused(.usedUp))
        await s.model.becomeActive()
        await s.settle()
        s.timer.fireAll()
        await s.model.idle()
        #expect(s.model.pairingFlow.phase == .paused(.usedUp))
        #expect(windowOpens(s.core) == 1)
    }

    /// A device gets as far as the code before the core has the
    /// background's close: back in front, its result stays and this device
    /// is not ready to pair without a tap.
    @Test func codeBeforeTheBackgroundCloseKeepsTheResult() async throws {
        let s = try await onThePairScreen()
        s.model.enterBackground()
        try #require(s.core.attemptOnWindow() != nil)
        s.core.endWindow(s.window, with: .failed(error: .connectionLost))
        await s.settle()
        #expect(s.model.pairingFlow.phase == .finished(.connectionLost(name: nil)))
        await s.model.becomeActive()
        await s.settle()
        s.timer.fireAll()
        await s.model.idle()
        #expect(s.model.pairingFlow.phase == .finished(.connectionLost(name: nil)))
        #expect(windowOpens(s.core) == 1)
    }

    /// Back in front with the pairing screen gone: no window opens.
    @Test func returningWithTheScreenGoneOpensNoWindow() async throws {
        let s = try await onThePairScreen()
        s.model.enterBackground()
        s.model.leavePairingScreen()
        await s.settle()
        await s.model.becomeActive()
        await s.settle()
        #expect(windowOpens(s.core) == 1)
        #expect(s.model.pairingFlow == PairingFlow())
    }

    /// While this device waits to pair, a refused connection is told on the
    /// pairing screen, not in a banner; away from it, the banner is back and
    /// offers Pair a device.
    @Test func refusalIsNotShownWhileThisDeviceWaitsToPair() async throws {
        let s = try await onThePairScreen()
        s.core.emit(.sessionClosed(peer: nil, code: "not_authorized"))
        await s.settle()
        #expect(s.model.banner == nil)
        s.model.leavePairingScreen()
        await s.settle()
        s.core.emit(.sessionClosed(peer: nil, code: "not_authorized"))
        await s.settle()
        #expect(s.model.banner?.title == "A device was refused")
        #expect(s.model.banner?.action == .openPairing)
    }

    /// The screen gone but this device's window still opening: no banner
    /// until the window has ended.
    @Test func refusalIsNotShownWhileThisDevicesWindowIsOpening() async throws {
        let (model, core, _) = try makeModel()
        await model.start()
        await model.finishOnboarding()
        let gate = core.holdNext("openPairingWindow")
        model.enterPairingScreen()
        await gate.reached()
        model.leavePairingScreen()
        model.apply([.sessionClosed(peer: nil, code: "not_authorized")])
        #expect(model.banner == nil)
        gate.open()
        await model.settle { core.emittedBatches }
        #expect(core.windowOperation == nil)
        model.apply([.sessionClosed(peer: nil, code: "not_authorized")])
        #expect(model.banner?.action == .openPairing)
    }

    /// The screen gone but a device still comparing a code through this
    /// device's window: no banner while the window is open.
    @Test func refusalIsNotShownWhileThisDevicesWindowIsOpen() async throws {
        let s = try await onThePairScreen()
        try #require(s.core.attemptOnWindow() != nil)
        await s.settle()
        s.model.leavePairingScreen()
        await s.settle()
        s.core.emit(.sessionClosed(peer: nil, code: "not_authorized"))
        await s.settle()
        #expect(s.model.banner == nil)
        s.core.endWindow(s.window, with: .failed(error: .pairingRejected))
        await s.settle()
        s.core.emit(.sessionClosed(peer: nil, code: "not_authorized"))
        await s.settle()
        #expect(s.model.banner?.action == .openPairing)
    }

    /// Reset right after leaving the pairing screen: the core refuses it
    /// while the window is open, so the reset waits for the window to end.
    @Test func resetIdentityRightAfterLeavingWaitsForTheWindowToClose() async throws {
        let s = try await onThePairScreen(.off)
        s.model.leavePairingScreen()
        await s.model.resetIdentity()
        #expect(s.model.identity?.fingerprint == FakeCoreClient.resetFingerprint)
        #expect(s.model.banner?.title == "New identity")
        let calls = s.core.calls
        let closed = try #require(calls.firstIndex(of: .cancel(s.window)))
        let reset = try #require(calls.firstIndex(of: .resetIdentity))
        #expect(closed < reset)
    }

    /// During a request the identity and the name stay: the user finishes
    /// or stops pairing first.
    @Test func resetIdentityDuringARequestSaysFinishPairingFirst() async throws {
        let s = try await onThePairScreen()
        _ = try await s.askMaya()
        await s.model.resetIdentity()
        #expect(s.model.banner?.title == "The identity was not reset")
        #expect(s.model.banner?.message == "Pairing is under way. Finish or stop it first.")
        #expect(!s.core.calls.contains(.resetIdentity))
        #expect(s.model.identity?.fingerprint == FakeCoreClient.fingerprint)
        await s.model.renameDevice("Kitchen iPad")
        #expect(s.model.banner?.title == "The name was not changed")
        #expect(s.model.banner?.message == "Pairing is under way. Finish or stop it first.")
        #expect(s.model.identity?.name == "Dropumi-1A2B")
    }

    /// Whatever the core reports and the screen does, the model never
    /// answers a code or stores trust by itself.
    @Test func theModelNeverAnswersACodeOrTrustsByItself() async throws {
        let s = try await onThePairScreen()
        try #require(s.core.attemptOnWindow() != nil)
        await s.settle()
        // The iOS code prompt covers the screen, and the app goes away and
        // comes back.
        s.model.leavePairingScreen()
        s.model.enterPairingScreen()
        s.model.enterBackground()
        await s.model.becomeActive()
        s.clock.advance(by: 300)
        s.core.endWindow(s.window, with: .failed(error: .timeout))
        await s.settle()
        s.model.startWaiting()
        await s.settle()
        let request = try await s.askMaya()
        s.core.emit(.promptOpened(prompt: Fixtures.code(id: 9, operation: request)))
        s.core.emit(.pairingFinished(operation: request, result: .paired(peer: Fixtures.peer(trusted: true))))
        await s.settle()
        #expect(!s.core.calls.contains { if case .answerCode = $0 { return true } else { return false } })
        #expect(!s.core.calls.contains { if case .trust = $0 { return true } else { return false } })
    }

    /// A device that connected is not shown as paired until the core says
    /// so (PairingFinished(.paired)), even if it claims a paired device's
    /// name.
    @Test func aConnectedPeerIsNeverShownAsPairedBeforePairingFinished() async throws {
        let s = try await onThePairScreen()
        try #require(s.core.attemptOnWindow(peer: Fixtures.peer(name: "Studio Mac")) != nil)
        await s.settle()
        #expect(s.model.pairingFlow == PairingFlow(phase: .comparing))
        #expect(s.model.pairing?.result == nil)
        #expect(s.model.trusted.isEmpty)
        let paired = Fixtures.peer(name: "Studio Mac", trusted: true, alias: "Studio Mac")
        s.core.endWindow(s.window, with: .paired(peer: paired))
        await s.settle()
        #expect(s.model.pairingFlow.phase == .finished(.paired(paired)))
    }

    /// Only the pairing screen opens a window: its appearing, Start again,
    /// a request of this device that ended before a code, and the app back
    /// in front on it. Nothing else does.
    @Test func aWindowOpensOnlyFromThePairScreenOrStartAgain() async throws {
        let (model, core, timer) = try makeModel()
        await model.start()
        await model.finishOnboarding()
        model.enterBackground()
        await model.becomeActive()
        await model.setVisibility(.everyone)
        await model.setVisibility(.pairedOnly)
        model.startWaiting()
        core.emit(.sessionClosed(peer: nil, code: "not_authorized"))
        await model.pair(with: maya, name: mayaName)
        let request = try #require(model.pairing?.operation)
        core.emit(.pairingFinished(operation: request, result: .failed(error: .notAuthorized)))
        await model.settle { core.emittedBatches }
        timer.fireAll()
        await model.idle()
        #expect(windowOpens(core) == 0)
        // The screen appears, and says what happened to the request.
        model.enterPairingScreen()
        await model.settle { core.emittedBatches }
        #expect(windowOpens(core) == 1)
        #expect(model.pairingFlow.note == .notWaiting(name: mayaName))
        // A stray uses the window up: only Start again opens the next.
        let window = try #require(core.windowOperation)
        core.endWindow(window, with: .failed(error: .notAuthorized))
        await model.settle { core.emittedBatches }
        await model.becomeActive()
        timer.fireAll()
        await model.idle()
        #expect(windowOpens(core) == 1)
        model.startWaiting()
        await model.settle { core.emittedBatches }
        #expect(windowOpens(core) == 2)
    }
}
