// SPDX-License-Identifier: MPL-2.0
//
// Every Dropumi notification on the lock screen (plan §6.6): private, with
// a generic public version that names no peer and no file. In 2c-1 that is
// the transfer service's notification. It is built here with the real
// NotificationFactory and posted nowhere.
package io.github.dropumi.notify

import android.app.Notification
import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import io.github.dropumi.appstate.TransferPhase
import io.github.dropumi.appstate.TransferRecord
import io.github.dropumi.ffi.Direction
import io.github.dropumi.ffi.Progress
import io.github.dropumi.testsupport.Fixtures
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class NotificationsTest {
    private val context get() = InstrumentationRegistry.getInstrumentation().targetContext

    /** Every text a notification carries in its extras. */
    private fun texts(n: Notification): String = n.extras.keySet().joinToString("\n") {
        @Suppress("DEPRECATION")
        "${n.extras.get(it)}"
    }

    @Test
    fun notificationsArePrivateOnLockScreen() {
        val peer = Fixtures.peer(name = "Maya's phone", trusted = true, alias = "Maya")
        val records = listOf(
            TransferRecord(
                id = "in-tt",
                direction = Direction.INCOMING,
                operation = null,
                transferId = "tt",
                peer = peer,
                targetName = "Maya's phone",
                fileNames = listOf("Beach-trip.mov"),
                totalBytes = 18_200_000u,
                progress = Progress(4_100_000u, 18_200_000u, 0u, 1u),
                phase = TransferPhase.Transferring,
                startedAtMs = 0,
            ),
        )
        val notification = NotificationFactory.transfers(context, records)
        assertEquals(Notification.VISIBILITY_PRIVATE, notification.visibility)
        assertEquals(NotificationFactory.TRANSFERS_CHANNEL, notification.channelId)
        val public = requireNotNull(notification.publicVersion) { "no public version" }
        assertEquals(TransferNotificationText.PUBLIC_TITLE, public.extras.getCharSequence(Notification.EXTRA_TITLE)?.toString())
        for ((which, n) in listOf("private" to notification, "public" to public)) {
            val words = texts(n)
            for (secret in listOf("Maya", "Beach-trip", peer.fingerprint.take(8), peer.fingerprintDisplay)) {
                assertFalse("the $which version names $secret", words.contains(secret))
            }
            assertTrue("the $which version has actions", n.actions.isNullOrEmpty())
        }
        // The private version still says what is happening, in counts and sizes.
        assertEquals("Receiving 1 file", notification.extras.getCharSequence(Notification.EXTRA_TITLE)?.toString())
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            assertTrue("a mutable content intent", requireNotNull(notification.contentIntent).isImmutable)
        }
    }
}
