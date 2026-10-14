// SPDX-License-Identifier: MPL-2.0
//
// iPhone and iPad. Dropumi receives only while it is open (D5): in the
// background the receiver stops accepting (the listener closes and the
// advertisement is withdrawn) and running transfers may finish.

import DropumiAppState
import DropumiDesign
import SwiftUI

@main
struct DropumiApp: App {
    @State private var loader = AppLoader()
    @Environment(\.scenePhase) private var scenePhase

    init() {
        DSFont.register()
    }

    var body: some Scene {
        WindowGroup {
            RootView(loader: loader)
        }
        .commands { DropumiCommands(router: loader.router) }
        .onChange(of: scenePhase) { _, phase in
            guard let model = loader.model else { return }
            switch phase {
            case .background:
                model.enterBackground()
            case .active:
                Task { await model.becomeActive() }
            default:
                break
            }
        }
    }
}
