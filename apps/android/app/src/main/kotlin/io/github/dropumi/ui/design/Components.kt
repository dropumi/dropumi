// SPDX-License-Identifier: MPL-2.0
//
// Components from design.md, shared by the screens. Peer strings (device
// names, file names) are always passed as plain `String`s to `Text`: never
// as HTML, Markdown, links or format strings, so "<b>x</b>" or "[a](b)" in a
// device name shows literally (ADR-0013 D8).
//
// Touch targets are at least 48 dp; smaller visuals get padded hit areas.
// Text is in sp, and rows stack their trailing control below the text at a
// font scale of 1.3 and above.
package io.github.dropumi.ui.design

import androidx.annotation.DrawableRes
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.SwitchDefaults
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.ReadOnlyComposable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.ImeAction

/** Whether the user's font scale is large enough that rows and button pairs stack (1.3 and up). */
@Composable
@ReadOnlyComposable
fun largeText(): Boolean = LocalDensity.current.fontScale >= LARGE_FONT_SCALE

private const val LARGE_FONT_SCALE = 1.3f

/** The widest a screen's content gets; wider windows centre it. */
val ContentMaxWidth: Dp = 760.dp

// Buttons

/** Accent fill, pill shape: the main action of a screen. */
@Composable
fun PrimaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    @DrawableRes icon: Int? = null,
) {
    val c = DS.colors
    Button(
        onClick = onClick,
        enabled = enabled,
        shape = CircleShape,
        colors = ButtonDefaults.buttonColors(
            containerColor = c.accent,
            contentColor = c.onAccent,
            disabledContainerColor = c.accent.copy(alpha = 0.4f),
            disabledContentColor = c.onAccent,
        ),
        contentPadding = PaddingValues(horizontal = 18.dp, vertical = 10.dp),
        modifier = modifier.fillMaxWidth().heightIn(min = 52.dp),
    ) {
        if (icon != null) {
            Icon(painterResource(icon), contentDescription = null, modifier = Modifier.size(18.dp))
            Spacer(Modifier.size(8.dp))
        }
        Text(text, style = DSType.button, textAlign = TextAlign.Center)
    }
}

/** An outlined pill: the second of two choices, or a destructive one in [color]. */
@Composable
fun OutlineButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    color: Color = DS.colors.textPrimary,
    outline: Color = DS.colors.border,
) {
    OutlinedButton(
        onClick = onClick,
        enabled = enabled,
        shape = CircleShape,
        border = androidx.compose.foundation.BorderStroke(1.5.dp, if (enabled) outline else outline.copy(alpha = 0.4f)),
        colors = ButtonDefaults.outlinedButtonColors(contentColor = color, disabledContentColor = color.copy(alpha = 0.4f)),
        contentPadding = PaddingValues(horizontal = 18.dp, vertical = 10.dp),
        modifier = modifier.fillMaxWidth().heightIn(min = 52.dp),
    ) {
        Text(text, style = DSType.button, textAlign = TextAlign.Center)
    }
}

/** `accent-soft` fill with `accentText` text: "Send" pills and small actions. 34 dp tall, 48 dp to touch. */
@Composable
fun SoftPill(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
    @DrawableRes icon: Int? = null,
) {
    val c = DS.colors
    Surface(
        onClick = onClick,
        enabled = enabled,
        shape = CircleShape,
        color = if (enabled) c.accentSoft else c.surface2,
        contentColor = if (enabled) c.accentText else c.textSecondary,
        modifier = modifier,
    ) {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.heightIn(min = 34.dp).padding(horizontal = 16.dp, vertical = 6.dp),
        ) {
            if (icon != null) {
                Icon(painterResource(icon), contentDescription = null, modifier = Modifier.size(16.dp))
                Spacer(Modifier.size(6.dp))
            }
            Text(text, style = DSType.smallButton)
        }
    }
}

