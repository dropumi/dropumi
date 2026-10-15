// SPDX-License-Identifier: MPL-2.0
//
// The generated Kotlin bindings against the real Rust core on the host JVM
// (JNA loads target/debug/libdropumi_ffi): two nodes on loopback, with
// Kotlin implementations of the callbacks. Every node uses fresh temporary
// directories.

package io.github.dropumi.ffi

import java.io.File
import java.nio.file.Files
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

private class Recorder(failures: Int = 0) : EventListener {
    private val queue = LinkedBlockingQueue<NodeEvent>()
    private val failures = AtomicInteger(failures)

    override fun onEvents(events: List<NodeEvent>) {
        if (failures.getAndUpdate { if (it > 0) it - 1 else 0 } > 0) {
            throw CallbackException.Failed("listener failure")
        }
        queue.addAll(events)
    }

    /** The next event, or null after `timeoutNanos`. */
    fun poll(timeoutNanos: Long): NodeEvent? =
        queue.poll(timeoutNanos.coerceAtLeast(0), TimeUnit.NANOSECONDS)
}

/** The next event of type [T] matching [match] (others are skipped). */
private inline fun <reified T : NodeEvent> Recorder.await(
    what: String,
    match: (T) -> Boolean = { true },
): T {
    val deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(30)
    while (true) {
        val e = poll(deadline - System.nanoTime()) ?: throw AssertionError("timed out waiting for $what")
        if (e is T && match(e)) return e
    }
}

private class TestNode(failures: Int = 0) : AutoCloseable {
    val root: File = Files.createTempDirectory(
        File(requireNotNull(System.getProperty("dropumi.test.tmp"))).also { it.mkdirs() }.toPath(),
        "node",
    ).toFile()
    val inbox = File(root, "inbox").also { it.mkdirs() }
    val events = Recorder(failures)
    val node = DropumiNode(
        NodeConfig(configDir = File(root, "config").path, receiveDir = inbox.path),
        events,
        null,
    )

    fun receive(accept: AcceptMode = AcceptMode.AUTO_TRUSTED): UShort =
        node.startReceiving(ReceiveOptions(ReceivePolicy.TrustedOnly, accept, 0u, false)).port

    fun finished(direction: Direction): TransferResult =
        events.await<NodeEvent.TransferFinished>("a finished transfer") {
            it.direction == direction
        }.result

    override fun close() {
        node.shutdown()
        node.destroy()
        root.deleteRecursively()
    }
}

private fun pair(a: TestNode, b: TestNode) {
    a.node.trustFingerprint(b.node.identity().fingerprint, "B")
    b.node.trustFingerprint(a.node.identity().fingerprint, "A")
}

private fun address(port: UShort) = SendTarget.Address("127.0.0.1:$port", null)

/** A document the "app" reads, in memory. */
private class MemFile(private val name: String, private val data: ByteArray) : ForeignFile {
    override fun displayName() = name
    override fun size(): ULong = data.size.toULong()
    override fun modified(): Long? = null
    override fun openReader(): ForeignReader = object : ForeignReader {
        private var position = 0

        @Synchronized
        override fun read(maxLen: UInt): ByteArray {
            val end = minOf(position + maxLen.toInt(), data.size)
            val chunk = data.copyOfRange(position, end)
            position = end
            return chunk
        }

        override fun closeDocument() {}
    }
}

class BindingsTest {
    private val nodes = mutableListOf<TestNode>()

    private fun node(failures: Int = 0) = TestNode(failures).also { nodes.add(it) }

    @After
    fun tearDown() = nodes.forEach { it.close() }

    @Test
    fun apiVersionIsOne() {
        assertEquals(1u, ffiApiVersion())
    }

    @Test
    fun loopbackTrustedTransfer() {
        val (a, b) = node() to node()
        pair(a, b)
        val port = b.receive()
        val data = ByteArray(500_000) { (it % 251).toByte() }
        val file = File(a.inbox, "photo.jpg").also { it.writeBytes(data) }
        a.node.send(address(port), listOf(SendItem.Path(file.path)), SendOptions(false))
        assertEquals(TransferStatus.COMPLETED, a.finished(Direction.OUTGOING).status)
        assertEquals(TransferStatus.COMPLETED, b.finished(Direction.INCOMING).status)
        assertArrayEquals(data, File(b.inbox, "photo.jpg").readBytes())
    }

    @Test
    fun promptAnsweredFromAnotherThread() {
        val (a, b) = node() to node()
        pair(a, b)
        val port = b.receive(AcceptMode.PROMPT)
        val file = File(a.inbox, "note.txt").also { it.writeText("hello") }
        a.node.send(address(port), listOf(SendItem.Path(file.path)), SendOptions(false))
        val opened = b.events.await<NodeEvent.PromptOpened>("an offer")
        val offer = opened.prompt as Prompt.Offer
        assertTrue(offer.peer.trusted)
        assertNull(offer.code)
        val answerer = Thread { b.node.answerOffer(offer.id, true) }
        answerer.start()
        answerer.join()
        assertEquals(TransferStatus.COMPLETED, a.finished(Direction.OUTGOING).status)
    }

    @Test
    fun kotlinDocumentIsSent() {
        val (a, b) = node() to node()
        pair(a, b)
        val port = b.receive()
        val data = ByteArray(300_000) { ((it * 7) % 256).toByte() }
        a.node.send(
            address(port),
            listOf(SendItem.Stream(MemFile("content/document/scan.pdf", data))),
            SendOptions(false),
        )
        assertEquals(TransferStatus.COMPLETED, a.finished(Direction.OUTGOING).status)
        assertArrayEquals(data, File(b.inbox, "scan.pdf").readBytes())
    }

    @Test
    fun listenerErrorsAreContained() {
        val (a, b) = node() to node(failures = 1)
        pair(a, b)
        val port = b.receive()
        val file = File(a.inbox, "x.txt").also { it.writeText("x") }
        a.node.send(address(port), listOf(SendItem.Path(file.path)), SendOptions(false))
        assertEquals(TransferStatus.COMPLETED, a.finished(Direction.OUTGOING).status)
        // One failed delivery is not fatal: later events still arrive.
        assertEquals(TransferStatus.COMPLETED, b.finished(Direction.INCOMING).status)
    }

    @Test
    fun errorsAreTyped() {
        val b = node()
        try {
            b.node.answerOffer(999u, true)
            fail("expected PromptNotFound")
        } catch (e: DropumiException.PromptNotFound) {
            // expected
        }
        b.node.shutdown()
        try {
            b.node.identity()
            fail("expected ShutDown")
        } catch (e: DropumiException.ShutDown) {
            // expected
        }
    }
}
