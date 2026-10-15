// SPDX-License-Identifier: MPL-2.0

import DropumiBindings
import DropumiCore
import DropumiShare
import DropumiTestSupport
import Foundation
import Testing
@testable import DropumiAppState

@MainActor
private func makeModel(inbox: ShareInbox? = nil) throws -> (AppModel, FakeCoreClient) {
    let core = FakeCoreClient()
    let paths = try AppPaths.temporary()
    try paths.prepare()
    let defaults = UserDefaults(suiteName: "dropumi-tests-\(UUID().uuidString)")!
    let model = AppModel(core: core, paths: paths, settings: SettingsStore(defaults: defaults), inbox: inbox)
    return (model, core)
}

@MainActor
@Suite struct AppModelTests {
    @Test func unverifiedOfferNeverAnsweredWithoutUserAction() async throws {
        let (model, core) = try makeModel()
        model.apply([
            .offerReceived(transferId: "aa", peer: Fixtures.peer(), files: 1, totalBytes: 10),
            .promptOpened(prompt: Fixtures.offer()),
        ])
        #expect(model.currentPrompt?.promptId == 1)
        #expect(!core.calls.contains { if case .answerOffer = $0 { return true } else { return false } })
    }

    @Test func acceptingAnOfferNeverTrustsTheSender() async throws {
        let (model, core) = try makeModel()
        model.apply([.promptOpened(prompt: Fixtures.offer())])
        await model.answer(Fixtures.offer(), yes: true)
        #expect(core.calls == [.answerOffer(1, true)])
        #expect(!core.calls.contains { if case .trust = $0 { return true } else { return false } })
    }

    @Test func promptGoesAwayOnlyWhenTheCoreClosesIt() async throws {
        let (model, _) = try makeModel()
        model.apply([.promptOpened(prompt: Fixtures.offer())])
        await model.answer(Fixtures.offer(), yes: false)
        #expect(model.prompts.count == 1, "still shown until PromptClosed")
        model.apply([.promptClosed(id: 1, reason: .answered)])
        #expect(model.prompts.isEmpty)
    }

    @Test func answerToAClosedPromptRemovesIt() async throws {
        let (model, core) = try makeModel()
        model.apply([.promptOpened(prompt: Fixtures.offer())])
        core.failures["answerOffer"] = .PromptNotFound
        await model.answer(Fixtures.offer(), yes: true)
        #expect(model.prompts.isEmpty)
        #expect(model.banner == nil)
    }

    @Test func successOnlyAfterFinishedCompleted() async throws {
        let (model, _) = try makeModel()
        model.apply([
            .offerReceived(transferId: "aa", peer: Fixtures.peer(trusted: true, alias: "Studio Mac"), files: 1, totalBytes: 10),
            .transferAccepted(operation: nil, transferId: "aa"),
            .fileSaved(transferId: "aa", index: 0, savedAs: "a.txt", size: 10, blake3: "00"),
        ])
        #expect(model.active.count == 1, "a saved file alone is not a finished transfer")
        #expect(model.history.isEmpty)
        model.apply([.transferFinished(operation: nil, direction: .incoming, result: Fixtures.result(.completed))])
        #expect(model.active.isEmpty)
        #expect(model.history.first?.outcome == .completed)
        #expect(model.history.first?.peerName == "Studio Mac")
        #expect(model.history.first?.savedAs == ["a.txt"])
    }

    @Test func partialTransferIsNotASuccess() async throws {
        let (model, _) = try makeModel()
        model.apply([
            .offerReceived(transferId: "aa", peer: Fixtures.peer(), files: 2, totalBytes: 10),
            .transferFinished(operation: nil, direction: .incoming, result: Fixtures.result(.completedWithFailures)),
        ])
        #expect(model.history.first?.outcome == .partial)
    }

    @Test func everyErrorCodeHasCatalogText() {
        let codes: [ErrorCode] = [
            .incompatibleVersion, .handshakeFailed, .peerIdentityMismatch, .unverifiedPeer, .notAuthorized,
            .pairingRejected, .sasRejected, .protocolViolation, .limitExceeded, .integrity, .insufficientSpace,
            .storage, .sourceChanged, .connectionLost, .timeout, .busy, .remoteError, .internal,
        ]
        for code in codes {
            let text = ErrorCatalog.text(for: code)
            #expect(!text.title.isEmpty && !text.message.isEmpty)
        }
        #expect(ErrorCatalog.text(for: .peerIdentityMismatch).isSecurity)
    }

    @Test func unverifiedPeerIsNeverLabelledWithAPairedName() {
        // A peer that claims the name of a paired device stays unverified.
        let label = PeerLabel(peer: Fixtures.peer(name: "Studio Mac", trusted: false))
        #expect(label.title == "Unverified device")
        #expect(label.claim == "Studio Mac")
        #expect(!label.verified)
        let paired = PeerLabel(peer: Fixtures.peer(name: "MacBook", trusted: true, alias: "Studio Mac"))
        #expect(paired.title == "Studio Mac")
        #expect(paired.claim == "MacBook")
        #expect(paired.verified)
    }

