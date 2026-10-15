// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core.files

import java.io.IOException
import java.net.URI
import java.util.Collections
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** A document a test provider serves. */
class FakeDoc(
    val meta: DocumentMeta = DocumentMeta(),
    val bytes: ByteArray = ByteArray(0),
    val fileId: FileId = FileId(1, nextInode()),
    val procPath: String = "/storage/emulated/0/Download/x",
    val regular: Boolean = true,
    /** When set, reads block until it opens (a provider pipe that never answers). */
    val block: CountDownLatch? = null,
    /** Thrown by query and by every read when set. */
    val failure: RuntimeException? = null,
) {
    companion object {
        private var inode = 1000L

        @Synchronized
        fun nextInode() = ++inode
    }
}

/** Serves [docs] by URI and records every query, open and read. */
class FakeDocumentSource(private val docs: Map<String, FakeDoc>) : DocumentSource {
    val calls: MutableList<String> = Collections.synchronizedList(mutableListOf())
    val readStarted = CountDownLatch(1)

    override fun query(uri: String): DocumentMeta {
        calls += "query $uri"
        val doc = docs[uri] ?: throw IOException("no such document")
        doc.failure?.let { throw it }
        return doc.meta
    }

    override fun open(uri: String): OpenDocument {
        calls += "open $uri"
        val doc = docs[uri] ?: throw IOException("no such document")
        return object : OpenDocument {
            private var position = 0

            override fun fileId() = doc.fileId
            override fun procPath() = doc.procPath
            override fun regularFileSize(): Long? = if (doc.regular) doc.bytes.size.toLong() else null

            override fun read(buffer: ByteArray, offset: Int, length: Int): Int {
                calls += "read $uri"
                readStarted.countDown()
                doc.failure?.let { throw it }
                doc.block?.let { if (!it.await(30, TimeUnit.SECONDS)) throw IOException("never released") }
                if (position >= doc.bytes.size) return -1
                val n = minOf(length, doc.bytes.size - position)
                System.arraycopy(doc.bytes, position, buffer, offset, n)
                position += n
                return n
            }

            override fun close() {
                calls += "close $uri"
            }
        }
    }
}

/** UriGuard's rules over java.net.URI (android.net.Uri is not on the host JVM). */
class PolicyGuard(
    private val own: Set<String> = setOf("io.github.dropumi.fileprovider"),
    private val protected: Set<FileId> = emptySet(),
    private val protectedDirs: List<String> = listOf("/data/user/0/io.github.dropumi/no_backup/dropumi"),
) : DocumentGuard {
    override fun checkUri(uri: String): UriGuardPolicy.Refusal? {
        val parsed = try {
            URI(uri)
        } catch (_: Exception) {
            return UriGuardPolicy.Refusal.UNCHECKABLE
        }
        return UriGuardPolicy.checkUri(parsed.scheme, parsed.authority, own)
    }

    override fun checkOpen(document: OpenDocument): UriGuardPolicy.Refusal? =
        UriGuardPolicy.checkFile(document.fileId(), document.procPath(), protected, protectedDirs)
}
