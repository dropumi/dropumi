// SPDX-License-Identifier: MPL-2.0
//
// Which interfaces each browsed instance was seen on, and the IPv4
// addresses each interface resolved. mDNSResponder reports one browse Add
// per interface; a device's host name resolves to a different address on
// each of them (a Mac with virtual-machine bridges answers with its bridge
// addresses too). Resolving only on the first interface could leave a
// device with one address that refuses connections, so every interface is
// resolved and the addresses are merged. A device is lost only when the
// last interface loses it. Pure bookkeeping: BonjourDiscovery does the I/O.

struct BonjourInstances {
    private var interfaces: [String: [UInt32: [String]]] = [:]
    /// The order interfaces were first seen in, per instance, so the merged
    /// list keeps a stable order.
    private var order: [String: [UInt32]] = [:]

    /// Records an Add. Returns false if this interface was already known
    /// for the instance (nothing new to resolve).
    mutating func seen(_ instance: String, on interface: UInt32) -> Bool {
        if interfaces[instance]?[interface] != nil { return false }
        interfaces[instance, default: [:]][interface] = []
        order[instance, default: []].append(interface)
        return true
    }

    /// Records the addresses one interface resolved, and returns the
    /// merged list for the instance (first-seen interface first, no
    /// duplicates).
    mutating func update(_ instance: String, on interface: UInt32, addresses: [String]) -> [String] {
        guard interfaces[instance]?[interface] != nil else { return merged(instance) }
        interfaces[instance]?[interface] = addresses
        return merged(instance)
    }

    /// Records a Remove on one interface. Returns nil when the instance is
    /// gone from every interface, or else the remaining merged addresses.
    mutating func lost(_ instance: String, on interface: UInt32) -> [String]? {
        guard interfaces[instance] != nil else { return nil }
        interfaces[instance]?.removeValue(forKey: interface)
        order[instance]?.removeAll { $0 == interface }
        if interfaces[instance]?.isEmpty ?? true {
            interfaces.removeValue(forKey: instance)
            order.removeValue(forKey: instance)
            return nil
        }
        return merged(instance)
    }

    /// The interfaces the instance is seen on, first seen first.
    func interfaces(of instance: String) -> [UInt32] {
        order[instance] ?? []
    }

    mutating func removeAll() {
        interfaces.removeAll()
        order.removeAll()
    }

    private func merged(_ instance: String) -> [String] {
        var result: [String] = []
        for interface in order[instance] ?? [] {
            for address in interfaces[instance]?[interface] ?? [] where !result.contains(address) {
                result.append(address)
            }
        }
        return result
    }
}
