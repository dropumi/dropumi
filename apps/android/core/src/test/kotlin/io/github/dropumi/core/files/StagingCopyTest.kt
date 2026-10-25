// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core.files

import java.io.File
import java.nio.file.Files
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class StagingCopyTest {
    private val root: File = Files.createTempDirectory("dropumi-staging").toFile()
    private val outgoing = File(root, "outgoing").also { it.mkdirs() }
    private val uri = "content://com.example.files/document/42"
    private val plenty = StagingCopy.RESERVE_BYTES + 1024L * 1024 * 1024
    private val release = CountDownLatch(1)

    @After
    fun cleanUp() {
        release.countDown()
        root.deleteRecursively()
    }

    private fun staging(doc: FakeDoc, free: Long = plenty, guard: DocumentGuard = PolicyGuard()) =
        FakeDocumentSource(mapOf(uri to doc)).let { source -> source to StagingCopy(source, guard, outgoing) { free } }

    private fun failure(job: StagingCopy.Job): StagingCopy.Failure =
        (runCatching { job.await(10_000) }.exceptionOrNull() as StagingCopy.StagingException).failure

    @Test
    fun stagingNameCannotTraverse() {
        val fallback = ContentUriFile.FALLBACK_NAME
        assertEquals("x", StagingCopy.stagingName("../x"))
        assertEquals("b", StagingCopy.stagingName("a/b"))
        assertEquals("b", StagingCopy.stagingName("a\\b"))
        assertEquals(fallback, StagingCopy.stagingName(".."))
        assertEquals(fallback, StagingCopy.stagingName("."))
        assertEquals(fallback, StagingCopy.stagingName(""))
        assertEquals(fallback, StagingCopy.stagingName(null))
        assertEquals(fallback, StagingCopy.stagingName("a/"))
        assertEquals("ab", StagingCopy.stagingName("a\u0000b"))
        assertEquals(fallback, StagingCopy.stagingName("\u0000"))
        assertEquals(fallback, StagingCopy.stagingName("x".repeat(300)))
        assertEquals("é".repeat(127), StagingCopy.stagingName("é".repeat(127)))
        assertEquals(fallback, StagingCopy.stagingName("é".repeat(128)))
    }

    @Test
    fun stagedCopyIsASingleFileInItsOwnFolder() {
        val bytes = ByteArray(200_000) { (it % 7).toByte() }
        val (_, staging) = staging(FakeDoc(DocumentMeta("../photo.jpg"), bytes, regular = false))
        val staged = staging.start(uri, "../photo.jpg").await(10_000)
        assertEquals(outgoing.canonicalFile, staged.folder.canonicalFile.parentFile)
        assertEquals(File(staged.folder, "photo.jpg"), staged.file)
        assertArrayEquals(bytes, staged.file.readBytes())
        assertEquals(bytes.size.toLong(), staged.size)
    }

    @Test
    fun stagingCopyStopsAtCap() {
        assertEquals(StagingCopy.MAX_BYTES, StagingCopy.capFor(Long.MAX_VALUE))
        assertEquals(10L, StagingCopy.capFor(StagingCopy.RESERVE_BYTES + 10))
        assertTrue(StagingCopy.capFor(StagingCopy.RESERVE_BYTES - 1) < 0)

        val (_, capped) = staging(FakeDoc(bytes = ByteArray(11), regular = false), free = StagingCopy.RESERVE_BYTES + 10)
        assertEquals(StagingCopy.Failure.TooLarge, failure(capped.start(uri, "a.bin")))
        val (_, exact) = staging(FakeDoc(bytes = ByteArray(10), regular = false), free = StagingCopy.RESERVE_BYTES + 10)
        assertEquals(10L, exact.start(uri, "a.bin").await(10_000).size)
        val (source, full) = staging(FakeDoc(bytes = ByteArray(1), regular = false), free = StagingCopy.RESERVE_BYTES)
        assertEquals(StagingCopy.Failure.TooLarge, failure(full.start(uri, "a.bin")))
        assertTrue("nothing opened without room", source.calls.none { it.startsWith("open") })
        assertEquals("only the finished copy is left", 1, outgoing.list()?.size)
    }

    /** Cancel returns at once even if the provider never answers, and leaves no partial copy. */
    @Test
    fun stagingCopyCancelAbandonsReader() {
        val (source, staging) = staging(FakeDoc(bytes = ByteArray(10), regular = false, block = release))
        val job = staging.start(uri, "a.bin")
        assertTrue(source.readStarted.await(10, TimeUnit.SECONDS))
        job.cancel()
        assertEquals(StagingCopy.Failure.Cancelled, failure(job))
        assertTrue("closed on cancel", "close $uri" in source.calls)
        assertTrue("no partial copy", outgoing.list().isNullOrEmpty())
    }

    @Test
    fun stagingRunsUriGuardBeforeAnyByteIsRead() {
        val key = FileId(64_768, 101)
        val (source, staging) = staging(FakeDoc(bytes = ByteArray(10), regular = false, fileId = key), guard = PolicyGuard(protected = setOf(key)))
        assertEquals(StagingCopy.Failure.Refused(UriGuardPolicy.Refusal.PROTECTED_FILE), failure(staging.start(uri, "a.bin")))
        assertTrue(source.calls.none { it.startsWith("read") })
        assertTrue(outgoing.list().isNullOrEmpty())

        val file = "file:///data/user/0/io.github.dropumi/no_backup/dropumi/identity.key"
        val (fileSource, fileStaging) = FakeDocumentSource(mapOf(file to FakeDoc())).let { it to StagingCopy(it, PolicyGuard(), outgoing) { plenty } }
        assertEquals(StagingCopy.Failure.Refused(UriGuardPolicy.Refusal.NOT_CONTENT), failure(fileStaging.start(file, "a")))
        assertTrue("never opened", fileSource.calls.isEmpty())
    }
}
