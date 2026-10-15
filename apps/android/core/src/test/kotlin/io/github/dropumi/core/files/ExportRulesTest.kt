// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core.files

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ExportRulesTest {
    @Test
    fun exportNameMustBeSingleComponent() {
        for (bad in listOf("", ".", "..", "../x", "a/b", "/abs", "x/", "a\u0000b")) {
            assertFalse(bad, ExportRules.isSingleName(bad))
        }
        for (good in listOf("photo.jpg", "photo (1).jpg", ".hidden", "a\\b", "名前.txt")) {
            assertTrue(good, ExportRules.isSingleName(good))
        }
    }

    /** The MIME type comes only from the saved name's extension (never from the peer's claims). */
    @Test
    fun exportMimeFromSanitizedNameOnly() {
        val asked = mutableListOf<String>()
        val lookup: (String) -> String? = { ext ->
            asked += ext
            mapOf("pdf" to "application/pdf", "jpg" to "image/jpeg", "gz" to "application/gzip")[ext]
        }
        assertEquals("application/pdf", ExportRules.mimeFor("Report.PDF", lookup))
        assertEquals("image/jpeg", ExportRules.mimeFor("photo (1).jpg", lookup))
        assertEquals("application/gzip", ExportRules.mimeFor("a.tar.gz", lookup))
        assertEquals(ExportRules.OCTET_STREAM, ExportRules.mimeFor("README", lookup))
        assertEquals(ExportRules.OCTET_STREAM, ExportRules.mimeFor(".bashrc", lookup))
        assertEquals(ExportRules.OCTET_STREAM, ExportRules.mimeFor("trailing.", lookup))
        assertEquals(ExportRules.OCTET_STREAM, ExportRules.mimeFor("x.unknown", lookup))
        assertEquals(listOf("pdf", "jpg", "gz", "unknown"), asked)
    }
}
