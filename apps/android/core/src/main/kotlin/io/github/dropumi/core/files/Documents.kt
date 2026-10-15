// SPDX-License-Identifier: MPL-2.0
//
// What AppModel needs to send content:// documents: their name and size for
// the draft, and the item to send. A document of known size is sent as a
// stream the core reads through ContentUriFile; one of unknown size is
// staged and sent as a path. Both paths run UriGuard.

package io.github.dropumi.core.files

import io.github.dropumi.ffi.SendItem
import java.io.File
import java.io.IOException

interface Documents {
    data class Described(val name: String, val size: ULong?)

    sealed interface Prepared {
        data class Stream(val item: SendItem.Stream) : Prepared

        /** Send [item]; delete [folder] when the transfer ends. */
        data class Staged(val item: SendItem.Path, val folder: File) : Prepared
    }

    /** The document's name and size, or null if UriGuard refuses it. Blocks; call it off the main thread. */
    fun describe(uri: String): Described?

    /** The item to send. Blocks while a copy is staged; throws [DocumentException]. */
    fun prepare(uri: String): Prepared
}

class DocumentException(val reason: Reason) : IOException("the document cannot be sent") {
    enum class Reason { REFUSED, TOO_LARGE, UNREADABLE }
}

class AndroidDocuments(
    private val source: DocumentSource,
    private val guard: DocumentGuard,
    private val staging: StagingCopy,
) : Documents {
    override fun describe(uri: String): Documents.Described? =
        when (val created = ContentUriFile.create(source, uri, guard)) {
            is ContentUriFile.Created.Refused -> null
            is ContentUriFile.Created.Allowed -> Documents.Described(created.file.displayName(), created.file.size())
        }

    override fun prepare(uri: String): Documents.Prepared {
        val file = when (val created = ContentUriFile.create(source, uri, guard)) {
            is ContentUriFile.Created.Refused -> throw DocumentException(DocumentException.Reason.REFUSED)
            is ContentUriFile.Created.Allowed -> created.file
        }
        if (file.size() != null) return Documents.Prepared.Stream(SendItem.Stream(file))
        val job = staging.start(uri, file.displayName())
        val staged = try {
            job.await()
        } catch (e: InterruptedException) {
            job.cancel()
            throw e
        } catch (e: StagingCopy.StagingException) {
            throw DocumentException(
                when (e.failure) {
                    is StagingCopy.Failure.Refused -> DocumentException.Reason.REFUSED
                    StagingCopy.Failure.TooLarge -> DocumentException.Reason.TOO_LARGE
                    else -> DocumentException.Reason.UNREADABLE
                },
            )
        }
        return Documents.Prepared.Staged(SendItem.Path(staged.file.path), staged.folder)
    }
}
