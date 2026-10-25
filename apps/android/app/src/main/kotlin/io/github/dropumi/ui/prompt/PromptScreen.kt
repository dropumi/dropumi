// SPDX-License-Identifier: MPL-2.0
//
// The consent and code screens (ADR-0004, ADR-0009, ADR-0013 D8; a port of
// the Apple app's PromptPresenter). Offers and codes, one at a time:
// - Drawn full screen inside MainActivity's own window, on the navy palette,
//   over everything else in it. Never a Compose Dialog, which would be a
//   separate window that the activity's overlay filtering cannot see.
// - Only the core closes a prompt: after an answer it shows "Waiting…" until
//   PromptClosed. At zero it reads "Expired" and Accept is off; nothing is
//   answered by itself.
// - No default action: nothing is focused when it opens, and Enter, NumPad
//   Enter, DPAD centre and Space act only on a button the user moved focus
//   to with Tab or an arrow (MainActivity, KeyGate), never on focus Android
//   gave a button by itself. On the code screen both choices are
//   equal-weight outline buttons.
// - Accept and "Codes match" are enabled 600 ms after the prompt appears,
//   and again 600 ms after the activity resumes or its window regains focus
//   (PromptArming; AppModel ignores an earlier Accept whatever the UI does).
// - Back (a completed back; a cancelled predictive back does nothing) and
//   Escape decline. Back is ignored while an answer is in flight.
// - Peer strings (names, file names) are passed to Text as plain Strings:
//   never HTML, Markdown, links or format strings.
// - No FLAG_SECURE (the code is not secret), and nothing on the lock screen.
package io.github.dropumi.ui.prompt

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.focusTarget
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.Formatting
import io.github.dropumi.appstate.PeerLabel
import io.github.dropumi.appstate.PromptArming
import io.github.dropumi.appstate.promptId
import io.github.dropumi.ffi.CodePurpose
import io.github.dropumi.ffi.OfferedFile
import io.github.dropumi.ffi.Prompt
import io.github.dropumi.ui.KeySurface
import io.github.dropumi.ui.LocalKeyGate
import io.github.dropumi.ui.LocalTouchGuard
import io.github.dropumi.ui.OverlayNotice
import io.github.dropumi.ui.OverlayNoticeText
import io.github.dropumi.ui.gatedButton
import io.github.dropumi.ui.design.DSIcons
import io.github.dropumi.ui.design.DSMetrics
import io.github.dropumi.ui.design.DSPalette
import io.github.dropumi.ui.design.DSReceive
import io.github.dropumi.ui.design.DSType
import io.github.dropumi.ui.design.DeviceAvatar
import io.github.dropumi.ui.design.DropumiTheme
import io.github.dropumi.ui.design.FileKind
import io.github.dropumi.ui.design.OutlineButton
import io.github.dropumi.ui.design.PrimaryButton
import io.github.dropumi.ui.design.largeText
import io.github.dropumi.ui.screens.LocalAppScope
import io.github.dropumi.ui.screens.LocalModel
import io.github.dropumi.ui.screens.ScreenText
import io.github.dropumi.ui.screens.rememberNowMs
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The prompt on screen, if any, drawn over everything else in the window.
 * [focus] marks where keyboard focus lands when it would enter the screens
 * below: the prompt's content, not a button.
 */
