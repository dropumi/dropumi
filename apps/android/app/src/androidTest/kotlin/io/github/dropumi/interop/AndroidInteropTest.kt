// SPDX-License-Identifier: MPL-2.0
//
// The app's real core (AppModel over LiveCoreClient, in the interop graph:
// runner argument `dropumi_app interop`) against the real CLI on the host
// (plan §8 D). Driven by crates/cli/tests/e2e/interop/android.rs with
// DROPUMI_TEST_ANDROID=1 through scripts/android/interop.sh, which runs one
// scenario per `am instrument -e class …#<scenario>` on the minified qa
// build. The harness passes the CLI's address and fingerprint as runner
// arguments (`dropumi_cli_addr`, `dropumi_cli_fp`) and reads
// `INSTRUMENTATION_STATUS: DROPUMI_STATUS={json}` lines, sent with
// Instrumentation.sendStatus while the test runs.
//
// This test plays the user. It confirms a code or accepts an offer only if
// the authenticated fingerprint is the CLI's, and only once the prompt is
// armed: it runs the UI's 600 ms arming timer itself (ADR-0013 D8), because
// most scenarios show no activity. The harness then checks that both sides
// showed the same code and that the bytes arrived intact.
//
// Reachability: the CLI listens on the host's loopback and the app dials
// 127.0.0.1 on the device through `adb reverse`; the CLI reaches the app's
// listener through `adb forward`. So every peer the app sees is 127.0.0.1,
// and per-IP admission is not covered here. The `nsd*` scenarios are the
// exception: they run on a real device on the host's Wi-Fi and use NSD.
//
// @CliInterop: neither pass of scripts/android/ui-tests.sh runs these.

package io.github.dropumi.interop

import android.app.Activity
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.runner.lifecycle.ActivityLifecycleMonitorRegistry
import androidx.test.runner.lifecycle.Stage
import io.github.dropumi.CliInterop
import io.github.dropumi.MainActivity
import io.github.dropumi.TestApplication
import io.github.dropumi.appstate.AppLoader
import io.github.dropumi.appstate.AppModel
import io.github.dropumi.appstate.AppState
import io.github.dropumi.appstate.HistoryEntry
import io.github.dropumi.appstate.OutgoingFile
import io.github.dropumi.appstate.PromptArming
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.appstate.promptId
import io.github.dropumi.core.discovery.AdvertiseState
import io.github.dropumi.ffi.CodePurpose
import io.github.dropumi.ffi.PairingResult
import io.github.dropumi.ffi.Prompt
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.foreign.ForeignProvider
import io.github.dropumi.foreign.ForeignSharer
import java.io.File
import java.security.MessageDigest
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
@CliInterop
class AndroidInteropTest {
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val arguments: Bundle get() = InstrumentationRegistry.getArguments()
    private val app: TestApplication get() = instrumentation.targetContext.applicationContext as TestApplication

    /** One status line for the harness, sent at once (not at the end of the test). */
    private fun status(event: String, fields: Map<String, String> = emptyMap()) {
        val json = JSONObject(fields + ("event" to event)).toString()
        instrumentation.sendStatus(STATUS_IN_PROGRESS, Bundle().apply { putString(STATUS_KEY, json) })
    }

    private fun argument(key: String): String =
        arguments.getString(key)?.takeIf { it.isNotEmpty() } ?: throw AssertionError("missing runner argument $key")

    /**
     * Opens the interop graph's model (the real core on
     * noBackupFilesDir/interop-<uuid>) and reports it ready.
     */
    private fun openModel(): AppModel {
        assertEquals(
            "run with -e ${TestApplication.APP_ARGUMENT} ${TestApplication.INTEROP}",
            TestApplication.INTEROP,
            arguments.getString(TestApplication.APP_ARGUMENT),
        )
        val graph = app.graph
        val ready = runBlocking(Dispatchers.Main) {
            graph.loader.load()
            withTimeout(STEP_MS) { graph.loader.state.first { it !is AppLoader.State.Loading } }
        }
        val model = (ready as? AppLoader.State.Ready)?.model ?: throw AssertionError("the real core did not open: $ready")
        val identity = runBlocking(Dispatchers.Main) { until(model, "the identity") { it.identity } }
        status(
            "ready",
            mapOf(
                "fingerprint" to identity.fingerprint,
                "sdk" to Build.VERSION.SDK_INT.toString(),
                "abi" to Build.SUPPORTED_ABIS.firstOrNull().orEmpty(),
            ),
        )
        return model
    }

