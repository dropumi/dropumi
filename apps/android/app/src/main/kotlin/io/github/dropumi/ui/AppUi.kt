// SPDX-License-Identifier: MPL-2.0
//
// The app's content (a port of the Apple app's RootView and MainView):
// Loading, "Dropumi could not start" with Try again, onboarding, or the
// tabs. Under 600 dp the tabs are a bottom NavigationBar; wider windows use
// a NavigationRail and the wide layouts (design.md "Layout"). Banners show
// at the top; confirmations are drawn over the screens, and a prompt over
// everything, all in this window (ADR-0013 D8). While either shows, the
// screens below are hidden from TalkBack and keyboard focus, and
// MainActivity protects it from overlays.
package io.github.dropumi.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.focusGroup
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.NavigationBarItemDefaults
import androidx.compose.material3.NavigationRail
import androidx.compose.material3.NavigationRailItem
import androidx.compose.material3.NavigationRailItemDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusProperties
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.dropumi.AppGraph
import io.github.dropumi.appstate.AppLoader
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.Appearance
import io.github.dropumi.appstate.Banner
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.appstate.Route
import io.github.dropumi.ui.design.BannerView
import io.github.dropumi.ui.design.BrandSymbol
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.DSMetrics
import io.github.dropumi.ui.design.DSReceive
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.DropumiTheme
import io.github.dropumi.ui.design.PrimaryButton
import io.github.dropumi.ui.design.Wordmark
import io.github.dropumi.ui.prompt.PromptHost
import io.github.dropumi.ui.screens.ConnectByAddressScreen
import io.github.dropumi.ui.screens.DocumentPicking
import io.github.dropumi.ui.screens.LicenceText
import io.github.dropumi.ui.screens.LicencesScreen
import io.github.dropumi.ui.screens.LocalAppScope
import io.github.dropumi.ui.screens.LocalConfirmations
import io.github.dropumi.ui.screens.LocalFileActions
import io.github.dropumi.ui.screens.LocalModel
import io.github.dropumi.ui.screens.LocalRouter
import io.github.dropumi.ui.screens.NearbyScreen
import io.github.dropumi.ui.screens.OnboardingScreen
import io.github.dropumi.ui.screens.PairedDevicesScreen
import io.github.dropumi.ui.screens.PairingScreen
import io.github.dropumi.ui.screens.PushedScreen
import io.github.dropumi.ui.screens.SendScreen
import io.github.dropumi.ui.screens.SettingsScreen
import io.github.dropumi.ui.screens.ThisDeviceScreen
import io.github.dropumi.ui.screens.TransfersScreen
import io.github.dropumi.ui.screens.everyoneConfirmation
import io.github.dropumi.ui.screens.rememberDocumentPicking
import kotlinx.coroutines.launch

/**
 * The whole content of MainActivity. [onDarkTheme] reports the theme in use,
 * so the activity can match the system bars to it; [onProtecting] reports
 * whether a prompt or a confirmation is on screen (overlay protection).
 */
@Composable
fun DropumiApp(
    graph: AppGraph,
    router: Router,
    confirmations: Confirmations,
    touchGuard: TouchGuard,
    keyGate: KeyGate,
    onDarkTheme: (Boolean) -> Unit,
    onProtecting: (Boolean) -> Unit,
) {
    val loaderState by graph.loader.state.collectAsStateWithLifecycle()
    when (val s = loaderState) {
        AppLoader.State.Loading -> {
            SideEffect {
                onDarkTheme(true)
                onProtecting(false)
            }
            LaunchScreen()
        }
        is AppLoader.State.Failed -> {
            val dark = isSystemInDarkTheme()
            SideEffect {
                onDarkTheme(dark)
                onProtecting(false)
            }
            DropumiTheme(dark) { StartupFailed(s.message) { graph.loader.retry() } }
        }
        is AppLoader.State.Ready -> Ready(graph, s.model, router, confirmations, touchGuard, keyGate, onDarkTheme, onProtecting)
    }
}

