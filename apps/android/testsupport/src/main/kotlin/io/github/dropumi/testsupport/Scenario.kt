// SPDX-License-Identifier: MPL-2.0
//
// UI state test scenarios (a port of the Apple app's UITestScenarios): what
// the fake core and the settings hold when MainActivity starts. A rule in
// the app's androidTest applies one to a fresh fake graph before the
// activity launches. Results are labelled "UI state test (fake core)"; they
// say nothing about the real core.

package io.github.dropumi.testsupport

import io.github.dropumi.appstate.SettingsStore
import io.github.dropumi.core.discovery.AdvertiseState
import io.github.dropumi.core.discovery.Advertising
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.ErrorCode
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.OfferedFile
import io.github.dropumi.ffi.PairingResult
import io.github.dropumi.ffi.Progress
import io.github.dropumi.ffi.PromptCloseReason
import io.github.dropumi.ffi.TransferStatus
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Every scenario but [ADVERTISING_OFF] runs with advertising allowed and
 * registered, so its screens show no "Not listed nearby" card.
 */
enum class Scenario {
    /** First launch: onboarding, nothing received or paired. */
    ONBOARDING,

    /** Onboarded, receiving from paired devices, nothing nearby. */
    NEARBY_EMPTY,

    /** Three devices nearby: one not paired, one whose name hints at a paired alias, one more not paired. */
    NEARBY,

    /** As NEARBY_EMPTY, but advertising is not allowed on this Android version (the R21 gate is closed). */
    ADVERTISING_OFF,

    /** An offer from a device that is not paired, with a code to compare. */
    OFFER_UNVERIFIED,

    /** An offer from a paired device (alias "Studio Mac"); no code. */
    OFFER_PAIRED,

    /** A pairing code. */
    CODE_PAIRING,

    /** An offer of 1500 files: the prompt carries 200, the rest are paged from the core. */
    OFFER_1500_FILES,

    /** A finished receive in the history and a receive in progress. */
    TRANSFERS,

    /** Two documents handed over by the share target ([sharedItems]), waiting for a device. */
    SHARED_ITEMS,

    /**
     * Pairing like the core: Maya's phone and a device whose name hints at
     * Studio Mac nearby, and this device on 192.168.1.20. Pair requests are
     * never answered; the test delivers what the other device does.
     */
    PAIRING,

    /** As PAIRING, with no device nearby. */
    PAIRING_EMPTY,

    /** As PAIRING; the device asked is not ready to pair (its pairing ends NOT_AUTHORIZED before a code). */
    PAIRING_NOT_READY,

    /**
     * As PAIRING; the device asked shows a code (prompt 7). Once this device
     * answers it, the core closes the prompt and the pairing ends
     * PAIRING_REJECTED, as it does when anyone says no.
     */
    PAIRING_CODES,
    ;

    /** Content URIs waiting in the share hand-off when the model starts. */
    val sharedItems: List<String>
        get() = if (this == SHARED_ITEMS) listOf(SHARED_PHOTO, SHARED_NOTES) else emptyList()

    /** This device's local IPv4 addresses (the model's localAddresses). */
    val localAddresses: List<String>
        get() = if (isPairing) listOf("192.168.1.20") else emptyList()

    private val isPairing: Boolean
        get() = this == PAIRING || this == PAIRING_EMPTY || this == PAIRING_NOT_READY || this == PAIRING_CODES

    /** The advertiser the model starts with. */
    fun advertising(): Advertising = if (this == ADVERTISING_OFF) Advertising.CLOSED else RegisteredAdvertising()

