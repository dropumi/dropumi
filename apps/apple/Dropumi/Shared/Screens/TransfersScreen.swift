// SPDX-License-Identifier: MPL-2.0
//
// Transfers: running transfers with progress, then the history. "Done" is
// shown only for TransferFinished(Completed); a partial transfer says so.

import DropumiAppState
import DropumiBindings
import DropumiDesign
import SwiftUI

struct TransfersScreen: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ScreenScroll {
            ScreenHeader("Transfers", subtitle: subtitle)
            if model.active.isEmpty && model.history.isEmpty {
                EmptyStateView(
                    symbol: Icons.transfers, title: String(localized: "No transfers yet"),
                    message: String(localized: "Files you send or receive show up here."))
                    .accessibilityIdentifier("transfers.empty")
            }
            ForEach(model.active) { record in
                TransferCard(record: record) { model.cancel(record) }
            }
            #if os(iOS)
            if !model.active.isEmpty {
                Text("Keep Dropumi open until transfers finish. If you leave, iOS may stop them.")
                    .font(DSFont.meta)
                    .foregroundStyle(DS.textSecondary)
            }
            #endif
            if !model.history.isEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text("Earlier")
                            .font(DSFont.sectionHeading)
                            .foregroundStyle(DS.textPrimary)
                            .accessibilityAddTraits(.isHeader)
                        Spacer()
                        Button("Clear") { model.clearHistory() }
                            .buttonStyle(.plain)
                            .font(DSFont.smallButton)
                            .foregroundStyle(DS.accentText)
                            .frame(minWidth: 44, minHeight: 44)
                            .contentShape(Rectangle())
                            .accessibilityIdentifier("transfers.clear")
                    }
                    ForEach(model.history) { entry in
                        HistoryRow(entry: entry, receiveDir: model.paths.receiveDir)
                    }
                }
            }
        }
    }

    private var subtitle: Text {
        let today = model.history.filter { Calendar.current.isDateInToday($0.finishedAt) }.count
        return Text("\(model.active.count) active · \(today) finished today")
    }
}

/// One running transfer.
struct TransferCard: View {
    @Environment(\.dynamicTypeSize) private var typeSize
    let record: TransferRecord
    let cancel: () -> Void

    var body: some View {
        let label = record.peerLabel
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                FileTile(symbol: Icons.file(named: record.fileNames.first ?? ""))
                VStack(alignment: .leading, spacing: 2) {
                    Text(verbatim: title)
                        .font(DSFont.rowTitle)
                        .foregroundStyle(DS.textPrimary)
                        .lineLimit(typeSize.isAccessibilitySize ? nil : 1)
                        .truncationMode(.middle)
                    peerLine(label)
                        .font(DSFont.meta)
                        .foregroundStyle(DS.textSecondary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                Spacer(minLength: 8)
                Button(action: cancel) {
                    Image(systemName: Icons.close)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(DS.textSecondary)
                        .frame(width: 32, height: 32)
                        .background(Circle().fill(DS.surface2))
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel(Text("Cancel"))
                .accessibilityIdentifier("transfer.cancel")
            }
            DSProgressBar(record.fraction)
            HStack {
                Text(status)
                    .font(DSFont.meta)
                    .foregroundStyle(DS.textSecondary)
                Spacer()
                Text(rate)
                    .font(DSFont.smallButton)
                    .monospacedDigit()
                    .foregroundStyle(DS.accentText)
            }
        }
        .padding(16)
        // The shadow is the card's only, never the text's.
        .background(
            RoundedRectangle(cornerRadius: DS.Metrics.cardRadius)
                .fill(DS.surface)
                .shadow(color: DS.navy.opacity(0.08), radius: 14, y: 5))
        .overlay(RoundedRectangle(cornerRadius: DS.Metrics.cardRadius).stroke(DS.border))
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("transfer.card")
    }

    private var title: String {
        guard let first = record.fileNames.first else {
            return Formatting.fileCount(record.fileCount)
        }
        let more = max(record.fileCount, record.fileNames.count) - 1
        return more > 0 ? String(localized: "\(first) and \(more) more") : first
    }

    private func peerLine(_ label: PeerLabel) -> Text {
        let name = Text(verbatim: label.title)
        switch record.direction {
        case .outgoing: return Text("Sending to \(name)")
        case .incoming: return Text("Receiving from \(name)")
        }
    }

    private var status: String {
        switch record.phase {
        case .connecting:
            return String(localized: "Connecting and checking the device…")
        case .waitingForAcceptance:
            return String(localized: "Waiting for the other person to accept…")
        case .waitingForAnswer:
            return String(localized: "Waiting for your answer…")
        case .transferring:
            if let progress = record.progress {
                return String(localized: "\(Formatting.bytes(progress.bytesDone)) of \(Formatting.bytes(progress.bytesTotal))")
            }
            return String(localized: "Starting…")
        case .finished:
            return ""
        }
    }

    private var rate: String {
        guard let fraction = record.fraction else { return "" }
        var parts = [fraction.formatted(.percent.precision(.fractionLength(0)))]
        if let speed = record.bytesPerSecond {
            parts.append(Formatting.speed(speed))
        }
        if let left = record.secondsLeft {
            parts.append(Formatting.timeLeft(left))
        }
        return parts.joined(separator: " · ")
    }
}

/// One finished transfer.
private struct HistoryRow: View {
    let entry: HistoryEntry
    let receiveDir: URL

    var body: some View {
        DSRow(title: title, subtitle: subtitle) {
            FileTile(symbol: Icons.file(named: entry.fileNames.first ?? ""), muted: true)
        } trailing: {
            HStack(spacing: 10) {
                if !entry.outgoing, let first = entry.savedAs.first {
                    Button {
                        Platform.reveal(savedAs: first, in: receiveDir)
                    } label: {
                        Image(systemName: Icons.folder)
                            .foregroundStyle(DS.accent)
                            .frame(width: 44, height: 44)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    #if os(iOS)
                    .accessibilityLabel(Text("Show in Files"))
                    #else
                    .accessibilityLabel(Text("Show in Finder"))
                    #endif
                }
                Image(systemName: outcomeSymbol)
                    .foregroundStyle(outcomeColor)
                    .accessibilityLabel(Text(outcomeLabel))
            }
        }
        .accessibilityElement(children: .combine)
    }

    private var title: String {
        guard let first = entry.fileNames.first else { return entry.summary }
        let more = entry.fileNames.count - 1
        return more > 0 ? String(localized: "\(first) and \(more) more") : first
    }

    private var subtitle: String {
        let time = entry.finishedAt.formatted(date: .omitted, time: .shortened)
        let who = entry.outgoing
            ? String(localized: "Sent to \(entry.peerName)")
            : String(localized: "Received from \(entry.peerName)")
        if entry.outcome == .completed {
            return "\(who) · \(time)"
        }
        return "\(who) · \(time)\n\(entry.summary)"
    }

    private var outcomeSymbol: String {
        switch entry.outcome {
        case .completed: return Icons.check
        case .partial: return Icons.partial
        case .rejected, .cancelled, .failed: return Icons.failed
        }
    }

    private var outcomeColor: Color {
        switch entry.outcome {
        case .completed: return DS.success
        case .partial: return DS.danger
        case .rejected, .cancelled, .failed: return DS.textSecondary
        }
    }

    private var outcomeLabel: String {
        switch entry.outcome {
        case .completed: return String(localized: "Done")
        case .partial: return String(localized: "Some files failed")
        case .rejected: return String(localized: "Declined")
        case .cancelled: return String(localized: "Cancelled")
        case .failed: return String(localized: "Failed")
        }
    }
}
