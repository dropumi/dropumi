// SPDX-License-Identifier: MPL-2.0

import DropumiAppState
import DropumiDesign
import SwiftUI

/// Loading, a startup failure, onboarding, or the app. Each app decides when
/// the core opens: the iPhone and iPad window, or the Mac's launch (the Mac
/// receives with no window open).
struct RootView: View {
    let loader: AppLoader

    var body: some View {
        Group {
            switch loader.state {
            case .loading:
                LaunchView()
            case let .failed(message):
                StartupFailedView(message: message) {
                    Task { await loader.reload() }
                }
            case let .ready(model):
                ReadyView(model: model, router: loader.router)
            }
        }
        .environment(loader)
        .onOpenURL { loader.open($0) }
    }
}

private struct ReadyView: View {
    @Bindable var model: AppModel
    let router: Router

    var body: some View {
        Group {
            if model.onboarded {
                MainView()
            } else {
                OnboardingView()
            }
        }
        .preferredColorScheme(model.appearance.colorScheme)
        // Links, toggles and the selected tab are small text: use the
        // accent that keeps 4.5:1 (design.md contrast notes).
        .tint(DS.accentText)
        .modifier(PromptPresenter(model: model))
        #if os(iOS)
        .modifier(PlatformHooks(model: model))
        #endif
        .overlay(alignment: .top) { BannerHost(model: model) }
        .onChange(of: model.route) { _, route in
            guard route == .send else { return }
            router.openSend()
            model.route = nil
        }
        // Outermost, so overlays and presented screens see them too.
        .environment(model)
        .environment(router)
    }
}

/// The tabs: a floating tab bar on iPhone, a sidebar on iPad and Mac.
struct MainView: View {
    @Environment(AppModel.self) private var model
    @Environment(Router.self) private var router

    var body: some View {
        @Bindable var router = router
        TabView(selection: $router.tab) {
            Tab("Nearby", systemImage: Icons.nearby, value: AppTab.nearby) {
                NavigationStack(path: $router.nearbyPath) {
                    NearbyScreen()
                        .navigationDestination(for: NearbyRoute.self) { route in
                            switch route {
                            case let .send(preselected): SendScreen(preselected: preselected)
                            case .connectByAddress: ConnectByAddressScreen()
                            case .pairing: PairingScreen()
                            case .pairByAddress: ConnectByAddressScreen(mode: .pair)
                            }
                        }
                }
            }
            .accessibilityIdentifier("tab.nearby")
            Tab("Transfers", systemImage: Icons.transfers, value: AppTab.transfers) {
                NavigationStack { TransfersScreen() }
            }
            .badge(model.active.count)
            .accessibilityIdentifier("tab.transfers")
            Tab("Settings", systemImage: Icons.settings, value: AppTab.settings) {
                NavigationStack(path: $router.settingsPath) {
                    SettingsScreen()
                        .navigationDestination(for: SettingsRoute.self) { route in
                            switch route {
                            case .thisDevice: ThisDeviceScreen()
                            case .trustedDevices: TrustedDevicesScreen()
                            case .pairing: PairingScreen()
                            case .connectByAddress: ConnectByAddressScreen()
                            case .pairByAddress: ConnectByAddressScreen(mode: .pair)
                            }
                        }
                }
            }
            .accessibilityIdentifier("tab.settings")
        }
        .tabViewStyle(.sidebarAdaptable)
        #if os(macOS)
        .tabViewSidebarHeader {
            Image("Wordmark")
                .resizable()
                .scaledToFit()
                .frame(height: 26)
                .padding(.vertical, 8)
                .accessibilityLabel(Text("Dropumi"))
        }
        .tabViewSidebarBottomBar {
            ThisDeviceSummary()
        }
        #endif
    }
}

/// Shown while the core opens.
struct LaunchView: View {
    var body: some View {
        ZStack {
            DS.Receive.background.ignoresSafeArea()
            Image("SymbolReversed")
                .resizable()
                .scaledToFit()
                .frame(width: 120, height: 120)
                .accessibilityLabel(Text("Dropumi"))
        }
    }
}

struct StartupFailedView: View {
    let message: String
    let retry: () -> Void

    var body: some View {
        VStack(spacing: 20) {
            Spacer()
            Image(systemName: Icons.warning)
                .font(.system(size: 40, weight: .medium))
                .foregroundStyle(DS.danger)
                .accessibilityHidden(true)
            Text("Dropumi could not start")
                .font(DSFont.sectionHeading)
                .foregroundStyle(DS.textPrimary)
            Text(message)
                .font(DSFont.body)
                .foregroundStyle(DS.textSecondary)
                .multilineTextAlignment(.center)
            Button("Try again", action: retry)
                .buttonStyle(PrimaryPillButtonStyle())
                .frame(maxWidth: 320)
            Spacer()
        }
        .padding(DS.Metrics.screenPadding)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(DS.bg)
    }
}

extension Appearance {
    var colorScheme: ColorScheme? {
        switch self {
        case .system: return nil
        case .light: return .light
        case .dark: return .dark
        }
    }
}
