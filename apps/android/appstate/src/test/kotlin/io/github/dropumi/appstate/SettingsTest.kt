// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.appstate

import io.github.dropumi.ffi.ReceivePolicy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

class SettingsTest {
    /** Everyone is short and must be opened on purpose: it is never restored at launch. */
    @Test
    fun everyoneIsNeverRestoredAndPairedOnlyIsTheDefault() {
        assertEquals(ReceiveVisibility.PAIRED_ONLY, ReceiveVisibility.restored(null))
        assertEquals(ReceiveVisibility.PAIRED_ONLY, ReceiveVisibility.restored("EVERYONE"))
        assertEquals(ReceiveVisibility.PAIRED_ONLY, ReceiveVisibility.restored("garbage"))
        assertEquals(ReceiveVisibility.OFF, ReceiveVisibility.restored("OFF"))
        val store = MemorySettingsStore(sdkInt = 36)
        store.visibility = ReceiveVisibility.EVERYONE
        assertEquals(ReceiveVisibility.PAIRED_ONLY, store.visibility)
        assertFalse(store.autoAcceptPaired)
    }

    @Test
    fun visibilityMapsToCorePolicies() {
        assertNull(ReceiveVisibility.OFF.policy)
        assertEquals(ReceivePolicy.TrustedOnly, ReceiveVisibility.PAIRED_ONLY.policy)
        assertEquals(ReceivePolicy.Everyone(10u), ReceiveVisibility.EVERYONE.policy)
    }

    /** API 26-28 have no system dark theme: Light and Dark only, Light by default. */
    @Test
    fun followSystemExistsOnlyFromApi29() {
        assertEquals(listOf(Appearance.LIGHT, Appearance.DARK), Appearance.choices(28))
        assertEquals(Appearance.LIGHT, Appearance.default(26))
        assertEquals(Appearance.LIGHT, Appearance.restored("SYSTEM", 28))
        assertEquals(Appearance.DARK, Appearance.restored("DARK", 26))
        assertEquals(listOf(Appearance.SYSTEM, Appearance.LIGHT, Appearance.DARK), Appearance.choices(29))
        assertEquals(Appearance.SYSTEM, Appearance.default(36))
        assertEquals(Appearance.SYSTEM, Appearance.restored(null, 36))
    }
}
