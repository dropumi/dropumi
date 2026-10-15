// SPDX-License-Identifier: MPL-2.0
//
// `PlatformDiscovery` on Apple platforms, with the system's DNS-SD service
// (`dns_sd`). Rust owns every rule (instance names, TXT building and
// parsing, address filtering); this adapter only moves bytes.
//
// Advertising uses a *proxy* registration: an A record for a random host
// name (`dropumi-<8 hex>.local`, the same random label as the instance)
// plus the service pointing at that host. A plain registration would make
// the system's own host name the SRV target, and that name can contain the
// owner's name, which discovery.md section 3 forbids. If the A records
// cannot be registered, nothing is advertised.
//
// `advertise` does not wait for the registration to complete. Until the
// user answers the system's Local Network question, DNS-SD holds requests
// without answering; waiting would block the receiver and give up on the
// advertisement before the user tapped Allow. The instance name is random,
// so it is registered without automatic renaming: the name returned is the
// name used. If the registration fails later, the advertisement is removed.
//
// Every `dns_sd` call and callback runs on one serial queue.

import DropumiBindings
import Foundation
import Network
import dnssd

public final class BonjourDiscovery: PlatformDiscovery, @unchecked Sendable {
    private let queue = DispatchQueue(label: "io.github.dropumi.bonjour")
    private var adverts: [String: Advert] = [:]
    private var browse: Browse?
    private let pathMonitor = NWPathMonitor()

    public init() {
        // The monitor calls back on `queue`, like every dns_sd callback.
        pathMonitor.pathUpdateHandler = { [weak self] _ in
            self?.refreshAddresses()
        }
        pathMonitor.start(queue: queue)
    }

    deinit {
        pathMonitor.cancel()
    }

    // MARK: Advertising

    public func advertise(ad: ServiceAd) throws -> String {
        let host = "\(ad.instance).local."
        let addresses = LocalAddresses.ipv4()
        guard !addresses.isEmpty else {
            throw CallbackError.Failed(detail: "no IPv4 address on a local network")
        }
        let txt = Self.txtBytes(ad.txt)
        let failure: String? = queue.sync {
            var connection: DNSServiceRef?
            guard DNSServiceCreateConnection(&connection) == kDNSServiceErr_NoError, let connection else {
                return "cannot reach the DNS-SD service"
            }
            DNSServiceSetDispatchQueue(connection, queue)
            let advert = Advert(connection: connection, host: host, instance: ad.instance)
            advert.onFailure = { [weak self] in
                // Called on `queue`.
                self?.adverts.removeValue(forKey: ad.instance)?.cancel()
            }
            guard advert.registerAddresses(addresses) else {
                advert.cancel()
                return "cannot register the host address"
            }
            var service: DNSServiceRef?
            let context = Unmanaged.passUnretained(advert).toOpaque()
            let err = txt.withUnsafeBytes { raw in
                DNSServiceRegister(
                    &service, DNSServiceFlags(kDNSServiceFlagsNoAutoRename), 0, ad.instance, ad.serviceType, nil,
                    host, ad.port.bigEndian, UInt16(raw.count), raw.baseAddress, registerReply, context)
            }
            guard err == kDNSServiceErr_NoError, let service else {
                advert.cancel()
                return "cannot register the service (\(err))"
            }
            DNSServiceSetDispatchQueue(service, queue)
            advert.service = service
            adverts[ad.instance] = advert
            return nil
        }
        if let failure {
            throw CallbackError.Failed(detail: failure)
        }
        return ad.instance
    }

    public func withdraw(instance: String) throws {
        withdrawSync(instance)
    }

    private func withdrawSync(_ instance: String) {
        queue.sync {
            adverts.removeValue(forKey: instance)?.cancel()
        }
    }

    /// Re-registers the host's A records after a network change.
    private func refreshAddresses() {
        let addresses = LocalAddresses.ipv4()
        for advert in adverts.values {
            advert.replaceAddresses(addresses)
        }
    }

    static func txtBytes(_ entries: [TxtEntry]) -> Data {
        var data = Data()
        for entry in entries {
            var item = Data(entry.key.utf8)
            item.append(UInt8(ascii: "="))
            item.append(entry.value)
            guard item.count <= 255 else { continue }
            data.append(UInt8(item.count))
            data.append(item)
        }
        return data
    }

    // MARK: Browsing

    public func startBrowse(sink: BrowseSink) throws {
        let failure: String? = queue.sync {
            browse?.cancel()
            let session = Browse(sink: sink, queue: queue)
            guard session.start() else {
                return "cannot browse for nearby devices"
            }
            browse = session
            return nil
        }
        if let failure {
            throw CallbackError.Failed(detail: failure)
        }
    }

