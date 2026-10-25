// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class AppPathsTest {
    private val noBackup = File("/data/user/0/io.github.dropumi/no_backup")
    private val files = File("/data/user/0/io.github.dropumi/files")

    /**
     * ADR-0013 D3/D4: the configuration, the history and staged copies live in
     * noBackupFilesDir; received files in filesDir/received, the only folder
     * the FileProvider shares from.
     */
    @Test
    fun configAndAppDataLiveInNoBackupAndReceivedFilesInFiles() {
        val paths = AppPaths.inside(noBackup, files)
        assertEquals(File(noBackup, "dropumi"), paths.configDir)
        assertEquals(File(noBackup, "app"), paths.appDataDir)
        assertEquals(File(noBackup, "app/history.json"), paths.historyFile)
        assertEquals(File(noBackup, "outgoing"), paths.outgoingDir)
        assertEquals(File(files, "received"), paths.receiveDir)
    }

    /** Test and harness paths stay inside the folder they are given. */
    @Test
    fun temporaryPathsStayUnderTheirRoot() {
        val root = File(noBackup, "interop-1234")
        val paths = AppPaths.under(root)
        listOf(paths.configDir, paths.appDataDir, paths.receiveDir, paths.outgoingDir, paths.historyFile).forEach {
            assertEquals(true, it.path.startsWith(root.path + "/"))
        }
    }

    @Test
    fun privateFolderMustBeAnOwned0700FolderAndNotALink() {
        val uid = 10123
        val dir = 0b100_000_000_000_000 // S_IFDIR
        fun problem(link: Boolean = false, isDir: Boolean = true, owner: Int = uid, mode: Int = dir or 0b111_000_000) =
            PrivateDirRule.problem(link, isDir, owner, mode, uid)

        assertNull(problem())
        assertEquals(PrivateDirRule.Problem.LINK, problem(link = true))
        assertEquals(PrivateDirRule.Problem.NOT_A_FOLDER, problem(isDir = false))
        assertEquals(PrivateDirRule.Problem.OTHER_OWNER, problem(owner = 0))
        // 0771 (what Android gives filesDir), 0755 and 0500 are all refused.
        assertEquals(PrivateDirRule.Problem.MODE, problem(mode = dir or 0b111_111_001))
        assertEquals(PrivateDirRule.Problem.MODE, problem(mode = dir or 0b111_101_101))
        assertEquals(PrivateDirRule.Problem.MODE, problem(mode = dir or 0b101_000_000))
    }
}
