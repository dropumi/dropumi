// SPDX-License-Identifier: MPL-2.0
//
// The app's real core (AppModel over LiveCoreClient) against the real CLI
// (D11). Driven by crates/cli/tests/e2e/interop/apple.rs with
// DROPUMI_TEST_APPLE=1; skipped otherwise. The harness passes the scenario
// and the CLI's address and fingerprint (TEST_RUNNER_DROPUMI_* variables)
// and reads `DROPUMI_STATUS {json}` lines from the test output.
//
// This test plays the user. It confirms a code or accepts an offer only if
// the authenticated fingerprint is the CLI's; the harness then checks that
// both sides showed the same code. Loopback only: no Bonjour, no OS
// permission prompts, no Wi-Fi, except the `bonjour-*` scenarios, which use
// the native Bonjour adapter on this machine's network.
//
// Scenario `agent` is the device-to-device harness's agent role
// (crates/cli/tests/e2e/interop/): the test connects to the driver at
// DROPUMI_CONTROL (`host:port`), says hello, then runs one JSON command per
// line (receive, wait_pairing, pair, send, trusted, quit) and reports
// events. The same fingerprint rules apply: it confirms a code or accepts an
// offer only for the fingerprint the driver expects.

import CryptoKit
import DropumiAppState
import DropumiBindings
import DropumiCore
import Foundation
import Network
import Testing
#if os(iOS)
import UIKit
#else
import AppKit
#endif
@testable import Dropumi

nonisolated enum Interop {
    static let environment = ProcessInfo.processInfo.environment
    static var scenario: String? { environment["DROPUMI_INTEROP"] }

    static func value(_ key: String) throws -> String {
        guard let value = environment[key], !value.isEmpty else {
            throw InteropFailure(message: "missing \(key)")
        }
        return value
    }

    /// This device's IPv4 addresses on Wi-Fi or Ethernet (comma-separated),
    /// so the harness can reach a real device over the LAN.
    static func wifiAddresses() -> String {
        LocalAddresses.ipv4().filter { $0.interface.hasPrefix("en") }.map(\.text).joined(separator: ",")
    }

    /// One status line for the harness.
    static func status(_ event: String, _ fields: [String: String] = [:]) {
        var object = fields
        object["event"] = event
        let data = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])) ?? Data("{}".utf8)
        let line = "DROPUMI_STATUS " + String(decoding: data, as: UTF8.self) + "\n"
        FileHandle.standardOutput.write(Data(line.utf8))
    }

    static func sha256(_ url: URL) throws -> String {
        let data = try Data(contentsOf: url)
        return SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}

nonisolated struct InteropFailure: Error, CustomStringConvertible {
    let message: String
    var description: String { message }
}

@MainActor
@Suite(.enabled(if: Interop.scenario != nil, "run by crates/cli/tests/e2e/interop/apple.rs"))
struct AppleInteropTests {
    @Test func scenario() async throws {
        let paths = try AppPaths.temporary()
        try paths.prepare()
        // Bonjour only for the discovery scenarios; the others are loopback.
        let bonjour = Interop.scenario?.hasPrefix("bonjour-") == true
        let core = try LiveCoreClient(
            configDir: paths.configDir, receiveDir: paths.receiveDir, discovery: bonjour ? BonjourDiscovery() : nil)
        let defaults = UserDefaults(suiteName: "io.github.dropumi.interop") ?? .standard
        defaults.removePersistentDomain(forName: "io.github.dropumi.interop")
        let model = AppModel(core: core, paths: paths, settings: SettingsStore(defaults: defaults))
        await model.start()
        let fingerprint = try #require(model.identity?.fingerprint)
        Interop.status("ready", ["fingerprint": fingerprint, "app_state": Self.appState])
        defer { Interop.status("done") }
        switch Interop.scenario {
        case "app-sends": try await appSends(model)
        case "app-receives": try await appReceives(model)
        case "app-pairs": try await appPairs(model)
        case "app-waits": try await appWaits(model)
        case "bonjour-advertise": try await bonjourAdvertise(model)
        case "bonjour-browse-and-send": try await bonjourBrowseAndSend(model)
        case "agent": try await InteropAgent(model: model, control: try Interop.value("DROPUMI_CONTROL")).run()
        default: throw InteropFailure(message: "unknown scenario")
        }
        await model.shutdownAndWait()
    }