    public func stopBrowse() throws {
        queue.sync {
            browse?.cancel()
            browse = nil
        }
    }
}

// MARK: - Advertisement

private final class Advert {
    let connection: DNSServiceRef
    let host: String
    let instance: String
    var service: DNSServiceRef?
    var records: [DNSRecordRef] = []
    /// Removes this advertisement after a failed registration.
    var onFailure: (() -> Void)?

    init(connection: DNSServiceRef, host: String, instance: String) {
        self.connection = connection
        self.host = host
        self.instance = instance
    }

    /// One unique A record per IPv4 address, on its interface.
    func registerAddresses(_ addresses: [LocalAddresses.Address]) -> Bool {
        var ok = false
        for address in addresses {
            var record: DNSRecordRef?
            var bytes = address.bytes
            let err = DNSServiceRegisterRecord(
                connection, &record, DNSServiceFlags(kDNSServiceFlagsUnique), address.interfaceIndex,
                host, UInt16(kDNSServiceType_A), UInt16(kDNSServiceClass_IN), 4, &bytes, 120,
                recordReply, nil)
            if err == kDNSServiceErr_NoError, let record {
                records.append(record)
                ok = true
            }
        }
        return ok
    }

    func replaceAddresses(_ addresses: [LocalAddresses.Address]) {
        for record in records {
            DNSServiceRemoveRecord(connection, record, 0)
        }
        records.removeAll()
        _ = registerAddresses(addresses)
    }

    func cancel() {
        if let service {
            DNSServiceRefDeallocate(service)
        }
        service = nil
        // Deallocating the connection also removes its records.
        DNSServiceRefDeallocate(connection)
        records.removeAll()
    }
}

private let recordReply: DNSServiceRegisterRecordReply = { _, _, _, _, _ in }

/// Runs on the adapter's queue. A failure (a name conflict, or Local
/// Network access turned off) removes the advertisement.
private let registerReply: DNSServiceRegisterReply = { _, _, errorCode, _, _, _, context in
    guard let context, errorCode != kDNSServiceErr_NoError else { return }
    let advert = Unmanaged<Advert>.fromOpaque(context).takeUnretainedValue()
    advert.onFailure?()
}

// MARK: - Browsing

private final class Browse {
    let sink: BrowseSink
    let queue: DispatchQueue
    var ref: DNSServiceRef?
    var resolving: [String: Resolve] = [:]

    init(sink: BrowseSink, queue: DispatchQueue) {
        self.sink = sink
        self.queue = queue
    }

    func start() -> Bool {
        var browse: DNSServiceRef?
        let context = Unmanaged.passUnretained(self).toOpaque()
        let err = DNSServiceBrowse(&browse, 0, 0, "_dropumi._tcp", nil, browseReply, context)
        guard err == kDNSServiceErr_NoError, let browse else { return false }
        DNSServiceSetDispatchQueue(browse, queue)
        ref = browse
        return true
    }

    func found(_ instance: String, type: String, domain: String, interface: UInt32) {
        guard resolving[instance] == nil else { return }
        let resolve = Resolve(browse: self, instance: instance, queue: queue)
        resolving[instance] = resolve
        resolve.start(type: type, domain: domain, interface: interface)
    }

    func lost(_ instance: String) {
        resolving.removeValue(forKey: instance)?.cancel()
        sink.lost(instance: instance)
    }

    func failed(_ code: DNSServiceErrorType) {
        sink.failed(reason: code == kDNSServiceErr_PolicyDenied
            ? "local network access is turned off for Dropumi"
            : "browsing failed (\(code))")
    }

    func cancel() {
        for resolve in resolving.values {
            resolve.cancel()
        }
        resolving.removeAll()
        if let ref {
            DNSServiceRefDeallocate(ref)
        }
        ref = nil
    }
}

private let browseReply: DNSServiceBrowseReply = { _, flags, interface, errorCode, name, type, domain, context in
    guard let context else { return }
    let browse = Unmanaged<Browse>.fromOpaque(context).takeUnretainedValue()
    guard errorCode == kDNSServiceErr_NoError else {
        browse.failed(errorCode)
        return
    }
    guard let name, let type, let domain else { return }
    let instance = String(cString: name)
    if flags & DNSServiceFlags(kDNSServiceFlagsAdd) != 0 {
        browse.found(instance, type: String(cString: type), domain: String(cString: domain), interface: interface)
    } else {
        browse.lost(instance)
    }
}

