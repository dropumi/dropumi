// SPDX-License-Identifier: MPL-2.0
//
// The generated Swift bindings against the real Rust core: two nodes in
// this process on loopback, with Swift implementations of the callbacks.
// Every node uses fresh temporary directories.

import DropumiBindings
import Foundation
import Testing

/// Records events; tests wait on them with a deadline.
final class Recorder: EventListener, @unchecked Sendable {
    private let condition = NSCondition()
    private var events: [NodeEvent] = []
    private var cursor = 0
    private var failures: Int

    init(failures: Int = 0) {
        self.failures = failures
    }

    func onEvents(events: [NodeEvent]) throws {
        condition.lock()
        defer { condition.unlock() }
        if failures > 0 {
            failures -= 1
            throw CallbackError.Failed(detail: "listener failure")
        }
        self.events.append(contentsOf: events)
        condition.broadcast()
    }

    /// The next event matching `match` (earlier events are skipped).
    func wait(_ what: String, timeout: TimeInterval = 30, _ match: (NodeEvent) -> Bool) throws -> NodeEvent {
        let deadline = Date().addingTimeInterval(timeout)
        condition.lock()
        defer { condition.unlock() }
        while true {
            while cursor < events.count {
                let event = events[cursor]
                cursor += 1
                if match(event) {
                    return event
                }
            }
            if !condition.wait(until: deadline) {
                throw TestError.timeout(what)
            }
        }
    }
}

enum TestError: Error {
    case timeout(String)
    case unexpected(String)
}

/// A node in fresh temporary directories.
final class TestNode {
    let node: DropumiNode
    let events: Recorder
    let root: URL
    let inbox: URL

    init(failures: Int = 0) throws {
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("dropumi-bindings-\(UUID().uuidString)")
        inbox = root.appendingPathComponent("inbox")
        try FileManager.default.createDirectory(at: inbox, withIntermediateDirectories: true)
        events = Recorder(failures: failures)
        node = try DropumiNode(
            config: NodeConfig(
                configDir: root.appendingPathComponent("config").path,
                receiveDir: inbox.path
            ),
            listener: events,
            discovery: nil
        )
    }

    deinit {
        node.shutdown()
        try? FileManager.default.removeItem(at: root)
    }

    func receive(_ policy: ReceivePolicy = .trustedOnly, accept: AcceptMode = .autoTrusted) throws -> UInt16 {
        try node.startReceiving(
            options: ReceiveOptions(policy: policy, accept: accept, port: 0, advertise: false)
        ).port
    }

    func finished(_ direction: Direction) throws -> TransferResult {
        let event = try events.wait("a finished transfer") {
            if case let .transferFinished(_, d, _) = $0 { return d == direction }
            return false
        }
        guard case let .transferFinished(_, _, result) = event else {
            throw TestError.unexpected("\(event)")
        }
        return result
    }
}

func pair(_ a: TestNode, _ b: TestNode) throws {
    let (ia, ib) = (try a.node.identity(), try b.node.identity())
    _ = try a.node.trustFingerprint(fingerprint: ib.fingerprint, alias: "B")
    _ = try b.node.trustFingerprint(fingerprint: ia.fingerprint, alias: "A")
}

func address(_ port: UInt16) -> SendTarget {
    .address(address: "127.0.0.1:\(port)", expectFingerprint: nil)
}

/// A document the "app" reads, in memory.
final class MemFile: ForeignFile, @unchecked Sendable {
    let name: String
    let data: Data

    init(name: String, data: Data) {
        self.name = name
        self.data = data
    }

    func displayName() -> String { name }
    func size() -> UInt64? { UInt64(data.count) }
    func modified() -> Int64? { nil }
    func openReader() throws -> ForeignReader { MemReader(data: data) }
}

final class MemReader: ForeignReader, @unchecked Sendable {
    private let data: Data
    private let lock = NSLock()
    private var position = 0

    init(data: Data) {
        self.data = data
    }

