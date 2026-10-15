// SPDX-License-Identifier: MPL-2.0
//
// What Transfers shows about leaving the app (ADR-0013 D7). UI state tests
// (fake core): the transfer is pushed by the test and the service starts
// are recorded by FakeServiceStarter, which answers as the test sets it;
// nothing here starts a real service.
package io.github.dropumi.ui

import androidx.compose.ui.test.assertTextEquals
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.dropumi.FakeScenarioRule
import io.github.dropumi.MainActivity
import io.github.dropumi.appstate.ServiceStart
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.testsupport.FakeServiceStarter
import io.github.dropumi.testsupport.Fixtures
import io.github.dropumi.testsupport.Scenario
import io.github.dropumi.ui.screens.ScreenText
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class TransfersBackgroundTest {
    private val fake = FakeScenarioRule(Scenario.NEARBY_EMPTY)
    private val compose = createEmptyComposeRule()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(fake).around(compose)

    private val services: FakeServiceStarter get() = fake.app.services as FakeServiceStarter

    private fun exists(tag: String) = compose.onAllNodesWithTag(tag).fetchSemanticsNodes().isNotEmpty()

    /** Launches MainActivity, starts an auto-accepted receive while in front and opens Transfers. */
    private fun transferOnTransfers(): ActivityScenario<MainActivity> {
        val scenario = ActivityScenario.launch(MainActivity::class.java)
        compose.waitUntil(TIMEOUT_MS) { fake.app.graph.loader.model?.state?.value?.receiver != null && exists("tab.transfers") }
        fake.core.emit(
            NodeEvent.OfferReceived("tt", Fixtures.peer(trusted = true, alias = "Maya"), 1u, 10u),
            NodeEvent.TransferAccepted(null, "tt"),
        )
        compose.onNodeWithTag("tab.transfers").performClick()
        compose.waitUntil(TIMEOUT_MS) { exists("transfer.card") }
        return scenario
    }

    /** Refused: the transfer stays, and its card says it may pause in the background. */
    @Test
    fun refusedServiceShowsMayPauseOnTheCard() {
        services.result = ServiceStart.REFUSED
        transferOnTransfers().use {
            assertTrue(services.starts.isNotEmpty())
            compose.onNodeWithTag("transfer.mayPause").assertTextEquals(ScreenText.TRANSFER_MAY_PAUSE)
            assertTrue(exists("transfer.cancel"))
        }
    }

    /** Started: no note. */
    @Test
    fun runningServiceShowsNoNote() {
        transferOnTransfers().use {
            assertTrue(services.starts.isNotEmpty())
            assertTrue(!exists("transfer.mayPause"))
            assertTrue(!exists("transfers.notifications"))
        }
    }

    /** Notifications that cannot be seen: after the transfer starts, Transfers asks; "Not now" puts it away. */
    @Test
    fun notificationCardAfterTransferStartsAndNotNowHidesIt() {
        services.notificationsVisible = false
        transferOnTransfers().use {
            compose.waitUntil(TIMEOUT_MS) { exists("transfers.notifications") }
            compose.onNodeWithTag("transfers.notifications.dismiss").performClick()
            compose.waitUntil(TIMEOUT_MS) { !exists("transfers.notifications") }
        }
    }

    private companion object {
        const val TIMEOUT_MS = 10_000L
    }
}
