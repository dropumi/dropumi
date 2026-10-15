// SPDX-License-Identifier: MPL-2.0
//
// Foreground and background through the real ProcessLifecycleOwner
// (ADR-0013 D7; plan §6.2). UI state tests (fake core): MainActivity runs on
// the fake core and a recording service starter, so these results speak for
// the lifecycle wiring, not for the real core.
package io.github.dropumi.lifecycle

import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.ProcessLifecycleOwner
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.FakeScenarioRule
import io.github.dropumi.MainActivity
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.testsupport.FakeCoreClient.Call
import io.github.dropumi.testsupport.FakeServiceStarter
import io.github.dropumi.testsupport.Scenario
import java.util.Collections
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ProcessLifecycleTest {
    private val fake = FakeScenarioRule(Scenario.NEARBY_EMPTY)
    private val compose = createEmptyComposeRule()

    @get:Rule
    val rules: RuleChain = RuleChain.outerRule(fake).around(compose)

    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()

    private val model: AppModel get() = requireNotNull(fake.app.graph.loader.model)

    /** The process lifecycle's events while the test runs. */
    private val events: MutableList<Lifecycle.Event> = Collections.synchronizedList(mutableListOf())
    private val recorder = LifecycleEventObserver { _, event -> events += event }

    @Before
    fun record() {
        instrumentation.runOnMainSync { ProcessLifecycleOwner.get().lifecycle.addObserver(recorder) }
        // addObserver replays the current state; only what follows counts.
        events.clear()
    }

    @After
    fun stopRecording() {
        instrumentation.runOnMainSync { ProcessLifecycleOwner.get().lifecycle.removeObserver(recorder) }
    }

    private fun stopAccepting() = fake.core.calls.count { it == Call.StopAccepting }

    /** Launches MainActivity and waits until the model receives. */
    private fun launch(): ActivityScenario<MainActivity> {
        val scenario = ActivityScenario.launch(MainActivity::class.java)
        compose.waitUntil(TIMEOUT_MS) { fake.app.graph.loader.model?.state?.value?.receiver != null }
        return scenario
    }

    /**
     * A configuration change (here a recreation, as rotation does) never
     * reaches the process lifecycle: no ON_PAUSE or ON_STOP, so accepting
     * never stops and the advert stays. ProcessLifecycleOwner drops its
     * pending pause as soon as the new activity resumes, so nothing can
     * arrive later either.
     */
    @Test
    fun configChangeDoesNotStopAccepting() {
        launch().use { scenario ->
            events.clear()
            scenario.recreate()
            instrumentation.waitForIdleSync()
            assertEquals(Lifecycle.State.RESUMED, scenario.state)
            var process: Lifecycle.State? = null
            instrumentation.runOnMainSync { process = ProcessLifecycleOwner.get().lifecycle.currentState }
            assertEquals(Lifecycle.State.RESUMED, process)
            assertTrue("process events during the recreation: $events", events.none { it == Lifecycle.Event.ON_PAUSE || it == Lifecycle.Event.ON_STOP })
            assertEquals(0, stopAccepting())
            assertFalse(model.state.value.isInBackground)
            assertNotNull(model.state.value.receiver)
            // Pausing for a recreation starts no service either.
            assertTrue((fake.app.services as FakeServiceStarter).starts.isEmpty())
        }
    }

    /**
     * Leaving the app (the activity stops) sends ON_STOP a moment later:
     * accepting stops once. Coming back sends ON_START; the stopping
     * receiver is left alone until the core reports it stopped.
     */
    @Test
    fun leavingTheAppStopsAcceptingOnce() {
        launch().use { scenario ->
            events.clear()
            scenario.moveToState(Lifecycle.State.CREATED)
            compose.waitUntil(TIMEOUT_MS) { Lifecycle.Event.ON_STOP in events }
            compose.waitUntil(TIMEOUT_MS) { stopAccepting() > 0 }
            assertTrue(model.state.value.isInBackground)
            scenario.moveToState(Lifecycle.State.RESUMED)
            compose.waitUntil(TIMEOUT_MS) { Lifecycle.Event.ON_START in events && !model.state.value.isInBackground }
            instrumentation.waitForIdleSync()
            assertEquals(1, stopAccepting())
            assertTrue(fake.core.calls.none { it is Call.StopReceiving || it is Call.SetReceivePolicy })
        }
    }

    private companion object {
        const val TIMEOUT_MS = 10_000L
    }
}