    /** Sets up [core] and [settings]; the events wait in the fake core for the model's start. */
    fun configure(core: FakeCoreClient, settings: SettingsStore) {
        settings.onboarded = this != ONBOARDING
        if (isPairing) core.scriptPairingLikeTheCore()
        if (isPairing && this != PAIRING_EMPTY) {
            core.emit(
                NodeEvent.PeerFound(Fixtures.nearby("Maya's phone", "dropumi-00000001")),
                NodeEvent.PeerFound(Fixtures.nearby("Studio Mac", "dropumi-00000002", hintAlias = "Studio Mac")),
            )
        }
        when (this) {
            ONBOARDING, NEARBY_EMPTY, ADVERTISING_OFF, SHARED_ITEMS, PAIRING, PAIRING_EMPTY -> Unit
            PAIRING_NOT_READY -> core.emitBeforeReturning("pair") { op ->
                listOf(NodeEvent.PairingFinished(op, PairingResult.Failed(ErrorCode.NOT_AUTHORIZED)))
            }
            PAIRING_CODES -> {
                var request: ULong? = null
                core.emitBeforeReturning("pair") { op ->
                    request = op
                    listOf(NodeEvent.PromptOpened(Fixtures.code(id = CODE_PROMPT, operation = op)))
                }
                core.onCall = { call ->
                    val op = request
                    if (call is FakeCoreClient.Call.AnswerCode && call.promptId == CODE_PROMPT && op != null) {
                        core.emit(
                            NodeEvent.PromptClosed(CODE_PROMPT, PromptCloseReason.ANSWERED),
                            NodeEvent.PairingFinished(op, PairingResult.Failed(ErrorCode.PAIRING_REJECTED)),
                        )
                    }
                }
            }
            NEARBY -> core.emit(
                NodeEvent.PeerFound(Fixtures.nearby("Maya's phone", "dropumi-00000001")),
                NodeEvent.PeerFound(Fixtures.nearby("Studio Mac", "dropumi-00000002", hintAlias = "Studio Mac")),
                NodeEvent.PeerFound(Fixtures.nearby("Pixel 8", "dropumi-00000003")),
            )
            OFFER_UNVERIFIED -> {
                val offer = Fixtures.offer(
                    files = listOf(
                        OfferedFile("Beach-trip.mov", 18_200_000u),
                        OfferedFile("IMG_2041.heic", 4_100_000u),
                        OfferedFile("Lease-draft.pdf", 2_300_000u),
                    ),
                )
                core.emit(
                    NodeEvent.OfferReceived("aa", Fixtures.peer(), 3u, 24_600_000u),
                    NodeEvent.PromptOpened(offer),
                )
            }
            OFFER_PAIRED -> {
                val paired = Fixtures.peer(name = "MacBook", trusted = true, alias = "Studio Mac")
                core.emit(
                    NodeEvent.OfferReceived("bb", paired, 1u, 2_300_000u),
                    NodeEvent.PromptOpened(
                        Fixtures.offer(
                            id = 3u,
                            transferId = "bb",
                            peer = paired,
                            code = null,
                            files = listOf(OfferedFile("Lease-draft.pdf", 2_300_000u)),
                        ),
                    ),
                )
            }
            CODE_PAIRING -> core.emit(NodeEvent.PromptOpened(Fixtures.code()))
            OFFER_1500_FILES -> {
                val inline = (0 until INLINE_FILES).map { OfferedFile("file-$it.bin", 1u) }
                core.emit(NodeEvent.PromptOpened(Fixtures.offer(files = inline, fileCount = 1_500u)))
            }
            TRANSFERS -> {
                val paired = Fixtures.peer(name = "MacBook", trusted = true, alias = "Studio Mac")
                core.emit(
                    NodeEvent.OfferReceived("cc", paired, 1u, 4_100_000u),
                    NodeEvent.TransferAccepted(null, "cc"),
                    NodeEvent.FileSaved("cc", 0u, "IMG_2041.heic", 4_100_000u, "00"),
                    NodeEvent.TransferFinished(null, Direction.INCOMING, Fixtures.result(TransferStatus.COMPLETED, "cc")),
                    NodeEvent.OfferReceived("dd", paired, 1u, 18_200_000u),
                    NodeEvent.TransferAccepted(null, "dd"),
                    NodeEvent.TransferProgress(null, "dd", Progress(11_300_000u, 18_200_000u, 0u, 1u)),
                )
            }
        }
    }

    /** Advertising allowed by the gate and registered. */
    private class RegisteredAdvertising : Advertising {
        override val gateOpen = true
        override val state: StateFlow<AdvertiseState> =
            MutableStateFlow<AdvertiseState>(AdvertiseState.Registered).asStateFlow()
    }

    companion object {
        /** The code prompt PAIRING_CODES opens. */
        val CODE_PROMPT: ULong = 7u

        /** The files an offer prompt carries; the core lists the rest through offerFiles. */
        const val INLINE_FILES = 200

        /** The documents SHARED_ITEMS hands over. */
        const val SHARED_PHOTO = "content://io.github.dropumi.test.documents/document/photo"
        const val SHARED_NOTES = "content://io.github.dropumi.test.documents/document/notes"
    }
}