    /// The app sends a file to a CLI receiver it has never met: both
    /// compare a code, then the file goes.
    private func appSends(_ model: AppModel) async throws {
        let address = try Interop.value("DROPUMI_CLI_ADDR")
        let cliFingerprint = try Interop.value("DROPUMI_CLI_FP")
        let file = model.paths.outgoingDir.appendingPathComponent("from app.bin")
        var bytes = [UInt8](repeating: 0, count: 3 * 1024 * 1024 + 7)
        for i in bytes.indices { bytes[i] = UInt8(truncatingIfNeeded: i &* 31 &+ 7) }
        try Data(bytes).write(to: file)
        Interop.status("file", ["name": file.lastPathComponent, "sha256": try Interop.sha256(file)])
        model.addToDraft([OutgoingFile(url: file, securityScoped: false, staged: false)])
        await model.sendDraft(
            to: .address(address: address, expectFingerprint: cliFingerprint), targetName: "CLI",
            allowUnverified: true)
        let prompt = try await waitForPrompt(model)
        guard case let .code(_, _, peer, code, purpose, _) = prompt else {
            throw InteropFailure(message: "expected a code prompt")
        }
        let matches = peer.fingerprint == cliFingerprint
        Interop.status("code", ["code": code, "matches": String(matches), "purpose": "\(purpose)"])
        await model.answer(prompt, yes: matches)
        let outcome = try await waitForHistory(model)
        Interop.status("finished", ["outcome": outcome.outcome.rawValue])
        #expect(matches)
        #expect(outcome.outcome == .completed)
    }

    /// A CLI sender the app has never met: the app lets everyone send,
    /// shows the code with the offer, and accepts.
    private func appReceives(_ model: AppModel) async throws {
        let cliFingerprint = try Interop.value("DROPUMI_CLI_FP")
        await model.finishOnboarding(visibility: .everyone)
        let port = try await until("receiving") { model.receiver?.port }
        Interop.status("listening", ["port": String(port), "addresses": Interop.wifiAddresses()])
        let prompt = try await waitForPrompt(model)
        guard case let .offer(_, _, peer, code, files, _, _, _) = prompt else {
            throw InteropFailure(message: "expected an offer")
        }
        let matches = peer.fingerprint == cliFingerprint && !peer.trusted
        Interop.status("offer", [
            "code": code ?? "", "matches": String(matches), "claimed_name": peer.claimedName,
            "file": files.first?.name ?? "",
        ])
        await model.answer(prompt, yes: matches)
        let entry = try await waitForHistory(model)
        var fields = ["outcome": entry.outcome.rawValue]
        if let saved = entry.savedAs.first {
            fields["saved_as"] = saved
            fields["sha256"] = try Interop.sha256(model.paths.receiveDir.appendingPathComponent(saved))
        }
        let leftovers = try FileManager.default.contentsOfDirectory(atPath: model.paths.receiveDir.path)
            .filter { $0.hasSuffix(".part") }
        fields["part_files"] = String(leftovers.count)
        Interop.status("finished", fields)
        #expect(matches)
        #expect(entry.outcome == .completed)
        #expect(model.trusted.isEmpty, "accepting an unverified sender never trusts it")
    }

    /// The app pairs with a CLI pairing window.
    private func appPairs(_ model: AppModel) async throws {
        let address = try Interop.value("DROPUMI_CLI_ADDR")
        let cliFingerprint = try Interop.value("DROPUMI_CLI_FP")
        await model.pair(with: .address(address: address, expectFingerprint: nil), name: "CLI")
        let prompt = try await waitForPrompt(model)
        guard case let .code(_, _, peer, code, purpose, _) = prompt else {
            throw InteropFailure(message: "expected a code prompt")
        }
        let matches = peer.fingerprint == cliFingerprint && purpose == .pairing
        Interop.status("code", ["code": code, "matches": String(matches)])
        await model.answer(prompt, yes: matches)
        let result = try await until("pairing to end") { model.pairing?.result }
        await model.refreshTrusted()
        let paired: Bool
        if case .paired = result { paired = true } else { paired = false }
        Interop.status("paired", [
            "paired": String(paired), "trusted": model.trusted.map(\.fingerprint).joined(separator: ","),
        ])
        #expect(paired)
        #expect(model.trusted.map(\.fingerprint) == [cliFingerprint])
    }

