// SPDX-License-Identifier: MPL-2.0
//
// UI state tests (fake core): the main screens in each scenario, the
// phone and tablet layouts, large text, and the Accessibility Test
// Framework's checks (touch targets, contrast, labels) on Nearby, the
// unverified offer, the code screen, Transfers and Pair a device.
// MainActivity runs on the fake core with temporary folders; these results
// say nothing about the real core, the network or OS permissions. In the
// pairing tests the fake core pairs like the core, and every outcome comes
// from an event the test (or the scenario) delivers.
package io.github.dropumi.ui

import android.os.Build
import android.provider.Settings
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.accessibility.enableAccessibilityChecks
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.tryPerformAccessibilityChecks
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.dropumi.DisplayOverride
import io.github.dropumi.FakeScenarioRule
import io.github.dropumi.FontScaleOverride
import io.github.dropumi.MainActivity
import io.github.dropumi.UseScenario
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.PairingFlow
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.ffi.ErrorCode
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.PairingResult
import io.github.dropumi.ffi.PromptCloseReason
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.testsupport.FakeCoreClient.Call
import io.github.dropumi.testsupport.Fixtures
import io.github.dropumi.testsupport.Scenario
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class UiStateTest {
    private val fake = FakeScenarioRule()
    private val compose = createEmptyComposeRule()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(fake).around(compose)

    private val model: AppModel get() = requireNotNull(fake.app.graph.loader.model)

    private fun exists(tag: String) = compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()

    private fun textExists(text: String, substring: Boolean = false) =
        compose.onAllNodesWithText(text, substring = substring).fetchSemanticsNodes().isNotEmpty()

    /** Launches MainActivity and waits until [tag] shows. */
    private fun launchShowing(tag: String): ActivityScenario<MainActivity> {
        val scenario = ActivityScenario.launch(MainActivity::class.java)
        compose.waitUntil(TIMEOUT_MS) { exists(tag) }
        compose.waitForIdle()
        return scenario
    }

    /** Runs the Accessibility Test Framework's checks on the whole window; any error fails the test. */
    private fun checkAccessibility() {
        compose.enableAccessibilityChecks()
        compose.onRoot().tryPerformAccessibilityChecks()
    }

    /** The text in an editable field (the field's own node, not its label). */
    private fun fieldText(tag: String): String? = compose.onNodeWithTag(tag, useUnmergedTree = true)
        .fetchSemanticsNode().config.getOrNull(SemanticsProperties.EditableText)?.text

    /**
     * The name field is prefilled only with the name the core reports (its
     * own random `Dropumi-XXXX`), never with a name Android knows for the
     * device or its owner.
     */
    @Test
    @UseScenario(Scenario.ONBOARDING)
    fun onboardingNamePrefilledFromCoreOnly() {
        fake.core.setDeviceName(CORE_NAME)
        launchShowing("onboarding.next").use {
            compose.onNodeWithText("Just drop it nearby.").assertExists()
            compose.onNodeWithTag("onboarding.next").performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("onboarding.name") }
            compose.waitUntil(TIMEOUT_MS) { fieldText("onboarding.name") == CORE_NAME }
            val context = fake.app
            val systemNames = listOfNotNull(
                Build.MODEL,
                Build.DEVICE,
                Build.PRODUCT,
                Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME),
            ).filter { it.isNotBlank() }
            for (name in systemNames) assertFalse("prefilled with a system name", fieldText("onboarding.name").orEmpty().contains(name))
            compose.onNodeWithText("Anyone on the same network can see it, so avoid your full name.").assertExists()
        }
    }

    /** Onboarding ends on Nearby, and receiving starts only on Start (paired devices only by default). */
    @Test
    @UseScenario(Scenario.ONBOARDING)
    fun onboardingEndsOnNearbyAndReceivesOnlyAfterStart() {
        launchShowing("onboarding.next").use {
            compose.onNodeWithTag("onboarding.next").performClick()
            compose.waitUntil(TIMEOUT_MS) { fieldText("onboarding.name") == fake.core.identity().name }
            compose.onNodeWithTag("onboarding.next").performClick()
            compose.waitUntil(TIMEOUT_MS) { textExists("Find devices on your network") }
            compose.onNodeWithTag("onboarding.next").performClick()
            compose.waitUntil(TIMEOUT_MS) { textExists("Who can send to this device?") }
            assertTrue("nothing received before Start", fake.core.calls.none { it is Call.StartReceiving })
            compose.onNodeWithTag("onboarding.next").performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("tab.nearby") }
            compose.waitUntil(TIMEOUT_MS) { fake.core.calls.any { it is Call.StartReceiving } }
            assertEquals(ReceiveVisibility.PAIRED_ONLY, model.state.value.visibility)
        }
    }

    /** A hint is only a hint: a row says "Looks like ‹alias›" or "Not paired", never "Paired". */
    @Test
    @UseScenario(Scenario.NEARBY)
    fun nearbyHintNeverSaysPaired() {
        launchShowing("nearby.row").use {
            compose.waitUntil(TIMEOUT_MS) { compose.onAllNodesWithTag("nearby.row").fetchSemanticsNodes().size == 3 }
            compose.onNodeWithText("Maya's phone").assertExists()
            compose.onNodeWithText("Looks like Studio Mac").assertExists()
            // Each text in the rows is its own node here, so a "Paired" anywhere
            // in a row's text or labels (alone, or as in "Paired · Studio Mac")
            // is counted: the only ones are the two "Not paired". (The screen's
            // own status pill, "Paired devices only", is outside the rows.)
            val inRow = hasAnyAncestor(hasTestTag("nearby.row"))
            val saysPaired = hasText("paired", substring = true, ignoreCase = true) or
                hasContentDescription("paired", substring = true, ignoreCase = true)
            compose.onAllNodes(inRow and hasText("Not paired"), useUnmergedTree = true).assertCountEquals(2)
            compose.onAllNodes(inRow and saysPaired, useUnmergedTree = true).assertCountEquals(2)
            compose.onNodeWithText("3 devices found · works without internet").assertExists()
        }
    }

    @Test
    @UseScenario(Scenario.NEARBY)
    fun nearbyPassesAccessibilityChecks() {
        launchShowing("nearby.row").use {
            compose.waitUntil(TIMEOUT_MS) { compose.onAllNodesWithTag("nearby.row").fetchSemanticsNodes().size == 3 }
            checkAccessibility()
        }
    }

    @Test
    fun emptyNearbyExplainsWhatToDo() {
        launchShowing("nearby.empty").use {
            compose.onNodeWithText("Looking for devices…").assertExists()
            // Advertising is allowed in this scenario: no "Not listed nearby" card.
            assertFalse(textExists("Not listed nearby"))
        }
    }

    /** Where the R21 gate is closed the device is browse-only, and Nearby says so and how to reach it. */
    @Test
    @UseScenario(Scenario.ADVERTISING_OFF)
    fun advertisingOffExplainsBrowseOnly() {
        launchShowing("nearby.empty").use {
            compose.onNodeWithText("Not listed nearby").assertExists()
            val kind = if (fake.app.resources.configuration.smallestScreenWidthDp >= TABLET_DP) "tablet" else "phone"
            compose.onNodeWithText(
                "Other devices can't see this $kind on this Android version. " +
                    "They can still send to it by typing its address.",
            ).assertExists()
            compose.onNodeWithText("Show my address").performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("device.fingerprint") }
        }
    }

    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun unverifiedOfferPassesAccessibilityChecks() {
        launchShowing("prompt.offer").use {
            compose.onNodeWithTag("code.digits").assertExists()
            checkAccessibility()
        }
    }

    @Test
    @UseScenario(Scenario.CODE_PAIRING)
    fun codeScreenPassesAccessibilityChecks() {
        launchShowing("prompt.code").use {
            compose.onNodeWithTag("code.match").assertExists()
            checkAccessibility()
        }
    }

    @Test
    @UseScenario(Scenario.TRANSFERS)
    fun transfersShowProgressAndHistory() {
        launchShowing("tab.transfers").use {
            compose.onNodeWithTag("tab.transfers").performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("transfer.card") }
            compose.onNodeWithTag("transfer.cancel").assertExists()
            compose.onNodeWithText("Earlier").performScrollTo()
            compose.onNodeWithText("IMG_2041.heic").assertExists()
        }
    }

    @Test
    @UseScenario(Scenario.TRANSFERS)
    fun transfersPassAccessibilityChecks() {
        launchShowing("tab.transfers").use {
            compose.onNodeWithTag("tab.transfers").performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("transfer.card") && exists("history.row") }
            checkAccessibility()
        }
    }

    /**
     * Items the share target handed over open Send with them in the draft;
     * nothing is sent until a device is chosen and Send is tapped.
     */
    @Test
    @UseScenario(Scenario.SHARED_ITEMS)
    fun sharedItemsOpenSend() {
        launchShowing("send.go").use {
            compose.waitUntil(TIMEOUT_MS) { model.state.value.draft.size == 2 }
            compose.onNodeWithText("Choose a device").assertExists()
            assertEquals(
                listOf("describe ${Scenario.SHARED_PHOTO}", "describe ${Scenario.SHARED_NOTES}"),
                fake.app.documents.calls,
            )
            assertTrue(fake.core.calls.none { it is Call.Send })
        }
    }

    /**
     * The activity is launched first and the display changed under it (a
     * launch while the display changes can stall on old images, whose
     * launcher restarts then); the window follows the new width.
     */
    private fun withDisplay(sidePx: Int, dpi: Int, check: () -> Unit) {
        launchShowing("tab.nearby").use {
            DisplayOverride.square(sidePx, dpi).use {
                check()
            }
        }
    }

    /** Under 600 dp: the bottom bar (checked on a 392 dp square display, on any device). */
    @Test
    fun phoneLayoutUsesNavigationBar() {
        withDisplay(sidePx = 1078, dpi = 440) {
            compose.waitUntil(TIMEOUT_MS) { exists("nav.bar") && !exists("nav.rail") }
            compose.onNodeWithTag("tab.settings").performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("settings.visibility") }
        }
    }

    /** 600 dp and wider: the navigation rail (checked on an 800 dp square display, on any device). */
    @Test
    fun tabletLayoutUsesNavigationRail() {
        withDisplay(sidePx = 1600, dpi = 320) {
            compose.waitUntil(TIMEOUT_MS) { exists("nav.rail") && !exists("nav.bar") }
            compose.onNodeWithTag("tab.settings").performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("settings.visibility") }
        }
    }

    /**
     * At a font scale of 2, Decline and Accept are stacked, full width, and
     * both stay on screen; the prompt's content scrolls to the peer's name.
     */
    @Test
    @UseScenario(Scenario.OFFER_UNVERIFIED)
    fun promptButtonsStackAtFontScale2() {
        FontScaleOverride.set(2f).use {
            launchShowing("prompt.offer").use {
                val decline = compose.onNodeWithTag("offer.decline").fetchSemanticsNode().boundsInRoot
                val accept = compose.onNodeWithTag("offer.accept").fetchSemanticsNode().boundsInRoot
                val root = compose.onNodeWithTag("prompt.offer").fetchSemanticsNode().boundsInRoot
                assertTrue("stacked: $decline above $accept", accept.top >= decline.bottom)
                assertEquals("same left edge", decline.left, accept.left, 1f)
                assertEquals("same width", decline.width, accept.width, 1f)
                assertTrue("both on screen", decline.top >= root.top && accept.bottom <= root.bottom)
                compose.onNodeWithTag("prompt.peer").performScrollTo()
                compose.onNodeWithTag("prompt.peer").assertExists()
            }
        }
    }

    // Pair a device (UI state tests, fake core)

    /** Opens Pair a device from Nearby's link and waits until this device is ready to pair. */
    private fun openPairing() {
        compose.onNodeWithTag("nearby.pair").performScrollTo().performClick()
        compose.waitUntil(TIMEOUT_MS) { exists("pair.countdown") }
        compose.waitForIdle()
    }

    /** This device's open window, once it listens. */
    private fun window(): ULong {
        compose.waitUntil(TIMEOUT_MS) { model.state.value.pairing?.port != null }
        return requireNotNull(model.state.value.pairing).operation
    }

    private fun countdownValue(): String? = compose.onNodeWithTag("pair.countdown")
        .fetchSemanticsNode().config.getOrNull(SemanticsProperties.StateDescription)

    /**
     * UI state test (fake core): opening the screen makes this device ready
     * to pair: the countdown (read as whole minutes), the name others see,
     * the address to type, the steps, and the nearby devices with Pair.
     */
    @Test
    @UseScenario(Scenario.PAIRING)
    fun pairingShowsReadyStateAndNearbyDevices() {
        launchShowing("nearby.pair").use {
            openPairing()
            assertEquals(1, fake.core.calls.count { it == Call.OpenPairingWindow })
            compose.onNodeWithText("Other devices see this one as “${fake.core.identity().name}”.").assertExists()
            assertTrue("minutes, not seconds: ${countdownValue()}", countdownValue().orEmpty().matches(Regex("\\d minutes? left")))
            compose.waitUntil(TIMEOUT_MS) { exists("pair.address") }
            compose.onNodeWithText("192.168.1.20:47321").assertExists()
            compose.onNodeWithTag("pair.steps").assertExists()
            compose.onNodeWithTag("pair.list").performScrollTo()
            compose.onAllNodesWithTag("pair.peer").assertCountEquals(2)
            compose.onNodeWithContentDescription("Pair with Maya's phone").assertExists()
            compose.onNodeWithTag("pair.byAddress").performScrollTo()
            compose.onNodeWithText(
                "Other devices may not list this phone. On them, choose Pair by address and type an address shown here.",
            ).assertExists()
            // A row is never "Paired": only "Not paired" or "Looks like".
            val inRow = hasAnyAncestor(hasTestTag("pair.row"))
            compose.onAllNodes(inRow and hasText("Paired", substring = false), useUnmergedTree = true).assertCountEquals(0)
            assertFalse(textExists("window", substring = true))
        }
    }

    /** UI state test (fake core): the ready screen passes the Accessibility Test Framework's checks. */
    @Test
    @UseScenario(Scenario.PAIRING)
    fun pairingPassesAccessibilityChecks() {
        launchShowing("nearby.pair").use {
            openPairing()
            compose.waitUntil(TIMEOUT_MS) { exists("pair.address") }
            checkAccessibility()
        }
    }

    /** UI state test (fake core): at font scale 1.3 the screen is one column and still passes the checks. */
    @Test
    @UseScenario(Scenario.PAIRING)
    fun pairingPassesAccessibilityChecksAtLargeText() {
        FontScaleOverride.set(1.3f).use {
            launchShowing("nearby.pair").use {
                openPairing()
                val ready = compose.onNodeWithTag("pair.ready").fetchSemanticsNode().boundsInRoot
                val steps = compose.onNodeWithTag("pair.steps").fetchSemanticsNode().boundsInRoot
                assertTrue("one column: the steps under the ready card", steps.top >= ready.bottom)
                checkAccessibility()
            }
        }
    }

    /** UI state test (fake core): with nobody nearby the list says what to do on the other device. */
    @Test
    @UseScenario(Scenario.PAIRING_EMPTY)
    fun pairingWithNoDevicesExplainsWhatToDo() {
        launchShowing("nearby.pair").use {
            openPairing()
            compose.onNodeWithTag("pair.empty").performScrollTo()
            compose.onNodeWithText("Open Pair a device on the other device. Both need to be on the same Wi-Fi network.").assertExists()
        }
    }

    /** UI state test (fake core): Pair closes this device's window, then asks that device, with Cancel. */
    @Test
    @UseScenario(Scenario.PAIRING)
    fun tappingPairAsksThatDevice() {
        launchShowing("nearby.pair").use {
            openPairing()
            val window = window()
            compose.onNodeWithContentDescription("Pair with Maya's phone").performScrollTo().performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("pair.status") && fake.core.calls.any { it is Call.Pair } }
            compose.onNodeWithText("Asking “Maya's phone”…").assertExists()
            compose.onNodeWithTag("pair.cancel").assertExists()
            val calls = fake.core.calls
            assertTrue(calls.indexOf(Call.Cancel(window)) in 0 until calls.indexOf(Call.Pair(SendTarget.Nearby("dropumi-00000001"))))
            assertFalse(fake.core.pairCalledWhileWindowOpen)
        }
    }

    /** UI state test (fake core): Stop ends being ready and goes back. */
    @Test
    @UseScenario(Scenario.PAIRING)
    fun stopLeavesThePairingScreen() {
        launchShowing("nearby.pair").use {
            openPairing()
            val window = window()
            compose.onNodeWithTag("pair.stop").performScrollTo().performClick()
            compose.waitUntil(TIMEOUT_MS) { !exists("pair.ready") && exists("nearby.pair") }
            compose.waitUntil(TIMEOUT_MS) { Call.Cancel(window) in fake.core.calls }
            assertEquals(PairingFlow(), model.state.value.pairingFlow)
            assertTrue(model.state.value.banner == null)
        }
    }

    /** UI state test (fake core): the result names the device by its alias, and Done goes back to where pairing was opened. */
    @Test
    @UseScenario(Scenario.PAIRING)
    fun pairedResultNamesTheDeviceAndDoneGoesBack() {
        launchShowing("nearby.pair").use {
            openPairing()
            val window = window()
            fake.core.endWindow(window, PairingResult.Paired(Fixtures.peer(name = "Pixel", trusted = true, alias = "Maya's phone")))
            compose.waitUntil(TIMEOUT_MS) { exists("pair.result.title") }
            compose.onNodeWithText("Paired with “Maya's phone”").assertExists()
            compose.onNodeWithTag("pair.done").performScrollTo().performClick()
            compose.waitUntil(TIMEOUT_MS) { !exists("pair.result") && exists("nearby.pair") }
            assertEquals(1, fake.core.calls.count { it == Call.OpenPairingWindow })
        }
    }

    /** UI state test (fake core): a device that is not ready to pair gets a note that says what to do; this device is ready again. */
    @Test
    @UseScenario(Scenario.PAIRING_NOT_READY)
    fun notReadySaysWhatToDo() {
        launchShowing("nearby.pair").use {
            openPairing()
            window()
            compose.onNodeWithContentDescription("Pair with Maya's phone").performScrollTo().performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("pair.note") && exists("pair.countdown") }
            assertTrue(textExists("“Maya's phone” isn't waiting to pair.", substring = true))
            assertFalse(textExists("Only paired devices", substring = true))
            assertEquals(2, fake.core.calls.count { it == Call.OpenPairingWindow })
        }
    }

    /** UI state test (fake core): "Codes don't match" on this device reads as this user's answer. */
    @Test
    @UseScenario(Scenario.PAIRING_CODES)
    fun codesDontMatchIsAttributedToYou() {
        launchShowing("nearby.pair").use {
            openPairing()
            window()
            compose.onNodeWithContentDescription("Pair with Maya's phone").performScrollTo().performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("prompt.code") }
            compose.onNodeWithTag("code.mismatch").performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("pair.result.title") }
            compose.onNodeWithText("You said the codes don't match").assertExists()
            assertTrue(Call.AnswerCode(Scenario.CODE_PROMPT, false) in fake.core.calls)
        }
    }

    /** UI state test (fake core): the other device not confirming reads as its answer, never a claimed name. */
    @Test
    @UseScenario(Scenario.PAIRING)
    fun otherDeviceDecliningIsAttributedToIt() {
        launchShowing("nearby.pair").use {
            openPairing()
            val window = window()
            val prompt = requireNotNull(fake.core.attemptOnWindow(peer = Fixtures.peer(name = "Mallory's laptop")))
            compose.waitUntil(TIMEOUT_MS) { exists("prompt.code") }
            fake.core.emit(NodeEvent.PromptClosed((prompt as io.github.dropumi.ffi.Prompt.Code).id, PromptCloseReason.WITHDRAWN))
            fake.core.endWindow(window, PairingResult.Failed(ErrorCode.PAIRING_REJECTED))
            compose.waitUntil(TIMEOUT_MS) { exists("pair.result.title") && !exists("prompt.code") }
            compose.onNodeWithText("The other device didn't confirm the code").assertExists()
            assertFalse(textExists("Mallory", substring = true))
        }
    }

    /** UI state test (fake core): a connection that was not a pairing request ends the readiness, with Start again. */
    @Test
    @UseScenario(Scenario.PAIRING)
    fun usedUpReadinessSaysPairingDidNotStart() {
        launchShowing("nearby.pair").use {
            openPairing()
            val window = window()
            fake.core.emit(NodeEvent.SessionClosed(null, "not_authorized"))
            fake.core.endWindow(window, PairingResult.Failed(ErrorCode.NOT_AUTHORIZED))
            compose.waitUntil(TIMEOUT_MS) { exists("pair.result.title") }
            compose.onNodeWithText("Pairing didn't start").assertExists()
            compose.onNodeWithText("Start again").assertExists()
            assertFalse("the screen says it, not a banner", exists("banner"))
            compose.onNodeWithTag("pair.again").performScrollTo().performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("pair.countdown") }
            assertEquals(2, fake.core.calls.count { it == Call.OpenPairingWindow })
        }
    }

    /** UI state test (fake core): the refusal banner offers Pair a device, and opens it. */
    @Test
    fun refusalBannerOpensPairing() {
        launchShowing("nearby.pair").use {
            fake.core.emit(NodeEvent.SessionClosed(null, "not_authorized"))
            compose.waitUntil(TIMEOUT_MS) { exists("banner.action") }
            compose.onNodeWithText("A device was refused").assertExists()
            compose.onNodeWithTag("banner.action").performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("pair.ready") }
            assertFalse(exists("banner"))
        }
    }

    /** UI state test (fake core): with no paired devices, Paired devices offers Pair a device and Add by fingerprint. */
    @Test
    fun emptyPairedDevicesOffersPairing() {
        launchShowing("tab.settings").use {
            compose.onNodeWithTag("tab.settings").performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("settings.paired") }
            compose.onNodeWithTag("settings.paired").performScrollTo().performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("paired.empty") }
            compose.onNodeWithTag("paired.emptyAdd").assertExists()
            compose.onNodeWithTag("paired.pair").assertExists()
            compose.onNodeWithTag("paired.emptyPair").performScrollTo().performClick()
            compose.waitUntil(TIMEOUT_MS) { exists("pair.ready") }
        }
    }

    /** UI state test (fake core): a rotation (the activity recreated) keeps the same window: nothing is closed or opened. */
    @Test
    @UseScenario(Scenario.PAIRING)
    fun recreatingTheActivityKeepsTheSameWindow() {
        launchShowing("nearby.pair").use { scenario ->
            openPairing()
            val window = window()
            scenario.recreate()
            compose.waitUntil(TIMEOUT_MS) { exists("pair.countdown") }
            compose.waitForIdle()
            assertEquals(1, fake.core.calls.count { it == Call.OpenPairingWindow })
            assertFalse(Call.Cancel(window) in fake.core.calls)
            assertEquals(window, fake.core.windowOperation)
        }
    }

    private companion object {
        const val TIMEOUT_MS = 10_000L
        const val TABLET_DP = 600

        /** What the core would report as its own random name. */
        const val CORE_NAME = "Dropumi-7Q4X"
    }
}
