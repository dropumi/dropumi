// SPDX-License-Identifier: MPL-2.0
//
// An in-process DocumentsProvider over a temporary folder: the destination
// of the Storage Access Framework Save tests. Like a real provider, it
// gives a created document a new name when the name is taken.

package io.github.dropumi.files

import android.Manifest
import android.content.Context
import android.content.pm.ProviderInfo
import android.database.Cursor
import android.database.MatrixCursor
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract.Document
import android.provider.DocumentsContract.Root
import android.provider.DocumentsProvider
import android.test.mock.MockContentResolver
import java.io.File
import java.io.FileNotFoundException
import java.util.Collections

class TestTreeProvider(private val folder: File) : DocumentsProvider() {
    /** Documents opened for writing, created, and deleted, by id. */
    val writes: MutableList<String> = Collections.synchronizedList(mutableListOf())
    val created: MutableList<String> = Collections.synchronizedList(mutableListOf())
    val deleted: MutableList<String> = Collections.synchronizedList(mutableListOf())

    /** When set, opening for writing fails (a full or refusing provider). */
    @Volatile
    var refuseWrites = false

    override fun onCreate() = true

    private fun fileOf(id: String): File = if (id == ROOT_ID) folder else File(folder, id)

    private fun MatrixCursor.addDocument(id: String) {
        val file = fileOf(id)
        addRow(
            arrayOf<Any?>(
                id,
                if (id == ROOT_ID) "Test folder" else id,
                if (file.isDirectory) Document.MIME_TYPE_DIR else "application/octet-stream",
                if (file.isDirectory) Document.FLAG_DIR_SUPPORTS_CREATE else Document.FLAG_SUPPORTS_WRITE or Document.FLAG_SUPPORTS_DELETE,
                file.length(),
            ),
        )
    }

    override fun queryRoots(projection: Array<out String>?): Cursor =
        MatrixCursor(arrayOf(Root.COLUMN_ROOT_ID, Root.COLUMN_DOCUMENT_ID, Root.COLUMN_TITLE, Root.COLUMN_FLAGS)).apply {
            addRow(arrayOf<Any?>("root", ROOT_ID, "Test", Root.FLAG_SUPPORTS_CREATE))
        }

    override fun queryDocument(documentId: String, projection: Array<out String>?): Cursor =
        MatrixCursor(COLUMNS).apply { addDocument(documentId) }

    override fun queryChildDocuments(parentDocumentId: String, projection: Array<out String>?, sortOrder: String?): Cursor =
        MatrixCursor(COLUMNS).apply { folder.list().orEmpty().sorted().forEach { addDocument(it) } }

    override fun openDocument(documentId: String, mode: String, signal: CancellationSignal?): ParcelFileDescriptor {
        if ('w' in mode) {
            writes += documentId
            if (refuseWrites) throw FileNotFoundException("this folder refuses writes")
        }
        return ParcelFileDescriptor.open(fileOf(documentId), ParcelFileDescriptor.parseMode(mode))
    }

    override fun createDocument(parentDocumentId: String, mimeType: String, displayName: String): String {
        val dot = displayName.lastIndexOf('.').takeIf { it > 0 } ?: displayName.length
        var name = displayName
        var n = 1
        while (File(folder, name).exists()) {
            name = displayName.substring(0, dot) + " (${n++})" + displayName.substring(dot)
        }
        if (!File(folder, name).createNewFile()) throw FileNotFoundException("not created")
        created += name
        return name
    }

    override fun deleteDocument(documentId: String) {
        deleted += documentId
        fileOf(documentId).delete()
    }

    override fun isChildDocument(parentDocumentId: String, documentId: String) =
        parentDocumentId == ROOT_ID && documentId != ROOT_ID && '/' !in documentId

    companion object {
        const val AUTHORITY = "io.github.dropumi.test.tree"
        const val ROOT_ID = "root"
        private val COLUMNS = arrayOf(
            Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME, Document.COLUMN_MIME_TYPE,
            Document.COLUMN_FLAGS, Document.COLUMN_SIZE,
        )

        /** A provider attached in-process (as DocumentsProvider requires it to be declared), and a resolver for it. */
        fun attach(context: Context, folder: File): Pair<TestTreeProvider, MockContentResolver> {
            val provider = TestTreeProvider(folder)
            provider.attachInfo(
                context,
                ProviderInfo().apply {
                    authority = AUTHORITY
                    exported = true
                    grantUriPermissions = true
                    readPermission = Manifest.permission.MANAGE_DOCUMENTS
                    writePermission = Manifest.permission.MANAGE_DOCUMENTS
                },
            )
            val resolver = MockContentResolver(context).apply { addProvider(AUTHORITY, provider) }
            return provider to resolver
        }
    }
}