    /// The app waits on the pairing screen and the CLI pairs with it. The
    /// app receives from paired devices, so the screen's window pauses the
    /// receiver, and the receiver is back once the screen is left.
    private func appWaits(_ model: AppModel) async throws {
        let cliFingerprint = try Interop.value("DROPUMI_CLI_FP")
        await model.finishOnboarding(visibility: .pairedOnly)
        _ = try await until("receiving", model) { model.receiver?.port }
        model.enterPairingScreen()
        let port = try await until("ready to pair", model) { () -> UInt16? in
            guard case .waiting = model.pairingFlow.phase, let session = model.pairing,
                  case .window = session.role else { return nil }
            return session.port
        }
        Interop.status("waiting", ["port": String(port), "addresses": Interop.wifiAddresses()])
        let prompt = try await waitForPrompt(model)
        guard case let .code(_, _, peer, code, purpose, _) = prompt else {
            throw InteropFailure(message: "expected a code prompt")
        }
        let matches = peer.fingerprint == cliFingerprint && purpose == .pairing
        Interop.status("code", ["code": code, "matches": String(matches)])
        await model.answer(prompt, yes: matches)
        let result = try await until("pairing to end", model) { model.pairing?.result }
        await model.refreshTrusted()
        let paired: Bool
        if case .paired = result { paired = true } else { paired = false }
        let shown: Bool
        if case .finished(.paired) = model.pairingFlow.phase { shown = true } else { shown = false }
        Interop.status("paired", [
            "paired": String(paired), "shown": String(shown),
            "trusted": model.trusted.map(\.fingerprint).joined(separator: ","),
        ])
        // Done.
        model.dismissPairing()
        model.leavePairingScreen()
        _ = try await until("receiving again", model) { model.receivingPausedForPairing ? nil : model.receiver }
        Interop.status("receiving", ["banner": model.banner?.title ?? ""])
        #expect(paired)
        #expect(shown, "the screen shows the pairing")
        #expect(model.trusted.map(\.fingerprint) == [cliFingerprint])
        #expect(model.banner == nil, "the window's pause is not a receiving problem")
    }

    /// The app advertises (Paired only). The harness finds it with the CLI,
    /// checks the advertised host name, then tries to send without being
    /// paired, which must be refused.
    private func bonjourAdvertise(_ model: AppModel) async throws {
        await model.finishOnboarding(visibility: .pairedOnly)
        let receiver = try await until("advertising", model) { model.receiver?.instance != nil ? model.receiver : nil }
        Interop.status("advertising", [
            "instance": receiver.instance ?? "", "port": String(receiver.port), "addresses": Interop.wifiAddresses(),
        ])
        let banner = try await until("the refused sender") { model.banner }
        Interop.status("refused", ["title": banner.title])
        #expect(model.prompts.isEmpty, "a sender that isn't paired never reaches a prompt")
        #expect(model.history.isEmpty)
    }

    /// The app finds the CLI with Bonjour and sends to it through the
    /// resolved address (D10): unverified, so both compare a code.
    private func bonjourBrowseAndSend(_ model: AppModel) async throws {
        let cliFingerprint = try Interop.value("DROPUMI_CLI_FP")
        let hint = String(cliFingerprint.prefix(8))
        model.beginBrowsing()
        let peer = try await until("the CLI nearby", model) { model.nearby.first { $0.fingerprintHint == hint } }
        Interop.status("found", [
            "name": peer.name, "instance": peer.instance, "compatible": String(peer.compatible),
            "looks_like": peer.hintAlias ?? "",
        ])
        let file = model.paths.outgoingDir.appendingPathComponent("via bonjour.bin")
        try Data((0..<(512 * 1024)).map { UInt8(truncatingIfNeeded: $0 &* 13) }).write(to: file)
        Interop.status("file", ["name": file.lastPathComponent, "sha256": try Interop.sha256(file)])
        model.addToDraft([OutgoingFile(url: file, securityScoped: false, staged: false)])
        await model.sendDraft(to: .nearby(instance: peer.instance), targetName: peer.name, allowUnverified: true)
        let prompt = try await waitForPrompt(model)
        guard case let .code(_, _, codePeer, code, _, _) = prompt else {
            throw InteropFailure(message: "expected a code prompt")
        }
        let matches = codePeer.fingerprint == cliFingerprint
        Interop.status("code", ["code": code, "matches": String(matches)])
        await model.answer(prompt, yes: matches)
        let entry = try await waitForHistory(model)
        Interop.status("finished", ["outcome": entry.outcome.rawValue])
        model.endBrowsing()
        #expect(matches)
        #expect(entry.outcome == .completed)
    }

