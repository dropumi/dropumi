// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core.files

import io.github.dropumi.ffi.CallbackException
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ContentUriFileTest {
    private val uri = "content://com.example.files/document/primary%3ADCIM%2FIMG_0001.jpg"

    private fun file(doc: FakeDoc, guard: DocumentGuard = PolicyGuard()): Pair<ContentUriFile, FakeDocumentSource> {
        val source = FakeDocumentSource(mapOf(uri to doc))
        val created = ContentUriFile.create(source, uri, guard) as ContentUriFile.Created.Allowed
        return created.file to source
    }

    @Test
    fun displayNameFallsBackToSharedFileNeverUri() {
        for (raw in listOf(null, "", "   ", "\u0000\u0001\u001f", "a/b/", "dir\\")) {
            val (f, _) = file(FakeDoc(DocumentMeta(displayName = raw)))
            assertEquals("$raw", ContentUriFile.FALLBACK_NAME, f.displayName())
        }
        val (named, _) = file(FakeDoc(DocumentMeta(displayName = "../../Holiday\u0007.jpg")))
        assertEquals("Holiday.jpg", named.displayName())
        // A failing query also gives the fallback, never a part of the URI.
        val (failing, _) = file(FakeDoc(failure = RuntimeException(uri)))
        assertEquals(ContentUriFile.FALLBACK_NAME, failing.displayName())
        for (part in listOf("IMG_0001", "primary", "com.example", "document")) {
            assertTrue(failing.displayName().contains(part).not())
        }
    }

    /** Steps 1-2 run before anything: a refused URI is never queried or opened. */
    @Test
    fun refusedUrisAreNeverQueriedOrOpened() {
        for (bad in listOf(
            "file:///data/user/0/io.github.dropumi/no_backup/dropumi/identity.key",
            "content://io.github.dropumi.fileprovider/received/a.jpg",
            "content://0@io.github.dropumi.fileprovider/received/a.jpg",
            "content://io.github.dropumi.fileprovider:1/received/a.jpg",
            "android.resource://io.github.dropumi/raw/x",
        )) {
            val source = FakeDocumentSource(mapOf(bad to FakeDoc(DocumentMeta("x", 1))))
            val created = ContentUriFile.create(source, bad, PolicyGuard())
            assertTrue(bad, created is ContentUriFile.Created.Refused)
            assertTrue(bad, source.calls.isEmpty())
        }
    }

    @Test
    fun sizeComesFromTheProviderElseFromAGuardedRegularFile() {
        val (sized, _) = file(FakeDoc(DocumentMeta("a", size = 42)))
        assertEquals(42uL, sized.size())
        val (stat, source) = file(FakeDoc(DocumentMeta("a"), bytes = ByteArray(7)))
        assertEquals(7uL, stat.size())
        assertTrue("the descriptor opened for fstat is closed", "close $uri" in source.calls)
        val (pipe, _) = file(FakeDoc(DocumentMeta("a"), regular = false))
        assertNull("unknown size: it is staged instead", pipe.size())
        val key = FileId(9, 9)
        val (protected, _) = file(FakeDoc(DocumentMeta("a"), fileId = key), PolicyGuard(protected = setOf(key)))
        assertNull(protected.size())
    }

    @Test
    fun modifiedIsInSeconds() {
        val (f, _) = file(FakeDoc(DocumentMeta("a", 1, lastModifiedMs = 1_760_000_123_456)))
        assertEquals(1_760_000_123L, f.modified())
        val (none, _) = file(FakeDoc(DocumentMeta("a", 1)))
        assertNull(none.modified())
    }

    @Test
    fun readerReadsTheGuardedDescriptor() {
        val bytes = ByteArray(300_000) { (it % 251).toByte() }
        val (f, _) = file(FakeDoc(DocumentMeta("a", bytes.size.toLong()), bytes = bytes))
        val reader = f.openReader()
        val out = java.io.ByteArrayOutputStream()
        while (true) {
            val chunk = reader.read(65_536u)
            if (chunk.isEmpty()) break
            assertTrue(chunk.size <= 65_536)
            out.write(chunk)
        }
        reader.closeDocument()
        assertArrayEquals(bytes, out.toByteArray())
    }

    /** The fd that would be read is the identity key (st_dev, st_ino): refused before any read. */
    @Test
    fun protectedDescriptorIsRefusedBeforeAnyRead() {
        val key = FileId(64_768, 101)
        val (f, source) = file(FakeDoc(DocumentMeta("innocent.jpg", 32), ByteArray(32), fileId = key), PolicyGuard(protected = setOf(key)))
        val e = runCatching { f.openReader() }.exceptionOrNull()
        assertEquals(ContentUriFile.UNREADABLE, (e as CallbackException.Failed).detail)
        assertTrue(source.calls.none { it.startsWith("read") })
        assertTrue("closed after the refusal", "close $uri" in source.calls)
    }
}
