// SPDX-License-Identifier: MPL-2.0
//
// The Bonjour adapter's bookkeeping: every interface a device is seen on is
// resolved and the addresses merged, and the device is lost only when the
// last interface loses it. Found on a Mac with virtual-machine bridges,
// where resolving on the first interface alone could leave a device with a
// single address that refuses connections (run 2026-10-25-01, Finding 3).

import Testing
@testable import DropumiCore

@Suite struct BonjourInstancesTests {
    @Test func addressesFromEveryInterfaceAreMerged() {
        var instances = BonjourInstances()
        let first = instances.seen("dropumi-1", on: 21)
        let second = instances.seen("dropumi-1", on: 14)
        #expect(first && second)
        let bridge = instances.update("dropumi-1", on: 21, addresses: ["192.168.194.1"])
        #expect(bridge == ["192.168.194.1"])
        let both = instances.update("dropumi-1", on: 14, addresses: ["192.168.0.20"])
        #expect(both == ["192.168.194.1", "192.168.0.20"])
    }

    @Test func theSameInterfaceIsResolvedOnce() {
        var instances = BonjourInstances()
        let first = instances.seen("dropumi-1", on: 14)
        let again = instances.seen("dropumi-1", on: 14)
        #expect(first)
        #expect(!again)
        #expect(instances.interfaces(of: "dropumi-1") == [14])
    }

    @Test func anAddressSeenOnTwoInterfacesIsListedOnce() {
        var instances = BonjourInstances()
        _ = instances.seen("dropumi-1", on: 14)
        _ = instances.seen("dropumi-1", on: 15)
        _ = instances.update("dropumi-1", on: 14, addresses: ["192.168.0.20"])
        let merged = instances.update("dropumi-1", on: 15, addresses: ["192.168.0.20"])
        #expect(merged == ["192.168.0.20"])
    }

    @Test func losingOneInterfaceKeepsTheOthersAddresses() {
        var instances = BonjourInstances()
        _ = instances.seen("dropumi-1", on: 21)
        _ = instances.seen("dropumi-1", on: 14)
        _ = instances.update("dropumi-1", on: 21, addresses: ["192.168.194.1"])
        _ = instances.update("dropumi-1", on: 14, addresses: ["192.168.0.20"])
        let remaining = instances.lost("dropumi-1", on: 21)
        #expect(remaining == ["192.168.0.20"])
        #expect(instances.interfaces(of: "dropumi-1") == [14])
    }

    @Test func theDeviceIsLostOnlyWithTheLastInterface() {
        var instances = BonjourInstances()
        _ = instances.seen("dropumi-1", on: 21)
        _ = instances.seen("dropumi-1", on: 14)
        let afterOne = instances.lost("dropumi-1", on: 21)
        let afterBoth = instances.lost("dropumi-1", on: 14)
        #expect(afterOne != nil)
        #expect(afterBoth == nil)
        #expect(instances.interfaces(of: "dropumi-1").isEmpty)
    }

    @Test func aDeviceSeenAgainAfterBeingLostIsResolvedAgain() {
        var instances = BonjourInstances()
        _ = instances.seen("dropumi-1", on: 14)
        let gone = instances.lost("dropumi-1", on: 14)
        let again = instances.seen("dropumi-1", on: 14)
        let addresses = instances.update("dropumi-1", on: 14, addresses: ["192.168.0.20"])
        #expect(gone == nil)
        #expect(again)
        #expect(addresses == ["192.168.0.20"])
    }

    @Test func addressesOfAnUnknownInterfaceAreIgnored() {
        var instances = BonjourInstances()
        _ = instances.seen("dropumi-1", on: 14)
        _ = instances.update("dropumi-1", on: 14, addresses: ["192.168.0.20"])
        let unknownInterface = instances.update("dropumi-1", on: 99, addresses: ["10.0.0.9"])
        let unknownInstance = instances.lost("dropumi-2", on: 14)
        #expect(unknownInterface == ["192.168.0.20"])
        #expect(unknownInstance == nil)
    }
}