    /// Whether the host app is in front (iOS only).
    static var appState: String {
        #if os(iOS)
        switch UIApplication.shared.applicationState {
        case .active: return "active"
        case .inactive: return "inactive"
        case .background: return "background"
        @unknown default: return "unknown"
        }
        #else
        return NSApplication.shared.isActive ? "active" : "not active"
        #endif
    }

    /// Waits for a prompt. A transfer that ends first (a send that failed
    /// before any code) fails at once with its outcome and summary, rather
    /// than as a timeout that hides why.
    private func waitForPrompt(_ model: AppModel) async throws -> Prompt {
        let ended = model.history.first?.id
        let found = try await until("a prompt", model) { () -> Result<Prompt, InteropFailure>? in
            if let prompt = model.currentPrompt { return .success(prompt) }
            if let entry = model.history.first, entry.id != ended {
                Interop.status("ended_before_prompt", ["outcome": entry.outcome.rawValue, "summary": entry.summary])
                return .failure(InteropFailure(
                    message: "the transfer ended before a prompt: \(entry.outcome.rawValue): \(entry.summary)"))
            }
            return nil
        }
        return try found.get()
    }

    private func waitForHistory(_ model: AppModel) async throws -> HistoryEntry {
        try await until("the transfer to end", model) { model.history.first }
    }

    /// Waits, bounded, until `value` returns something. On timeout it
    /// reports what the model knows about discovery, receiving and pairing.
    private func until<T>(_ what: String, _ model: AppModel? = nil, _ value: () -> T?) async throws -> T {
        let deadline = ContinuousClock.now + .seconds(25)
        while ContinuousClock.now < deadline {
            if let result = value() { return result }
            try await Task.sleep(for: .milliseconds(20))
        }
        Interop.status("timeout", [
            "waiting_for": what,
            "discovery_problem": model?.discoveryProblem ?? "",
            "receiving": String(model?.receiver != nil),
            "nearby": String(model?.nearby.count ?? 0),
            "banner": model?.banner?.message ?? "",
            "pairing": model.map { String(describing: $0.pairingFlow.phase) } ?? "",
        ])
        throw InteropFailure(message: "timed out waiting for \(what)")
    }
}

// MARK: - Device-to-device agent

/// The agent role of the device-to-device harness: it runs the driver's
/// commands on the real app model and reports what happens. It answers
/// prompts only by the driver's fingerprint rules.
@MainActor
final class InteropAgent {
    private let model: AppModel
    private let channel: ControlChannel
    /// The current `receive` rule: offers are accepted only from this
    /// fingerprint (and only from a paired device when `paired`).
    private var receiveRule: (paired: Bool, fingerprint: String)?
    /// The fingerprint a pairing code is confirmed for.
    private var pairFingerprint: String?
    /// The fingerprint a sender's code (unverified receiver) is confirmed for.
    private var sendFingerprint: String?
    /// The pairing operation whose result is still to be reported.
    private var pairOperation: UInt64?
    /// Whether this agent is on the pairing screen (`wait_pairing`).
    private var onPairingScreen = false
    private var handledPrompts: Set<UInt64> = []
    /// Accepted or declined offers whose end is still to be reported.
    private var incoming: [String: TransferRecord?] = [:]
    /// Sends whose end is still to be reported.
    private var outgoing: [TransferRecord] = []
    private var sendCount = 0

    /// No command for this long fails the test.
    private static let commandTimeout: TimeInterval = 120
    /// The longest a single step may take.
    /// Shorter than the driver's step deadline (60 s), so a timeout reaches
    /// the driver as this command's reply, with the reason.
    private static let stepTimeout: Duration = .seconds(45)

    init(model: AppModel, control: String) throws {
        self.model = model
        channel = try ControlChannel(address: control)
    }

