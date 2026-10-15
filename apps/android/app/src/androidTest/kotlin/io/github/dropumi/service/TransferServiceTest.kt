// SPDX-License-Identifier: MPL-2.0
//
// The real TransferService (ADR-0013 D7), started by the app model through
// AndroidServiceStarter while MainActivity is in front. The transfers
// themselves come from the fake core (a pushed OfferReceived and
// TransferAccepted, then TransferFinished), so these results speak for the
// service and its start, not for the real core's transfers. The service is
// observed in `dumpsys activity services`, as the system records it. They
// start a foreground service, so the CI emulator job leaves them out.
package io.github.dropumi.service

import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.ParcelFileDescriptor
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.FakeScenarioRule
import io.github.dropumi.LocalOrDevice
import io.github.dropumi.MainActivity
import io.github.dropumi.TransferService
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.TransferServiceState
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.TransferStatus
import io.github.dropumi.testsupport.Fixtures
import io.github.dropumi.testsupport.Scenario
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
@LocalOrDevice
class TransferServiceTest {
    private val fake = FakeScenarioRule(Scenario.NEARBY_EMPTY, realServices = true)
    private val compose = createEmptyComposeRule()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(fake).around(compose)

    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    private val model: AppModel get() = requireNotNull(fake.app.graph.loader.model)

    /** A failed test may leave the service running: stop it, so the next test starts clean. */
    @After
    fun stopLeftoverService() {
        if (serviceRecord().isEmpty()) return
        instrumentation.targetContext.stopService(Intent(instrumentation.targetContext, TransferService::class.java))
    }

    /** Runs [command] as the shell user and returns what it printed. */
    private fun shell(command: String): String {
        val pfd = instrumentation.uiAutomation.executeShellCommand(command)
        return ParcelFileDescriptor.AutoCloseInputStream(pfd).bufferedReader().use { it.readText() }
    }

    /** TransferService's record in `dumpsys activity services`; empty while it does not run. */
    private fun serviceRecord(): String {
        val component = "${instrumentation.targetContext.packageName}/${TransferService::class.java.name}"
        val lines = shell("dumpsys activity services $component").lines()
        val start = lines.indexOfFirst { it.trim().startsWith("* ServiceRecord{") && it.contains("TransferService") }
        if (start < 0) return ""
        val end = (start + 1 until lines.size).firstOrNull { lines[it].trim().startsWith("* ") } ?: lines.size
        return lines.subList(start, end).joinToString("\n")
    }

    /** Launches MainActivity and waits for the model to receive (its event collector runs then). */
    private fun launch(): ActivityScenario<MainActivity> {
        val scenario = ActivityScenario.launch(MainActivity::class.java)
        compose.waitUntil(TIMEOUT_MS) { fake.app.graph.loader.model?.state?.value?.receiver != null }
        compose.waitUntil(TIMEOUT_MS) { compose.onAllNodesWithTag("tab.nearby").fetchSemanticsNodes().isNotEmpty() }
        return scenario
    }

    /** An auto-accepted receive starts while Dropumi is in front: the model starts the service. */
    private fun transferStarts() {
        fake.core.emit(
            NodeEvent.OfferReceived("tt", Fixtures.peer(name = "Maya's phone", trusted = true, alias = "Maya"), 1u, 10u),
            NodeEvent.TransferAccepted(null, "tt"),
        )
        compose.waitUntil(TIMEOUT_MS) { model.state.value.transferService != TransferServiceState.OFF }
        assertEquals(TransferServiceState.RUNNING, model.state.value.transferService)
        compose.waitUntil(TIMEOUT_MS) { serviceRecord().contains("isForeground=true") }
    }

    private fun transferEnds() {
        fake.core.emit(NodeEvent.TransferFinished(null, Direction.INCOMING, Fixtures.result(TransferStatus.COMPLETED, "tt")))
        compose.waitUntil(TIMEOUT_MS) { model.state.value.transferService == TransferServiceState.OFF }
        // The service stops itself once the model no longer wants it.
        compose.waitUntil(TIMEOUT_MS) { serviceRecord().isEmpty() }
    }

    /**
     * Started in the foreground as a dataSync service, with the Transfers
     * channel's private notification; it stops itself when the transfer
     * ends. API 29+ reports the type the service went to the foreground
     * with; API 26-28 have no service types, and there the test checks the
     * foreground start only (this is the plan's transferServiceStartsOnApi26
     * when it runs on those images).
     */
    @Test
    fun transferServiceStartsWithDataSyncType() {
        launch().use {
            transferStarts()
            val record = serviceRecord()
            assertTrue(record, record.contains("isForeground=true"))
            assertTrue(record, record.contains("channel=transfers"))
            assertTrue(record, record.contains("vis=PRIVATE"))
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                val type = TYPE.find(record)?.groupValues?.get(1)?.toInt(16)
                assertEquals(record, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC, type)
            }
            transferEnds()
        }
    }

    /** START_NOT_STICKY: the system records that the service is not restarted after its process dies. */
    @Test
    fun servicesAreNotSticky() {
        launch().use {
            transferStarts()
            val record = serviceRecord()
            assertTrue(record, record.contains("stopIfKilled=true"))
            transferEnds()
        }
    }

    private companion object {
        const val TIMEOUT_MS = 15_000L

        /** The service's foreground types: `types=` on API 34+, `foregroundServiceType=` on API 29-33. */
        val TYPE = Regex("""(?:\btypes|foregroundServiceType)=0x([0-9a-fA-F]+)""")
    }
}