/** A text action in `accentText` (small teal text keeps 4.5:1), at least 48 dp to touch. */
@Composable
fun LinkButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, @DrawableRes icon: Int? = null) {
    TextButton(
        onClick = onClick,
        enabled = enabled,
        colors = ButtonDefaults.textButtonColors(contentColor = DS.colors.accentText),
        modifier = modifier.heightIn(min = 48.dp),
    ) {
        if (icon != null) {
            Icon(painterResource(icon), contentDescription = null, modifier = Modifier.size(18.dp))
            Spacer(Modifier.size(6.dp))
        }
        Text(text, style = DSType.smallButton)
    }
}

/** An icon-only button with a label for TalkBack; 48 dp. */
@Composable
fun IconAction(@DrawableRes icon: Int, label: String, onClick: () -> Unit, modifier: Modifier = Modifier, tint: Color = DS.colors.textSecondary) {
    IconButton(onClick = onClick, modifier = modifier.size(48.dp)) {
        Icon(painterResource(icon), contentDescription = label, tint = tint, modifier = Modifier.size(20.dp))
    }
}

// Status and labels

/** The status pill ("Paired devices only"): a dot and a short text. */
@Composable
fun StatusPill(text: String, active: Boolean, modifier: Modifier = Modifier, dot: Color? = null) {
    val c = DS.colors
    Row(
        verticalAlignment = Alignment.CenterVertically,
        modifier = modifier
            .clip(CircleShape)
            .background(c.surface2)
            .padding(horizontal = 14.dp, vertical = 8.dp)
            .semantics(mergeDescendants = true) {},
    ) {
        Box(Modifier.size(8.dp).clip(CircleShape).background(dot ?: if (active) c.success else c.textSecondary))
        Spacer(Modifier.size(8.dp))
        Text(text, style = DSType.meta, color = c.textPrimary)
    }
}

/** An uppercase section label in `text-secondary`. */
@Composable
fun SectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(
        text.uppercase(),
        style = DSType.sectionLabel,
        color = DS.colors.textSecondary,
        modifier = modifier.semantics { heading() },
    )
}

/** The big title and subtitle at the top of a screen. */
@Composable
fun ScreenHeader(title: String, subtitle: String?, modifier: Modifier = Modifier) {
    Column(verticalArrangement = Arrangement.spacedBy(4.dp), modifier = modifier.fillMaxWidth()) {
        Text(title, style = DSType.screenTitle, color = DS.colors.textPrimary, modifier = Modifier.semantics { heading() })
        if (subtitle != null) Text(subtitle, style = DSType.body, color = DS.colors.textSecondary)
    }
}

/** A section heading ("Send to", "Earlier"). */
@Composable
fun SectionHeading(text: String, modifier: Modifier = Modifier) {
    Text(text, style = DSType.sectionHeading, color = DS.colors.textPrimary, modifier = modifier.semantics { heading() })
}

/** Small secondary text. */
@Composable
fun Caption(text: String, modifier: Modifier = Modifier, color: Color = DS.colors.textSecondary) {
    Text(text, style = DSType.meta, color = color, modifier = modifier)
}

/** A scrolling screen with the design's padding and background, centred on wide windows. */
@Composable
fun ScreenColumn(
    modifier: Modifier = Modifier,
    horizontalPadding: Dp = DSMetrics.screenPadding,
    content: @Composable ColumnScope.() -> Unit,
) {
    Box(
        contentAlignment = Alignment.TopCenter,
        modifier = modifier.fillMaxSize().background(DS.colors.bg).verticalScroll(rememberScrollState()),
    ) {
        Column(
            verticalArrangement = Arrangement.spacedBy(DSMetrics.sectionGap),
            modifier = Modifier
                .widthIn(max = ContentMaxWidth)
                .fillMaxWidth()
                .padding(start = horizontalPadding, end = horizontalPadding, top = 8.dp, bottom = 32.dp),
            content = content,
        )
    }
}

// Rows and tiles

/** A tinted disc with a device icon. Decorative. */
@Composable
fun DeviceAvatar(size: Dp = 44.dp, @DrawableRes icon: Int = DSIcons.phone) {
    val c = DS.colors
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier.size(size).clip(CircleShape).background(c.accentSoft),
    ) {
        Icon(painterResource(icon), contentDescription = null, tint = c.accentText, modifier = Modifier.size(size * 0.45f))
    }
}

