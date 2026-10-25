// SPDX-License-Identifier: MPL-2.0
//
// The radar from design.md: this device at the centre, three rings, nearby
// devices as discs on the rings. Decorative: hidden from VoiceOver (the
// device list below it is the accessible way to pick a device) and static
// (one gentle pulse when a device appears, none with Reduce Motion).

import SwiftUI

public struct RadarItem: Identifiable, Equatable {
    public let id: String
    public let name: String
    public init(id: String, name: String) {
        self.id = id
        self.name = name
    }
}

public struct RadarView<Center: View>: View {
    let items: [RadarItem]
    let center: Center
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(items: [RadarItem], @ViewBuilder center: () -> Center) {
        self.items = items
        self.center = center()
    }

    public var body: some View {
        GeometryReader { geo in
            // Room below the outer ring for the last row of labels.
            let side = min(geo.size.width, geo.size.height - 28)
            let mid = CGPoint(x: geo.size.width / 2, y: side / 2)
            let shown = Array(items.prefix(Self.slots))
            let slots = Self.assignSlots(shown.map(\.id))
            ZStack {
                RadialGradient(colors: [DS.accentSoft.opacity(0.6), .clear], center: .center, startRadius: 0, endRadius: side / 2)
                    .frame(width: side, height: side)
                    .position(mid)
                ForEach([0.36, 0.66, 0.96], id: \.self) { scale in
                    Circle()
                        .stroke(DS.ring, lineWidth: 1.5)
                        .frame(width: side * scale, height: side * scale)
                        .position(mid)
                }
                center
                    .frame(width: side * 0.26, height: side * 0.26)
                    .position(mid)
                ForEach(shown) { item in
                    let point = Self.position(slot: slots[item.id] ?? 0, side: side)
                    VStack(spacing: 4) {
                        DeviceAvatar(symbol: Icons.device, size: 44)
                            .background(Circle().fill(DS.surface).shadow(color: DS.navy.opacity(0.1), radius: 8, y: 3))
                        Text(verbatim: item.name)
                            .font(DSFont.meta)
                            .foregroundStyle(DS.textPrimary)
                            .lineLimit(1)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(Capsule().fill(DS.bg.opacity(0.92)))
                            .frame(maxWidth: 120)
                    }
                    .position(x: mid.x + point.x, y: mid.y + point.y + 12)
                    .transition(reduceMotion ? .identity : .scale.combined(with: .opacity))
                }
            }
            .frame(width: geo.size.width, height: geo.size.height)
            .animation(reduceMotion ? nil : .easeOut(duration: 0.35), value: items)
        }
        .aspectRatio(0.92, contentMode: .fit)
        .accessibilityHidden(true)
    }

    /// Eight places, alternating between the two outer rings, so nothing
    /// covers this device in the centre.
    static var slots: Int { 8 }

    /// A stable place for each device (from a hash of its id; the next free
    /// place if that one is taken), so devices do not jump around when the
    /// list changes.
    static func assignSlots(_ ids: [String]) -> [String: Int] {
        var taken = Set<Int>()
        var result: [String: Int] = [:]
        for id in ids {
            var hash: UInt64 = 1469598103934665603
            for byte in id.utf8 {
                hash = (hash ^ UInt64(byte)) &* 1099511628211
            }
            var slot = Int(hash % UInt64(slots))
            while taken.contains(slot) {
                slot = (slot + 1) % slots
            }
            taken.insert(slot)
            result[id] = slot
        }
        return result
    }

    static func position(slot: Int, side: CGFloat) -> CGPoint {
        let angle = (Double(slot) * 45 - 112.5) * .pi / 180
        let ring = (slot.isMultiple(of: 2) ? 0.96 : 0.66) * side / 2
        return CGPoint(x: cos(angle) * ring, y: sin(angle) * ring)
    }
}
