// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.appstate

import java.io.File
import java.nio.file.Files
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class HistoryStoreTest {
    private val root: File = Files.createTempDirectory("dropumi-history").toFile()

    @After
    fun cleanUp() {
        root.deleteRecursively()
    }

    private fun entry(id: String, names: List<String> = listOf("a.txt")) = HistoryEntry(
        id = id,
        outgoing = false,
        peerName = "\"Maya\" \\ <b>x</b> %s \u0000  😀",
        peerVerified = false,
        fileNames = names,
        totalBytes = ULong.MAX_VALUE,
        outcome = HistoryEntry.Outcome.PARTIAL,
        summary = "1 of 2 files saved",
        finishedAtMs = 1_760_000_000_000,
        savedAs = listOf("a (1).txt"),
    )

    /** Peer and file names are kept verbatim, whatever they contain. */
    @Test
    fun historyRoundTripsEveryFieldVerbatim() {
        val store = HistoryStore(File(root, "app/history.json"))
        val entries = listOf(entry("1", listOf("x\ny.txt", "")), entry("2"))
        assertTrue(store.save(entries))
        assertEquals(entries, store.load())
    }

    @Test
    fun unreadableHistoryIsEmptyNotACrash() {
        val file = File(root, "history.json")
        val store = HistoryStore(file)
        for (text in listOf("", "{", "[]", "{\"version\":2,\"entries\":[]}", "{\"version\":1,\"entries\":[1]}", "\u0000")) {
            file.writeText(text)
            assertEquals(text, emptyList<HistoryEntry>(), store.load())
        }
        // Deep nesting is refused, not recursed into.
        file.writeText("[".repeat(10_000))
        assertEquals(emptyList<HistoryEntry>(), store.load())
    }

    @Test
    fun oversizedHistoryIsNotRead() {
        val file = File(root, "history.json")
        val store = HistoryStore(file)
        assertTrue(store.save(listOf(entry("1"))))
        file.appendText(" ".repeat(HistoryStore.MAX_BYTES.toInt()))
        assertEquals(emptyList<HistoryEntry>(), store.load())
    }

    @Test
    fun saveLeavesNoTemporaryFiles() {
        val store = HistoryStore(File(root, "app/history.json"))
        repeat(3) { assertTrue(store.save(listOf(entry("$it")))) }
        assertEquals(listOf("history.json"), File(root, "app").list()?.toList())
    }
}
