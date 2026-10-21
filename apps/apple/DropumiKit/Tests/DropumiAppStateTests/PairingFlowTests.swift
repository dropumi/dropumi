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

@MainActor
private func makeModel() throws -> (AppModel, FakeCoreClient, ManualTimer) {
    let core = FakeCoreClient()
    core.scriptPairingLikeTheCore()
    let paths = try AppPaths.temporary()
    try paths.prepare()
    let defaults = UserDefaults(suiteName: "dropumi-tests-\(UUID().uuidString)")!
    let timer = ManualTimer()
    let model = AppModel(core: core, paths: paths, settings: SettingsStore(defaults: defaults), timer: timer)
    return (model, core, timer)
}

private func starts(_ core: FakeCoreClient) -> Int {
    core.calls.filter { if case .startReceiving = $0 { return true } else { return false } }.count
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
    /// accepting at once.
    @Test func receiverResumedInTheBackgroundStopsAccepting() async throws {
        let s = try await receivingWithAPairingWindow()
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
}
