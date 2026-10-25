// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.ui.screens

import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.Appearance
import io.github.dropumi.appstate.HistoryEntry
import io.github.dropumi.appstate.PairingSession
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.appstate.TransferPhase
import io.github.dropumi.appstate.TransferRecord
import io.github.dropumi.core.discovery.AdvertiseState
import io.github.dropumi.ffi.AcceptMode
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.NearbyPeer
import io.github.dropumi.ffi.PairingResult
import io.github.dropumi.ffi.PeerInfo
import io.github.dropumi.ffi.Progress
import io.github.dropumi.ffi.ReceivePolicy
import io.github.dropumi.ffi.ReceiverInfo
import java.time.LocalDateTime
import java.time.ZoneOffset
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ScreenTextTest {
    private val tricky = "%s %n <b>x</b> [a](b)"

    private fun state(
        visibility: ReceiveVisibility = ReceiveVisibility.PAIRED_ONLY,
        receiver: ReceiverInfo? = null,
        advertising: AdvertiseState = AdvertiseState.GateClosed,
        history: List<HistoryEntry> = emptyList(),
    ) = AppState(
        visibility = visibility,
        autoAcceptPaired = false,
        appearance = Appearance.SYSTEM,
        onboarded = true,
        receiver = receiver,
        advertising = advertising,
        history = history,
    )

    private fun entry(finishedAtMs: Long, outcome: HistoryEntry.Outcome = HistoryEntry.Outcome.COMPLETED) = HistoryEntry(
        id = "e$finishedAtMs",
        outgoing = false,
        peerName = tricky,
        peerVerified = false,
        fileNames = listOf(tricky, "b.txt"),
        totalBytes = 10u,
        outcome = outcome,
        summary = "You declined",
        finishedAtMs = finishedAtMs,
        savedAs = emptyList(),
    )

    /** ADR-0004: a nearby device is at most "looks like" a paired one; nothing on Nearby says "Paired". */
    @Test
    fun nearbySubtitleNeverSaysPaired() {
        val looksLike = NearbyPeer("i1", "Maya's phone", "ZYXWVUTS", true, "Maya")
        val unknown = NearbyPeer("i2", "Maya's phone", null, true, null)
        val old = NearbyPeer("i3", "Maya's phone", "ZYXWVUTS", false, "Maya")
        assertEquals("Looks like Maya", ScreenText.nearbySubtitle(looksLike))
        assertEquals("Not paired", ScreenText.nearbySubtitle(unknown))
        assertEquals("Needs a newer version of Dropumi", ScreenText.nearbySubtitle(old))
        for (peer in listOf(looksLike, unknown, old)) {
            assertFalse(ScreenText.nearbySubtitle(peer).startsWith("Paired"))
        }
    }

    /** Peer strings go in verbatim: no format string, no markup (ADR-0013 D8). */
    @Test
    fun peerStringsAreInsertedLiterally() {
        assertEquals("Looks like $tricky", ScreenText.nearbySubtitle(NearbyPeer("i", "n", null, true, tricky)))
        val record = TransferRecord(
            id = "out-1",
            direction = Direction.OUTGOING,
            operation = 1u,
            peer = PeerInfo("F", "F", false, null, tricky),
            targetName = tricky,
            fileNames = listOf(tricky),
            totalBytes = 1u,
            phase = TransferPhase.Connecting,
            startedAtMs = 0,
        )
        assertEquals(tricky, ScreenText.transferTitle(record))
        assertEquals("Sending to Unverified device", ScreenText.transferPeerLine(record))
        assertEquals("$tricky and 1 more", ScreenText.historyTitle(entry(0)))
        assertTrue(ScreenText.historySubtitle(entry(0), ZoneOffset.UTC, Locale.US).startsWith("Received from $tricky · "))
    }

    @Test
    fun visibilityTextFollowsTheReceiver() {
        val now = 1_000_000L
        assertEquals("Starting…", ScreenText.visibilityLabel(state(), now))
        assertEquals("Not receiving", ScreenText.visibilityLabel(state(ReceiveVisibility.OFF), now))
        val paired = ReceiverInfo(47321u, null, ReceivePolicy.TrustedOnly, AcceptMode.PROMPT, null)
        assertEquals("Paired devices only", ScreenText.visibilityLabel(state(receiver = paired), now))
        val everyone = ReceiverInfo(47321u, null, ReceivePolicy.Everyone(10u), AcceptMode.PROMPT, (now + 125_000).toULong())
        assertEquals("Everyone · 2:05", ScreenText.visibilityLabel(state(ReceiveVisibility.EVERYONE, everyone), now))
        // Browse-only devices are not listed nearby, so the caption does not say others see the name.
        assertFalse(ScreenText.visibilityCaption(state(), now).contains("see this device's name"))
        assertTrue(ScreenText.visibilityCaption(state(advertising = AdvertiseState.Registered), now).contains("see this device's name"))
        assertTrue(ScreenText.visibilityCaption(state(), now).endsWith("Only while Dropumi is open."))
    }

    /** While this device's pairing window is open it replaces the receiver: the pill says so (no "window"). */
    @Test
    fun visibilityTextSaysReadyToPairWhileTheWindowIsOpen() {
        val now = 1_000_000L
        val window = PairingSession(7u, PairingSession.Role.Window, port = 47321u)
        assertEquals("Ready to pair", ScreenText.visibilityLabel(state().copy(pairing = window), now))
        assertTrue(ScreenText.readyToPair(state().copy(pairing = window)))
        // Not once it has ended, and never for a request.
        val ended = window.copy(result = PairingResult.Cancelled)
        assertEquals("Starting…", ScreenText.visibilityLabel(state().copy(pairing = ended), now))
        val request = PairingSession(8u, PairingSession.Role.Initiator("Maya's phone"))
        assertFalse(ScreenText.readyToPair(state().copy(pairing = request)))
    }

    /** TalkBack hears the time left in whole minutes, so the value changes once a minute. */
    @Test
    fun countdownIsSpokenInWholeMinutes() {
        assertEquals("5 minutes left", minutesLeft(300))
        assertEquals("4 minutes left", minutesLeft(299))
        assertEquals("1 minute left", minutesLeft(119))
        assertEquals("1 minute left", minutesLeft(60))
        assertEquals("Less than a minute left", minutesLeft(59))
        assertEquals("Less than a minute left", minutesLeft(0))
    }

    @Test
    fun transferTextShowsProgressOnlyWhenKnown() {
        val record = TransferRecord(
            id = "in-aa",
            direction = Direction.INCOMING,
            operation = null,
            targetName = "Maya",
            fileNames = emptyList(),
            fileCount = 3,
            totalBytes = 2_000_000u,
            phase = TransferPhase.Transferring,
            startedAtMs = 0,
        )
        assertEquals("3 files", ScreenText.transferTitle(record))
        assertEquals("Starting…", ScreenText.transferStatus(record))
        assertEquals("", ScreenText.transferRate(record))
        val moving = record.recorded(Progress(1_000_000u, 2_000_000u, 0u, 3u), 0)
        assertEquals("1 MB of 2 MB", ScreenText.transferStatus(moving))
        assertEquals("50%", ScreenText.transferRate(moving))
    }

    @Test
    fun finishedTodayCountsOnlyToday() {
        val zone = ZoneOffset.UTC
        val now = LocalDateTime.of(2026, 10, 15, 12, 0).toInstant(zone).toEpochMilli()
        val today = LocalDateTime.of(2026, 10, 15, 0, 1).toInstant(zone).toEpochMilli()
        val yesterday = LocalDateTime.of(2026, 10, 14, 23, 59).toInstant(zone).toEpochMilli()
        val s = state(history = listOf(entry(today), entry(yesterday)))
        assertEquals("0 active · 1 finished today", ScreenText.transfersSubtitle(s, now, zone))
    }

    @Test
    fun fingerprintIsSpokenGroupByGroup() {
        assertEquals("A B C D, E F G H", ScreenText.spokenFingerprint("ABCDEFGH"))
    }
}
