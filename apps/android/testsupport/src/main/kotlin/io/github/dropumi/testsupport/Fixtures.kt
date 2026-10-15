// SPDX-License-Identifier: MPL-2.0

package io.github.dropumi.testsupport

import io.github.dropumi.ffi.CodePurpose
import io.github.dropumi.ffi.ErrorCode
import io.github.dropumi.ffi.NearbyPeer
import io.github.dropumi.ffi.OfferedFile
import io.github.dropumi.ffi.PeerInfo
import io.github.dropumi.ffi.Prompt
import io.github.dropumi.ffi.TransferResult
import io.github.dropumi.ffi.TransferStatus

/** Builders for test fixtures. */
object Fixtures {
    const val PEER_FINGERPRINT = "ZYXWVUTSRQPONMLKJIHGFEDCBA765432ZYXWVUTSRQPONMLKJIHG"

    fun peer(name: String = "Maya's phone", trusted: Boolean = false, alias: String? = null): PeerInfo =
        PeerInfo(PEER_FINGERPRINT, "ZYXW-VUTS-RQPO-NMLK-JIHG-FEDC", trusted, alias, name)

    fun offer(
        id: ULong = 1u,
        transferId: String = "aa",
        peer: PeerInfo = peer(),
        code: String? = "123456",
        files: List<OfferedFile> = listOf(OfferedFile("Beach-trip.mov", 18_200_000u)),
        fileCount: UInt = files.size.toUInt(),
    ): Prompt = Prompt.Offer(
        id = id,
        transferId = transferId,
        peer = peer,
        code = code,
        files = files,
        fileCount = fileCount,
        totalBytes = files.fold(0uL) { sum, f -> sum + f.size },
        expiresAtMs = (System.currentTimeMillis() + 120_000).toULong(),
    )

    fun code(id: ULong = 2u, operation: ULong? = 1u, code: String = "654321"): Prompt = Prompt.Code(
        id = id,
        operation = operation,
        peer = peer(),
        code = code,
        purpose = CodePurpose.PAIRING,
        expiresAtMs = (System.currentTimeMillis() + 120_000).toULong(),
    )

    fun result(status: TransferStatus, transferId: String? = "aa"): TransferResult = TransferResult(
        status = status,
        transferId = transferId,
        filesStored = if (status == TransferStatus.COMPLETED) 1u else 0u,
        filesFailed = 0u,
        bytes = 10u,
        elapsedMs = 5u,
        rejectCause = null,
        cancelledBy = null,
        cancelCause = null,
        error = if (status == TransferStatus.FAILED) ErrorCode.CONNECTION_LOST else null,
    )

    fun nearby(name: String, instance: String, hintAlias: String? = null): NearbyPeer =
        NearbyPeer(instance, name, "ZYXWVUTS", true, hintAlias)
}
