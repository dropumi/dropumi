// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.testsupport

import io.github.dropumi.appstate.AppLoader
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.HistoryStore
import io.github.dropumi.appstate.MemorySettingsStore
import io.github.dropumi.ffi.ConfigProblem
import io.github.dropumi.ffi.DropumiException
import java.io.File
import java.nio.file.Files
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AppLoaderTest {
    private val root: File = Files.createTempDirectory("dropumi-loader").toFile()
    private val scopes = mutableListOf<CoroutineScope>()

    @After
    fun cleanUp() {
        scopes.forEach { it.cancel() }
        root.deleteRecursively()
    }

    /** A scope on the test's scheduler that advanceUntilIdle() runs and that ends with the test. */
    private fun TestScope.appScope(): CoroutineScope =
        CoroutineScope(SupervisorJob() + StandardTestDispatcher(testScheduler)).also(scopes::add)

    private fun TestScope.model(core: FakeCoreClient, onboarded: Boolean = true) = AppModel(
        core,
        MemorySettingsStore(sdkInt = 36).apply { this.onboarded = onboarded },
        HistoryStore(File(root, "history.json")),
        appScope(),
        StandardTestDispatcher(testScheduler),
    )

    @Test
    fun loaderOpensTheCoreOnceAndStartsTheModel() = runTest {
        val core = FakeCoreClient()
        var opened = 0
        val loader = AppLoader(appScope(), StandardTestDispatcher(testScheduler)) {
            opened++
            model(core)
        }
        loader.load()
        loader.load()
        advanceUntilIdle()
        loader.load()
        advanceUntilIdle()
        assertEquals(1, opened)
        assertTrue(loader.state.value is AppLoader.State.Ready)
        assertEquals("Dropumi-1A2B", loader.model?.state?.value?.identity?.name)
        assertEquals(1, core.calls.count { it is FakeCoreClient.Call.StartReceiving })
    }

    /** A failure shows catalog text only (never exception text) and can be retried. */
    @Test
    fun loaderFailureShowsCatalogTextAndRetries() = runTest {
        val core = FakeCoreClient()
        var attempts = 0
        val loader = AppLoader(appScope(), StandardTestDispatcher(testScheduler)) {
            attempts++
            if (attempts == 1) throw DropumiException.Config(ConfigProblem.INSECURE_PERMISSIONS, "/data/user/0/x/no_backup/dropumi")
            model(core)
        }
        loader.load()
        advanceUntilIdle()
        val failed = loader.state.value as AppLoader.State.Failed
        assertFalse(failed.message.contains("/data/"))
        loader.retry()
        advanceUntilIdle()
        assertTrue(loader.state.value is AppLoader.State.Ready)
    }

    /** A model created while the app is in the background does not receive until it is in front. */
    @Test
    fun loaderStartsAModelInTheBackgroundWithoutReceiving() = runTest {
        val core = FakeCoreClient()
        val loader = AppLoader(appScope(), StandardTestDispatcher(testScheduler)) { model(core) }
        loader.isInBackground = true
        loader.load()
        advanceUntilIdle()
        assertTrue(core.calls.none { it is FakeCoreClient.Call.StartReceiving })
        loader.model?.becomeActive()
        assertEquals(1, core.calls.count { it is FakeCoreClient.Call.StartReceiving })
    }
}
