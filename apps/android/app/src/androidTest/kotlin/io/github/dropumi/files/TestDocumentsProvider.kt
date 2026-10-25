// SPDX-License-Identifier: MPL-2.0
//
// An in-process document provider for the UriGuard tests. It is attached to
// a MockContentResolver in the test, so it runs in the app's process under
// the app's UID (a provider declared in the test manifest would run as the
// test package and could not open the app's private files).

package io.github.dropumi.files

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.content.pm.ProviderInfo
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import android.test.mock.MockContentResolver
import java.io.File
import java.io.FileNotFoundException
import java.io.IOException
import java.util.Collections
import java.util.concurrent.ConcurrentHashMap
import kotlin.concurrent.thread

class TestDocumentsProvider : ContentProvider() {
    /** A document: a file, or bytes written through a pipe (a descriptor of unknown size). */
    class Doc(val name: String?, val size: Long?, val file: File? = null, val pipe: ByteArray? = null)

    private val docs = ConcurrentHashMap<String, Doc>()
    val calls: MutableList<String> = Collections.synchronizedList(mutableListOf())

    fun add(id: String, doc: Doc): String {
        docs[id] = doc
        return "content://$AUTHORITY/$id"
    }

    override fun onCreate() = true

    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, sort: String?): Cursor {
        calls += "query ${uri.lastPathSegment}"
        val doc = docs[uri.lastPathSegment] ?: return MatrixCursor(arrayOf(OpenableColumns.DISPLAY_NAME))
        return MatrixCursor(arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)).apply {
            addRow(arrayOf<Any?>(doc.name, doc.size))
        }
    }

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        calls += "open ${uri.lastPathSegment}"
        val doc = docs[uri.lastPathSegment] ?: throw FileNotFoundException("no such document")
        val bytes = doc.pipe ?: return ParcelFileDescriptor.open(doc.file, ParcelFileDescriptor.MODE_READ_ONLY)
        val (read, write) = ParcelFileDescriptor.createPipe()
        thread(isDaemon = true, name = "test-pipe") {
            try {
                ParcelFileDescriptor.AutoCloseOutputStream(write).use { it.write(bytes) }
            } catch (_: IOException) {
                // The reader closed its end (a refused document).
            }
        }
        return read
    }

    override fun getType(uri: Uri): String? = null
    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun delete(uri: Uri, selection: String?, args: Array<out String>?) = 0
    override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<out String>?) = 0

    companion object {
        const val AUTHORITY = "io.github.dropumi.test.documents"

        /** A provider attached in-process, and a resolver that reaches it. */
        fun attach(context: Context): Pair<TestDocumentsProvider, MockContentResolver> {
            val provider = TestDocumentsProvider()
            provider.attachInfo(context, ProviderInfo().apply { authority = AUTHORITY })
            val resolver = MockContentResolver(context).apply { addProvider(AUTHORITY, provider) }
            return provider to resolver
        }
    }
}
