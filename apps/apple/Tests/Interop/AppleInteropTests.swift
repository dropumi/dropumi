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

import CryptoKit
import DropumiAppState
import DropumiBindings
import DropumiCore
import Foundation
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
