// SPDX-License-Identifier: MPL-2.0
//
// Send: the chosen files and where they go. A device whose hint matches
// exactly one paired device is sent to with that device's key pinned; any
// other device is unverified, and both screens show a code to compare
// before anything is sent (D10).

import DropumiAppState
import DropumiBindings
import DropumiDesign
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

struct SendScreen: View {
    @Environment(AppModel.self) private var model
    @Environment(Router.self) private var router
    @State private var selected: String?
    @State private var importing = false
    @State private var preparing = 0
    #if os(iOS)
    @State private var photos: [PhotosPickerItem] = []
    #endif

    init(preselected: String?) {
        _selected = State(initialValue: preselected)
    }

    var body: some View {
        ScreenScroll {
            ScreenHeader("Send", subtitle: subtitle)
            files
            addButtons
            targets
            footer
        }
        .toolbar {
            if !model.draft.isEmpty {
                ToolbarItem(placement: .primaryAction) {
                    Button("Clear all") { model.clearDraft() }
                        .accessibilityIdentifier("send.clear")
                }
            }
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            if case let .success(urls) = result {
                model.addToDraft(FileImport.picked(urls))
            }
        }
        #if os(iOS)
        .onChange(of: photos) { _, items in
            guard !items.isEmpty else { return }
            photos = []
            importPhotos(items)
        }
        .dropDestination(for: StagedCopy.self) { copies, _ in
            model.addToDraft(copies.map(\.outgoing))
            return !copies.isEmpty
        }
        #else
        .dropDestination(for: URL.self) { urls, _ in
            let files = urls.filter(\.isFileURL).map { OutgoingFile(url: $0, securityScoped: false, staged: false) }
            model.addToDraft(files)
            return !files.isEmpty
        }
        #endif
        .onAppear {
            model.beginBrowsing()
            takeFilePickerRequest()
        }
        .onDisappear { model.endBrowsing() }
        .onChange(of: router.requestFilePicker) { takeFilePickerRequest() }
    }

    private func takeFilePickerRequest() {
        guard router.requestFilePicker else { return }
        router.requestFilePicker = false
        importing = true
    }

    private var subtitle: Text {
        let total = model.draft.reduce(UInt64(0)) { $0 + $1.size }
        if model.draft.isEmpty {
            return Text("Choose files, then a device")
        }
        return Text("^[\(model.draft.count) file](inflect: true) · \(Formatting.bytes(total)) ready to go")
    }