@Composable
fun PromptHost(state: AppState, focus: FocusRequester) {
    val prompt = state.currentPrompt ?: return
    val model = LocalModel.current
    val scope = LocalAppScope.current
    val id = prompt.promptId
    val waiting = id in state.answering || id in state.answered

    // A completed back declines; a predictive back the user cancels does
    // nothing. While this is registered the system never runs back-to-home.
    BackHandler { scope.launch { model.back() } }

    // Arming: 600 ms after this prompt appeared, or after the latest resume
    // or regained window focus (each starts a new epoch).
    val epoch = state.arming.epoch
    LaunchedEffect(id, epoch) {
        delay(PromptArming.ARM_AFTER_MS)
        model.promptElapsed(id, epoch, PromptArming.ARM_AFTER_MS)
    }
    val armed = state.arming.canAccept(id)

    // Nothing behind the prompt keeps the keyboard focus (or a keyboard).
    val focusManager = LocalFocusManager.current
    LaunchedEffect(id) { focusManager.clearFocus(force = true) }

    val answer: (Boolean) -> Unit = { yes -> scope.launch { model.answer(prompt, yes) } }
    // Each button reports its focus, so an activation key acts only on one
    // the user moved focus to (MainActivity, KeyGate).
    val gate = LocalKeyGate.current
    val surface = KeySurface.Prompt(id)
    val button: (String) -> Modifier = { name -> Modifier.gatedButton(gate, surface, name).testTag(name) }
    DropumiTheme(darkTheme = true) {
        when (prompt) {
            is Prompt.Offer -> OfferPrompt(prompt, armed, waiting, focus, button, answer)
            is Prompt.Code -> CodePrompt(prompt, armed, waiting, focus, button, answer)
        }
    }
}

/** Whole seconds until [expiresAtMs] (wall clock, as the core sets it), never below zero. */
@Composable
private fun secondsLeft(expiresAtMs: ULong): Long {
    val now = rememberNowMs(ticking = true)
    val left = (expiresAtMs.toLong() - now) / 1_000
    return left.coerceAtLeast(0)
}

/**
 * The frame both prompts share: navy, full window, scrolling content, then
 * the overlay notice, "Waiting…" and the two buttons at the bottom (side by
 * side, stacked from a font scale of 1.3). It takes every touch, so nothing
 * reaches the screen below.
 */
@Composable
private fun PromptLayout(
    title: String,
    tag: String,
    waiting: Boolean,
    focus: FocusRequester,
    first: @Composable (Modifier) -> Unit,
    second: @Composable (Modifier) -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) {
    val guard = LocalTouchGuard.current
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier
            .fillMaxSize()
            .background(DSReceive.background)
            .pointerInput(Unit) { awaitPointerEventScope { while (true) awaitPointerEvent() } }
            .windowInsetsPadding(WindowInsets.safeDrawing)
            .semantics { paneTitle = title }
            .testTag(tag),
    ) {
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            modifier = Modifier
                .weight(1f)
                .fillMaxWidth()
                .focusRequester(focus)
                .focusTarget()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = DSMetrics.screenPadding, vertical = 16.dp),
        ) {
            Column(
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(18.dp),
                modifier = Modifier.widthIn(max = 520.dp).fillMaxWidth(),
                content = content,
            )
        }
        Column(
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(12.dp),
            modifier = Modifier
                .widthIn(max = 520.dp)
                .fillMaxWidth()
                .padding(horizontal = DSMetrics.screenPadding)
                .padding(bottom = DSMetrics.screenPadding, top = 8.dp),
        ) {
            if (guard.notice == OverlayNotice.ANSWER) {
                OverlayNoticeText(guard, OverlayNotice.ANSWER, color = DSPalette.Dark.danger)
            }
            if (waiting) {
                Row(
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(10.dp),
                    modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite }.testTag("prompt.waiting"),
                ) {
                    CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp, color = DSReceive.accent)
                    Text("Waiting…", style = DSType.meta, color = DSReceive.muted)
                }
            }
            if (largeText()) {
                first(Modifier)
                second(Modifier)
            } else {
                // Same width and height, even when one label wraps.
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.height(IntrinsicSize.Min)) {
                    first(Modifier.weight(1f).fillMaxHeight())
                    second(Modifier.weight(1f).fillMaxHeight())
                }
            }
        }
    }
}

