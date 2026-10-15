// SPDX-License-Identifier: MPL-2.0
//
// Finished transfers, kept on this device only (never synced, never sent,
// never backed up: the file lives in noBackupFilesDir/app) and cleared by
// the user.

package io.github.dropumi.appstate

import io.github.dropumi.ffi.TransferStatus
import java.io.File
import java.io.FileOutputStream
import java.io.IOException
import java.util.UUID

data class HistoryEntry(
    val id: String,
    val outgoing: Boolean,
    /** The peer as shown when it ended (alias first). */
    val peerName: String,
    val peerVerified: Boolean,
    val fileNames: List<String>,
    val totalBytes: ULong,
    val outcome: Outcome,
    /** The headline shown for the outcome. */
    val summary: String,
    val finishedAtMs: Long,
    /** For received files: their final names in the receive folder. */
    val savedAs: List<String>,
) {
    enum class Outcome { COMPLETED, PARTIAL, REJECTED, CANCELLED, FAILED }

    companion object {
        fun outcome(status: TransferStatus): Outcome = when (status) {
            TransferStatus.COMPLETED -> Outcome.COMPLETED
            TransferStatus.COMPLETED_WITH_FAILURES -> Outcome.PARTIAL
            TransferStatus.REJECTED -> Outcome.REJECTED
            TransferStatus.CANCELLED -> Outcome.CANCELLED
            TransferStatus.FAILED -> Outcome.FAILED
        }
    }
}

/** A JSON file with at most [limit] entries, newest first. Use it off the main thread. */
class HistoryStore(val file: File, val limit: Int = 200) {
    /** The stored entries; empty if the file is missing, too large or not valid. */
    fun load(): List<HistoryEntry> {
        if (!file.isFile || file.length() > MAX_BYTES) return emptyList()
        return try {
            decode(file.readText()).take(limit)
        } catch (_: IOException) {
            emptyList()
        } catch (_: Json.SyntaxException) {
            emptyList()
        }
    }

    /**
     * Writes [entries] (capped) to a temporary file in the same folder, syncs
     * it and renames it over the history file. Returns false on failure; the
     * old file is then left as it was.
     */
    fun save(entries: List<HistoryEntry>): Boolean {
        val dir = file.parentFile ?: return false
        if (!dir.isDirectory && !dir.mkdirs()) return false
        val temp = File(dir, ".history-" + UUID.randomUUID() + ".tmp")
        return try {
            FileOutputStream(temp).use { out ->
                out.write(encode(entries.take(limit)).toByteArray(Charsets.UTF_8))
                out.fd.sync()
            }
            temp.renameTo(file)
        } catch (_: IOException) {
            false
        } finally {
            if (temp.exists()) temp.delete()
        }
    }

    companion object {
        /** A history file larger than this is not read. */
        const val MAX_BYTES = 4L * 1024 * 1024

        internal fun encode(entries: List<HistoryEntry>): String = Json.write(
            mapOf(
                "version" to 1,
                "entries" to entries.map {
                    mapOf(
                        "id" to it.id,
                        "outgoing" to it.outgoing,
                        "peerName" to it.peerName,
                        "peerVerified" to it.peerVerified,
                        "fileNames" to it.fileNames,
                        "totalBytes" to it.totalBytes,
                        "outcome" to it.outcome.name,
                        "summary" to it.summary,
                        "finishedAtMs" to it.finishedAtMs,
                        "savedAs" to it.savedAs,
                    )
                },
            ),
        )

        /** Decodes the file; an entry that does not have the expected shape is skipped. */
        internal fun decode(text: String): List<HistoryEntry> {
            val root = Json.parse(text) as? Map<*, *> ?: return emptyList()
            if (root["version"] != 1L) return emptyList()
            val entries = root["entries"] as? List<*> ?: return emptyList()
            return entries.mapNotNull { (it as? Map<*, *>)?.let(::entry) }
        }

        private fun entry(map: Map<*, *>): HistoryEntry? {
            fun strings(key: String): List<String>? =
                (map[key] as? List<*>)?.map { it as? String ?: return null }
            val totalBytes = when (val v = map["totalBytes"]) {
                is Long -> if (v >= 0) v.toULong() else return null
                is String -> v.toULongOrNull() ?: return null
                else -> return null
            }
            return HistoryEntry(
                id = map["id"] as? String ?: return null,
                outgoing = map["outgoing"] as? Boolean ?: return null,
                peerName = map["peerName"] as? String ?: return null,
                peerVerified = map["peerVerified"] as? Boolean ?: return null,
                fileNames = strings("fileNames") ?: return null,
                totalBytes = totalBytes,
                outcome = HistoryEntry.Outcome.entries.firstOrNull { it.name == map["outcome"] } ?: return null,
                summary = map["summary"] as? String ?: return null,
                finishedAtMs = map["finishedAtMs"] as? Long ?: return null,
                savedAs = strings("savedAs") ?: return null,
            )
        }
    }
}
