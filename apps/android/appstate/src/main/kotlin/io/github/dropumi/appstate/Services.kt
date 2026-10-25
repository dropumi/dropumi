// SPDX-License-Identifier: MPL-2.0
//
// The port through which the model starts Android services (ADR-0013 D7).
// Pure Kotlin: the Android starter in :app builds the Intent, its ClipData
// and the grant flags; the JVM tests use a fake.

package io.github.dropumi.appstate

/** The services the model starts. */
enum class ServiceKind {
    /** The dataSync foreground service that lets running transfers finish after the user leaves. */
    TRANSFER,
}

/**
 * A content URI the started service must be able to read: its read grant
 * goes with the start, so a shared document stays readable after the
 * activity that received it is gone. A URI string and its read flag only,
 * never ClipData.
 */
data class GrantSpec(val uri: String, val read: Boolean = true)

/** What a start did. */
enum class ServiceStart {
    STARTED,

    /** The system refused it (for example a start from the background on API 31+). */
    REFUSED,
}

/** Where the transfer service stands, as the model last arranged it. */
enum class TransferServiceState {
    /** No transfer needs it; a running service stops itself. */
    OFF,

    /** Started for the running transfers; it stops itself when they end. */
    RUNNING,

    /**
     * The system refused it. The transfers go on, but may pause while
     * Dropumi is in the background; nothing is cancelled.
     */
    REFUSED,
}

/** Starts services for the model. Called on the model's thread. */
interface ServiceStarter {
    /** Starts [kind] in the foreground with [grants]. Never throws: a refusal is [ServiceStart.REFUSED]. */
    fun start(kind: ServiceKind, grants: List<GrantSpec>): ServiceStart

    /**
     * Whether Dropumi's notifications can be seen: not blocked for the app or
     * its channel, and on API 33+ the notification permission granted.
     */
    val notificationsVisible: Boolean

    companion object {
        /** No services (a model without an Android host): every start is refused. */
        val NONE: ServiceStarter = object : ServiceStarter {
            override fun start(kind: ServiceKind, grants: List<GrantSpec>): ServiceStart = ServiceStart.REFUSED
            override val notificationsVisible: Boolean get() = true
        }
    }
}
