// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.ui.design

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Test

class FileKindTest {
    @Test
    fun fileKindComesFromTheExtensionOnly() {
        assertEquals(FileKind.Image, FileKind.of("photo.JPG"))
        assertEquals(FileKind.Video, FileKind.of("clip.mov"))
        assertEquals(FileKind.Audio, FileKind.of("song.flac"))
        assertEquals(FileKind.Archive, FileKind.of("backup.tar.gz"))
        assertEquals(FileKind.Document, FileKind.of("notes.pdf"))
        assertEquals(FileKind.Other, FileKind.of("program.bin"))
    }

    @Test
    fun namesWithoutAnExtensionAreOther() {
        assertEquals(FileKind.Other, FileKind.of("README"))
        assertEquals(FileKind.Other, FileKind.of(".jpg"))
        assertEquals(FileKind.Other, FileKind.of("photo."))
        assertEquals(FileKind.Other, FileKind.of(""))
    }

    @Test
    fun everyKindHasAnIcon() {
        val icons = FileKind.entries.map { DSIcons.file(it) }
        for (icon in icons) assertNotEquals(0, icon)
        assertEquals(icons.size, icons.toSet().size)
    }
}