/** A rounded tile with the icon for a file name's kind. Decorative. */
@Composable
fun FileTile(name: String, muted: Boolean = false) {
    val c = DS.colors
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier
            .size(44.dp)
            .clip(RoundedCornerShape(DSMetrics.tileRadius))
            .background(if (muted) c.surface2 else c.accentSoft),
    ) {
        Icon(
            painterResource(DSIcons.file(FileKind.of(name))),
            contentDescription = null,
            tint = if (muted) c.textSecondary else c.accentText,
            modifier = Modifier.size(20.dp),
        )
    }
}

/** The navy disc with the reversed symbol: this device. Decorative. */
@Composable
fun ThisDeviceDisc(size: Dp) {
    Box(
        contentAlignment = Alignment.Center,
        modifier = Modifier.size(size).clip(CircleShape).background(DS.colors.navy),
    ) {
        BrandSymbol(Modifier.size(size * 0.7f))
    }
}

/** A bottom-only divider for list rows. */
fun Modifier.rowDivider(color: Color): Modifier = drawBehind {
    val y = size.height - 0.5.dp.toPx()
    drawLine(color, Offset(0f, y), Offset(size.width, y), strokeWidth = 1.dp.toPx())
}

/**
 * A list row: a leading view, a title, a subtitle and a trailing control.
 * The title and subtitle are read together; the trailing control stays its
 * own focus target. [title] and [subtitle] are shown verbatim. At large
 * font scales the trailing control moves below the text and nothing is cut.
 * [onLongClick] (with [longClickLabel] for TalkBack) adds a long press.
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun DSRow(
    title: String,
    subtitle: String?,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    onLongClick: (() -> Unit)? = null,
    longClickLabel: String? = null,
    leading: @Composable () -> Unit = {},
    trailing: @Composable RowScope.() -> Unit = {},
) {
    val c = DS.colors
    val large = largeText()
    val clickable = if (onClick != null || onLongClick != null) {
        Modifier.combinedClickable(
            onClick = onClick ?: {},
            onLongClick = onLongClick,
            onLongClickLabel = longClickLabel,
        )
    } else {
        Modifier
    }
    val texts: @Composable (Modifier) -> Unit = { m ->
        Column(verticalArrangement = Arrangement.spacedBy(2.dp), modifier = m.semantics(mergeDescendants = true) {}) {
            Text(
                title,
                style = DSType.rowTitle,
                color = c.textPrimary,
                maxLines = if (large) Int.MAX_VALUE else 1,
                overflow = TextOverflow.Ellipsis,
            )
            if (subtitle != null) {
                Text(
                    subtitle,
                    style = DSType.meta,
                    color = c.textSecondary,
                    maxLines = if (large) Int.MAX_VALUE else 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
    if (large) {
        Column(
            verticalArrangement = Arrangement.spacedBy(8.dp),
            modifier = modifier.fillMaxWidth().rowDivider(c.border).then(clickable).padding(vertical = DSMetrics.rowPadding),
        ) {
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                leading()
                texts(Modifier.weight(1f))
            }
            Row(verticalAlignment = Alignment.CenterVertically, content = trailing)
        }
    } else {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
            modifier = modifier
                .fillMaxWidth()
                .heightIn(min = 56.dp)
                .rowDivider(c.border)
                .then(clickable)
                .padding(vertical = DSMetrics.rowPadding),
        ) {
            leading()
            texts(Modifier.weight(1f))
            trailing()
        }
    }
}

/** A settings row: an icon, a title, a subtitle, and a chevron or a trailing control. */
@Composable
fun SettingsRow(
    @DrawableRes icon: Int,
    title: String,
    subtitle: String?,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    trailing: (@Composable () -> Unit)? = null,
) {
    val c = DS.colors
    val large = largeText()
    val base = modifier
        .fillMaxWidth()
        .heightIn(min = 56.dp)
        .rowDivider(c.border)
        .then(if (onClick != null) Modifier.clickable(role = Role.Button, onClick = onClick) else Modifier)
        .padding(vertical = DSMetrics.rowPadding)
    val texts: @Composable (Modifier) -> Unit = { m ->
        Column(verticalArrangement = Arrangement.spacedBy(2.dp), modifier = m) {
            Text(title, style = DSType.rowTitle, color = c.textPrimary)
            if (subtitle != null) Text(subtitle, style = DSType.meta, color = c.textSecondary)
        }
    }
    val iconView: @Composable () -> Unit = {
        Icon(painterResource(icon), contentDescription = null, tint = c.accent, modifier = Modifier.size(22.dp))
    }
    if (large && trailing != null) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = base) {
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
                iconView()
                texts(Modifier.weight(1f))
            }
            trailing()
        }
    } else {
        Row(
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(14.dp),
            modifier = base,
        ) {
            iconView()
            texts(Modifier.weight(1f))
            when {
                trailing != null -> trailing()
                onClick != null -> Icon(
                    painterResource(DSIcons.chevron),
                    contentDescription = null,
                    tint = c.textSecondary,
                    modifier = Modifier.size(18.dp),
                )
            }
        }
    }
}

