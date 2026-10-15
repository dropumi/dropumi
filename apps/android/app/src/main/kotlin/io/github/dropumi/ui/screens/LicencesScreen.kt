// SPDX-License-Identifier: MPL-2.0
//
// Licences: Dropumi's own (MPL-2.0) and the notices of everything it is
// built from, read from the app's assets (docs/dependencies.md,
// "Third-party notices"): the Rust crates in the core, the Android
// libraries, the font (OFL) and the icons (ISC, some also MIT).
package io.github.dropumi.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.github.dropumi.ui.LICENCE_ASSETS
import io.github.dropumi.ui.design.Caption
import io.github.dropumi.ui.design.DS
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.DSMetrics
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.ScreenColumn
import io.github.dropumi.ui.design.ScreenHeader
import io.github.dropumi.ui.design.SettingsRow
import io.github.dropumi.ui.design.SectionHeading
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

private class Notice(val title: String, val subtitle: String, val asset: String)

private val notices = listOf(
    Notice("Rust crates in the Dropumi core", "Apache-2.0, MIT, MPL-2.0, BSD-3-Clause, BlueOak-1.0.0, Unicode-3.0", LICENCE_ASSETS[0]),
    Notice("Android libraries", "AndroidX, Jetpack Compose, Kotlin, kotlinx, JNA: Apache-2.0", LICENCE_ASSETS[1]),
    Notice("Plus Jakarta Sans", "The app's font: SIL Open Font License 1.1", LICENCE_ASSETS[2]),
    Notice("Lucide icons", "ISC; icons derived from Feather are also MIT", LICENCE_ASSETS[3]),
)

@Composable
fun LicencesScreen(onOpen: (String) -> Unit) {
    ScreenColumn {
        ScreenHeader("Licences", null)
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            SectionHeading("Dropumi")
            Caption(
                "Dropumi is free software under the Mozilla Public License 2.0. " +
                    "Its source code is at https://github.com/dropumi/dropumi.",
            )
        }
        Column {
            for (notice in notices) {
                SettingsRow(DSIcons.licences, notice.title, notice.subtitle, onClick = { onOpen(notice.asset) })
            }
        }
    }
}

/** One notice's text, read from the app's assets off the main thread. */
@Composable
fun LicenceText(asset: String) {
    val notice = notices.firstOrNull { it.asset == asset } ?: return
    val context = LocalContext.current
    val c = DS.colors
    val paragraphs by produceState<List<String>?>(null, notice.asset) {
        value = withContext(Dispatchers.IO) {
            try {
                context.assets.open(notice.asset).bufferedReader().use { it.readText() }
                    .split("\n\n").map { it.trimEnd() }.filter { it.isNotBlank() }
            } catch (_: IOException) {
                listOf("This notice could not be read.")
            }
        }
    }
    val list = paragraphs
    if (list == null) {
        ScreenColumn { CircularProgressIndicator(color = c.accent) }
        return
    }
    LazyColumn(
        verticalArrangement = Arrangement.spacedBy(12.dp),
        contentPadding = PaddingValues(DSMetrics.screenPadding),
        modifier = Modifier.fillMaxSize().background(c.bg),
    ) {
        item { ScreenHeader(notice.title, null) }
        items(list) { paragraph ->
            Text(paragraph, style = DSType.meta.copy(fontFamily = FontFamily.Monospace), color = c.textPrimary)
        }
    }
}
