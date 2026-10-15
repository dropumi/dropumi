// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * The JVM tests of this module never load libdropumi_ffi: the build points
 * the bindings at a library name that does not exist, so an accidental load
 * fails loudly instead of running the real core.
 */
class NativeLibraryGuardTest {
    @Test
    fun unitTestsNeverLoadTheNativeLibrary() {
        assertEquals(
            "dropumi_ffi_must_not_load",
            System.getProperty("uniffi.component.dropumi_ffi.libraryOverride"),
        )
    }
}
