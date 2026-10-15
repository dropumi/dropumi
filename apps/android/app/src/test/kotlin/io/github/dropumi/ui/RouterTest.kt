// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.ui

import androidx.compose.runtime.saveable.SaverScope
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RouterTest {
    private fun roundTrip(router: Router): Router {
        val saved = with(Router.Saver) { SaverScope { true }.save(router) }
        return Router.Saver.restore(requireNotNull(saved)) as Router
    }

    @Test
    fun eachTabKeepsItsOwnStack() {
        val router = Router()
        router.push(Screen.ConnectByAddress)
        router.select(Tab.SETTINGS)
        router.push(Screen.PairedDevices)
        assertEquals(Screen.PairedDevices, router.current)
        router.select(Tab.NEARBY)
        assertEquals(Screen.ConnectByAddress, router.current)
        assertTrue(router.pop())
        assertNull(router.current)
        assertFalse(router.pop())
    }

    @Test
    fun sendOpensInNearbyAndAStartedSendGoesToTransfers() {
        val router = Router(Tab.SETTINGS)
        router.push(Screen.ThisDevice)
        router.openSend("dropumi-0000abcd")
        assertEquals(Tab.NEARBY, router.tab)
        assertEquals(Screen.Send("dropumi-0000abcd"), router.current)
        // A second Send replaces the first rather than stacking.
        router.openSend(null)
        assertEquals(listOf<Screen>(Screen.Send(null)), router.stack(Tab.NEARBY))
        router.openTransfers()
        assertEquals(Tab.TRANSFERS, router.tab)
        assertTrue(router.stack(Tab.NEARBY).isEmpty())
        assertTrue(router.stack(Tab.SETTINGS).isEmpty())
    }

    /** The chosen device is a peer string: it is not written into the saved instance state. */
    @Test
    fun savedStateKeepsScreensButNotTheChosenDevice() {
        val router = Router()
        router.openSend("Maya's phone instance")
        router.select(Tab.SETTINGS)
        router.push(Screen.Licences)
        router.push(Screen.Licence(LICENCE_ASSETS[2]))
        val saved = with(Router.Saver) { SaverScope { true }.save(router) }
        assertFalse(saved.toString().contains("Maya"))
        val restored = roundTrip(router)
        assertEquals(Tab.SETTINGS, restored.tab)
        assertEquals(listOf(Screen.Licences, Screen.Licence(LICENCE_ASSETS[2])), restored.stack(Tab.SETTINGS))
        assertEquals(listOf<Screen>(Screen.Send(null)), restored.stack(Tab.NEARBY))
    }

    /** Only known screens come back from saved state; anything else is dropped. */
    @Test
    fun unknownSavedScreensAreDropped() {
        val restored = Router.Saver.restore(arrayListOf("SETTINGS", "", "", "licences,licence:99,licence:x,other")) as Router
        assertEquals(listOf<Screen>(Screen.Licences), restored.stack(Tab.SETTINGS))
    }
}
