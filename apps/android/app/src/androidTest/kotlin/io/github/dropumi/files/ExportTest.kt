// SPDX-License-Identifier: MPL-2.0
//
// Save, Share and Open of received files (ADR-0013 D4) on the device.

package io.github.dropumi.files

import android.content.ContentUris
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.DocumentsContract
import android.provider.MediaStore
import androidx.core.content.FileProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.filters.SdkSuppress
import io.github.dropumi.ReceivedFileActions
import io.github.dropumi.core.files.ExportResult
import io.github.dropumi.core.files.ReceivedExport
import io.github.dropumi.core.files.ReceivedFile
import io.github.dropumi.realcore.targetContext
import java.io.File
import java.util.UUID
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ExportTest {
    private val root = File(targetContext.noBackupFilesDir, "test-" + UUID.randomUUID()).also { it.mkdirs() }
    private val received = File(root, "received").also { it.mkdirs() }
    private val data = ByteArray(100_000) { (it % 251).toByte() }
    private val mediaRows = mutableListOf<Uri>()

    @After
    fun tearDown() {
        mediaRows.forEach { targetContext.contentResolver.delete(it, null, null) }
        root.deleteRecursively()
    }

    private fun receivedFile(name: String): ReceivedFile {
        File(received, name).writeBytes(data)
        return ReceivedFile(name, data.size.toLong())
    }

    private fun exporter(resolver: android.content.ContentResolver = targetContext.contentResolver) =
        ReceivedExport(resolver, received, sharedFreeSpace = { Long.MAX_VALUE }, mimeLookup = { "text/plain" })

    private fun displayName(uri: Uri): String? =
        targetContext.contentResolver.query(uri, arrayOf(MediaStore.MediaColumns.DISPLAY_NAME), null, null, null)?.use {
            if (it.moveToFirst()) it.getString(0) else null
        }

    /**
     * API 29+: two saves of the same name make two items; the first is never
     * overwritten. Not run below 29 (no MediaStore Downloads; Save uses SAF there).
     */
    @Test
    @SdkSuppress(minSdkVersion = Build.VERSION_CODES.Q)
    fun saveToDownloadsNeverOverwrites() {
        val name = "dropumi-test-${UUID.randomUUID()}.txt"
        val file = receivedFile(name)
        val first = exporter().saveToDownloads(listOf(file)) as ExportResult.Saved
        val second = exporter().saveToDownloads(listOf(file)) as ExportResult.Saved
        val uris = (first.uris + second.uris).map(Uri::parse).also(mediaRows::addAll)
        assertEquals(name, displayName(uris[0]))
        assertNotEquals(name, displayName(uris[1]))
        for (uri in uris) {
            val bytes = targetContext.contentResolver.openInputStream(uri)!!.use { it.readBytes() }
            assertArrayEquals(data, bytes)
            targetContext.contentResolver.query(
                uri,
                arrayOf(MediaStore.MediaColumns.RELATIVE_PATH, MediaStore.MediaColumns.IS_PENDING),
                null, null, null,
            )!!.use {
                assertTrue(it.moveToFirst())
                assertEquals(ReceivedExport.DOWNLOADS_FOLDER, it.getString(0))
                assertEquals("no longer pending", 0, it.getInt(1))
            }
        }
        assertTrue("Save keeps the internal copy", File(received, name).isFile)
    }

    /** SAF (API 26-28's path, runnable on every level): only new documents are written. */
    @Test
    fun saveViaSafNeverOverwrites() {
        val folder = File(root, "picked").also { it.mkdirs() }
        File(folder, "photo.jpg").writeText("already here")
        val (provider, resolver) = TestTreeProvider.attach(targetContext, folder)
        val tree = DocumentsContract.buildTreeDocumentUri(TestTreeProvider.AUTHORITY, TestTreeProvider.ROOT_ID)
        val result = exporter(resolver).saveToFolder(tree.toString(), listOf(receivedFile("photo.jpg"))) as ExportResult.Saved
        assertEquals(1, result.uris.size)
        assertEquals(listOf("photo (1).jpg"), provider.created)
        assertEquals("only the document it created was opened for writing", provider.created, provider.writes)
        assertEquals("already here", File(folder, "photo.jpg").readText())
        assertArrayEquals(data, File(folder, "photo (1).jpg").readBytes())
        assertTrue(File(received, "photo.jpg").isFile)
    }

    @Test
    fun saveFailsAndCleansUpOnSizeMismatch() {
        val file = receivedFile("notes.txt")
        val wrongSize = file.copy(size = file.size + 1)

        // A file that is not what the core reported is never copied anywhere.
        val folder = File(root, "picked").also { it.mkdirs() }
        val (provider, resolver) = TestTreeProvider.attach(targetContext, folder)
        val tree = DocumentsContract.buildTreeDocumentUri(TestTreeProvider.AUTHORITY, TestTreeProvider.ROOT_ID)
        assertEquals(ExportResult.Failed(ExportResult.Reason.CHANGED, 0), exporter(resolver).saveToFolder(tree.toString(), listOf(wrongSize)))
        assertTrue(provider.created.isEmpty())
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val unique = "dropumi-test-${UUID.randomUUID()}.txt"
            val renamed = receivedFile(unique).copy(size = data.size + 1L)
            assertEquals(ExportResult.Failed(ExportResult.Reason.CHANGED, 0), exporter().saveToDownloads(listOf(renamed)))
            val rows = targetContext.contentResolver.query(
                MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY),
                arrayOf(MediaStore.MediaColumns._ID),
                "${MediaStore.MediaColumns.DISPLAY_NAME} = ?",
                arrayOf(unique),
                null,
            )!!.use { c -> List(c.count) { c.moveToPosition(it); ContentUris.withAppendedId(MediaStore.Downloads.getContentUri(MediaStore.VOLUME_EXTERNAL_PRIMARY), c.getLong(0)) } }
            mediaRows += rows
            assertTrue("no row was left", rows.isEmpty())
        }

        // A destination that fails while writing: the created document is deleted again.
        provider.refuseWrites = true
        val failed = exporter(resolver).saveToFolder(tree.toString(), listOf(file))
        assertTrue(failed is ExportResult.Failed)
        assertEquals(provider.created, provider.deleted)
        assertTrue("the internal copy is kept", File(received, "notes.txt").isFile)
        assertTrue("nothing new was left in the folder", folder.list().isNullOrEmpty())
    }

    /** The FileProvider shares received files only, never the configuration or app data. */
    @Test
    fun fileProviderSharesOnlyReceivedFiles() {
        val receiveDir = File(targetContext.filesDir, "received").also { it.mkdirs() }
        val name = "dropumi-test-${UUID.randomUUID()}.txt"
        val file = File(receiveDir, name).also { it.writeBytes(data) }
        try {
            val actions = ReceivedFileActions(targetContext, receiveDir)
            val uri = actions.uriFor(name)!!
            assertEquals(targetContext.packageName + ReceivedFileActions.AUTHORITY_SUFFIX, uri.authority)
            assertArrayEquals(data, targetContext.contentResolver.openInputStream(uri)!!.use { it.readBytes() })
            assertNull(actions.uriFor("../$name"))
            assertNull(actions.uriFor("missing.txt"))
            val open = actions.open(name)!!
            assertEquals(Intent.ACTION_CHOOSER, open.action)
            assertTrue(open.flags and Intent.FLAG_GRANT_READ_URI_PERMISSION != 0)
            val secret = File(targetContext.noBackupFilesDir, "dropumi-test-secret").also { it.writeText("x") }
            try {
                val refused = runCatching {
                    FileProvider.getUriForFile(targetContext, uri.authority!!, secret)
                }.exceptionOrNull()
                assertTrue("noBackupFilesDir is outside the provider's paths", refused is IllegalArgumentException)
            } finally {
                secret.delete()
            }
        } finally {
            file.delete()
        }
    }
}