/** Shown while the core opens: the symbol on navy, like the launch window. */
@Composable
private fun LaunchScreen() {
    Box(contentAlignment = Alignment.Center, modifier = Modifier.fillMaxSize().background(DSReceive.background)) {
        BrandSymbol(Modifier.size(120.dp).semantics { liveRegion = LiveRegionMode.Polite })
    }
}

@Composable
private fun StartupFailed(message: String, retry: () -> Unit) {
    val c = DS.colors
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier.fillMaxSize().background(c.bg).windowInsetsPadding(WindowInsets.safeDrawing).padding(DSMetrics.screenPadding),
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(20.dp),
            modifier = Modifier.widthIn(max = 420.dp),
        ) {
            Icon(painterResource(DSIcons.warning), contentDescription = null, tint = c.danger, modifier = Modifier.size(40.dp))
            Text("Dropumi could not start", style = DSType.sectionHeading, color = c.textPrimary)
            Text(message, style = DSType.body, color = c.textSecondary, textAlign = TextAlign.Center)
            PrimaryButton("Try again", retry, modifier = Modifier.widthIn(max = 320.dp).testTag("startup.retry"))
        }
    }
}

@Composable
private fun Ready(
    graph: AppGraph,
    model: AppModel,
    router: Router,
    confirmations: Confirmations,
    touchGuard: TouchGuard,
    keyGate: KeyGate,
    onDarkTheme: (Boolean) -> Unit,
    onProtecting: (Boolean) -> Unit,
) {
    val state by model.state.collectAsStateWithLifecycle()
    val dark = when (state.appearance) {
        Appearance.SYSTEM -> isSystemInDarkTheme()
        Appearance.LIGHT -> false
        Appearance.DARK -> true
    }
    val prompting = state.currentPrompt != null
    val protecting = prompting || confirmations.showing
    SideEffect {
        // The prompt is navy in both themes.
        onDarkTheme(dark || prompting)
        onProtecting(protecting)
    }
    LaunchedEffect(state.route) {
        if (state.route == Route.SEND) {
            router.openSend()
            model.routeHandled()
        }
    }
    DropumiTheme(dark) {
        CompositionLocalProvider(
            LocalModel provides model,
            LocalRouter provides router,
            LocalConfirmations provides confirmations,
            LocalFileActions provides graph.receivedFiles,
            LocalAppScope provides graph.scope,
            LocalTouchGuard provides touchGuard,
            LocalKeyGate provides keyGate,
        ) {
            // While a confirmation or a prompt shows, TalkBack and the
            // keyboard reach only that: keyboard focus that would enter the
            // screens below goes to the prompt or the panel instead.
            val promptFocus = remember { FocusRequester() }
            val panelFocus = remember { FocusRequester() }
            Box(Modifier.fillMaxSize().background(DS.colors.bg)) {
                val contentRedirect = when {
                    prompting -> promptFocus
                    confirmations.showing -> panelFocus
                    else -> null
                }
                Box(Modifier.fillMaxSize().covered(contentRedirect)) {
                    if (state.onboarded) MainShell(state) else OnboardingScreen(state)
                    BannerHost(state)
                }
                Box(Modifier.fillMaxSize().covered(if (prompting) promptFocus else null)) {
                    ConfirmationPanel(confirmations, backEnabled = !prompting, focus = panelFocus)
                }
                if (!protecting) OverlayNoticeHost(touchGuard)
                PromptHost(state, promptFocus)
            }
        }
    }
}