/// Resolves one instance: SRV and TXT, then the target host's IPv4
/// addresses. Never connects to the device (that would use up one of the
/// receiver's pre-authentication slots).
private final class Resolve {
    weak var browse: Browse?
    let instance: String
    let queue: DispatchQueue
    var resolveRef: DNSServiceRef?
    var addressRef: DNSServiceRef?
    var port: UInt16 = 0
    var txt: [TxtEntry] = []
    var addresses: [String] = []

    init(browse: Browse, instance: String, queue: DispatchQueue) {
        self.browse = browse
        self.instance = instance
        self.queue = queue
    }

    func start(type: String, domain: String, interface: UInt32) {
        var ref: DNSServiceRef?
        let context = Unmanaged.passUnretained(self).toOpaque()
        guard DNSServiceResolve(&ref, 0, interface, instance, type, domain, resolveReply, context)
                == kDNSServiceErr_NoError, let ref
        else { return }
        DNSServiceSetDispatchQueue(ref, queue)
        resolveRef = ref
    }

    func resolved(host: String, port: UInt16, txt: [TxtEntry], interface: UInt32) {
        self.port = port
        self.txt = txt
        if let addressRef {
            DNSServiceRefDeallocate(addressRef)
            self.addressRef = nil
        }
        addresses.removeAll()
        var ref: DNSServiceRef?
        let context = Unmanaged.passUnretained(self).toOpaque()
        guard DNSServiceGetAddrInfo(
            &ref, 0, interface, DNSServiceProtocol(kDNSServiceProtocol_IPv4), host, addressReply, context)
                == kDNSServiceErr_NoError, let ref
        else { return }
        DNSServiceSetDispatchQueue(ref, queue)
        addressRef = ref
    }

    func address(_ ip: String, added: Bool) {
        if added {
            if !addresses.contains(ip) { addresses.append(ip) }
        } else {
            addresses.removeAll { $0 == ip }
        }
        guard !addresses.isEmpty, port != 0 else { return }
        browse?.sink.resolved(instance: instance, txt: txt, addresses: addresses, port: port)
    }

    func cancel() {
        if let resolveRef { DNSServiceRefDeallocate(resolveRef) }
        if let addressRef { DNSServiceRefDeallocate(addressRef) }
        resolveRef = nil
        addressRef = nil
    }

    static func parseTxt(_ bytes: UnsafeRawBufferPointer) -> [TxtEntry] {
        var entries: [TxtEntry] = []
        var offset = 0
        while offset < bytes.count {
            let length = Int(bytes[offset])
            offset += 1
            guard offset + length <= bytes.count else { break }
            let item = Data(bytes[offset..<(offset + length)])
            offset += length
            guard !item.isEmpty else { continue }
            if let eq = item.firstIndex(of: UInt8(ascii: "=")) {
                let key = String(decoding: item[item.startIndex..<eq], as: UTF8.self)
                entries.append(TxtEntry(key: key, value: Data(item[item.index(after: eq)...])))
            } else {
                entries.append(TxtEntry(key: String(decoding: item, as: UTF8.self), value: Data()))
            }
        }
        return entries
    }
}

private let resolveReply: DNSServiceResolveReply = { _, _, interface, errorCode, _, host, port, txtLength, txtRecord, context in
    guard let context, errorCode == kDNSServiceErr_NoError, let host else { return }
    let resolve = Unmanaged<Resolve>.fromOpaque(context).takeUnretainedValue()
    let txt = txtRecord.map { Resolve.parseTxt(UnsafeRawBufferPointer(start: $0, count: Int(txtLength))) } ?? []
    resolve.resolved(host: String(cString: host), port: UInt16(bigEndian: port), txt: txt, interface: interface)
}

private let addressReply: DNSServiceGetAddrInfoReply = { _, flags, _, errorCode, _, address, _, context in
    guard let context, errorCode == kDNSServiceErr_NoError, let address,
          address.pointee.sa_family == sa_family_t(AF_INET)
    else { return }
    let resolve = Unmanaged<Resolve>.fromOpaque(context).takeUnretainedValue()
    var ip = address.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { $0.pointee.sin_addr }
    var text = [CChar](repeating: 0, count: Int(INET_ADDRSTRLEN))
    guard inet_ntop(AF_INET, &ip, &text, socklen_t(INET_ADDRSTRLEN)) != nil else { return }
    let added = flags & DNSServiceFlags(kDNSServiceFlagsAdd) != 0
    let ascii = text.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
    resolve.address(String(decoding: ascii, as: UTF8.self), added: added)
}
