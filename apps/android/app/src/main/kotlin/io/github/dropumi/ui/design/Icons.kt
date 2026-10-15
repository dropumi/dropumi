// SPDX-License-Identifier: MPL-2.0
//
// Lucide icons (ISC; a few derive from Feather, MIT), converted to vector
// drawables in res/drawable/lucide_*.xml. Licence: assets/licenses/LUCIDE-ISC.txt.
package io.github.dropumi.ui.design

import androidx.annotation.DrawableRes
import io.github.dropumi.R

/** The icons the screens use, by meaning. */
object DSIcons {
    @DrawableRes val nearby = R.drawable.lucide_radar
    @DrawableRes val transfers = R.drawable.lucide_arrow_left_right
    @DrawableRes val settings = R.drawable.lucide_settings
    @DrawableRes val phone = R.drawable.lucide_smartphone
    @DrawableRes val laptop = R.drawable.lucide_laptop
    @DrawableRes val desktop = R.drawable.lucide_monitor
    @DrawableRes val upload = R.drawable.lucide_upload
    @DrawableRes val send = R.drawable.lucide_send
    @DrawableRes val check = R.drawable.lucide_circle_check
    @DrawableRes val shield = R.drawable.lucide_shield_check
    @DrawableRes val warning = R.drawable.lucide_triangle_alert
    @DrawableRes val info = R.drawable.lucide_info
    @DrawableRes val folder = R.drawable.lucide_folder
    @DrawableRes val receiveFolder = R.drawable.lucide_folder_down
    @DrawableRes val code = R.drawable.lucide_code_xml
    @DrawableRes val pair = R.drawable.lucide_link
    @DrawableRes val add = R.drawable.lucide_plus
    @DrawableRes val close = R.drawable.lucide_x
    @DrawableRes val failed = R.drawable.lucide_circle_x
    @DrawableRes val partial = R.drawable.lucide_circle_alert
    @DrawableRes val back = R.drawable.lucide_arrow_left
    @DrawableRes val chevron = R.drawable.lucide_chevron_right
    @DrawableRes val more = R.drawable.lucide_ellipsis_vertical
    @DrawableRes val wifi = R.drawable.lucide_wifi
    @DrawableRes val noCloud = R.drawable.lucide_cloud_off
    @DrawableRes val appearance = R.drawable.lucide_sun_moon
    @DrawableRes val licences = R.drawable.lucide_scroll_text

    /** The icon for a kind of file. */
    @DrawableRes
    fun file(kind: FileKind): Int = when (kind) {
        FileKind.Image -> R.drawable.lucide_image
        FileKind.Video -> R.drawable.lucide_video
        FileKind.Audio -> R.drawable.lucide_music
        FileKind.Archive -> R.drawable.lucide_archive
        FileKind.Document -> R.drawable.lucide_file_text
        FileKind.Other -> R.drawable.lucide_file
    }
}

/**
 * The kind of a file, from its name's extension only (never its content, and
 * never a provider's MIME type). It only picks an icon.
 */
enum class FileKind {
    Image,
    Video,
    Audio,
    Archive,
    Document,
    Other,
    ;

    companion object {
        private val byExtension: Map<String, FileKind> = buildMap {
            listOf("jpg", "jpeg", "png", "gif", "webp", "heic", "heif", "bmp", "tif", "tiff", "svg", "avif")
                .forEach { put(it, Image) }
            listOf("mp4", "m4v", "mov", "mkv", "webm", "avi", "3gp", "mpg", "mpeg")
                .forEach { put(it, Video) }
            listOf("mp3", "m4a", "aac", "wav", "flac", "ogg", "oga", "opus", "aiff", "mid", "midi")
                .forEach { put(it, Audio) }
            listOf("zip", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "zst")
                .forEach { put(it, Archive) }
            listOf("pdf", "txt", "md", "rtf", "doc", "docx", "odt", "xls", "xlsx", "ods", "ppt", "pptx", "odp", "csv", "json", "xml", "html", "htm")
                .forEach { put(it, Document) }
        }

        /** `Other` when the name has no extension (or is only an extension, like `.jpg`). */
        fun of(name: String): FileKind {
            val dot = name.lastIndexOf('.')
            if (dot <= 0 || dot == name.length - 1) return Other
            return byExtension[name.substring(dot + 1).lowercase()] ?: Other
        }
    }
}
