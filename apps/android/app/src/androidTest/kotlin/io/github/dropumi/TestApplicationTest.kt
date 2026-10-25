// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class TestApplicationTest {
    /** ADR-0013 D9: tests run under TestApplication, so the app's real configuration is never opened. */
    @Test
    fun instrumentedTestsNeverBuildTheAppsApplication() {
        val app = InstrumentationRegistry.getInstrumentation().targetContext.applicationContext
        assertTrue(app is TestApplication)
        assertFalse(app is DropumiApplication)
    }
}
