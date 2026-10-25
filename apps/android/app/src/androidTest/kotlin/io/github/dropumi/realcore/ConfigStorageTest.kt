// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.realcore

import android.content.pm.ApplicationInfo
import android.system.Os
import android.system.OsConstants
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.dropumi.R
import io.github.dropumi.core.AppPaths
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.xmlpull.v1.XmlPullParser

/**
 * The configuration folder is private and nothing is backed up (ADR-0013 D3).
 * The checks of the backup transport itself are host-driven script steps on
 * a rootable image (docs/testing.md); these run on every image.
 */
@RunWith(AndroidJUnit4::class)
class ConfigStorageTest {
    @Test
    fun configFolderIsPrivateAndExcludedFromBackups() {
        val context = targetContext
        // The app's own configuration and history live in noBackupFilesDir.
        // Checked by path only: this test never opens the app's real config.
        val standard = AppPaths.standard(context)
        assertEquals(File(context.noBackupFilesDir, "dropumi"), standard.configDir)
        assertEquals(context.noBackupFilesDir, standard.historyFile.parentFile?.parentFile)
        assertEquals("allowBackup", 0, context.applicationInfo.flags and ApplicationInfo.FLAG_ALLOW_BACKUP)
        assertEveryDomainExcluded()

        // What the core creates there: the folder 0700, every file 0600.
        TestCore().use { core ->
            TestCore().use { other ->
                core.client.identity()
                core.client.setDeviceName("Test device")
                core.client.trustFingerprint(other.client.identity().fingerprint, "Other")
            }
            val config = core.paths.configDir
            assertEquals(MODE_0700, modeOf(config))
            assertEquals(Os.getuid(), Os.lstat(config.path).st_uid)
            val entries = config.listFiles().orEmpty()
            val names = entries.map { it.name }.toSet()
            assertTrue("identity.key in $names", "identity.key" in names)
            assertTrue("trusted_devices.json in $names", "trusted_devices.json" in names)
            for (entry in entries) {
                val stat = Os.lstat(entry.path)
                assertTrue("${entry.name} is a regular file", OsConstants.S_ISREG(stat.st_mode))
                assertEquals(entry.name, MODE_0600, stat.st_mode and 0b111_111_111)
                assertEquals(entry.name, Os.getuid(), stat.st_uid)
            }
        }
    }

    @Test
    fun backupRulesExcludeEveryDomain() {
        assertEveryDomainExcluded()
    }

    private fun assertEveryDomainExcluded() {
        val extraction = parse(R.xml.data_extraction_rules)
        assertEquals(setOf("cloud-backup", "device-transfer"), extraction.keys)
        extraction.forEach { (section, rules) -> assertAllExcluded("data_extraction_rules/$section", rules) }
        val full = parse(R.xml.backup_rules)
        assertEquals(setOf("full-backup-content"), full.keys)
        assertAllExcluded("backup_rules", full.getValue("full-backup-content"))
    }

    private fun assertAllExcluded(where: String, rules: List<Rule>) {
        assertFalse("$where has no include", rules.any { it.tag == "include" })
        val excluded = rules.filter { it.tag == "exclude" && it.path == "." }.map { it.domain }.toSet()
        assertEquals(where, ALL_DOMAINS, excluded)
    }

    private data class Rule(val tag: String, val domain: String?, val path: String?)

    /** The rules of an XML resource, by the section they appear in. */
    private fun parse(id: Int): Map<String, List<Rule>> {
        val result = linkedMapOf<String, MutableList<Rule>>()
        targetContext.resources.getXml(id).use { parser ->
            var section: String? = null
            while (parser.next() != XmlPullParser.END_DOCUMENT) {
                if (parser.eventType != XmlPullParser.START_TAG) continue
                when (parser.name) {
                    "cloud-backup", "device-transfer", "full-backup-content" -> {
                        section = parser.name
                        result.getOrPut(parser.name) { mutableListOf() }
                    }
                    "include", "exclude" -> {
                        val rule = Rule(
                            parser.name,
                            parser.getAttributeValue(null, "domain"),
                            parser.getAttributeValue(null, "path"),
                        )
                        result.getValue(requireNotNull(section) { "rule outside a section" }).add(rule)
                    }
                }
            }
        }
        return result
    }

    private companion object {
        val ALL_DOMAINS = setOf(
            "root", "file", "database", "sharedpref", "external",
            "device_root", "device_file", "device_database", "device_sharedpref",
        )
    }
}
