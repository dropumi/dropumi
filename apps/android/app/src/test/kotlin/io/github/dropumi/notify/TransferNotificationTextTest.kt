// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.notify

import io.github.dropumi.appstate.TransferPhase
import io.github.dropumi.appstate.TransferRecord
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.PeerInfo
import io.github.dropumi.ffi.Progress
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

class TransferNotificationTextTest {
    private val tricky = "%s %n <b>x</b> [a](b)"
    private val peer = PeerInfo("ZYXWVUTSRQPONMLKJIHGFEDCBA765432ZYXWVUTSRQPONMLKJIHG", "ZYXW-VUTS-RQPO-NMLK-JIHG-FEDC", true, "Maya", tricky)

    private fun record(
        direction: Direction,
        files: List<String> = listOf("Beach-trip.mov"),
        progress: Progress? = null,
        id: String = "r",
    ) = TransferRecord(
        id = id,
        direction = direction,
        operation = null,
        peer = peer,
        targetName = "Maya's phone",
        fileNames = files,
        totalBytes = 0u,
        progress = progress,
        phase = if (progress == null) TransferPhase.Connecting else TransferPhase.Transferring,
        startedAtMs = 0,
    )

    /** Counts and sizes only: the peer's alias, claimed name and fingerprint and the file names never appear. */
    @Test
    fun transferNotificationNeverShowsPeerOrFileNames() {
        val active = listOf(
            record(Direction.INCOMING, listOf("Beach-trip.mov", "Lease.pdf"), Progress(5_000_000u, 10_000_000u, 0u, 2u), "a"),
            record(Direction.OUTGOING, listOf(tricky), null, "b"),
        )
        val content = TransferNotificationText.of(active)
        val words = content.title + "\n" + content.text
        for (secret in listOf("Maya", tricky, "Beach-trip", "Lease", "Maya's phone", "ZYXW")) {
            assertFalse("names $secret: $words", words.contains(secret))
        }
        assertEquals("2 transfers", content.title)
    }

    @Test
    fun transferNotificationSaysWhatMovesAndHowMuch() {
        val receiving = TransferNotificationText.of(
            listOf(record(Direction.INCOMING, listOf("a", "b", "c"), Progress(4_100_000u, 18_200_000u, 0u, 3u))),
        )
        assertEquals("Receiving 3 files", receiving.title)
        assertEquals("4.1 MB of 18.2 MB", receiving.text)
        assertEquals(225, receiving.progress)

        val sending = TransferNotificationText.of(listOf(record(Direction.OUTGOING)))
        assertEquals("Sending 1 file", sending.title)
        assertEquals("Waiting…", sending.text)
        assertNull("no progress yet: indeterminate", sending.progress)

        assertEquals(TransferNotificationText.PUBLIC_TITLE, TransferNotificationText.of(emptyList()).title)
    }
}
