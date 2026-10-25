// SPDX-License-Identifier: MPL-2.0
//
// Copies a document of unknown size into noBackupFilesDir/outgoing/<uuid>/
// so it can be sent as a path (ADR-0013 D6); the Rust path guards then
// apply too. The copy is guarded like any stream item: the scheme and
// own-provider checks first, then UriGuard on the descriptor it reads.
// It runs on a thread of its own and stops with an error at
// min(free space - 256 MiB, 4 GiB). Cancelling closes the descriptor,
// deletes the folder and abandons the thread: a blocking provider-pipe read
// may not return on close, so nothing waits for it.

package io.github.dropumi.core.files

import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

class StagingCopy(
    private val source: DocumentSource,
    private val guard: DocumentGuard,
    private val outgoingDir: File,
    /** Free bytes where [outgoingDir] lives (production: StatFs). */
    private val freeSpace: () -> Long,
) {
    /** A finished copy: send [file]; delete [folder] when the transfer ends. */
    data class Staged(val file: File, val folder: File, val size: Long)

    sealed interface Failure {
        data class Refused(val reason: UriGuardPolicy.Refusal) : Failure
        data object TooLarge : Failure
        data object Unreadable : Failure
        data object Cancelled : Failure
    }

    class StagingException(val failure: Failure) : IOException("the document could not be staged")

    /** A copy under way. */
    inner class Job internal constructor(private val uri: String, private val name: String) {
        private val done = CountDownLatch(1)
        private val cancelled = AtomicBoolean(false)
        private val folder = File(outgoingDir, UUID.randomUUID().toString())

        @Volatile
        private var outcome: Result<Staged>? = null

        @Volatile
        private var document: OpenDocument? = null

        internal fun start() {
            Thread({ run() }, THREAD_NAME).apply { isDaemon = true }.start()
        }

        private fun finish(result: Result<Staged>) {
            if (outcome == null) {
                outcome = result
                done.countDown()
            }
        }

        private fun run() {
            val result = try {
                Result.success(copy())
            } catch (e: StagingException) {
                Result.failure(e)
            } catch (_: Throwable) {
                Result.failure(StagingException(if (cancelled.get()) Failure.Cancelled else Failure.Unreadable))
            }
            if (result.isFailure || cancelled.get()) folder.deleteRecursively()
            finish(if (cancelled.get()) Result.failure(StagingException(Failure.Cancelled)) else result)
        }

        private fun copy(): Staged {
            guard.checkUri(uri)?.let { throw StagingException(Failure.Refused(it)) }
            val cap = capFor(freeSpace())
            if (cap <= 0) throw StagingException(Failure.TooLarge)
            val opened = source.open(uri)
            document = opened
            opened.use { doc ->
                val refusal = try {
                    guard.checkOpen(doc)
                } catch (_: Throwable) {
                    UriGuardPolicy.Refusal.UNCHECKABLE
                }
                // Refused before a single byte is read or copied.
                refusal?.let { throw StagingException(Failure.Refused(it)) }
                if (cancelled.get()) throw StagingException(Failure.Cancelled)
                if (!folder.mkdirs()) throw StagingException(Failure.Unreadable)
                val target = File(folder, name)
                if (target.canonicalFile.parentFile != folder.canonicalFile) throw StagingException(Failure.Unreadable)
                if (!target.createNewFile()) throw StagingException(Failure.Unreadable)
                var total = 0L
                FileOutputStream(target).use { out ->
                    val buffer = ByteArray(BUFFER)
                    while (true) {
                        if (cancelled.get()) throw StagingException(Failure.Cancelled)
                        val n = doc.read(buffer, 0, buffer.size)
                        if (n < 0) break
                        total += n
                        if (total > cap) throw StagingException(Failure.TooLarge)
                        out.write(buffer, 0, n)
                    }
                    out.fd.sync()
                }
                return Staged(target, folder, total)
            }
        }

        /**
         * Waits for the copy; throws [StagingException] if it failed or was
         * cancelled. An interrupt (the caller was cancelled) is passed on;
         * the caller then cancels the copy.
         */
        fun await(): Staged {
            done.await()
            return requireNotNull(outcome).getOrThrow()
        }

        /** [await] for at most [timeoutMs]; the copy is cancelled when the time is up. */
        fun await(timeoutMs: Long): Staged {
            if (!done.await(timeoutMs, TimeUnit.MILLISECONDS)) cancel()
            return requireNotNull(outcome).getOrThrow()
        }

        /** Stops now: closes the descriptor, deletes the folder, and does not wait for the thread. */
        fun cancel() {
            if (!cancelled.compareAndSet(false, true)) return
            document?.let(ContentUriFile::closeQuietly)
            folder.deleteRecursively()
            finish(Result.failure(StagingException(Failure.Cancelled)))
        }
    }

    /** Starts copying [uri], offered under [displayName] (sanitized; never a part of the URI). */
    fun start(uri: String, displayName: String?): Job = Job(uri, stagingName(displayName)).also { it.start() }

    companion object {
        const val RESERVE_BYTES = 256L * 1024 * 1024
        const val MAX_BYTES = 4L * 1024 * 1024 * 1024
        private const val MAX_NAME_BYTES = 255
        private const val BUFFER = 64 * 1024
        private const val THREAD_NAME = "dropumi-staging"

        /** The most a copy may hold: free space less 256 MiB, at most 4 GiB. */
        fun capFor(free: Long): Long = minOf(free - RESERVE_BYTES, MAX_BYTES)

        /**
         * A single, safe file name: the last component, without `/`, `\`, NUL
         * and control characters; `shared-file` if that leaves nothing, `.`,
         * `..` or more than 255 bytes.
         */
        fun stagingName(displayName: String?): String {
            val last = displayName?.substringAfterLast('/')?.substringAfterLast('\\') ?: return ContentUriFile.FALLBACK_NAME
            val clean = last.filterNot { it.isISOControl() || it == '/' || it == '\\' }.trim()
            val unsafe = clean.isEmpty() || clean == "." || clean == ".." ||
                clean.toByteArray(Charsets.UTF_8).size > MAX_NAME_BYTES
            return if (unsafe) ContentUriFile.FALLBACK_NAME else clean
        }
    }
}