    func read(maxLen: UInt32) throws -> Data {
        lock.lock()
        defer { lock.unlock() }
        let end = min(position + Int(maxLen), data.count)
        let chunk = data.subdata(in: position..<end)
        position = end
        return chunk
    }

    func closeDocument() {}
}

@Suite(.serialized)
struct BindingsTests {
    @Test func apiVersionIsOne() {
        #expect(ffiApiVersion() == 1)
    }

    @Test func loopbackTrustedTransfer() throws {
        let (a, b) = (try TestNode(), try TestNode())
        try pair(a, b)
        let port = try b.receive()
        let data = Data((0..<500_000).map { UInt8($0 % 251) })
        let file = a.inbox.appendingPathComponent("photo.jpg")
        try data.write(to: file)
        _ = try a.node.send(
            target: address(port),
            items: [.path(path: file.path)],
            options: SendOptions(allowUnverified: false)
        )
        #expect(try a.finished(.outgoing).status == .completed)
        #expect(try b.finished(.incoming).status == .completed)
        #expect(try Data(contentsOf: b.inbox.appendingPathComponent("photo.jpg")) == data)
    }

    @Test func promptAnsweredFromAnotherThread() throws {
        let (a, b) = (try TestNode(), try TestNode())
        try pair(a, b)
        let port = try b.receive(accept: .prompt)
        let file = a.inbox.appendingPathComponent("note.txt")
        try Data("hello".utf8).write(to: file)
        _ = try a.node.send(
            target: address(port),
            items: [.path(path: file.path)],
            options: SendOptions(allowUnverified: false)
        )
        let event = try b.events.wait("an offer") {
            if case .promptOpened = $0 { return true }
            return false
        }
        guard case let .promptOpened(.offer(id, _, peer, code, files, _, _, _)) = event else {
            throw TestError.unexpected("\(event)")
        }
        #expect(peer.trusted)
        #expect(code == nil)
        #expect(files.first?.name == "note.txt")
        let node = b.node
        let answered = DispatchSemaphore(value: 0)
        DispatchQueue.global().async {
            try? node.answerOffer(promptId: id, accept: true)
            answered.signal()
        }
        answered.wait()
        #expect(try a.finished(.outgoing).status == .completed)
    }

    @Test func swiftDocumentIsSent() throws {
        let (a, b) = (try TestNode(), try TestNode())
        try pair(a, b)
        let port = try b.receive()
        let data = Data((0..<300_000).map { UInt8(($0 * 7) % 256) })
        _ = try a.node.send(
            target: address(port),
            items: [.stream(file: MemFile(name: "shared/from-photos.heic", data: data))],
            options: SendOptions(allowUnverified: false)
        )
        #expect(try a.finished(.outgoing).status == .completed)
        #expect(try Data(contentsOf: b.inbox.appendingPathComponent("from-photos.heic")) == data)
    }

    @Test func listenerErrorsAreContained() throws {
        let (a, b) = (try TestNode(), try TestNode(failures: 1))
        try pair(a, b)
        let port = try b.receive()
        let file = a.inbox.appendingPathComponent("x.txt")
        try Data("x".utf8).write(to: file)
        _ = try a.node.send(
            target: address(port),
            items: [.path(path: file.path)],
            options: SendOptions(allowUnverified: false)
        )
        #expect(try a.finished(.outgoing).status == .completed)
        // One failed delivery is not fatal: later events still arrive.
        #expect(try b.finished(.incoming).status == .completed)
    }

    @Test func errorsAreTyped() throws {
        let b = try TestNode()
        #expect(throws: DropumiError.PromptNotFound) {
            try b.node.answerOffer(promptId: 999, accept: true)
        }
        #expect(throws: DropumiError.self) {
            try b.node.trustFingerprint(fingerprint: "not a fingerprint", alias: nil)
        }
        b.node.shutdown()
        #expect(throws: DropumiError.ShutDown) {
            _ = try b.node.identity()
        }
    }
}
