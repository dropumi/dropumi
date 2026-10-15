// SPDX-License-Identifier: MPL-2.0
//
// UriGuard on Android: parses the URI, reads Dropumi's own provider
// authorities from its package (GET_PROVIDERS, disabled ones too; never
// resolveContentProvider), and lists the protected folders with lstat at
// check time. The rules are in UriGuardPolicy. It fails closed.

package io.github.dropumi.core.files

import android.content.Context
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import java.io.File
import java.io.IOException

/**
 * @param protectedDirs the configuration folder and the app-data folder;
 *   every entry in them, whatever its name, is refused as a source.
 */
class UriGuard(context: Context, private val protectedDirs: List<File>) : DocumentGuard {
    private val packageManager = context.packageManager
    private val packageName = context.packageName

    /** Null if they could not be read: then every URI is refused. */
    private val ownAuthorities: Set<String>? by lazy { readOwnAuthorities() }

    override fun checkUri(uri: String): UriGuardPolicy.Refusal? {
        val own = ownAuthorities ?: return UriGuardPolicy.Refusal.UNCHECKABLE
        val parsed = Uri.parse(uri)
        // The decoded authority: what ContentResolver uses to find the provider.
        return UriGuardPolicy.checkUri(parsed.scheme, parsed.authority, own)
    }

    override fun checkOpen(document: OpenDocument): UriGuardPolicy.Refusal? {
        val fileId = try {
            document.fileId()
        } catch (_: Exception) {
            null
        }
        val procPath = try {
            document.procPath()
        } catch (_: Exception) {
            null
        }
        return UriGuardPolicy.checkFile(fileId, procPath, protectedEntries(), protectedPaths())
    }

    /** The identity of every entry of the protected folders now; null if any cannot be read. */
    private fun protectedEntries(): List<FileId>? {
        val entries = mutableListOf<FileId>()
        for (dir in protectedDirs) {
            try {
                Os.lstat(dir.path)
            } catch (e: ErrnoException) {
                // A folder that does not exist yet holds nothing to protect.
                if (e.errno == OsConstants.ENOENT) continue
                return null
            }
            val names = dir.list() ?: return null
            for (name in names) {
                val stat = try {
                    Os.lstat(File(dir, name).path)
                } catch (_: ErrnoException) {
                    return null
                }
                entries += FileId(stat.st_dev, stat.st_ino)
            }
        }
        return entries
    }

    /** Each folder as written and as resolved (for example /data/user/0 and /data/data). */
    private fun protectedPaths(): List<String> = protectedDirs.flatMap { dir ->
        val canonical = try {
            dir.canonicalPath
        } catch (_: IOException) {
            null
        }
        listOfNotNull(dir.absolutePath, canonical)
    }.distinct()

    private fun readOwnAuthorities(): Set<String>? = try {
        val flags = PackageManager.GET_PROVIDERS or PackageManager.MATCH_DISABLED_COMPONENTS
        val info = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            packageManager.getPackageInfo(packageName, PackageManager.PackageInfoFlags.of(flags.toLong()))
        } else {
            @Suppress("DEPRECATION")
            packageManager.getPackageInfo(packageName, flags)
        }
        info.providers.orEmpty()
            .flatMap { it.authority.orEmpty().split(';') }
            .map { it.trim() }
            .filter { it.isNotEmpty() }
            .toSet()
    } catch (_: Exception) {
        null
    }
}
