// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core.files

import io.github.dropumi.core.files.UriGuardPolicy.Refusal
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class UriGuardPolicyTest {
    private val own = setOf("io.github.dropumi.fileprovider", "io.github.dropumi.androidx-startup")
    private val config = "/data/user/0/io.github.dropumi/no_backup/dropumi"
    private val appData = "/data/user/0/io.github.dropumi/no_backup/app"

    /** The configuration and app-data folders as listed at check time, by name. */
    private val listing = mapOf(
        "identity.key" to FileId(64_768, 101),
        "device.json" to FileId(64_768, 102),
        "trusted_devices.json" to FileId(64_768, 103),
        ".lock" to FileId(64_768, 104),
        ".dropumi-cfg-a1b2c3" to FileId(64_768, 105),
        ".dropumi-key-9z8y7x" to FileId(64_768, 106),
        "something-new" to FileId(64_768, 107),
        "history.json" to FileId(64_768, 201),
        ".history-1234.tmp" to FileId(64_768, 202),
    )

    private fun checkFd(id: FileId, path: String = "/storage/emulated/0/Download/x") =
        UriGuardPolicy.checkFile(id, path, listing.values, listOf(config, appData))

    @Test
    fun uriGuardRefusesFileScheme() {
        assertEquals(Refusal.NOT_CONTENT, UriGuardPolicy.checkUri("file", "", own))
        assertEquals(Refusal.NOT_CONTENT, UriGuardPolicy.checkUri("file", null, own))
    }

    @Test
    fun uriGuardRefusesOtherSchemes() {
        for (scheme in listOf("android.resource", "http", "https", "CONTENT", "Content", "", null)) {
            assertEquals("$scheme", Refusal.NOT_CONTENT, UriGuardPolicy.checkUri(scheme, "com.example.files", own))
        }
        assertNull(UriGuardPolicy.checkUri("content", "com.example.files", own))
    }

    /** The resolver strips `userId@`, so every form that reaches Dropumi's own provider is refused. */
    @Test
    fun uriGuardRefusesOwnAuthority() {
        val provider = "io.github.dropumi.fileprovider"
        for (authority in listOf(
            provider, "0@$provider", "10@$provider", "a@b@$provider", "$provider:1", "0@$provider:1",
            "IO.GitHub.Dropumi.FileProvider",
        )) {
            assertEquals(authority, Refusal.OWN_PROVIDER, UriGuardPolicy.checkUri("content", authority, own))
        }
        for (empty in listOf("", null, "0@", ":1", " ")) {
            assertEquals("$empty", Refusal.NO_AUTHORITY, UriGuardPolicy.checkUri("content", empty, own))
        }
        assertNull(UriGuardPolicy.checkUri("content", "0@com.example.files", own))
    }

    @Test
    fun uriGuardRefusesConfigFiles() {
        for (name in listOf("identity.key", "device.json", "trusted_devices.json", ".lock", "something-new")) {
            assertEquals(name, Refusal.PROTECTED_FILE, checkFd(listing.getValue(name)))
        }
        assertNull(checkFd(FileId(64_768, 999)))
        // Same inode on another device is another file.
        assertNull(checkFd(FileId(1, 101)))
    }

    @Test
    fun uriGuardRefusesConfigTempFile() {
        assertEquals(Refusal.PROTECTED_FILE, checkFd(listing.getValue(".dropumi-cfg-a1b2c3")))
    }

    /** The file that holds the private key before it is persisted. */
    @Test
    fun uriGuardRefusesKeyTempFile() {
        assertEquals(Refusal.PROTECTED_FILE, checkFd(listing.getValue(".dropumi-key-9z8y7x")))
    }

    @Test
    fun uriGuardRefusesHistoryFile() {
        assertEquals(Refusal.PROTECTED_FILE, checkFd(listing.getValue("history.json")))
        assertEquals(Refusal.PROTECTED_FILE, checkFd(listing.getValue(".history-1234.tmp")))
    }

    @Test
    fun uriGuardRefusesProcFdPathUnderConfig() {
        val other = FileId(64_768, 999)
        assertEquals(Refusal.PROTECTED_FILE, checkFd(other, "$config/identity.key"))
        assertEquals(Refusal.PROTECTED_FILE, checkFd(other, config))
        assertEquals(Refusal.PROTECTED_FILE, checkFd(other, "$appData/history.json"))
        // A sibling whose name only starts the same is not under it.
        assertNull(checkFd(other, "$config-other/x"))
    }

    @Test
    fun uriGuardFailsClosedOnStatError() {
        val id = FileId(64_768, 999)
        val path = "/storage/emulated/0/Download/x"
        assertEquals(Refusal.UNCHECKABLE, UriGuardPolicy.checkFile(null, path, listing.values, listOf(config)))
        assertEquals(Refusal.UNCHECKABLE, UriGuardPolicy.checkFile(id, null, listing.values, listOf(config)))
        assertEquals(Refusal.UNCHECKABLE, UriGuardPolicy.checkFile(id, path, null, listOf(config)))
    }
}