    @ViewBuilder
    private var files: some View {
        if model.draft.isEmpty && preparing == 0 {
            EmptyStateView(
                symbol: Icons.upload, title: String(localized: "No files yet"),
                message: String(localized: "Add files to send them to a nearby device."))
        } else {
            VStack(spacing: 0) {
                ForEach(model.draft) { file in
                    DSRow(title: file.name, subtitle: Formatting.bytes(file.size)) {
                        FileTile(symbol: Icons.file(named: file.name))
                    } trailing: {
                        Button {
                            model.removeFromDraft(file)
                        } label: {
                            Image(systemName: Icons.close)
                                .foregroundStyle(DS.textSecondary)
                                .frame(width: 44, height: 44)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(Text("Remove \(Text(verbatim: file.name))"))
                    }
                }
                if preparing > 0 {
                    HStack(spacing: 10) {
                        ProgressView()
                        Text("Preparing ^[\(preparing) item](inflect: true)…")
                            .font(DSFont.meta)
                            .foregroundStyle(DS.textSecondary)
                    }
                    .padding(.vertical, 12)
                }
            }
        }
    }

    private var addButtons: some View {
        HStack(spacing: 18) {
            Button {
                importing = true
            } label: {
                Label("Add files", systemImage: Icons.add)
            }
            .accessibilityIdentifier("send.addFiles")
            #if os(iOS)
            PhotosPicker(selection: $photos, maxSelectionCount: 100, preferredItemEncoding: .current) {
                Label("Add photos", systemImage: "photo.on.rectangle")
            }
            #endif
        }
        .buttonStyle(.plain)
        .font(DSFont.smallButton)
        .foregroundStyle(DS.accentText)
        .frame(minHeight: 44)
    }

    private var targets: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Send to")
                .font(DSFont.sectionHeading)
                .foregroundStyle(DS.textPrimary)
                .accessibilityAddTraits(.isHeader)
            if model.nearby.isEmpty {
                Text("Looking for devices… Open Dropumi on the other device.")
                    .font(DSFont.meta)
                    .foregroundStyle(DS.textSecondary)
            }
            FlowLayout(spacing: 8) {
                ForEach(model.nearby, id: \.instance) { peer in
                    TargetChip(name: peer.name, selected: selected == peer.instance, enabled: peer.compatible) {
                        selected = peer.instance
                    }
                }
                Button {
                    router.nearbyPath.append(.connectByAddress)
                } label: {
                    Label("Address…", systemImage: Icons.code)
                        .font(DSFont.smallButton)
                        .padding(.horizontal, 12)
                        .frame(minHeight: 34)
                        .overlay(Capsule().stroke(DS.border))
                }
                .buttonStyle(.plain)
                .foregroundStyle(DS.textPrimary)
            }
            if let peer = selectedPeer {
                Text(peer.hintAlias != nil
                     ? "Dropumi checks that this is your paired device before sending."
                     : "This device isn't paired. You'll both see a code to compare before anything is sent.")
                    .font(DSFont.meta)
                    .foregroundStyle(DS.textSecondary)
                if peer.hintAlias == nil {
                    pairHint
                }
            }
        }
    }

    /// Pairing opens above this screen, so the chosen files stay.
    private var pairHint: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("Pair it once to skip the code next time.")
                .font(DSFont.meta)
                .foregroundStyle(DS.textSecondary)
            Button {
                router.openPairing()
            } label: {
                Label("Pair a device", systemImage: Icons.pair)
            }
            .buttonStyle(TextLinkButtonStyle())
            .accessibilityIdentifier("send.pair")
        }
    }

    private var footer: some View {
        VStack(spacing: 10) {
            Button {
                send()
            } label: {
                if let peer = selectedPeer {
                    Label {
                        Text("Send to \(Text(verbatim: peer.name))")
                    } icon: {
                        Image(systemName: Icons.send)
                    }
                } else {
                    Text("Choose a device")
                }
            }
            .buttonStyle(PrimaryPillButtonStyle())
            .disabled(selectedPeer == nil || model.draft.isEmpty || preparing > 0)
            .accessibilityIdentifier("send.go")
            Text("Sent directly over your local network")
                .font(DSFont.meta)
                .foregroundStyle(DS.textSecondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 8)
    }

    private var selectedPeer: NearbyPeer? {
        model.nearby.first { $0.instance == selected }
    }

    private func send() {
        guard let peer = selectedPeer else { return }
        Task {
            await model.sendDraft(
                to: .nearby(instance: peer.instance), targetName: peer.name,
                allowUnverified: peer.hintAlias == nil)
            if model.draft.isEmpty {
                router.nearbyPath = []
                router.tab = .transfers
            }
        }
    }

    #if os(iOS)
    private func importPhotos(_ items: [PhotosPickerItem]) {
        preparing += items.count
        for item in items {
            Task {
                defer { preparing -= 1 }
                if let copy = try? await item.loadTransferable(type: StagedCopy.self) {
                    model.addToDraft([copy.outgoing])
                } else {
                    model.banner = Banner(
                        style: .warning, title: String(localized: "A photo could not be added"),
                        message: String(localized: "Try again, or add it from Files."))
                }
            }
        }
    }
    #endif
}

private struct TargetChip: View {
    let name: String
    let selected: Bool
    let enabled: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Image(systemName: Icons.device)
                    .font(.system(size: 12, weight: .medium))
                    .accessibilityHidden(true)
                Text(verbatim: name)
                    .font(DSFont.smallButton)
                    .lineLimit(1)
            }
            .padding(.horizontal, 12)
            .frame(minHeight: 34)
            .foregroundStyle(selected ? DS.onAccent : DS.textPrimary)
            .background(Capsule().fill(selected ? DS.accent : DS.surface))
            .overlay(Capsule().stroke(selected ? .clear : DS.border))
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityAddTraits(selected ? [.isSelected] : [])
    }
}

/// Lays out chips in rows.
struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = arrange(proposal: proposal, subviews: subviews)
        let height = rows.map(\.height).reduce(0, +) + spacing * CGFloat(max(0, rows.count - 1))
        let width = rows.map(\.width).max() ?? 0
        return CGSize(width: proposal.width ?? width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var y = bounds.minY
        for row in arrange(proposal: ProposedViewSize(width: bounds.width, height: nil), subviews: subviews) {
            var x = bounds.minX
            for index in row.indices {
                let size = subviews[index].sizeThatFits(.unspecified)
                subviews[index].place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
                x += size.width + spacing
            }
            y += row.height + spacing
        }
    }

    private struct Row {
        var indices: [Int] = []
        var width: CGFloat = 0
        var height: CGFloat = 0
    }

    private func arrange(proposal: ProposedViewSize, subviews: Subviews) -> [Row] {
        let maxWidth = proposal.width ?? .infinity
        var rows: [Row] = [Row()]
        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(.unspecified)
            let needed = rows[rows.count - 1].indices.isEmpty ? size.width : rows[rows.count - 1].width + spacing + size.width
            if needed > maxWidth, !rows[rows.count - 1].indices.isEmpty {
                rows.append(Row())
            }
            var row = rows[rows.count - 1]
            row.width = row.indices.isEmpty ? size.width : row.width + spacing + size.width
            row.height = max(row.height, size.height)
            row.indices.append(index)
            rows[rows.count - 1] = row
        }
        return rows
    }
}
