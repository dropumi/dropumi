// SPDX-License-Identifier: MPL-2.0
//
// UriGuard on the device, with the real core's configuration folder
// (temporary, never the app's own) and in-process providers.

package io.github.dropumi.files

import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.dropumi.core.files.AndroidDocumentSource
import io.github.dropumi.core.files.AndroidDocuments
import io.github.dropumi.core.files.ContentUriFile
import io.github.dropumi.core.files.DocumentMeta
import io.github.dropumi.core.files.DocumentSource
import io.github.dropumi.core.files.Documents
import io.github.dropumi.core.files.OpenDocument
import io.github.dropumi.core.files.StagingCopy
import io.github.dropumi.core.files.UriGuard
import io.github.dropumi.core.files.UriGuardPolicy
import io.github.dropumi.ffi.CallbackException
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.SendItem
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ffi.TransferStatus
import io.github.dropumi.realcore.TestCore
import io.github.dropumi.realcore.pair
import io.github.dropumi.realcore.targetContext
import java.io.File
import java.util.Collections
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class DocumentGuardTest {
    private val core = TestCore()
    private val attached = TestDocumentsProvider.attach(targetContext)
    private val provider = attached.first
    private val resolver = attached.second
    private val guard = UriGuard(targetContext, listOf(core.paths.configDir, core.paths.appDataDir))
    private val source = AndroidDocumentSource(resolver)
    private val identityKey: File

    init {
        core.client.identity()
        identityKey = File(core.paths.configDir, "identity.key")
        check(identityKey.isFile)
    }

    @After
    fun tearDown() = core.close()

    /** Records every query and open, whatever the scheme. */
    private class Recording(private val inner: DocumentSource) : DocumentSource {
        val calls: MutableList<String> = Collections.synchronizedList(mutableListOf())

        override fun query(uri: String): DocumentMeta {
            calls += "query $uri"
            return inner.query(uri)
        }

        override fun open(uri: String): OpenDocument {
            calls += "open $uri"
            return inner.open(uri)
        }
    }

    private fun staging(src: DocumentSource = source) = StagingCopy(src, guard, core.paths.outgoingDir) { Long.MAX_VALUE }

    private fun stagingFailure(job: StagingCopy.Job) =
        (runCatching { job.await(30_000) }.exceptionOrNull() as StagingCopy.StagingException).failure

    private fun allowed(uri: String) = (ContentUriFile.create(source, uri, guard) as ContentUriFile.Created.Allowed).file

    private fun assertUnreadable(file: ContentUriFile) {
        val e = runCatching { file.openReader() }.exceptionOrNull()
        assertEquals(ContentUriFile.UNREADABLE, (e as CallbackException.Failed).detail)
    }

    /**
     * A hard link to the identity key under an innocent name. Where the
     * image refuses links inside app data, the test fails with that reason
     * (it is never skipped); the no-link variants below run everywhere.
     */
    private fun linkToIdentityKey(): File {
        val link = File(core.root, "innocent.jpg")
        try {
            Os.link(identityKey.path, link.path)
        } catch (e: ErrnoException) {
            fail("not runnable on this image (link ${OsConstants.errnoName(e.errno)})")
        }
        return link
    }

    @Test
    fun uriGuardSchemeCheckedAtOpen() {
        val recording = Recording(source)
        val fileProvider = targetContext.packageName + ".fileprovider"
        val startup = targetContext.packageName + ".androidx-startup"
        val readable = File(core.root, "readable.txt").apply { writeText("not to be sent") }
        val refused = listOf("file://${readable.path}") + listOf(fileProvider, startup).flatMap { own ->
            listOf("content://$own/received/x", "content://0@$own/received/x", "content://$own:1/x", "content://0@$own:1/x")
        }
        for (uri in refused) {
            assertTrue(uri, ContentUriFile.create(recording, uri, guard) is ContentUriFile.Created.Refused)
            assertTrue(uri, stagingFailure(staging(recording).start(uri, "x")) is StagingCopy.Failure.Refused)
            assertEquals(uri, null, AndroidDocuments(recording, guard, staging(recording)).describe(uri))
        }
        assertTrue("no query and no open: ${recording.calls.size}", recording.calls.isEmpty())
    }

    @Test
    fun uriGuardRefusesFdOpenedOnIdentityKey() {
        val uri = provider.add("key", TestDocumentsProvider.Doc("holiday.jpg", 32, file = identityKey))
        assertUnreadable(allowed(uri))
    }

    @Test
    fun stagingRunsUriGuardOnFdOpenedOnIdentityKey() {
        val uri = provider.add("key", TestDocumentsProvider.Doc("holiday.jpg", null, file = identityKey))
        assertEquals(
            StagingCopy.Failure.Refused(UriGuardPolicy.Refusal.PROTECTED_FILE),
            stagingFailure(staging().start(uri, "holiday.jpg")),
        )
        assertTrue("nothing was copied", core.paths.outgoingDir.list().isNullOrEmpty())
    }

    @Test
    fun uriGuardRefusesHardLinkToIdentityKey() {
        val uri = provider.add("link", TestDocumentsProvider.Doc("innocent.jpg", 32, file = linkToIdentityKey()))
        assertUnreadable(allowed(uri))
    }

    @Test
    fun stagingRunsUriGuard() {
        val uri = provider.add("link", TestDocumentsProvider.Doc("innocent.jpg", null, file = linkToIdentityKey()))
        assertTrue(stagingFailure(staging().start(uri, "innocent.jpg")) is StagingCopy.Failure.Refused)
        assertTrue("nothing was copied", core.paths.outgoingDir.list().isNullOrEmpty())
    }

    @Test
    fun uriGuardRefusesLockAndTempFiles() {
        val config = core.paths.configDir
        assertTrue(".lock exists", File(config, ".lock").isFile)
        File(config, ".dropumi-cfg-test01").writeBytes(ByteArray(8))
        File(config, ".dropumi-key-test01").writeBytes(ByteArray(32))
        File(core.paths.appDataDir, "history.json").writeText("{}")
        for (file in listOf(
            File(config, ".lock"),
            File(config, ".dropumi-cfg-test01"),
            File(config, ".dropumi-key-test01"),
            File(core.paths.appDataDir, "history.json"),
        )) {
            val uri = provider.add(file.name, TestDocumentsProvider.Doc("a.bin", 8, file = file))
            assertUnreadable(allowed(uri))
        }
    }

    @Test
    fun twoNodesLoopbackTransferWithContentUri() {
        TestCore().use { receiver ->
            receiver.client.identity()
            pair(core, receiver)
            val port = receiver.receiveFromPaired()
            val bytes = ByteArray(300_000) { (it % 251).toByte() }
            val file = File(core.root, "doc.bin").apply { writeBytes(bytes) }
            val uri = provider.add("doc", TestDocumentsProvider.Doc("Holiday photo.jpg", bytes.size.toLong(), file = file))
            core.client.send(SendTarget.Address("127.0.0.1:$port", null), listOf(SendItem.Stream(allowed(uri))), false)
            assertEquals(TransferStatus.COMPLETED, core.finished(Direction.OUTGOING).status)
            assertEquals(TransferStatus.COMPLETED, receiver.finished(Direction.INCOMING).status)
            val received = File(receiver.paths.receiveDir, "Holiday photo.jpg")
            assertArrayEquals(bytes, received.readBytes())
        }
    }

    @Test
    fun unknownSizeDocumentIsStagedAsPath() {
        TestCore().use { receiver ->
            receiver.client.identity()
            pair(core, receiver)
            val port = receiver.receiveFromPaired()
            val bytes = ByteArray(40_000) { (it % 13).toByte() }
            val uri = provider.add("pipe", TestDocumentsProvider.Doc("From a pipe.txt", null, pipe = bytes))
            val prepared = AndroidDocuments(source, guard, staging()).prepare(uri) as Documents.Prepared.Staged
            assertEquals(core.paths.outgoingDir.canonicalFile, prepared.folder.canonicalFile.parentFile)
            core.client.send(SendTarget.Address("127.0.0.1:$port", null), listOf(prepared.item), false)
            assertEquals(TransferStatus.COMPLETED, core.finished(Direction.OUTGOING).status)
            assertEquals(TransferStatus.COMPLETED, receiver.finished(Direction.INCOMING).status)
            assertArrayEquals(bytes, File(receiver.paths.receiveDir, "From a pipe.txt").readBytes())
        }
    }
}
