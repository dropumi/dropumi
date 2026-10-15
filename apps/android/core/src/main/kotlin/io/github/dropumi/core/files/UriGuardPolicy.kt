// SPDX-License-Identifier: MPL-2.0
//
// UriGuard's rules, the Android counterpart of threat-model row G27
// (ADR-0013 D6). Pure, so every step is tested on the host JVM; UriGuard
// supplies the parsed URI, the app's provider authorities, the descriptor's
// identity and the protected folders' listings.
//
// 1. The scheme must be exactly `content` (`file:`, `android.resource:` and
//    everything else are refused).
// 2. The authority, normalized (the part after the last `@`, without a
//    `:port`, case-insensitive; empty is refused), must not be one of
//    Dropumi's own providers. ContentResolver strips a `userId@` prefix, so
//    `0@<own authority>` would reach Dropumi's own provider.
// 3. The descriptor that is read must not be (st_dev, st_ino) any entry of
//    the configuration folder or of the app-data folder, whatever its name
//    (identity.key, the trust store, .lock, .dropumi-cfg-* and the
//    .dropumi-key-* file that holds the key before it is persisted), and
//    `/proc/self/fd/<n>` must not resolve under either folder.
// It fails closed: anything that cannot be checked is refused.

package io.github.dropumi.core.files

import java.util.Locale

object UriGuardPolicy {
    enum class Refusal { NOT_CONTENT, NO_AUTHORITY, OWN_PROVIDER, PROTECTED_FILE, UNCHECKABLE }

    const val CONTENT = "content"

    /** The authority as the resolver uses it: after the last `@`, without `:port`, lower case; null if empty. */
    fun normalizeAuthority(authority: String?): String? {
        val host = authority?.substringAfterLast('@')?.substringBefore(':')?.trim()?.lowercase(Locale.ROOT)
        return host?.takeIf { it.isNotEmpty() }
    }

    /** Steps 1 and 2. [authority] is the decoded authority; [own] the app's provider authorities. */
    fun checkUri(scheme: String?, authority: String?, own: Set<String>): Refusal? {
        if (scheme != CONTENT) return Refusal.NOT_CONTENT
        val normalized = normalizeAuthority(authority) ?: return Refusal.NO_AUTHORITY
        val ownNormalized = own.mapNotNull(::normalizeAuthority).toSet()
        return if (normalized in ownNormalized) Refusal.OWN_PROVIDER else null
    }

    /**
     * Step 3. Any null input means it could not be checked, and is refused.
     * [protectedEntries] are the identities of every entry of the protected
     * folders; [protectedDirs] are their paths (each as written and as
     * resolved).
     */
    fun checkFile(fileId: FileId?, procPath: String?, protectedEntries: Collection<FileId>?, protectedDirs: Collection<String>): Refusal? {
        if (fileId == null || procPath == null || protectedEntries == null) return Refusal.UNCHECKABLE
        if (fileId in protectedEntries) return Refusal.PROTECTED_FILE
        val under = protectedDirs.any { dir ->
            val root = dir.trimEnd('/')
            root.isNotEmpty() && (procPath == root || procPath.startsWith("$root/"))
        }
        return if (under) Refusal.PROTECTED_FILE else null
    }
}
