// SPDX-License-Identifier: MPL-2.0
//
// Receiving into the app-private folder with the real core on this image.
// It replaces spike S-AN3's scratch test: identity creation (getrandom and
// the key's no-clobber persist) runs as its own step first, then files are
// finalized with the no-clobber rename (renameat2) into filesDir-style
// storage. A seccomp kill would end the process, so a pass on an image
// shows those calls are allowed there.

package io.github.dropumi.realcore

import android.os.Build
import android.system.Os
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.SendItem
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.ffi.TransferStatus
import java.io.File
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class ReceiveTest {
    private val data = ByteArray(200_000) { (it % 251).toByte() }

    @Test
    fun receivedDirectoryIs0700AndFilesAre0600() {
        TestCore().use { sender ->
            TestCore().use { receiver ->
                val received = transfer(sender, receiver, times = 1)
                assertEquals(MODE_0700, modeOf(receiver.paths.receiveDir))
                assertEquals(Os.getuid(), Os.lstat(receiver.paths.receiveDir.path).st_uid)
                val file = received.single()
                assertEquals(MODE_0600, modeOf(file))
                assertEquals(Os.getuid(), Os.lstat(file.path).st_uid)
            }
        }
    }

    @Test
    fun receivingTheSameNameTwiceNeverOverwrites() {
        TestCore().use { sender ->
            TestCore().use { receiver ->
                val received = transfer(sender, receiver, times = 2)
                assertEquals(listOf("photo (1).jpg", "photo.jpg"), received.map { it.name })
                received.forEach { assertArrayEquals(it.name, data, it.readBytes()) }
            }
        }
    }

    /** Sends `photo.jpg` [times] times and returns what the receiver holds, sorted. */
    private fun transfer(sender: TestCore, receiver: TestCore, times: Int): List<File> {
        // Identity creation first, on its own, as on a first launch.
        sender.client.identity()
        receiver.client.identity()
        pair(sender, receiver)
        val port = receiver.receiveFromPaired()
        val source = File(sender.root, "photo.jpg").also { it.writeBytes(data) }
        repeat(times) {
            sender.client.send(
                SendTarget.Address("127.0.0.1:$port", null),
                listOf(SendItem.Path(source.path)),
                allowUnverified = false,
            )
            assertEquals(TransferStatus.COMPLETED, sender.finished(Direction.OUTGOING).status)
            assertEquals(TransferStatus.COMPLETED, receiver.finished(Direction.INCOMING).status)
        }
        val files = receiver.paths.receiveDir.listFiles().orEmpty().sortedBy { it.name }
        assertTrue(
            "no partial files left on API ${Build.VERSION.SDK_INT}",
            files.none { it.name.endsWith(".part") },
        )
        return files
    }
}