/** The tabs: a bottom bar under 600 dp, a rail otherwise. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun MainShell(state: AppState) {
    val model = LocalModel.current
    val router = LocalRouter.current
    val scope = LocalAppScope.current
    val picking = rememberDocumentPicking(model, router, scope)
    // A prompt's own Back (decline) is the only one while it shows.
    BackHandler(enabled = (router.current != null || router.tab != Tab.NEARBY) && state.currentPrompt == null) {
        if (!router.pop()) router.select(Tab.NEARBY)
    }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val wide = maxWidth >= DSMetrics.wideLayoutMinWidth
        if (wide) {
            Row(Modifier.fillMaxSize()) {
                Rail(state, router)
                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxHeight()
                        .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.End + WindowInsetsSides.Bottom)),
                ) {
                    TabContent(state, router, wide = true, picking)
                }
            }
        } else {
            val imeVisible = WindowInsets.isImeVisible
            Column(Modifier.fillMaxSize()) {
                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxWidth()
                        .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))
                        .then(if (imeVisible) Modifier.windowInsetsPadding(WindowInsets.ime) else Modifier),
                ) {
                    TabContent(state, router, wide = false, picking)
                }
                // The keyboard covers the bar; it is left out while the keyboard shows.
                if (!imeVisible) BottomBar(state, router)
            }
        }
    }
}

private data class TabItem(val tab: Tab, val label: String, val icon: Int, val tag: String)

private val tabItems = listOf(
    TabItem(Tab.NEARBY, "Nearby", DSIcons.nearby, "tab.nearby"),
    TabItem(Tab.TRANSFERS, "Transfers", DSIcons.transfers, "tab.transfers"),
    TabItem(Tab.SETTINGS, "Settings", DSIcons.settings, "tab.settings"),
)

@Composable
private fun TabIcon(item: TabItem, activeCount: Int) {
    if (item.tab == Tab.TRANSFERS && activeCount > 0) {
        BadgedBox(badge = { Badge(containerColor = DS.colors.accent, contentColor = DS.colors.onAccent) { Text("$activeCount") } }) {
            Icon(painterResource(item.icon), contentDescription = null, modifier = Modifier.size(22.dp))
        }
    } else {
        Icon(painterResource(item.icon), contentDescription = null, modifier = Modifier.size(22.dp))
    }
}

@Composable
private fun BottomBar(state: AppState, router: Router) {
    val c = DS.colors
    NavigationBar(containerColor = c.surface, modifier = Modifier.testTag("nav.bar")) {
        for (item in tabItems) {
            NavigationBarItem(
                selected = router.tab == item.tab,
                onClick = { router.select(item.tab) },
                icon = { TabIcon(item, state.active.size) },
                label = { Text(item.label, style = DSType.tabLabel) },
                colors = NavigationBarItemDefaults.colors(
                    selectedIconColor = c.accentText,
                    selectedTextColor = c.accentText,
                    indicatorColor = c.accentSoft,
                    unselectedIconColor = c.textSecondary,
                    unselectedTextColor = c.textSecondary,
                ),
                modifier = Modifier.testTag(item.tag),
            )
        }
    }
}

@Composable
private fun Rail(state: AppState, router: Router) {
    val c = DS.colors
    NavigationRail(
        containerColor = c.surface2,
        header = { Wordmark(height = 15.dp, modifier = Modifier.padding(vertical = 12.dp)) },
        modifier = Modifier.testTag("nav.rail"),
    ) {
        Spacer(Modifier.size(8.dp))
        for (item in tabItems) {
            NavigationRailItem(
                selected = router.tab == item.tab,
                onClick = { router.select(item.tab) },
                icon = { TabIcon(item, state.active.size) },
                label = { Text(item.label, style = DSType.tabLabel) },
                colors = NavigationRailItemDefaults.colors(
                    selectedIconColor = c.accentText,
                    selectedTextColor = c.accentText,
                    indicatorColor = c.accentSoft,
                    unselectedIconColor = c.textSecondary,
                    unselectedTextColor = c.textSecondary,
                ),
                modifier = Modifier.testTag(item.tag),
            )
        }
    }
}

@Composable
private fun TabContent(state: AppState, router: Router, wide: Boolean, picking: DocumentPicking) {
    val screen = router.current
    if (screen == null) {
        when (router.tab) {
            Tab.NEARBY -> NearbyScreen(state, wide) { picking.pick(openSend = true) }
            Tab.TRANSFERS -> TransfersScreen(state)
            Tab.SETTINGS -> SettingsScreen(state)
        }
        return
    }
    PushedScreen(onBack = { router.pop() }) {
        when (screen) {
            is Screen.Send -> SendScreen(state, screen.preselected, picking)
            Screen.ConnectByAddress -> ConnectByAddressScreen(state, picking)
            Screen.ThisDevice -> ThisDeviceScreen(state)
            Screen.PairedDevices -> PairedDevicesScreen(state)
            Screen.Pairing -> PairingScreen(state)
            Screen.Licences -> LicencesScreen { router.push(Screen.Licence(it)) }
            is Screen.Licence -> LicenceText(screen.asset)
        }
    }
}

/** The banner at the top of the window. Its text is the app's own (the error catalog), never a peer's. */
@Composable
private fun BannerHost(state: AppState) {
    val banner = state.banner ?: return
    val model = LocalModel.current
    val router = LocalRouter.current
    val scope = LocalAppScope.current
    val confirmations = LocalConfirmations.current
    val action: (() -> Unit)? = banner.action?.let { action ->
        {
            model.dismissBanner()
            when (action) {
                Banner.Action.RETRY_RECEIVING -> scope.launch { model.applyVisibility() }
                Banner.Action.OPEN_EVERYONE -> confirmations.ask(
                    everyoneConfirmation { scope.launch { model.setVisibility(ReceiveVisibility.EVERYONE) } },
                )
                Banner.Action.OPEN_SETTINGS -> router.select(Tab.SETTINGS)
            }
        }
    }
    val label = when (banner.action) {
        Banner.Action.RETRY_RECEIVING -> "Try again"
        Banner.Action.OPEN_EVERYONE -> "Let everyone send for 10 minutes"
        Banner.Action.OPEN_SETTINGS -> "Open Settings"
        null -> null
    }
    Box(
        contentAlignment = Alignment.TopCenter,
        modifier = Modifier
            .fillMaxWidth()
            .windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Top + WindowInsetsSides.Horizontal))
            .padding(horizontal = 16.dp, vertical = 8.dp),
    ) {
        BannerView(
            title = banner.title,
            message = banner.message,
            warning = banner.style == Banner.Style.DANGER,
            actionLabel = label,
            onAction = action,
            onDismiss = { model.dismissBanner() },
            modifier = Modifier
                .widthIn(max = 560.dp)
                .semantics { liveRegion = LiveRegionMode.Polite }
                .testTag("banner"),
        )
    }
}