    /** Runs [scenario] on the model's thread (the main thread), as the screens do. */
    private fun play(scenario: suspend (AppModel) -> Unit) {
        val model = openModel()
        try {
            runBlocking(Dispatchers.Main) { scenario(model) }
        } finally {
            status("done")
        }
    }

    /** The app sends a file to a CLI receiver it has never met: both compare a code, then the file goes. */
    @Test
    fun appSendsToCli() = play { model ->
        val address = argument(CLI_ADDR)
        val cliFingerprint = argument(CLI_FP)
        val file = File(sourceDir(), "from app.bin")
        file.writeBytes(ByteArray(3 * 1024 * 1024 + 7) { (it * 31 + 7).toByte() })
        status("file", mapOf("name" to file.name, "sha256" to sha256(file.readBytes())))
        model.addToDraft(listOf(OutgoingFile(file.name, file.length().toULong(), OutgoingFile.Source.Path(file.path))))
        model.sendDraft(SendTarget.Address(address, cliFingerprint), "CLI", allowUnverified = true)
        val entry = confirmCodeAndFinish(model, cliFingerprint)
        assertEquals(HistoryEntry.Outcome.COMPLETED, entry.outcome)
    }

    /**
     * The app sends a document another app shared with it: ForeignSharer
     * (the test package's own UID) shares ForeignProvider's document to
     * ShareActivity with a read grant; MainActivity opens with it in the
     * draft. The send reads it through the app's real path
     * (AndroidDocuments: UriGuard, ContentUriFile, AndroidDocumentSource).
     */
    @Test
    fun appSendsContentUri() = play { model ->
        val address = argument(CLI_ADDR)
        val cliFingerprint = argument(CLI_FP)
        val context = instrumentation.context
        context.startActivity(
            Intent().setClassName(context, ForeignSharer::class.java.name).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
        )
        try {
            val shared = until(model, "the shared document in the draft") { it.draft.singleOrNull() }
            assertTrue("a content URI", shared.source is OutgoingFile.Source.Document)
            status("file", mapOf("name" to shared.name, "sha256" to sha256(ForeignProvider.content())))
            model.sendDraft(SendTarget.Address(address, cliFingerprint), "CLI", allowUnverified = true)
            val entry = confirmCodeAndFinish(model, cliFingerprint)
            assertEquals(HistoryEntry.Outcome.COMPLETED, entry.outcome)
        } finally {
            finishActivities()
        }
    }

    /** A CLI sender the app has never met: the app lets everyone send, shows the code with the offer, and accepts. */
    @Test
    fun cliSendsToApp() = play { model ->
        val cliFingerprint = argument(CLI_FP)
        model.finishOnboarding(ReceiveVisibility.EVERYONE)
        val receiver = until(model, "receiving") { it.receiver }
        status("listening", mapOf("port" to receiver.port.toString()))
        val prompt = until(model, "an offer") { it.currentPrompt as? Prompt.Offer }
        val matches = prompt.peer.fingerprint == cliFingerprint && !prompt.peer.trusted
        status(
            "offer",
            mapOf(
                "code" to prompt.code.orEmpty(),
                "matches" to matches.toString(),
                "file" to prompt.files.firstOrNull()?.name.orEmpty(),
            ),
        )
        answer(model, prompt, matches)
        val entry = until(model, "the transfer to end") { it.history.firstOrNull() }
        val receiveDir = app.interopPaths.receiveDir
        val fields = mutableMapOf("outcome" to entry.outcome.name.lowercase())
        entry.savedAs.firstOrNull()?.let { name ->
            val saved = File(receiveDir, name)
            fields["saved_as"] = name
            fields["sha256"] = sha256(saved.readBytes())
            // App-private: under the app's own data, never shared storage.
            fields["private"] = saved.canonicalPath.startsWith(app.noBackupFilesDir.canonicalPath + File.separator).toString()
        }
        fields["part_files"] = receiveDir.list().orEmpty().count { it.endsWith(".part") }.toString()
        status("finished", fields)
        assertTrue(matches)
        assertEquals(HistoryEntry.Outcome.COMPLETED, entry.outcome)
        model.refreshTrusted()
        assertTrue("accepting an unverified sender never trusts it", model.state.value.trusted.isEmpty())
    }

