// SPDX-License-Identifier: MPL-2.0
//
// A scripted service starter for app-state tests and UI state tests. It
// starts nothing: it records each start and answers with [result]. Results
// that rely on it say nothing about Android's service rules; the
// instrumented tests start the real TransferService for that.

package io.github.dropumi.testsupport

import io.github.dropumi.appstate.GrantSpec
import io.github.dropumi.appstate.ServiceKind
import io.github.dropumi.appstate.ServiceStart
import io.github.dropumi.appstate.ServiceStarter

class FakeServiceStarter(
    /** What every start answers. */
    @Volatile var result: ServiceStart = ServiceStart.STARTED,
    @Volatile override var notificationsVisible: Boolean = true,
) : ServiceStarter {
    data class Start(val kind: ServiceKind, val grants: List<GrantSpec>)

    private val log = mutableListOf<Start>()

    /** Every start, in order. */
    val starts: List<Start> get() = synchronized(log) { log.toList() }

    override fun start(kind: ServiceKind, grants: List<GrantSpec>): ServiceStart {
        synchronized(log) { log.add(Start(kind, grants)) }
        return result
    }
}
