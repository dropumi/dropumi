// SPDX-License-Identifier: MPL-2.0
//
// UI state test scenarios (a port of the Apple app's UITestScenarios): what
// the fake core and the settings hold when MainActivity starts. A rule in
// the app's androidTest applies one to a fresh fake graph before the
// activity launches. Results are labelled "UI state test (fake core)"; they
// say nothing about the real core.

package io.github.dropumi.testsupport

import io.github.dropumi.appstate.SettingsStore
import io.github.dropumi.ffi.NodeEvent
import io.github.dropumi.ffi.OfferedFile

enum class Scenario {
    /** Onboarded, receiving from paired devices, nothing nearby. */
    NEARBY_EMPTY,

    /** An offer from a device that is not paired, with a code to compare. */
    OFFER_UNVERIFIED,

    /** An offer from a paired device (alias "Studio Mac"); no code. */
    OFFER_PAIRED,

    /** A pairing code. */
    CODE_PAIRING,

    /** An offer of 1500 files: the prompt carries 200, the rest are paged from the core. */
    OFFER_1500_FILES,
    ;

    /** Sets up [core] and [settings]; the events wait in the fake core for the model's start. */
    fun configure(core: FakeCoreClient, settings: SettingsStore) {
        settings.onboarded = true
        when (this) {
            NEARBY_EMPTY -> Unit
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
        }
    }

    companion object {
        /** The files an offer prompt carries; the core lists the rest through offerFiles. */
        const val INLINE_FILES = 200
    }
}
