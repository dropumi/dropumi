// SPDX-License-Identifier: MPL-2.0
//
// This device's IPv4 addresses on local networks: for the advertised host's
// A records, and for "Connection details" (manual addressing).

import Foundation

public enum LocalAddresses {
    public struct Address: Equatable, Sendable {
        /// Dotted-quad text.
        public let text: String
        /// Network-order bytes.
        public let bytes: [UInt8]
        /// The interface it belongs to.
        public let interfaceIndex: UInt32
        /// Interface name, for example `en0`.
        public let interface: String
    }

    /// IPv4 addresses of interfaces that are up and running, excluding
    /// loopback and link-local (169.254/16) addresses.
    public static func ipv4() -> [Address] {
        var head: UnsafeMutablePointer<ifaddrs>?
        guard getifaddrs(&head) == 0, let first = head else { return [] }
        defer { freeifaddrs(head) }
        var result: [Address] = []
        var cursor: UnsafeMutablePointer<ifaddrs>? = first
        while let entry = cursor {
            defer { cursor = entry.pointee.ifa_next }
            let flags = Int32(entry.pointee.ifa_flags)
            guard flags & IFF_UP != 0, flags & IFF_RUNNING != 0, flags & IFF_LOOPBACK == 0,
                  let sa = entry.pointee.ifa_addr, sa.pointee.sa_family == sa_family_t(AF_INET)
            else { continue }
            let addr = sa.withMemoryRebound(to: sockaddr_in.self, capacity: 1) { $0.pointee.sin_addr }
            let bytes = withUnsafeBytes(of: addr.s_addr) { Array($0) }
            guard bytes.count == 4, !(bytes[0] == 169 && bytes[1] == 254) else { continue }
            let name = String(cString: entry.pointee.ifa_name)
            let text = bytes.map(String.init).joined(separator: ".")
            result.append(Address(text: text, bytes: bytes, interfaceIndex: if_nametoindex(entry.pointee.ifa_name), interface: name))
        }
        return result
    }
}