    /** The app pairs with a CLI pairing window. */
    @Test
    fun pairingCodesMatch() = play { model ->
        val address = argument(CLI_ADDR)
        val cliFingerprint = argument(CLI_FP)
        model.pair(SendTarget.Address(address, null), "CLI")
        val prompt = until(model, "a code") { it.currentPrompt as? Prompt.Code }
        val matches = prompt.peer.fingerprint == cliFingerprint && prompt.purpose == CodePurpose.PAIRING
        status("code", mapOf("code" to prompt.code, "matches" to matches.toString()))
        answer(model, prompt, matches)
        val result = until(model, "pairing to end") { it.pairing?.result }
        model.refreshTrusted()
        val trusted = model.state.value.trusted.map { it.fingerprint }
        status("paired", mapOf("paired" to (result is PairingResult.Paired).toString(), "trusted" to trusted.joinToString(",")))
        assertTrue(matches)
        assertTrue("paired: $result", result is PairingResult.Paired)
        assertEquals(listOf(cliFingerprint), trusted)
    }

    /** Paired only: a sender the app has not paired with is refused before any prompt. */
    @Test
    fun unpairedSenderRefused() = play { model ->
        model.finishOnboarding(ReceiveVisibility.PAIRED_ONLY)
        val receiver = until(model, "receiving") { it.receiver }
        status("listening", mapOf("port" to receiver.port.toString()))
        awaitRefusal(model)
    }

    /**
     * Device only. The app receives (Paired only) and advertises where the
     * R21 gate is open; the harness looks for it with the CLI, then sends
     * without being paired, which must be refused.
     */
    @Test
    fun nsdAdvertise() = play { model ->
        model.finishOnboarding(ReceiveVisibility.PAIRED_ONLY)
        val receiver = until(model, "receiving") { it.receiver }
        val gateOpen = model.state.value.advertising != AdvertiseState.GateClosed
        if (gateOpen) until(model, "the advertisement") { it.advertising.takeIf { a -> a == AdvertiseState.Registered } }
        status(
            "advertising",
            mapOf(
                "gate_open" to gateOpen.toString(),
                "instance" to receiver.instance.orEmpty(),
                "port" to receiver.port.toString(),
            ),
        )
        awaitRefusal(model)
    }

    /** Device only. The app finds the CLI with NSD and sends to it (unverified, so both compare a code). */
    @Test
    fun nsdBrowseAndSend() = play { model ->
        val cliFingerprint = argument(CLI_FP)
        val hint = cliFingerprint.take(HINT_LENGTH)
        model.beginBrowsing()
        try {
            val peer = until(model, "the CLI nearby") { s -> s.nearby.firstOrNull { it.fingerprintHint == hint } }
            status(
                "found",
                mapOf("name" to peer.name, "compatible" to peer.compatible.toString(), "looks_like" to peer.hintAlias.orEmpty()),
            )
            val file = File(sourceDir(), "via nsd.bin")
            file.writeBytes(ByteArray(512 * 1024) { (it * 13).toByte() })
            status("file", mapOf("name" to file.name, "sha256" to sha256(file.readBytes())))
            model.addToDraft(listOf(OutgoingFile(file.name, file.length().toULong(), OutgoingFile.Source.Path(file.path))))
            model.sendDraft(SendTarget.Nearby(peer.instance), peer.name, allowUnverified = true)
            val entry = confirmCodeAndFinish(model, cliFingerprint)
            assertEquals(HistoryEntry.Outcome.COMPLETED, entry.outcome)
        } finally {
            model.endBrowsing()
        }
    }

