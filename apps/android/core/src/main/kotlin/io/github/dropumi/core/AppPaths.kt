// SPDX-License-Identifier: MPL-2.0
//
// Where the app keeps its configuration, data, received files and staged
// copies (ADR-0013 D3 and D4):
//
// - configuration (identity, trust store): noBackupFilesDir/dropumi. The app
//   creates only the parent; the core creates the folder 0700 and runs its
//   owner and mode checks. noBackupFilesDir is never backed up, and the
//   backup rules exclude everything else too.
// - app data (history): noBackupFilesDir/app, 0700.
// - received files: filesDir/received, 0700. The core finalizes files there
//   0600 and never overwrites; they leave only through an explicit Save,
//   Share or Open.
// - staged outgoing copies: noBackupFilesDir/outgoing, 0700 (not cacheDir,
//   which the OS may clear while a transfer reads it).

package io.github.dropumi.core

import android.content.Context
import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import java.io.File
import java.io.IOException

data class AppPaths(
    val configDir: File,
    val appDataDir: File,
    val receiveDir: File,
    val outgoingDir: File,
) {
    /** Finished transfers, kept on this device only. */
    val historyFile: File get() = File(appDataDir, HISTORY_FILE)

    /**
     * Creates the folders the app needs. The configuration folder is left to
     * the core; only its parent must exist. The others are created 0700 with
     * `mkdir` (never relying on the umask) and then checked: a folder, not a
     * link, owned by this app, mode 0700. Throws [IOException] otherwise.
     */
    fun prepare() {
        val parent = configDir.parentFile ?: throw IOException("the configuration folder has no parent")
        if (!parent.isDirectory && !parent.mkdirs()) {
            throw IOException("the app's private folder could not be created")
        }
        ensurePrivateDir(appDataDir)
        ensurePrivateDir(receiveDir)
        ensurePrivateDir(outgoingDir)
    }

    /** Deletes staged outgoing copies (at launch). */
    fun clearOutgoing() {
        outgoingDir.deleteRecursively()
        ensurePrivateDir(outgoingDir)
    }

    companion object {
        const val CONFIG_FOLDER = "dropumi"
        const val APP_DATA_FOLDER = "app"
        const val RECEIVED_FOLDER = "received"
        const val OUTGOING_FOLDER = "outgoing"
        const val HISTORY_FILE = "history.json"

        /** The app's standard locations. */
        fun standard(context: Context): AppPaths = inside(context.noBackupFilesDir, context.filesDir)

        /** The standard layout for the given app folders. */
        fun inside(noBackupDir: File, filesDir: File): AppPaths = AppPaths(
            configDir = File(noBackupDir, CONFIG_FOLDER),
            appDataDir = File(noBackupDir, APP_DATA_FOLDER),
            receiveDir = File(filesDir, RECEIVED_FOLDER),
            outgoingDir = File(noBackupDir, OUTGOING_FOLDER),
        )

        /**
         * Everything under one app-private folder, for tests and harness runs
         * (for example `noBackupFilesDir/interop-<uuid>`); never the user's
         * configuration.
         */
        fun under(root: File): AppPaths = AppPaths(
            configDir = File(root, "config"),
            appDataDir = File(root, APP_DATA_FOLDER),
            receiveDir = File(root, RECEIVED_FOLDER),
            outgoingDir = File(root, OUTGOING_FOLDER),
        )
    }
}

/** What a private folder must be. Pure, so the rule is tested on the JVM. */
object PrivateDirRule {
    /** rwx for the owner only. */
    const val MODE: Int = 0b111_000_000
    private const val PERMISSION_BITS: Int = 0b111_111_111

    enum class Problem { LINK, NOT_A_FOLDER, OTHER_OWNER, MODE }

    /** The first problem with a folder described by `lstat`, or null. */
    fun problem(isLink: Boolean, isDirectory: Boolean, ownerUid: Int, mode: Int, uid: Int): Problem? = when {
        isLink -> Problem.LINK
        !isDirectory -> Problem.NOT_A_FOLDER
        ownerUid != uid -> Problem.OTHER_OWNER
        (mode and PERMISSION_BITS) != MODE -> Problem.MODE
        else -> null
    }
}

/**
 * Creates [dir] 0700 if it is missing, then checks it with `lstat`. A folder
 * of this app's with the wrong mode is set to 0700 and checked again;
 * anything else fails. Messages name no path.
 */
internal fun ensurePrivateDir(dir: File) {
    try {
        Os.mkdir(dir.path, PrivateDirRule.MODE)
    } catch (e: ErrnoException) {
        if (e.errno != OsConstants.EEXIST) throw IOException("a private folder could not be created")
    }
    var problem = checkPrivateDir(dir)
    if (problem == PrivateDirRule.Problem.MODE) {
        try {
            Os.chmod(dir.path, PrivateDirRule.MODE)
        } catch (_: ErrnoException) {
            throw IOException("a private folder could not be made private")
        }
        problem = checkPrivateDir(dir)
    }
    if (problem != null) throw IOException("a private folder failed its check: $problem")
}

private fun checkPrivateDir(dir: File): PrivateDirRule.Problem? {
    val stat = try {
        Os.lstat(dir.path)
    } catch (_: ErrnoException) {
        throw IOException("a private folder could not be checked")
    }
    return PrivateDirRule.problem(
        isLink = OsConstants.S_ISLNK(stat.st_mode),
        isDirectory = OsConstants.S_ISDIR(stat.st_mode),
        ownerUid = stat.st_uid,
        mode = stat.st_mode,
        uid = Os.getuid(),
    )
}