@Composable
private fun OfferPrompt(
    offer: Prompt.Offer,
    armed: Boolean,
    waiting: Boolean,
    focus: FocusRequester,
    button: (String) -> Modifier,
    answer: (Boolean) -> Unit,
) {
    val label = PeerLabel.of(offer.peer)
    val left = secondsLeft(offer.expiresAtMs)
    PromptLayout(
        title = "Incoming files",
        tag = "prompt.offer",
        waiting = waiting,
        focus = focus,
        first = { m ->
            OutlineButton(
                "Decline",
                { answer(false) },
                enabled = !waiting,
                color = Color.White,
                outline = DSReceive.outline,
                modifier = m.then(button("offer.decline")),
            )
        },
        second = { m ->
            PrimaryButton(
                "Accept",
                { answer(true) },
                enabled = armed && !waiting && left > 0,
                icon = DSIcons.check,
                modifier = m.then(button("offer.accept")),
            )
        },
    ) {
        Text(
            "Incoming files",
            style = DSType.smallButton,
            color = DSReceive.accent,
            modifier = Modifier.clip(CircleShape).background(DSReceive.card).padding(horizontal = 12.dp, vertical = 6.dp),
        )
        DeviceAvatar(size = 72.dp)
        PeerIdentity(label, fingerprint = if (label.verified) null else offer.peer.fingerprintDisplay)
        Text(
            "wants to send you " + Formatting.fileCount(offer.fileCount.toInt()),
            style = DSType.body,
            color = DSReceive.muted,
            textAlign = TextAlign.Center,
        )
        val code = offer.code
        if (code != null) {
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("Compare this code with the sender's screen", style = DSType.meta, color = DSReceive.muted, textAlign = TextAlign.Center)
                SasCode(code)
                Text("Accept only if both screens show the same code.", style = DSType.meta, color = DSReceive.muted, textAlign = TextAlign.Center)
            }
        }
        OfferFiles(offer)
        Expiry(left)
    }
}

@Composable
private fun CodePrompt(
    prompt: Prompt.Code,
    armed: Boolean,
    waiting: Boolean,
    focus: FocusRequester,
    button: (String) -> Modifier,
    answer: (Boolean) -> Unit,
) {
    val label = PeerLabel.of(prompt.peer)
    val left = secondsLeft(prompt.expiresAtMs)
    val pairing = prompt.purpose == CodePurpose.PAIRING
    val title = if (pairing) "Pair with this device?" else "Check the code"
    PromptLayout(
        title = title,
        tag = "prompt.code",
        waiting = waiting,
        focus = focus,
        // Equal weight: two outline buttons of the same size, neither filled.
        first = { m ->
            OutlineButton(
                "Codes don't match",
                { answer(false) },
                enabled = !waiting,
                color = Color.White,
                outline = DSReceive.outline,
                modifier = m.then(button("code.mismatch")),
            )
        },
        second = { m ->
            OutlineButton(
                "Codes match",
                { answer(true) },
                enabled = armed && !waiting && left > 0,
                color = DSReceive.accent,
                outline = DSReceive.accent,
                modifier = m.then(button("code.match")),
            )
        },
    ) {
        Icon(
            painterResource(DSIcons.shield),
            contentDescription = null,
            tint = DSReceive.accent,
            modifier = Modifier.padding(top = 24.dp).size(34.dp),
        )
        Text(
            title,
            style = DSType.sectionHeading,
            color = Color.White,
            textAlign = TextAlign.Center,
            modifier = Modifier.semantics { heading() },
        )
        PeerIdentity(label, fingerprint = prompt.peer.fingerprintDisplay)
        SasCode(prompt.code)
        Text(
            if (pairing) {
                "Check that the other device shows the same code. Only then tap Codes match."
            } else {
                "This device isn't paired. Check that its screen shows the same code before anything is sent."
            },
            style = DSType.body,
            color = DSReceive.muted,
            textAlign = TextAlign.Center,
        )
        Expiry(left)
    }
}

/**
 * Who is asking (ADR-0004): the local alias first and "Paired", or
 * "Unverified device", the name it claims and its fingerprint. Every string
 * from the peer is plain text.
 */
