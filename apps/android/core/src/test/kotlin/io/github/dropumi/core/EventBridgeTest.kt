// SPDX-License-Identifier: MPL-2.0
package io.github.dropumi.core

import io.github.dropumi.ffi.NodeEvent
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class EventBridgeTest {
    /**
     * The node's event thread is never held up: with nobody collecting, every
     * batch is taken at once, and a collector later gets them all in order.
     */
    @Test
    fun eventBridgeNeverBlocksAndKeepsOrder() = runTest {
        val bridge = EventBridge()
        val sent = (0 until 10_000).map { listOf<NodeEvent>(NodeEvent.EventsDropped(it.toULong())) }
        sent.forEach(bridge::onEvents)
        bridge.finish()
        assertEquals(sent, bridge.events.toList())
    }

    /** A batch that arrives after the bridge finished is dropped, not thrown. */
    @Test
    fun eventsAfterFinishAreDroppedQuietly() = runTest {
        val bridge = EventBridge()
        bridge.onEvents(listOf(NodeEvent.ResyncRequired))
        bridge.finish()
        bridge.onEvents(listOf(NodeEvent.EventsDropped(1u)))
        assertEquals(listOf(listOf<NodeEvent>(NodeEvent.ResyncRequired)), bridge.events.toList())
    }
}