    @Test func peerStringsAreKeptVerbatim() {
        let label = PeerLabel(peer: Fixtures.peer(name: "**bold** [link](https://example.com)"))
        #expect(label.claim == "**bold** [link](https://example.com)")
    }

    @Test func fileCountsArePlural() {
        #expect(Formatting.fileCount(1) == "1 file")
        #expect(Formatting.fileCount(3) == "3 files")
    }

    @Test func codesAreGroupedAndSpokenDigitByDigit() {
        #expect(Formatting.code("123456") == "123 456")
        #expect(Formatting.spokenCode("123456") == "1 2 3, 4 5 6")
    }

    /// "This device" shows the whole 52-character fingerprint that "Add by
    /// fingerprint" needs, grouped in fours; the grouping is only for reading.
    @Test func fullFingerprintIsGroupedInFoursAndRoundTrips() {
        let full = FakeCoreClient.fingerprint
        #expect(full.count == 52)
        let grouped = Formatting.fingerprint(full)
        let groups = grouped.split(separator: "-", omittingEmptySubsequences: false)
        #expect(groups.count == 13)
        #expect(groups.allSatisfy { $0.count == 4 })
        // The add sheet and the core drop spaces and dashes.
        #expect(grouped.filter { !$0.isWhitespace && $0 != "-" } == full)
        // It starts with the short form shown elsewhere.
        #expect(grouped.hasPrefix("ABCD-EFGH-IJKL-MNOP-QRST-UVWX"))
    }

    @Test func backgroundStopsAcceptingButDoesNotStopReceiving() async throws {
        let (model, core) = try makeModel()
        await model.finishOnboarding()
        #expect(model.receiver != nil)
        model.enterBackground()
        // The stop is queued behind receiver changes; this waits for it.
        await model.applyVisibility()
        #expect(core.calls.contains(.stopAccepting))
        #expect(!core.calls.contains(.stopReceiving))
    }

    /// ADR-0012 D5: the app went to the background while the core was still
    /// opening (the loader hands the state over before `start()`). It does
    /// not receive until it is in front again.
    @Test func backgroundBeforeStartNeverStartsReceiving() async throws {
        let (model, core) = try makeModel()
        model.onboarded = true
        model.enterBackground()
        await model.start()
        #expect(!core.calls.contains { if case .startReceiving = $0 { return true } else { return false } })
        #expect(model.receiver == nil)
        await model.becomeActive()
        #expect(core.calls.contains { if case .startReceiving = $0 { return true } else { return false } })
        #expect(model.receiver != nil)
    }

    /// The app goes to the background while the receiver is still starting:
    /// once it is up, it stops accepting.
    @Test func backgroundWhileTheReceiverStartsStopsAcceptingOnceItIsUp() async throws {
        let (model, core) = try makeModel()
        let gate = core.holdNextStartReceiving()
        let onboarding = Task { await model.finishOnboarding() }
        await gate.reached()
        model.enterBackground()
        gate.open()
        await onboarding.value
        #expect(model.receiver != nil)
        // Receiver changes run one after another; this one runs last.
        await model.applyVisibility()
        #expect(core.calls.contains(.stopAccepting))
        #expect(!core.calls.contains(.stopReceiving))
    }