    func run() async throws {
        defer { channel.close() }
        try await channel.open(timeout: 60)
        let identity = try #require(model.identity)
        #if os(iOS)
        let platform = "ios"
        #else
        let platform = "macos"
        #endif
        Interop.status("control_connected")
        emit([
            "event": "hello", "platform": platform, "fingerprint": identity.fingerprint, "name": identity.name,
            "addresses": Self.addresses(),
        ])
        let watcher = Task { [weak self] in
            while !Task.isCancelled {
                await self?.tick()
                try? await Task.sleep(for: .milliseconds(20))
            }
        }
        defer { watcher.cancel() }
        while true {
            guard let line = try await channel.nextLine(timeout: Self.commandTimeout) else {
                Interop.status("control_closed")
                break
            }
            guard !line.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { continue }
            guard let data = line.data(using: .utf8),
                  let command = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] else {
                throw InteropFailure(message: "the driver sent a line that is not a JSON object")
            }
            let id = command["id"] ?? NSNull()
            let name = command["cmd"] as? String ?? ""
            Interop.status("command", ["cmd": name])
            var reply: [String: Any]
            do {
                reply = try await handle(name, command)
                reply["ok"] = true
            } catch {
                reply = ["ok": false, "error": String(describing: error)]
            }
            reply["id"] = id
            if name == "quit" {
                try await channel.sendAndWait(Self.encode(reply))
                break
            }
            emit(reply)
        }
        watcher.cancel()
        // Like the other scenarios: leave the pairing screen; the caller
        // then shuts the model down.
        if onPairingScreen {
            model.dismissPairing()
            model.leavePairingScreen()
            onPairingScreen = false
        }
    }

    // MARK: Commands

    private func handle(_ name: String, _ command: [String: Any]) async throws -> [String: Any] {
        switch name {
        case "receive": return try await receive(command)
        case "wait_pairing": return try await waitPairing(command)
        case "pair": return try await pair(command)
        case "send": return try await send(command)
        case "trusted":
            await model.refreshTrusted()
            return ["fingerprints": model.trusted.map(\.fingerprint)]
        case "quit": return [:]
        default: throw InteropFailure(message: "unknown command \(name)")
        }
    }

    /// Receives with the given policy and accept mode prompt.
    private func receive(_ command: [String: Any]) async throws -> [String: Any] {
        let policy = try Self.string(command, "policy")
        guard policy == "paired" || policy == "everyone" else {
            throw InteropFailure(message: "unknown policy \(policy)")
        }
        let paired = policy == "paired"
        receiveRule = (paired, try Self.string(command, "expect_fp"))
        if model.autoAcceptPaired { await model.setAutoAcceptPaired(false) }
        let visibility: ReceiveVisibility = paired ? .pairedOnly : .everyone
        if model.onboarded {
            await model.setVisibility(visibility)
        } else {
            await model.finishOnboarding(visibility: visibility)
        }
        let model = model
        let info = try await until("receiving (\(policy))") { () -> ReceiverInfo? in
            guard !model.receivingPausedForPairing, let info = model.receiver, info.accept == .prompt else {
                return nil
            }
            switch info.policy {
            case .trustedOnly: return paired ? info : nil
            case .everyone: return paired ? nil : info
            }
        }
        return ["port": Int(info.port), "addresses": Self.addresses()]
    }

    /// Opens this device's pairing window as the Pair a device screen does.
    private func waitPairing(_ command: [String: Any]) async throws -> [String: Any] {
        pairFingerprint = try Self.string(command, "expect_fp")
        let model = model
        if !model.onboarded {
            // The screen opens a window only after onboarding: receive from
            // paired devices, as the app's default.
            await model.finishOnboarding(visibility: .pairedOnly)
            _ = try await until("receiving") { model.receiver }
        }
        if model.pairing?.result != nil { model.dismissPairing() }
        if !onPairingScreen {
            model.enterPairingScreen()
            onPairingScreen = true
        } else {
            model.startWaiting()
        }
        let (operation, port) = try await until("ready to pair") { () -> (UInt64, UInt16)? in
            guard case .waiting = model.pairingFlow.phase, let session = model.pairing,
                  case .window = session.role, let port = session.port else { return nil }
            return (session.operation, port)
        }
        pairOperation = operation
        return ["port": Int(port), "addresses": Self.addresses()]
    }

    /// Asks the address to pair, with the expected fingerprint pinned.
    private func pair(_ command: [String: Any]) async throws -> [String: Any] {
        let address = try Self.string(command, "address")
        let fingerprint = try Self.string(command, "expect_fp")
        pairFingerprint = fingerprint
        if model.pairing?.result != nil { model.dismissPairing() }
        await model.pair(with: .address(address: address, expectFingerprint: fingerprint), name: "Interop peer")
        guard let session = model.pairing, case .initiator = session.role else {
            throw InteropFailure(message: "pairing did not start: \(model.banner?.message ?? "no session")")
        }
        pairOperation = session.operation
        return [:]
    }

    /// Writes the files with the spec's pattern and sends them, pinned.
    private func send(_ command: [String: Any]) async throws -> [String: Any] {
        let address = try Self.string(command, "address")
        let fingerprint = try Self.string(command, "expect_fp")
        guard let specs = command["files"] as? [[String: Any]], !specs.isEmpty else {
            throw InteropFailure(message: "missing files")
        }
        sendCount += 1
        let dir = model.paths.outgoingDir.appendingPathComponent("agent-send-\(sendCount)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        var files: [OutgoingFile] = []
        for spec in specs {
            let name = try Self.string(spec, "name")
            guard !name.contains("/"), name != ".", name != "..",
                  let bytes = (spec["bytes"] as? NSNumber)?.int64Value, bytes >= 0 else {
                throw InteropFailure(message: "bad file entry")
            }
            let url = dir.appendingPathComponent(name)
            let sha256 = try await Task.detached { try Self.writePattern(to: url, count: UInt64(bytes)) }.value
            emit(["event": "sent_file", "name": name, "bytes": bytes, "sha256": sha256])
            files.append(OutgoingFile(url: url, securityScoped: false, staged: false))
        }
        sendFingerprint = fingerprint
        model.clearDraft()
        model.addToDraft(files)
        let before = Set(model.active.map(\.id))
        await model.sendDraft(
            to: .address(address: address, expectFingerprint: fingerprint), targetName: "Interop peer",
            allowUnverified: true)
        guard model.draft.isEmpty,
              let record = model.active.first(where: { $0.direction == .outgoing && !before.contains($0.id) })
        else {
            model.clearDraft()
            throw InteropFailure(message: "the send did not start: \(model.banner?.message ?? "no transfer")")
        }
        outgoing.append(record)
        return [:]
    }

    // MARK: Watching the model

    /// Answers new prompts and reports pairing and transfer ends.
    private func tick() async {
        for prompt in model.prompts where !handledPrompts.contains(prompt.promptId) {
            handledPrompts.insert(prompt.promptId)
            await answer(prompt)
        }
        reportPairing()
        reportIncoming()
        reportOutgoing()
    }

    private func answer(_ prompt: Prompt) async {
        switch prompt {
        case let .offer(_, transferId, peer, code, _, fileCount, _, _):
            var accept = false
            if let rule = receiveRule {
                accept = peer.fingerprint == rule.fingerprint && (!rule.paired || peer.trusted)
            }
            emit([
                "event": "offer", "peer_fp": peer.fingerprint, "trusted": peer.trusted, "code": code ?? "",
                "files": Int(fileCount), "accepted": accept,
            ])
            // Kept even before its card exists (a nil value is "not seen yet").
            incoming.updateValue(
                model.active.first { $0.direction == .incoming && $0.transferId == transferId }, forKey: transferId)
            await model.answer(prompt, yes: accept)
        case let .code(_, _, peer, code, purpose, _):
            let expected: String?
            switch purpose {
            case .pairing: expected = pairFingerprint
            case .unverifiedReceiver: expected = sendFingerprint
            }
            emit(["event": "code", "code": code, "peer_fp": peer.fingerprint])
            await model.answer(prompt, yes: expected != nil && peer.fingerprint == expected)
        }
    }

    private func reportPairing() {
        guard let operation = pairOperation, let session = model.pairing, session.operation == operation,
              let result = session.result else { return }
        pairOperation = nil
        var fields: [String: Any] = ["event": "pair_result", "error": "", "peer_fp": session.peer?.fingerprint ?? ""]
        switch result {
        case let .paired(peer):
            fields["result"] = "paired"
            fields["peer_fp"] = peer.fingerprint
        case let .failed(error):
            fields["result"] = "failed"
            fields["error"] = Self.code(error)
        case .cancelled:
            fields["result"] = "cancelled"
        }
        emit(fields)
        // Done, as the screen's Done button: receiving resumes after a window.
        model.dismissPairing()
        if onPairingScreen {
            model.leavePairingScreen()
            onPairingScreen = false
        }
    }

    private func reportIncoming() {
        for (transferId, known) in incoming {
            let record = known ?? model.active.first { $0.direction == .incoming && $0.transferId == transferId }
            if let record {
                incoming.updateValue(record, forKey: transferId)
                guard let result = record.result else { continue }
                incoming.removeValue(forKey: transferId)
                for file in record.fileResults where file.status == .stored {
                    guard let saved = file.savedAs else { continue }
                    let index = Int(file.index)
                    let name = record.fileNames.indices.contains(index) ? record.fileNames[index] : saved
                    reportReceived(name: name, savedAs: saved)
                }
                emit([
                    "event": "receive_finished", "outcome": Self.outcome(result.status),
                    "error": result.error.map(Self.code) ?? "",
                ])
            } else if let entry = model.history.first(where: { $0.id.hasPrefix("in-\(transferId)-") }) {
                // Ended before its card was seen: the history has the files.
                incoming.removeValue(forKey: transferId)
                for saved in entry.savedAs {
                    reportReceived(name: saved, savedAs: saved)
                }
                let outcome = entry.outcome == .rejected ? "declined" : entry.outcome.rawValue
                emit(["event": "receive_finished", "outcome": outcome, "error": ""])
            }
        }
    }

    private func reportReceived(name: String, savedAs: String) {
        let url = model.paths.receiveDir.appendingPathComponent(savedAs)
        let bytes = (try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? -1
        let sha256 = (try? Interop.sha256(url)) ?? ""
        emit(["event": "received", "name": name, "saved_as": savedAs, "bytes": bytes, "sha256": sha256])
    }

    private func reportOutgoing() {
        for record in outgoing {
            guard let result = record.result else { continue }
            outgoing.removeAll { $0 === record }
            emit([
                "event": "send_result", "outcome": Self.outcome(result.status),
                "error": result.error.map(Self.code) ?? "",
            ])
        }
    }

    // MARK: Helpers

    private func emit(_ object: [String: Any]) {
        channel.send(Self.encode(object))
    }

    /// Waits, bounded by the step timeout, until `value` returns something.
    private func until<T>(_ what: String, _ value: () -> T?) async throws -> T {
        let deadline = ContinuousClock.now + Self.stepTimeout
        while ContinuousClock.now < deadline {
            if let result = value() { return result }
            try await Task.sleep(for: .milliseconds(20))
        }
        Interop.status("timeout", [
            "waiting_for": what, "receiving": String(model.receiver != nil),
            "banner": model.banner?.message ?? "", "pairing": String(describing: model.pairingFlow.phase),
        ])
        let receiver = model.receiver.map { "\($0.policy) \($0.accept)" } ?? "none"
        throw InteropFailure(message: "timed out waiting for \(what); onboarded=\(model.onboarded) "
            + "receiver=\(receiver) stopping=\(model.receiverStopping) "
            + "paused=\(model.receivingPausedForPairing) visibility=\(model.visibility) "
            + "banner=\(model.banner?.title ?? "-"): \(model.banner?.message ?? "-")")
    }

    private static func encode(_ object: [String: Any]) -> Data {
        var data = (try? JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])) ?? Data("{}".utf8)
        data.append(0x0A)
        return data
    }

    private static func string(_ object: [String: Any], _ key: String) throws -> String {
        guard let value = object[key] as? String, !value.isEmpty else {
            throw InteropFailure(message: "missing \(key)")
        }
        return value
    }

    /// This device's Wi-Fi or Ethernet IPv4 addresses (`en*`), never loopback.
    private static func addresses() -> [String] {
        LocalAddresses.ipv4().filter { $0.interface.hasPrefix("en") }.map(\.text)
    }

    private static func outcome(_ status: TransferStatus) -> String {
        switch status {
        case .completed: return "completed"
        case .completedWithFailures: return "partial"
        case .rejected: return "declined"
        case .cancelled: return "cancelled"
        case .failed: return "failed"
        }
    }

    /// The core's error code in snake case (`notAuthorized` → `not_authorized`).
    private static func code(_ error: ErrorCode) -> String {
        var text = ""
        for character in String(describing: error) {
            if character.isUppercase {
                text += "_" + character.lowercased()
            } else {
                text.append(character)
            }
        }
        return text
    }

    /// Writes `count` bytes where byte `i` is `(i * 31 + 7) % 251`, and
    /// returns their SHA-256 (hex).
    nonisolated private static func writePattern(to url: URL, count: UInt64) throws -> String {
        guard FileManager.default.createFile(atPath: url.path, contents: nil) else {
            throw InteropFailure(message: "could not create \(url.lastPathComponent)")
        }
        let handle = try FileHandle(forWritingTo: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        let chunkSize: UInt64 = 1 << 20
        var offset: UInt64 = 0
        while offset < count {
            let length = min(chunkSize, count - offset)
            var chunk = Data(count: Int(length))
            chunk.withUnsafeMutableBytes { buffer in
                for j in 0..<Int(length) {
                    buffer[j] = UInt8(((offset + UInt64(j)) * 31 + 7) % 251)
                }
            }
            hasher.update(data: chunk)
            try handle.write(contentsOf: chunk)
            offset += length
        }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

/// The agent's TCP connection to the driver: one JSON object per line.
/// All state is touched only on `queue`.
nonisolated final class ControlChannel: @unchecked Sendable {
    private let connection: NWConnection
    private let queue = DispatchQueue(label: "io.github.dropumi.interop.control")
    private var buffer = Data()
    private var closed = false
    private var opening: CheckedContinuation<Void, Error>?
    private var waiter: CheckedContinuation<String?, Error>?
    private var generation = 0

    init(address: String) throws {
        guard let colon = address.lastIndex(of: ":"),
              let port = NWEndpoint.Port(String(address[address.index(after: colon)...])),
              colon != address.startIndex else {
            throw InteropFailure(message: "DROPUMI_CONTROL is not host:port")
        }
        connection = NWConnection(host: NWEndpoint.Host(String(address[..<colon])), port: port, using: .tcp)
    }

    /// Connects, failing after `timeout` seconds.
    func open(timeout: TimeInterval) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            queue.async {
                self.opening = continuation
                // The handler goes away with the connection (`close()`).
                self.connection.stateUpdateHandler = { state in
                    switch state {
                    case .ready:
                        self.finishOpening(nil)
                        self.receiveMore()
                    case let .failed(error):
                        self.finishOpening(error)
                        self.markClosed()
                    case .cancelled:
                        self.finishOpening(InteropFailure(message: "control connection cancelled"))
                        self.markClosed()
                    default:
                        break
                    }
                }
                self.connection.start(queue: self.queue)
                self.queue.asyncAfter(deadline: .now() + timeout) {
                    self.finishOpening(InteropFailure(message: "could not connect to the driver"))
                }
            }
        }
    }

    /// The next line, or nil once the driver closed the connection. Throws
    /// if none comes within `timeout` seconds.
    func nextLine(timeout: TimeInterval) async throws -> String? {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<String?, Error>) in
            queue.async {
                self.generation += 1
                let generation = self.generation
                self.waiter = continuation
                self.deliver()
                self.queue.asyncAfter(deadline: .now() + timeout) {
                    guard self.generation == generation, let waiter = self.waiter else { return }
                    self.waiter = nil
                    waiter.resume(throwing: InteropFailure(message: "no command from the driver for \(Int(timeout)) s"))
                }
            }
        }
    }

    /// Sends a line (in order with the others).
    func send(_ data: Data) {
        connection.send(content: data, completion: .contentProcessed { _ in })
    }

    /// Sends a line and waits until it was handed to the network.
    func sendAndWait(_ data: Data) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            connection.send(content: data, completion: .contentProcessed { error in
                if let error {
                    continuation.resume(throwing: error)
                } else {
                    continuation.resume()
                }
            })
        }
    }

    func close() {
        connection.cancel()
    }

    private func finishOpening(_ error: Error?) {
        guard let opening else { return }
        self.opening = nil
        if let error {
            opening.resume(throwing: error)
        } else {
            opening.resume()
        }
    }

    private func receiveMore() {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 65_536) { [weak self] data, _, isComplete, error in
            guard let self else { return }
            if let data, !data.isEmpty { self.buffer.append(data) }
            if isComplete || error != nil {
                self.closed = true
            } else {
                self.receiveMore()
            }
            self.deliver()
        }
    }

    private func markClosed() {
        closed = true
        deliver()
    }

    private func deliver() {
        guard let waiter else { return }
        if let newline = buffer.firstIndex(of: 0x0A) {
            let line = String(decoding: buffer[buffer.startIndex..<newline], as: UTF8.self)
            buffer.removeSubrange(buffer.startIndex...newline)
            self.waiter = nil
            waiter.resume(returning: line)
        } else if closed {
            self.waiter = nil
            waiter.resume(returning: nil)
        }
    }
}
