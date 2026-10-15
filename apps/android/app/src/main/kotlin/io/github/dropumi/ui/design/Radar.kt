// SPDX-License-Identifier: MPL-2.0
//
// The radar from design.md (a port of the Apple app's RadarView): this
// device at the centre, three rings, nearby devices as discs on the rings.
// Decorative: hidden from TalkBack (the device list below it is the
// accessible way to pick a device). Nothing moves except a short grow when a
// device appears, and with animations turned off in the system settings
// even that is skipped (Compose follows the animator duration scale).
package io.github.dropumi.ui.design

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sin

/** A device on the radar: its discovery instance and the name it advertises (shown verbatim). */
data class RadarItem(val id: String, val name: String)

object RadarLayout {
    /** Eight places, alternating between the two outer rings, so nothing covers this device. */
    const val SLOTS = 8

    private val RINGS = listOf(0.36f, 0.66f, 0.96f)

    /** The ring sizes, as fractions of the radar's side. */
    val rings: List<Float> get() = RINGS

    /**
     * A stable place for each device (from an FNV-1a hash of its id; the
     * next free place if that one is taken), so devices do not jump around
     * when the list changes. The same rule as the Apple app.
     */
    fun assignSlots(ids: List<String>): Map<String, Int> {
        val taken = HashSet<Int>()
        val result = LinkedHashMap<String, Int>()
        for (id in ids.take(SLOTS)) {
            var hash = 1469598103934665603uL
            for (byte in id.toByteArray(Charsets.UTF_8)) {
                hash = (hash xor byte.toUByte().toULong()) * 1099511628211uL
            }
            var slot = (hash % SLOTS.toULong()).toInt()
            while (slot in taken) slot = (slot + 1) % SLOTS
            taken += slot
            result[id] = slot
        }
        return result
    }

    /** The centre of [slot], relative to the radar's centre, for a radar of [side]. */
    fun position(slot: Int, side: Float): Pair<Float, Float> {
        val angle = Math.toRadians(slot * 45.0 - 112.5)
        val ring = (if (slot % 2 == 0) 0.96f else 0.66f) * side / 2
        return (cos(angle) * ring).toFloat() to (sin(angle) * ring).toFloat()
    }
}

@Composable
fun Radar(items: List<RadarItem>, modifier: Modifier = Modifier) {
    val c = DS.colors
    val density = LocalDensity.current
    BoxWithConstraints(
        modifier = modifier
            .fillMaxWidth()
            .aspectRatio(0.92f)
            .clearAndSetSemantics {},
    ) {
        val width = with(density) { maxWidth.toPx() }
        val height = with(density) { maxHeight.toPx() }
        // Room below the outer ring for the last row of labels.
        val side = min(width, height - with(density) { 28.dp.toPx() })
        val midX = width / 2
        val midY = side / 2
        Canvas(Modifier.matchParentSize()) {
            drawCircle(
                brush = Brush.radialGradient(
                    listOf(c.accentSoft.copy(alpha = 0.6f), Color.Transparent),
                    center = androidx.compose.ui.geometry.Offset(midX, midY),
                    radius = side / 2,
                ),
                radius = side / 2,
                center = androidx.compose.ui.geometry.Offset(midX, midY),
            )
            for (scale in RadarLayout.rings) {
                drawCircle(
                    color = c.ring,
                    radius = side * scale / 2,
                    center = androidx.compose.ui.geometry.Offset(midX, midY),
                    style = Stroke(width = 1.5.dp.toPx()),
                )
            }
        }
        val centre = with(density) { (side * 0.26f).toDp() }
        Box(
            Modifier
                .offset { IntOffset((midX - side * 0.13f).roundToInt(), (midY - side * 0.13f).roundToInt()) }
                .shadow(14.dp, CircleShape, ambientColor = c.navy, spotColor = c.navy),
        ) {
            ThisDeviceDisc(centre)
        }
        val shown = items.take(RadarLayout.SLOTS)
        val slots = RadarLayout.assignSlots(shown.map { it.id })
        for (item in shown) {
            val (dx, dy) = RadarLayout.position(slots[item.id] ?: 0, side)
            RadarDevice(
                item = item,
                centreX = midX + dx,
                centreY = midY + dy + with(density) { 12.dp.toPx() },
            )
        }
    }
}

/** One device: a disc and its name, centred on (centreX, centreY). */
@Composable
private fun RadarDevice(item: RadarItem, centreX: Float, centreY: Float) {
    val c = DS.colors
    val density = LocalDensity.current
    val grow = remember(item.id) { Animatable(0.6f) }
    LaunchedEffect(item.id) { grow.animateTo(1f, tween(durationMillis = 350)) }
    val labelWidth = 120.dp
    val halfWidth = with(density) { (labelWidth / 2).toPx() }
    val halfDisc = with(density) { 22.dp.toPx() }
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(4.dp),
        modifier = Modifier
            .offset { IntOffset((centreX - halfWidth).roundToInt(), (centreY - halfDisc).roundToInt()) }
            .width(labelWidth)
            .graphicsLayer {
                scaleX = grow.value
                scaleY = grow.value
                alpha = ((grow.value - 0.6f) / 0.4f).coerceIn(0f, 1f)
            },
    ) {
        Box(
            Modifier
                .shadow(8.dp, CircleShape, ambientColor = c.navy, spotColor = c.navy)
                .clip(CircleShape)
                .background(c.surface),
        ) {
            DeviceAvatar(size = 44.dp, icon = DSIcons.phone)
        }
        Text(
            item.name,
            style = DSType.meta,
            color = c.textPrimary,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier
                .clip(CircleShape)
                .background(c.bg.copy(alpha = 0.92f))
                .padding(horizontal = 6.dp, vertical = 2.dp),
        )
    }
}