    @Test func pairedOnlyIsTheDefaultAndAutoAcceptIsOff() async throws {
        let (model, core) = try makeModel()
        await model.finishOnboarding()
        #expect(core.calls.first == .startReceiving(
            ReceiveOptions(policy: .trustedOnly, accept: .prompt, port: nil, advertise: true)))
        #expect(model.visibility == .pairedOnly)
        #expect(!model.autoAcceptPaired)
    }

    @Test func everyoneRevertsWhenTheCoreEndsTheWindow() async throws {
        let (model, _) = try makeModel()
        await model.finishOnboarding()
        await model.setVisibility(.everyone)
        model.apply([.policyChanged(policy: .trustedOnly, accept: .prompt, everyoneUntil: nil)])
        #expect(model.visibility == .pairedOnly)
    }

    @Test func overlappingRequestsStartTheReceiverOnce() async throws {
        let (model, core) = try makeModel()
        await model.finishOnboarding()
        async let launch: Void = model.becomeActive()
        async let active: Void = model.becomeActive()
        _ = await (launch, active)
        let starts = core.calls.filter { if case .startReceiving = $0 { return true } else { return false } }
        #expect(starts.count == 1, "one receiver; later requests only update its policy")
    }

    @Test func receivingResumesWhenAStopFinishesLate() async throws {
        let (model, core) = try makeModel()
        await model.finishOnboarding()
        await model.setVisibility(.off)
        await model.setVisibility(.pairedOnly)
        #expect(!core.calls.contains { if case .setReceivePolicy = $0 { return true } else { return false } },
                "a stopping receiver is never reconfigured")
        model.apply([.receiverStopped(reason: .stopped)])
        for _ in 0..<100 where core.calls.filter({ if case .startReceiving = $0 { return true } else { return false } }).count < 2 {
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(core.calls.filter { if case .startReceiving = $0 { return true } else { return false } }.count == 2)
        #expect(model.receiver != nil)
    }

    @Test func offStopsTheReceiver() async throws {
        let (model, core) = try makeModel()
        await model.finishOnboarding()
        await model.setVisibility(.off)
        #expect(core.calls.contains(.stopReceiving))
    }

    @Test func sendingKeepsStagedCopiesUntilTheTransferEnds() async throws {
        let (model, core) = try makeModel()
        let copy = model.paths.outgoingDir.appendingPathComponent("photo.heic")
        try Data([1, 2, 3]).write(to: copy)
        model.addToDraft([OutgoingFile(url: copy, securityScoped: false, staged: true)])
        await model.sendDraft(to: .nearby(instance: "dropumi-00000001"), targetName: "Pixel 8", allowUnverified: false)
        #expect(model.draft.isEmpty)
        #expect(model.active.count == 1)
        #expect(FileManager.default.fileExists(atPath: copy.path), "the core still reads it")
        guard case let .send(_, paths, allowUnverified) = core.calls.last else {
            Issue.record("no send")
            return
        }
        #expect(paths == [copy.path])
        #expect(!allowUnverified)
        model.apply([.transferFinished(operation: 1, direction: .outgoing, result: Fixtures.result(.completed))])
        #expect(!FileManager.default.fileExists(atPath: copy.path))
    }

    @Test func cancellingAnOutgoingTransferCancelsItsOperation() async throws {
        let (model, core) = try makeModel()
        let file = model.paths.outgoingDir.appendingPathComponent("x.bin")
        try Data([1]).write(to: file)
        model.addToDraft([OutgoingFile(url: file, securityScoped: false, staged: true)])
        await model.sendDraft(to: .nearby(instance: "i"), targetName: "X", allowUnverified: false)
        let record = try #require(model.active.first)
        model.cancel(record)
        for _ in 0..<100 where core.operation(1)?.cancelled != true {
            try await Task.sleep(for: .milliseconds(10))
        }
        #expect(core.operation(1)?.cancelled == true)
    }

    @Test func historyPersistsAndClears() async throws {
        let (model, _) = try makeModel()
        model.apply([
            .offerReceived(transferId: "aa", peer: Fixtures.peer(), files: 1, totalBytes: 10),
            .transferFinished(operation: nil, direction: .incoming, result: Fixtures.result(.completed)),
        ])
        #expect(HistoryStore(file: model.paths.historyFile).load().count == 1)
        model.clearHistory()
        #expect(HistoryStore(file: model.paths.historyFile).load().isEmpty)
    }

    @Test func historyIsCapped() throws {
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("history-\(UUID().uuidString).json")
        let store = HistoryStore(file: file, limit: 3)
        let entries = (0..<5).map {
            HistoryEntry(id: "\($0)", outgoing: true, peerName: "p", peerVerified: true, fileNames: [],
                         totalBytes: 0, outcome: .completed, summary: "", finishedAt: Date(), savedAs: [])
        }
        store.save(entries)
        #expect(store.load().map(\.id) == ["0", "1", "2"])
    }

    @Test func sharedItemsArePickedUpAndOfferedForSending() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("inbox-\(UUID().uuidString)")
        let inbox = ShareInbox(root: root)
        let source = FileManager.default.temporaryDirectory.appendingPathComponent("src-\(UUID().uuidString).txt")
        try Data(repeating: 1, count: 4).write(to: source)
        let batch = try inbox.beginBatch()
        try batch.add(copyOf: source, suggestedName: "note.txt")
        _ = try batch.commit()
        let (model, _) = try makeModel(inbox: inbox)
        model.takeSharedItems()
        #expect(model.draft.map(\.name) == ["note.txt"])
        #expect(model.route == .send)
        #expect(inbox.pendingBatches().isEmpty)
    }

    @Test func unpairedSenderNoticeIsRateLimited() async throws {
        let (model, _) = try makeModel()
        model.apply([.sessionClosed(peer: Fixtures.peer(), code: "not_authorized")])
        let first = try #require(model.banner)
        model.apply([.sessionClosed(peer: Fixtures.peer(), code: "not_authorized")])
        #expect(model.banner == first)
    }

    @Test func nearbyDevicesThatLookLikePairedOnesComeFirst() async throws {
        let (model, _) = try makeModel()
        model.apply([
            .peerFound(peer: Fixtures.nearby("Zed", instance: "a")),
            .peerFound(peer: Fixtures.nearby("Amy", instance: "b")),
            .peerFound(peer: Fixtures.nearby("Mac", instance: "c", hintAlias: "Studio Mac")),
        ])
        #expect(model.nearby.map(\.instance) == ["c", "b", "a"])
        model.apply([.peerLost(instance: "c")])
        #expect(model.nearby.map(\.instance) == ["b", "a"])
    }
}