    /** The code prompt of a send: confirmed only for the CLI's key; then the transfer's history entry. */
    private suspend fun confirmCodeAndFinish(model: AppModel, cliFingerprint: String): HistoryEntry {
        val prompt = until(model, "a code") { it.currentPrompt as? Prompt.Code }
        val matches = prompt.peer.fingerprint == cliFingerprint
        status("code", mapOf("code" to prompt.code, "matches" to matches.toString(), "purpose" to prompt.purpose.name.lowercase()))
        answer(model, prompt, matches)
        val entry = until(model, "the transfer to end") { it.history.firstOrNull() }
        status("finished", mapOf("outcome" to entry.outcome.name.lowercase()))
        assertTrue("the code came from the CLI's key", matches)
        return entry
    }

    /** A refused sender shows a notice, never a prompt or a transfer. */
    private suspend fun awaitRefusal(model: AppModel) {
        val banner = until(model, "the refused sender") { it.banner }
        status("refused", mapOf("title" to banner.title))
        assertTrue("a sender that isn't paired never reaches a prompt", model.state.value.prompts.isEmpty())
        assertTrue(model.state.value.history.isEmpty())
    }

    /**
     * Answers [prompt]. A Yes waits for the prompt to arm, running the UI's
     * timer (PromptScreen: ARM_AFTER_MS after the arming epoch began): the
     * model ignores an Accept for a prompt that is not armed.
     */
    private suspend fun answer(model: AppModel, prompt: Prompt, yes: Boolean) {
        val id = prompt.promptId
        if (yes) {
            withTimeout(STEP_MS) {
                while (!model.state.value.arming.canAccept(id)) {
                    val epoch = model.state.first { it.arming.promptId == id }.arming.epoch
                    val started = SystemClock.elapsedRealtime()
                    delay(PromptArming.ARM_AFTER_MS)
                    model.promptElapsed(id, epoch, SystemClock.elapsedRealtime() - started)
                }
            }
        }
        model.answer(prompt, yes)
    }

    /**
     * Waits, bounded, until [value] gives something. On timeout it reports
     * what the model knows about receiving and discovery (the app's own
     * texts, never a peer's).
     */
    private suspend fun <T : Any> until(model: AppModel, what: String, value: (AppState) -> T?): T {
        var found: T? = null
        withTimeoutOrNull(STEP_MS) { model.state.first { s -> value(s)?.also { found = it } != null } }
        found?.let { return it }
        val s = model.state.value
        status(
            "timeout",
            mapOf(
                "waiting_for" to what,
                "receiving" to (s.receiver != null).toString(),
                "nearby" to s.nearby.size.toString(),
                "banner" to s.banner?.title.orEmpty(),
                "discovery_problem" to s.discoveryProblem.orEmpty(),
            ),
        )
        throw AssertionError("timed out waiting for $what")
    }

    /** A folder for the files the app sends, next to the interop graph's folders. */
    private fun sourceDir(): File =
        File(app.interopPaths.configDir.parentFile, "sources").apply { check(isDirectory || mkdirs()) }

    /** Finishes MainActivity (opened by the share) and waits until none is left. */
    private suspend fun finishActivities() {
        val stages = listOf(Stage.RESUMED, Stage.PAUSED, Stage.STARTED, Stage.STOPPED, Stage.CREATED)
        fun open(): List<Activity> = stages.flatMap { stage ->
            ActivityLifecycleMonitorRegistry.getInstance().getActivitiesInStage(stage).filterIsInstance<MainActivity>()
        }
        open().forEach { it.finish() }
        withTimeout(STEP_MS) {
            while (open().isNotEmpty()) delay(POLL_MS)
        }
    }

    private fun sha256(bytes: ByteArray): String =
        MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

    private companion object {
        const val STATUS_KEY = "DROPUMI_STATUS"

        /** Not one of AndroidJUnitRunner's own status codes (1, 0 and negative). */
        const val STATUS_IN_PROGRESS = 2
        const val CLI_ADDR = "dropumi_cli_addr"
        const val CLI_FP = "dropumi_cli_fp"
        const val STEP_MS = 25_000L
        const val POLL_MS = 50L

        /** The fingerprint hint in an advertisement: the first 8 characters. */
        const val HINT_LENGTH = 8
    }
}
