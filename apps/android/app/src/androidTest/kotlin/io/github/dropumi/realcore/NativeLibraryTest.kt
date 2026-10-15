// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.realcore

import android.content.pm.ApplicationInfo
import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.util.zip.ZipEntry
import java.util.zip.ZipFile
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class NativeLibraryTest {
    /**
     * The Rust core and JNA's dispatcher are stored uncompressed in the APK and
     * loaded from it (not extracted), and the core opens on this image.
     */
    @Test
    fun nativeLibrariesLoadFromUncompressedApk() {
        val info = targetContext.applicationInfo
        assertEquals("extractNativeLibs", 0, info.flags and ApplicationInfo.FLAG_EXTRACT_NATIVE_LIBS)
        val abi = Build.SUPPORTED_ABIS.first { it == "arm64-v8a" || it == "x86_64" }
        ZipFile(info.sourceDir).use { apk ->
            for (library in listOf("libdropumi_ffi.so", "libjnidispatch.so")) {
                val entry = apk.getEntry("lib/$abi/$library")
                assertNotNull("lib/$abi/$library in the APK", entry)
                assertEquals("lib/$abi/$library is stored", ZipEntry.STORED, entry.method)
            }
        }
        TestCore().use { core ->
            assertEquals(52, core.client.identity().fingerprint.length)
        }
    }
}
