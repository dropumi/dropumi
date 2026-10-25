// SPDX-License-Identifier: MPL-2.0
//
// The interop pass's Application graph (runner argument `dropumi_app
// interop`, ADR-0013 D9): the app's own live graph with the real Rust
// core, on temporary folders. It runs only in that pass (@InteropApp).

package io.github.dropumi.realcore

import android.system.Os
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.dropumi.InteropApp
import io.github.dropumi.TestApplication
import io.github.dropumi.appstate.AppLoader
import io.github.dropumi.core.AppPaths
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
@InteropApp
class InteropRunnerTest {
    /** What a folder holds, as (name, size, modified time); null when it does not exist. */
    private fun snapshot(dir: File): List<Triple<String, Long, Long>>? =
        dir.takeIf { it.exists() }?.listFiles().orEmpty().map { Triple(it.name, it.length(), it.lastModified()) }.sortedBy { it.first }

    /**
     * The real core opens on noBackupFilesDir/interop-<uuid>: its identity
     * key is created there (the fake core writes no files), the folder is
     * 0700, and the app's own configuration folder (noBackupFilesDir/dropumi)
     * is neither created nor changed.
     */
    @Test
    fun interopRunnerUsesPrivateTempConfig() {
        val app = targetContext.applicationContext as TestApplication
        val standard = AppPaths.standard(app)
        val before = snapshot(standard.configDir)
        val graph = app.graph
        val ready = runBlocking {
            withContext(Dispatchers.Main) { graph.loader.load() }
            withTimeout(TIMEOUT_MS) { graph.loader.state.first { it !is AppLoader.State.Loading } }
        }
        assertTrue("the real core opened: $ready", ready is AppLoader.State.Ready)
        // The model reads (and so creates) the identity when it starts.
        runBlocking { withTimeout(TIMEOUT_MS) { (ready as AppLoader.State.Ready).model.state.first { it.identity != null } } }

        val paths = app.interopPaths
        val root = paths.configDir.parentFile!!
        assertEquals(app.noBackupFilesDir.canonicalPath, root.parentFile!!.canonicalPath)
        assertTrue(root.name.startsWith(TestApplication.INTEROP_PREFIX))
        for (dir in listOf(paths.appDataDir, paths.receiveDir, paths.outgoingDir)) {
            assertEquals(root.canonicalPath, dir.parentFile!!.canonicalPath)
        }
        val key = File(paths.configDir, "identity.key")
        assertTrue("the identity was created in the temporary folder", key.isFile)
        assertEquals(MODE_0700, modeOf(paths.configDir))
        assertEquals(MODE_0600, modeOf(key))
        assertEquals(Os.getuid(), Os.lstat(key.path).st_uid)

        assertEquals("the app's configuration folder is untouched", before, snapshot(standard.configDir))
    }

    private companion object {
        const val TIMEOUT_MS = 30_000L
    }
}