@Composable
private fun PeerIdentity(label: PeerLabel, fingerprint: String?) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label.title, style = DSType.dialogName, color = Color.White, textAlign = TextAlign.Center, modifier = Modifier.testTag("prompt.peer"))
        val claim = label.claim
        if (claim != null) {
            Text(
                "claims “" + claim + "”",
                style = DSType.body,
                color = DSReceive.muted,
                textAlign = TextAlign.Center,
                modifier = Modifier.testTag("prompt.claim"),
            )
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Icon(
                painterResource(if (label.verified) DSIcons.shield else DSIcons.warning),
                contentDescription = null,
                tint = if (label.verified) DSReceive.accent else DSReceive.muted,
                modifier = Modifier.size(16.dp),
            )
            Text(
                if (label.verified) "Paired" else "Not paired",
                style = DSType.smallButton,
                color = if (label.verified) DSReceive.accent else DSReceive.muted,
            )
        }
        if (fingerprint != null) {
            Text(
                fingerprint,
                style = DSType.fingerprint,
                color = DSReceive.muted,
                textAlign = TextAlign.Center,
                modifier = Modifier
                    .semantics { contentDescription = "Fingerprint: " + ScreenText.spokenFingerprint(fingerprint.filter { it != '-' }) }
                    .testTag("prompt.fingerprint"),
            )
        }
    }
}

/** The six-digit code, large, in two groups; TalkBack reads it digit by digit. */
@Composable
private fun SasCode(digits: String) {
    Text(
        Formatting.code(digits),
        style = DSType.code,
        color = Color.White,
        textAlign = TextAlign.Center,
        modifier = Modifier
            .semantics { contentDescription = "Verification code: " + Formatting.spokenCode(digits) }
            .testTag("code.digits"),
    )
}

/** "Expires in m:ss", then "Expired", which TalkBack announces once. */
@Composable
private fun Expiry(secondsLeft: Long) {
    val expired = secondsLeft == 0L
    Text(
        if (expired) "Expired" else "Expires in " + Formatting.countdown(secondsLeft),
        style = DSType.meta,
        color = DSReceive.muted,
        modifier = Modifier
            .then(if (expired) Modifier.semantics { liveRegion = LiveRegionMode.Polite } else Modifier)
            .testTag("prompt.expiry"),
    )
}

/**
 * The offered files and their total. The prompt carries the first 200;
 * the rest are read from the core a page at a time (at most 1000 each) as
 * the list is scrolled to its end.
 */
@Composable
private fun OfferFiles(offer: Prompt.Offer) {
    val model = LocalModel.current
    val total = offer.fileCount.toInt()
    val files = remember(offer.id) { mutableStateListOf<OfferedFile>().apply { addAll(offer.files.take(total)) } }
    var exhausted by remember(offer.id) { mutableStateOf(false) }
    val more = total - files.size
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(18.dp))
            .background(DSReceive.card)
            .padding(16.dp),
    ) {
        LazyColumn(Modifier.fillMaxWidth().heightIn(max = if (largeText()) 360.dp else 220.dp).testTag("prompt.files")) {
            items(files.size) { index -> FileRow(files[index]) }
            if (more > 0) {
                item(key = "more") {
                    Text(
                        "and $more more",
                        style = DSType.meta,
                        color = DSReceive.muted,
                        modifier = Modifier.fillMaxWidth().padding(vertical = 9.dp).testTag("prompt.files.more"),
                    )
                    if (!exhausted) {
                        LaunchedEffect(files.size) {
                            val page = model.offerFiles(offer.id, offset = files.size, limit = more)
                            if (page.isNullOrEmpty()) exhausted = true else files.addAll(page.take(more))
                        }
                    }
                }
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(DSReceive.divider))
        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth().padding(top = 10.dp)) {
            Text("Total", style = DSType.meta, color = DSReceive.muted, modifier = Modifier.weight(1f))
            Text(Formatting.bytes(offer.totalBytes), style = DSType.rowTitle, color = Color.White)
        }
    }
}

@Composable
private fun FileRow(file: OfferedFile) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
        modifier = Modifier.fillMaxWidth().padding(vertical = 9.dp).semantics(mergeDescendants = true) {},
    ) {
        Icon(
            painterResource(DSIcons.file(FileKind.of(file.name))),
            contentDescription = null,
            tint = DSReceive.accent,
            modifier = Modifier.size(18.dp),
        )
        Text(
            file.name,
            style = DSType.body,
            color = Color.White,
            maxLines = 1,
            overflow = TextOverflow.MiddleEllipsis,
            modifier = Modifier.weight(1f),
        )
        Text(Formatting.bytes(file.size), style = DSType.meta, color = DSReceive.muted)
    }
}
