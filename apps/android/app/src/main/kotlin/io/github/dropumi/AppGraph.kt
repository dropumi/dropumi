// SPDX-License-Identifier: MPL-2.0

package io.github.dropumi

import io.github.dropumi.appstate.AppLoader
import io.github.dropumi.appstate.PendingShare
import io.github.dropumi.core.files.DocumentGuard

/**
 * Implemented by the Application. Activities and services reach the graph
 * only as `(application as AppGraphHost).graph`, so a test Application can
 * supply its own graph and the app's Application is never built in tests.
 */
interface AppGraphHost {
    val graph: AppGraph
}

/** The app's long-lived objects: one core and one app model, opened by [loader]. */
class AppGraph(
    val loader: AppLoader,
    /** The in-process share hand-off; only ShareActivity writes to it. */
    val pendingShare: PendingShare,
    /** UriGuard, for ShareActivity's early filter (every read runs it again). */
    val uriGuard: DocumentGuard,
)
