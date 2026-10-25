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
// The `agent` scenario is the device-to-device harness's agent instead: no
// CLI; a driver on the host steers it over a control connection (runner
// argument `dropumi_control`, through `adb reverse`) while it pairs with and
// sends to another device's app over Wi-Fi.
//
// @CliInterop: neither pass of scripts/android/ui-tests.sh runs these.

package io.github.dropumi.interop

import android.app.Activity
import android.content.Intent
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.Build
import android.os.Bundle
import android.os.Process
import android.os.SystemClock
import android.system.Os
import android.system.OsConstants
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.runner.lifecycle.ActivityLifecycleCallback
import androidx.test.runner.lifecycle.ActivityLifecycleMonitor
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
import io.github.dropumi.appstate.PairingSession
import io.github.dropumi.appstate.PromptArming
import io.github.dropumi.appstate.ReceiveVisibility
import io.github.dropumi.appstate.promptId
import io.github.dropumi.core.discovery.AdvertiseState
import io.github.dropumi.core.discovery.LanCandidate
import io.github.dropumi.core.discovery.LocalAddressRules
import io.github.dropumi.ffi.AcceptMode
import io.github.dropumi.ffi.CodePurpose
import io.github.dropumi.ffi.PairingResult
import io.github.dropumi.ffi.Prompt
import io.github.dropumi.ffi.ReceivePolicy
import io.github.dropumi.ffi.SendTarget
import io.github.dropumi.foreign.ForeignProvider
import io.github.dropumi.foreign.ForeignSharer
import java.io.BufferedReader
import java.io.BufferedWriter
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.io.IOException
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.net.InetAddress
import java.net.InetSocketAddress
import java.net.Socket
import java.nio.charset.StandardCharsets
import java.security.MessageDigest
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.cancelChildren
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
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
            fields += savedFileFacts(receiveDir, name)
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
        assertNoBanner(model)
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
        assertNoBanner(model)
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

    /**
     * The agent of the device-to-device harness: no CLI, another device's
     * app on the same Wi-Fi. The driver on the host listens; this test
     * connects to runner argument `dropumi_control` (127.0.0.1:<port>,
     * through `adb reverse`), says hello, then runs the driver's commands
     * one at a time (receive, wait_pairing, pair, send, trusted, quit) with
     * one JSON object per line each way, and sends events as things happen.
     * The other device reaches this one over Wi-Fi (the core listens on
     * every interface); only the control channel goes through adb. Answers
     * like the other scenarios: yes only for the expected fingerprint, once
     * the prompt is armed. Fails after [AGENT_IDLE_MS] without a command.
     */
    @Test
    fun agent() = play { model -> Agent(model).run(argument(CONTROL)) }

    /** One run of [agent]: the control connection and what the commands asked for. */
    private inner class Agent(private val model: AppModel) {
        /** Lines for the driver, written by one thread in order (replies and events). */
        private val outbox = LinkedBlockingQueue<Any>()

        /** The driver's commands, then [CLOSED] when the connection ends. */
        private val commands = LinkedBlockingQueue<JSONObject>()

        @Volatile
        private var readFailure: String? = null

        private lateinit var scope: CoroutineScope

        /** Who offers are accepted from (receive), and whose codes are confirmed (pairing, send). */
        private var receiveExpect: String? = null
        private var receivePairedOnly = true
        private var pairExpect: String? = null
        private var sendExpect: String? = null

        /** The peer of the last pairing code, for a pairing that ends without a peer. */
        private var pairingCodePeer = ""

        suspend fun run(control: String) {
            // A plain loop: the test APK may only call the Kotlin library members proguard-qa.pro keeps.
            var colon = -1
            for (i in 0 until control.length) if (control[i] == ':') colon = i
            if (colon <= 0) throw AssertionError("$CONTROL is not host:port")
            val address = InetSocketAddress(control.substring(0, colon), Integer.parseInt(control.substring(colon + 1)))
            val socket = withContext(Dispatchers.IO) {
                val s = Socket()
                s.connect(address, CONNECT_MS)
                s.tcpNoDelay = true
                s
            }
            val reader = Thread({ readCommands(socket) }, "dropumi-agent-read").apply { isDaemon = true }
            val writer = Thread({ writeLines(socket) }, "dropumi-agent-write").apply { isDaemon = true }
            reader.start()
            writer.start()
            try {
                coroutineScope {
                    scope = this
                    hello()
                    launch { answerPrompts() }
                    launch { reportReceived() }
                    try {
                        serve()
                    } finally {
                        coroutineContext.cancelChildren()
                    }
                }
            } finally {
                // The writer sends what is queued (the reply to quit last), then closes the socket.
                outbox.put(END_OF_LINES)
                withContext(Dispatchers.IO) { writer.join(CONNECT_MS.toLong()) }
            }
        }

        private fun readCommands(socket: Socket) {
            try {
                val input = BufferedReader(InputStreamReader(socket.getInputStream(), StandardCharsets.UTF_8))
                while (true) {
                    val line = input.readLine() ?: break
                    if (line.length == 0) continue
                    commands.put(JSONObject(line))
                }
            } catch (_: IOException) {
                // The connection ended.
            } catch (e: JSONException) {
                readFailure = "a control line is not a JSON object: ${e.message}"
            } finally {
                commands.put(CLOSED)
            }
        }

        private fun writeLines(socket: Socket) {
            try {
                val output = BufferedWriter(OutputStreamWriter(socket.getOutputStream(), StandardCharsets.UTF_8))
                while (true) {
                    val line = outbox.take()
                    if (line === END_OF_LINES) break
                    output.write(line as String)
                    output.write("\n")
                    output.flush()
                }
            } catch (_: IOException) {
                // The driver went away.
            } finally {
                // Also ends the reader.
                try {
                    socket.close()
                } catch (_: IOException) {
                }
            }
        }

        /** Queues one line for the driver (never blocks). */
        private fun emit(json: JSONObject) {
            outbox.put(json.toString())
        }

        private fun event(name: String): JSONObject = JSONObject().put("event", name)

        private fun hello() {
            val identity = model.state.value.identity ?: throw AssertionError("no identity")
            emit(
                event("hello")
                    .put("platform", "android")
                    .put("fingerprint", identity.fingerprint)
                    .put("name", identity.name)
                    .put("addresses", JSONArray(wifiAddresses())),
            )
        }

        /** Runs commands until quit or the end of the connection; fails after [AGENT_IDLE_MS] without one. */
        private suspend fun serve() {
            while (true) {
                val command = withContext(Dispatchers.IO) { commands.poll(AGENT_IDLE_MS, TimeUnit.MILLISECONDS) }
                    ?: throw AssertionError("no command for ${AGENT_IDLE_MS / 1000} s")
                if (command === CLOSED) {
                    readFailure?.let { throw AssertionError(it) }
                    return
                }
                if (!handle(command)) return
            }
        }

        /** Runs [command] and replies; false after quit. */
        private suspend fun handle(command: JSONObject): Boolean {
            val id = command.opt("id")
            val name = command.optString("cmd")
            var reply = JSONObject()
            try {
                when (name) {
                    "receive" -> receive(command, reply)
                    "wait_pairing" -> waitPairing(command, reply)
                    "pair" -> pair(command)
                    "send" -> send(command)
                    "trusted" -> trusted(reply)
                    "quit" -> Unit
                    else -> throw IllegalArgumentException("unknown command $name")
                }
                reply.put("ok", true)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                reply = JSONObject().put("ok", false).put("error", e.message ?: e.javaClass.name)
            } catch (e: AssertionError) {
                reply = JSONObject().put("ok", false).put("error", e.message ?: e.javaClass.name)
            }
            emit(reply.put("id", id))
            return name != "quit"
        }

        /** Receives with that policy and the accept mode Ask; replies once the receiver runs so. */
        private suspend fun receive(command: JSONObject, reply: JSONObject) {
            val everyone = when (val policy = command.getString("policy")) {
                "paired" -> false
                "everyone" -> true
                else -> throw IllegalArgumentException("unknown policy $policy")
            }
            receiveExpect = command.getString("expect_fp")
            receivePairedOnly = !everyone
            val visibility = if (everyone) ReceiveVisibility.EVERYONE else ReceiveVisibility.PAIRED_ONLY
            if (model.state.value.onboarded) model.setVisibility(visibility) else model.finishOnboarding(visibility)
            if (model.state.value.autoAcceptPaired) model.setAutoAcceptPaired(false)
            val info = until(model, "receiving with that policy") { s ->
                s.receiver?.takeIf { it.accept == AcceptMode.PROMPT && (it.policy is ReceivePolicy.Everyone) == everyone }
            }
            reply.put("port", info.port.toInt()).put("addresses", JSONArray(wifiAddresses()))
        }

        /** Opens the pairing window as the Pair a device screen does; replies once it listens. */
        private suspend fun waitPairing(command: JSONObject, reply: JSONObject) {
            pairExpect = command.getString("expect_fp")
            val before = model.state.value.pairing?.operation
            model.openPairingWindow()
            val session = model.state.value.pairing
            if (session == null || session.operation == before || session.role != PairingSession.Role.Window) {
                throw IllegalStateException("the pairing window did not open: ${model.state.value.banner?.title}")
            }
            val op = session.operation
            val listening = until(model, "the pairing window to listen") { s ->
                s.pairing?.takeIf { it.operation == op && (it.port != null || it.result != null) }
            }
            val port = listening.port ?: throw IllegalStateException("the pairing window ended: ${listening.result}")
            scope.launch { reportPairing(op) }
            reply.put("port", port.toInt()).put("addresses", JSONArray(wifiAddresses()))
        }

        /** Asks that address to pair, with the expected key pinned; replies once the request runs. */
        private suspend fun pair(command: JSONObject) {
            val address = command.getString("address")
            val expect = command.getString("expect_fp")
            pairExpect = expect
            val before = model.state.value.pairing?.operation
            model.pair(SendTarget.Address(address, expect), "Peer")
            val session = model.state.value.pairing
            if (session == null || session.operation == before || session.role !is PairingSession.Role.Initiator) {
                throw IllegalStateException("pairing did not start: ${model.state.value.banner?.title}")
            }
            val op = session.operation
            scope.launch { reportPairing(op) }
        }

        private suspend fun reportPairing(op: ULong) {
            val result = next { s -> s.pairing?.takeIf { it.operation == op }?.result }
            // if/else, not an exhaustive when: the test APK may only call what proguard-qa.pro keeps.
            val name: String
            var error = ""
            var peer = pairingCodePeer
            if (result is PairingResult.Paired) {
                name = "paired"
                peer = result.peer.fingerprint
            } else if (result is PairingResult.Failed) {
                name = "failed"
                error = result.error.name.lowercase()
            } else {
                name = "cancelled"
            }
            emit(event("pair_result").put("result", name).put("error", error).put("peer_fp", peer))
        }

        /**
         * Writes the files (byte i is (i * 31 + 7) % 251) and sends them to
         * that address with the expected key pinned, as the Send screen does;
         * replies once the send runs.
         */
        private suspend fun send(command: JSONObject) {
            val address = command.getString("address")
            val expect = command.getString("expect_fp")
            val specs = command.getJSONArray("files")
            sendExpect = expect
            val dir = File(sourceDir(), "send-${command.opt("id")}")
            withContext(Dispatchers.IO) {
                dir.deleteRecursively()
                check(dir.mkdirs())
            }
            val files = ArrayList<OutgoingFile>()
            for (i in 0 until specs.length()) {
                val spec = specs.getJSONObject(i)
                val name = spec.getString("name")
                val size = spec.getLong("bytes")
                if (File(name).name != name || name == "." || name == ".." || size < 0) {
                    throw IllegalArgumentException("not a plain file name and size: $name")
                }
                val file = File(dir, name)
                val sha = withContext(Dispatchers.IO) { writePattern(file, size) }
                emit(event("sent_file").put("name", name).put("bytes", size).put("sha256", sha))
                files.add(OutgoingFile(name, size.toULong(), OutgoingFile.Source.Path(file.path)))
            }
            val before = HashSet<String>()
            for (entry in model.state.value.history) before.add(entry.id)
            model.addToDraft(files)
            model.sendDraft(SendTarget.Address(address, expect), "Peer", allowUnverified = true)
            if (model.state.value.draft.isNotEmpty()) {
                model.clearDraft()
                throw IllegalStateException("not sent: ${model.state.value.banner?.title}")
            }
            scope.launch {
                val entry = next { s -> s.history.firstOrNull { it.outgoing && it.id !in before } }
                val outcome = outcomeName(entry.outcome)
                val error = if (entry.outcome == HistoryEntry.Outcome.COMPLETED) "" else outcome
                emit(event("send_result").put("outcome", outcome).put("error", error).put("detail", entry.summary))
            }
        }

        private suspend fun trusted(reply: JSONObject) {
            model.refreshTrusted()
            val fingerprints = JSONArray()
            for (device in model.state.value.trusted) fingerprints.put(device.fingerprint)
            reply.put("fingerprints", fingerprints)
        }

        /**
         * Answers every prompt in turn, as the user would: an offer only from
         * the expected key (and, when receiving from paired devices only, a
         * paired one); a code only from the key the pairing or send expects.
         */
        private suspend fun answerPrompts() {
            val handled = HashSet<Long>()
            while (true) {
                val prompt = next { s -> s.currentPrompt?.takeIf { it.promptId.toLong() !in handled } }
                handled.add(prompt.promptId.toLong())
                val yes: Boolean
                if (prompt is Prompt.Offer) {
                    val expect = receiveExpect
                    yes = expect != null && prompt.peer.fingerprint == expect && (!receivePairedOnly || prompt.peer.trusted)
                    emit(
                        event("offer")
                            .put("peer_fp", prompt.peer.fingerprint)
                            .put("trusted", prompt.peer.trusted)
                            .put("code", prompt.code ?: "")
                            .put("files", prompt.fileCount.toLong())
                            .put("accepted", yes),
                    )
                } else {
                    val code = prompt as Prompt.Code
                    val pairing = code.purpose == CodePurpose.PAIRING
                    val expect = if (pairing) pairExpect else sendExpect
                    if (pairing) pairingCodePeer = code.peer.fingerprint
                    emit(event("code").put("code", code.code).put("peer_fp", code.peer.fingerprint))
                    yes = expect != null && code.peer.fingerprint == expect
                }
                try {
                    answer(model, prompt, yes)
                } catch (e: TimeoutCancellationException) {
                    // The prompt never armed (or closed first): say so, and decline.
                    status("agent_answer_timeout", mapOf("yes" to yes.toString()))
                    model.answer(prompt, false)
                }
            }
        }

        /** For every receive that ends: each saved file, hashed where the app saved it, then the outcome. */
        private suspend fun reportReceived() {
            val seen = HashSet<String>()
            for (entry in model.state.value.history) seen.add(entry.id)
            val receiveDir = app.interopPaths.receiveDir
            while (true) {
                val ended = next { s -> s.history.lastOrNull { !it.outgoing && it.id !in seen } }
                seen.add(ended.id)
                for (name in ended.savedAs) {
                    val file = File(receiveDir, name)
                    val sha = withContext(Dispatchers.IO) { fileSha256(file) }
                    emit(event("received").put("name", name).put("bytes", file.length()).put("sha256", sha))
                }
                emit(event("receive_finished").put("outcome", outcomeName(ended.outcome)))
            }
        }

        /** Waits, without a bound of its own (the driver bounds each step), until [value] gives something. */
        private suspend fun <T : Any> next(value: (AppState) -> T?): T {
            var found: T? = null
            model.state.first { s -> value(s)?.also { found = it } != null }
            return found ?: throw IllegalStateException("no value")
        }
    }

    /** The protocol's outcome words (REJECTED is the receiver's decline). */
    private fun outcomeName(outcome: HistoryEntry.Outcome): String =
        if (outcome == HistoryEntry.Outcome.REJECTED) "declined" else outcome.name.lowercase()

    /**
     * This device's IPv4 addresses on Wi-Fi (and Ethernet) networks, by the
     * app's own rule (LocalAddressRules: never loopback, link-local or a
     * cellular network), read from the system now.
     */
    @Suppress("DEPRECATION") // allNetworks: the callbacks LocalAddresses uses need time to report.
    private fun wifiAddresses(): List<String> {
        val connectivity = app.getSystemService(ConnectivityManager::class.java) ?: return ArrayList()
        val candidates = ArrayList<LanCandidate>()
        for (network in connectivity.allNetworks) {
            val capabilities = connectivity.getNetworkCapabilities(network) ?: continue
            val link = connectivity.getLinkProperties(network) ?: continue
            val addresses = ArrayList<InetAddress>()
            for (linkAddress in link.linkAddresses) addresses.add(linkAddress.address)
            candidates.add(
                LanCandidate(
                    capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI),
                    capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET),
                    capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR),
                    addresses,
                ),
            )
        }
        return LocalAddressRules.ipv4(candidates)
    }

    /** Writes [size] bytes of the harness's pattern (byte i is (i * 31 + 7) % 251) to [file]; returns their SHA-256. */
    private fun writePattern(file: File, size: Long): String {
        val digest = MessageDigest.getInstance("SHA-256")
        val buffer = ByteArray(PATTERN_CHUNK)
        val out = FileOutputStream(file)
        try {
            var offset = 0L
            while (offset < size) {
                val count = Math.min(size - offset, PATTERN_CHUNK.toLong()).toInt()
                for (j in 0 until count) buffer[j] = (((offset + j) * 31 + 7) % 251).toByte()
                out.write(buffer, 0, count)
                digest.update(buffer, 0, count)
                offset += count
            }
            out.fd.sync()
        } finally {
            out.close()
        }
        return hex(digest.digest())
    }

    private fun fileSha256(file: File): String {
        val digest = MessageDigest.getInstance("SHA-256")
        val buffer = ByteArray(PATTERN_CHUNK)
        val input = FileInputStream(file)
        try {
            while (true) {
                val count = input.read(buffer)
                if (count < 0) break
                digest.update(buffer, 0, count)
            }
        } finally {
            input.close()
        }
        return hex(digest.digest())
    }

    private fun hex(bytes: ByteArray): String {
        val text = StringBuilder(bytes.size * 2)
        for (b in bytes) text.append("%02x".format(b))
        return text.toString()
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

    /**
     * The model shows the refusal notice only when no other banner is up
     * (AppModel.noticeRefused), so a banner already showing would hide it:
     * checked before the harness lets the sender connect. The title is the
     * app's own text.
     */
    private fun assertNoBanner(model: AppModel) {
        assertNull("a banner already showing would hide the refusal notice", model.state.value.banner?.title)
    }

    /**
     * A refused sender shows the refusal notice, never a prompt or a
     * transfer. Waits for one of the refusal titles (not any banner), and
     * looks at every state on the way for a prompt or a transfer. The notice
     * comes when the core closes the refused session, so nothing of that
     * session can follow it.
     */
    private suspend fun awaitRefusal(model: AppModel) {
        var prompted = false
        var transferred = false
        val notice = until(model, "the refusal notice") { s ->
            if (s.prompts.isNotEmpty()) prompted = true
            if (s.history.isNotEmpty()) transferred = true
            s.banner?.takeIf { it.title in REFUSAL_TITLES }
        }
        val after = model.state.value
        prompted = prompted || after.prompts.isNotEmpty()
        transferred = transferred || after.history.isNotEmpty()
        status(
            "refused",
            mapOf("title" to notice.title, "prompted" to prompted.toString(), "transferred" to transferred.toString()),
        )
        assertFalse("a sender that isn't paired never reaches a prompt", prompted)
        assertFalse("a sender that isn't paired never starts a transfer", transferred)
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

    /**
     * What can fail about a received file: it is the entry [name] of the
     * receive folder itself (its canonical path: no link, no `..`, no
     * separator), a regular file (lstat: a link is not one) of this app's
     * UID with mode 0600, in a folder (not a link) of this app's UID with
     * mode 0700. This is the interop graph's receive folder
     * (noBackupFilesDir/interop-<uuid>/received); the app's own layout
     * (filesDir/received) is AppPathsTest's.
     */
    private fun savedFileFacts(dir: File, name: String): Map<String, String> {
        val file = File(dir, name)
        val canonical = file.canonicalFile
        val f = Os.lstat(file.path)
        val d = Os.lstat(dir.path)
        return mapOf(
            "in_receive_dir" to (canonical.parentFile == dir.canonicalFile && canonical.name == name).toString(),
            "regular_file" to OsConstants.S_ISREG(f.st_mode).toString(),
            "file_mode" to "%o".format(f.st_mode and PERMISSION_BITS),
            "file_owner_is_app" to (f.st_uid == Process.myUid()).toString(),
            "dir_is_folder" to OsConstants.S_ISDIR(d.st_mode).toString(),
            "dir_mode" to "%o".format(d.st_mode and PERMISSION_BITS),
            "dir_owner_is_app" to (d.st_uid == Process.myUid()).toString(),
        )
    }

    /** A folder for the files the app sends, next to the interop graph's folders. */
    private fun sourceDir(): File =
        File(app.interopPaths.configDir.parentFile, "sources").apply { check(isDirectory || mkdirs()) }

    /**
     * Finishes MainActivity (opened by the share) and waits, bounded, until
     * the last one is destroyed: a lifecycle callback completes the wait
     * (no polling). Runs on the main thread, where the monitor reports.
     */
    private suspend fun finishActivities() {
        val monitor = ActivityLifecycleMonitorRegistry.getInstance()
        val gone = CompletableDeferred<Unit>()
        // The monitor holds callbacks weakly; this reference keeps it until removed.
        val callback = ActivityLifecycleCallback { activity, stage ->
            if (activity is MainActivity && stage == Stage.DESTROYED && openMainActivities(monitor).isEmpty()) {
                gone.complete(Unit)
            }
        }
        monitor.addLifecycleCallback(callback)
        try {
            val open = openMainActivities(monitor)
            if (open.isEmpty()) return
            open.forEach { it.finish() }
            withTimeout(STEP_MS) { gone.await() }
        } finally {
            monitor.removeLifecycleCallback(callback)
        }
    }

    private fun openMainActivities(monitor: ActivityLifecycleMonitor): List<Activity> =
        Stage.values().filter { it != Stage.DESTROYED }.flatMap { stage ->
            monitor.getActivitiesInStage(stage).filterIsInstance<MainActivity>()
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
        const val CONTROL = "dropumi_control"
        const val CONNECT_MS = 10_000

        /** The agent fails when no command came for this long. */
        const val AGENT_IDLE_MS = 120_000L
        const val PATTERN_CHUNK = 64 * 1024

        /** Ends the agent's outbox. */
        val END_OF_LINES = Any()

        /** Ends the agent's commands (compared by identity). */
        val CLOSED = JSONObject()

        /** The two refusal notices AppModel.noticeRefused shows (without and with the peer). */
        val REFUSAL_TITLES = setOf("A device was refused", "A device that isn't paired tried to send")

        /** rwxrwxrwx. */
        const val PERMISSION_BITS = 0b111_111_111

        /** The fingerprint hint in an advertisement: the first 8 characters. */
        const val HINT_LENGTH = 8
    }
}