/** An 8 dp track with an accent fill. Hidden from TalkBack: the status text says it. */
@Composable
fun DSProgressBar(fraction: Double?, modifier: Modifier = Modifier) {
    val c = DS.colors
    val value = (fraction ?: 0.0).coerceIn(0.0, 1.0).toFloat()
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(8.dp)
            .clip(CircleShape)
            .background(c.surface2)
            .clearAndSetSemantics {},
    ) {
        Box(Modifier.fillMaxWidth(value).fillMaxHeight().clip(CircleShape).background(c.accent))
    }
}

/** design.md segmented control: a `surface-2` track with a `surface` selected segment. */
@Composable
fun <T> DSSegmented(
    options: List<Pair<T, String>>,
    selected: T,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
    enabled: Boolean = true,
) {
    val c = DS.colors
    Row(
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(13.dp))
            .background(c.surface2)
            .padding(4.dp)
            .selectableGroup(),
    ) {
        for ((value, label) in options) {
            val isSelected = value == selected
            Box(
                contentAlignment = Alignment.Center,
                modifier = Modifier
                    .weight(1f)
                    .heightIn(min = 48.dp)
                    .clip(RoundedCornerShape(10.dp))
                    .background(if (isSelected) c.surface else Color.Transparent)
                    .selectable(selected = isSelected, enabled = enabled, role = Role.RadioButton, onClick = { onSelect(value) })
                    .padding(horizontal = 6.dp, vertical = 8.dp),
            ) {
                Text(
                    label,
                    style = DSType.smallButton,
                    color = (if (isSelected) c.textPrimary else c.textSecondary).copy(alpha = if (enabled) 1f else 0.5f),
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

/**
 * design.md toggle: on = `accent`. The off state keeps 3:1 against the
 * surface: the track has a `text-secondary` outline and the thumb is
 * `text-secondary`, not white on a light track.
 */
@Composable
fun DSSwitch(checked: Boolean, onCheckedChange: (Boolean) -> Unit, label: String, modifier: Modifier = Modifier, enabled: Boolean = true) {
    val c = DS.colors
    Switch(
        checked = checked,
        onCheckedChange = onCheckedChange,
        enabled = enabled,
        colors = SwitchDefaults.colors(
            checkedThumbColor = c.onAccent,
            checkedTrackColor = c.accent,
            checkedBorderColor = c.accent,
            uncheckedThumbColor = c.textSecondary,
            uncheckedTrackColor = c.surface,
            uncheckedBorderColor = c.textSecondary,
        ),
        modifier = modifier.semantics { contentDescription = label },
    )
}

/** A text field in the design's colours. */
@Composable
fun DSTextField(
    value: String,
    onValueChange: (String) -> Unit,
    label: String,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    monospace: Boolean = false,
    capitalizeAll: Boolean = false,
    keyboardType: KeyboardType = KeyboardType.Text,
    singleLine: Boolean = true,
    isError: Boolean = false,
) {
    val c = DS.colors
    val style: TextStyle = if (monospace) DSType.fingerprint.copy(fontSize = DSType.body.fontSize) else DSType.body
    OutlinedTextField(
        value = value,
        onValueChange = onValueChange,
        label = { Text(label) },
        placeholder = placeholder?.let { { Text(it, style = style.copy(fontFamily = if (monospace) FontFamily.Monospace else style.fontFamily)) } },
        textStyle = style.copy(color = c.textPrimary),
        singleLine = singleLine,
        isError = isError,
        keyboardOptions = KeyboardOptions(
            capitalization = if (capitalizeAll) KeyboardCapitalization.Characters else KeyboardCapitalization.None,
            autoCorrectEnabled = false,
            keyboardType = keyboardType,
            imeAction = ImeAction.Done,
        ),
        colors = OutlinedTextFieldDefaults.colors(
            focusedBorderColor = c.accent,
            unfocusedBorderColor = c.textSecondary,
            focusedLabelColor = c.accentText,
            unfocusedLabelColor = c.textSecondary,
            cursorColor = c.accent,
            focusedContainerColor = c.surface,
            unfocusedContainerColor = c.surface,
            errorBorderColor = c.danger,
            errorLabelColor = c.danger,
        ),
        modifier = modifier.fillMaxWidth(),
    )
}

// Banners, cards and empty states

/** A card on `surface` with the design's radius and border. */
@Composable
fun DSCard(modifier: Modifier = Modifier, padding: Dp = 16.dp, content: @Composable ColumnScope.() -> Unit) {
    val c = DS.colors
    Column(
        verticalArrangement = Arrangement.spacedBy(12.dp),
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(DSMetrics.cardRadius))
            .background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(DSMetrics.cardRadius))
            .padding(padding),
        content = content,
    )
}

/** A notice inside a screen, with an optional action. All its text is the app's own. */
@Composable
fun ProblemCard(title: String, message: String, actionLabel: String? = null, onAction: (() -> Unit)? = null) {
    val c = DS.colors
    Row(
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(18.dp))
            .padding(16.dp),
    ) {
        Icon(painterResource(DSIcons.info), contentDescription = null, tint = c.accent, modifier = Modifier.size(20.dp))
        Column(verticalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.weight(1f)) {
            Text(title, style = DSType.rowTitle, color = c.textPrimary)
            Text(message, style = DSType.meta, color = c.textSecondary)
            if (actionLabel != null && onAction != null) SoftPill(actionLabel, onAction)
        }
    }
}

/** An empty list: an icon, a title and a hint. */
@Composable
fun EmptyState(@DrawableRes icon: Int, title: String, message: String, modifier: Modifier = Modifier) {
    val c = DS.colors
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(10.dp),
        modifier = modifier.fillMaxWidth().padding(24.dp),
    ) {
        Icon(painterResource(icon), contentDescription = null, tint = c.accent, modifier = Modifier.size(30.dp))
        Text(title, style = DSType.rowTitle, color = c.textPrimary, textAlign = TextAlign.Center)
        Text(message, style = DSType.meta, color = c.textSecondary, textAlign = TextAlign.Center)
    }
}