/**
 * While something is drawn over it ([redirect] is not null): hidden from
 * TalkBack, and keyboard focus that would enter it goes to [redirect]
 * instead (or nowhere).
 */
private fun Modifier.covered(redirect: FocusRequester?): Modifier = if (redirect == null) {
    this
} else {
    this
        .clearAndSetSemantics {}
        .focusProperties { onEnter = { if (!redirect.requestFocus()) cancelFocusChange() } }
        .focusGroup()
}

/**
 * The overlay notices that never block anything (R23): another app's window
 * lies over Dropumi, or a Send button ignored a click because of it. Drawn
 * above the bottom bar; it takes no touches.
 */
@Composable
private fun OverlayNoticeHost(guard: TouchGuard) {
    val notice = guard.notice ?: return
    if (notice == OverlayNotice.ANSWER) return
    BoxWithConstraints(
        contentAlignment = Alignment.BottomCenter,
        modifier = Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.safeDrawing),
    ) {
        val clearOfBar = if (maxWidth >= DSMetrics.wideLayoutMinWidth) 24.dp else 96.dp
        OverlayNoticeText(
            guard,
            notice,
            color = Color.White,
            modifier = Modifier
                .padding(start = 16.dp, end = 16.dp, bottom = clearOfBar)
                .widthIn(max = 480.dp)
                .clip(RoundedCornerShape(14.dp))
                .background(DS.colors.navy)
                .padding(horizontal = 14.dp, vertical = 10.dp),
        )
    }
}
