// SPDX-License-Identifier: MPL-2.0
//
// Gives each UI state test a fresh fake graph set up for its scenario,
// before the test launches MainActivity. Runner arguments apply to a whole
// run, so the scenario is chosen here, per test: the rule's default, or the
// test's own @UseScenario. Chain it outside the Compose rule:
//
//     RuleChain.outerRule(FakeScenarioRule()).around(createEmptyComposeRule())

package io.github.dropumi

import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.testsupport.FakeCoreClient
import io.github.dropumi.testsupport.Scenario
import org.junit.rules.ExternalResource
import org.junit.runner.Description
import org.junit.runners.model.Statement

/** The scenario a test starts from (FakeScenarioRule). */
@Target(AnnotationTarget.FUNCTION)
@Retention(AnnotationRetention.RUNTIME)
annotation class UseScenario(val value: Scenario)

/** [realServices]: the model starts the app's real services instead of recording the starts. */
class FakeScenarioRule(
    private val default: Scenario = Scenario.NEARBY_EMPTY,
    private val realServices: Boolean = false,
) : ExternalResource() {
    private var scenario = default

    val app: TestApplication
        get() = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as TestApplication

    /** The fake core of the graph this test runs on. */
    val core: FakeCoreClient get() = app.fakeCore

    override fun apply(base: Statement, description: Description): Statement {
        scenario = description.getAnnotation(UseScenario::class.java)?.value ?: default
        return super.apply(base, description)
    }

    override fun before() {
        // Key presses in an earlier test leave the display out of touch mode
        // (it is global per display); each test starts as a touch user would.
        InstrumentationRegistry.getInstrumentation().setInTouchMode(true)
        app.resetFakeGraph(scenario, realServices)
    }
}
