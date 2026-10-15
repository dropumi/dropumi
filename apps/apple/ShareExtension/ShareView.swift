// SPDX-License-Identifier: MPL-2.0

import DropumiDesign
import SwiftUI

struct ShareView: View {
    let state: ShareState
    let done: () -> Void
    let cancel: () -> Void

    var body: some View {
        VStack(spacing: 18) {
            HStack {
                Spacer()
                if case .ready = state.phase {
                    EmptyView()
                } else {
                    Button("Cancel", action: cancel)
                        .font(DSFont.smallButton)
                        .foregroundStyle(DS.accent)
                }
            }
            Spacer()
            switch state.phase {
            case let .copying(done, total):
                ProgressView(value: Double(done), total: Double(max(total, 1)))
                    .tint(DS.accent)
                    .frame(maxWidth: 260)
                Text("Preparing \(done) of \(total)…")
                    .font(DSFont.body)
                    .foregroundStyle(DS.textSecondary)
            case let .ready(count):
                Image(systemName: "checkmark.circle")
                    .font(.system(size: 44, weight: .medium))
                    .foregroundStyle(DS.success)
                    .accessibilityHidden(true)
                Text("Ready in Dropumi")
                    .font(DSFont.sectionHeading)
                    .foregroundStyle(DS.textPrimary)
                Text("^[\(count) item](inflect: true) will be waiting. Open Dropumi to choose a device.")
                    .font(DSFont.body)
                    .foregroundStyle(DS.textSecondary)
                    .multilineTextAlignment(.center)
                Button("Done", action: done)
                    .buttonStyle(PrimaryPillButtonStyle())
                    .frame(maxWidth: 260)
            case let .failed(message):
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 40, weight: .medium))
                    .foregroundStyle(DS.danger)
                    .accessibilityHidden(true)
                Text(message)
                    .font(DSFont.body)
                    .foregroundStyle(DS.textPrimary)
                    .multilineTextAlignment(.center)
                Button("Close", action: cancel)
                    .buttonStyle(OutlinePillButtonStyle())
                    .frame(maxWidth: 260)
            }
            Spacer()
        }
        .padding(DS.Metrics.screenPadding)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(DS.bg)
    }
}