/** The banner at the top of the window. */
@Composable
fun BannerView(
    title: String,
    message: String,
    warning: Boolean,
    actionLabel: String?,
    onAction: (() -> Unit)?,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val c = DS.colors
    Row(
        modifier = modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(16.dp))
            .background(c.surface)
            .border(1.dp, c.border, RoundedCornerShape(16.dp))
            .padding(start = 14.dp, top = 6.dp, bottom = 6.dp),
    ) {
        Icon(
            painterResource(if (warning) DSIcons.warning else DSIcons.info),
            contentDescription = null,
            tint = if (warning) c.danger else c.accent,
            modifier = Modifier.padding(top = 8.dp).size(20.dp),
        )
        Spacer(Modifier.size(12.dp))
        Column(verticalArrangement = Arrangement.spacedBy(4.dp), modifier = Modifier.weight(1f).padding(vertical = 8.dp)) {
            Text(title, style = DSType.rowTitle, color = c.textPrimary)
            Text(message, style = DSType.meta, color = c.textSecondary)
            if (actionLabel != null && onAction != null) {
                LinkButton(actionLabel, onAction, modifier = Modifier.padding(start = 0.dp).testTag("banner.action"))
            }
        }
        IconAction(DSIcons.close, "Dismiss", onDismiss)
    }
}
