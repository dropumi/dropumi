// SPDX-License-Identifier: MPL-2.0
//
// UI state tests (fake core): the main screens in each scenario, the
// phone and tablet layouts, large text, and the Accessibility Test
// Framework's checks (touch targets, contrast, labels) on Nearby, the
// unverified offer, the code screen and Transfers. MainActivity runs on the
// fake core with temporary folders; these results say nothing about the
// real core, the network or OS permissions.
package io.github.dropumi.ui

import android.os.Build
import android.provider.Settings
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.accessibility.enableAccessibilityChecks
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
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
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.testsupport.FakeCoreClient.Call
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

    private fun textExists(text: String) = compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty()

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
            compose.onAllNodesWithText("Not paired").assertCountEquals(2)
            compose.onAllNodesWithText("Paired").assertCountEquals(0)
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
                    "They can still send to it after pairing, or by address.",
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

    private companion object {
        const val TIMEOUT_MS = 10_000L
        const val TABLET_DP = 600

        /** What the core would report as its own random name. */
        const val CORE_NAME = "Dropumi-7Q4X"
    }
}
