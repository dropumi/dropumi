// SPDX-License-Identifier: MPL-2.0
//
// The NSD adapter's logic (ADR-0013 D5; discovery.md). Rust owns every
// discovery rule (instance names, TXT, IPv4 only, the self-filter, caps);
// this only moves bytes between NSD and the core, with these bounds:
//
// - Threading: all state lives on one `dropumi-nsd` thread. Every NSD
//   callback hops to it first; the core's calls (each on a thread of its
//   own) run there and wait for it. The browse sink is only ever called on
//   that thread, never on the main thread.
// - One new listener object per browse session and per registration (NSD
//   refuses a reused one); callbacks from an older session are ignored.
// - Resolving: below API 34 one resolve at a time from a FIFO of at most 64
//   names (deduplicated, overflow dropped), FAILURE_ALREADY_ACTIVE retried
//   at most three times; from 34 at most 64 service-info callbacks, removed
//   when a service is lost, when browsing stops and when a new session
//   starts.
// - Advertising: only where the R21 gate is open, and only with an IPv4
//   address on a local network. advertise() returns the requested name at
//   once (ADR-0012). If NSD later registers a different name, it is
//   unregistered at once and, until that completes, browse results with
//   that name are dropped: the one adapter-side filter, a deliberate
//   exception to "Rust owns every rule". Such late failures cannot reach
//   the core (it has already recorded the advertisement), so they are
//   reported through [state].
// - Nothing here logs, and no exception text leaves: NsdDiscovery turns
//   every failure into a fixed phrase.

package io.github.dropumi.core.discovery

import io.github.dropumi.ffi.BrowseSinkInterface
import io.github.dropumi.ffi.ServiceAd
import io.github.dropumi.ffi.TxtEntry
import java.util.concurrent.Callable
import java.util.concurrent.ExecutionException
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/** A refusal with a fixed phrase, safe to hand to Rust. */
class NsdFailure(val phrase: String) : Exception(phrase)

class NsdAdapterCore(
    private val port: NsdPort,
    gate: HostLabelGate,
    sdkInt: Int,
    moduleVersion: Long?,
    /** This device's IPv4 addresses on local networks (LocalAddresses). */
    private val ipv4Addresses: () -> List<String>,
    private val executor: ExecutorService = newExecutor(),
) : Advertising {
    override val gateOpen: Boolean = gate.allows(sdkInt, moduleVersion)

    private val _state = MutableStateFlow<AdvertiseState>(
        if (gateOpen) AdvertiseState.Idle else AdvertiseState.GateClosed,
    )
    override val state: StateFlow<AdvertiseState> = _state.asStateFlow()

    // Everything below is touched only on the adapter thread.
    private val registrations = LinkedHashMap<String, Registration>()
    private var droppedName: String? = null
    private var session: Session? = null
    private var sessions = 0L

    // Advertising

    /** Registers [ad] and returns its name without waiting for NSD. */
    fun advertise(ad: ServiceAd): String {
        if (!gateOpen) throw NsdFailure(GATE_CLOSED)
        if (ipv4Addresses().isEmpty()) {
            _state.value = AdvertiseState.Failed(AdvertiseState.Reason.NO_IPV4_NETWORK)
            throw NsdFailure(NO_IPV4)
        }
        onAdapterThread {
            registrations.remove(ad.instance)?.let(::stop)
            val registration = Registration(ad.instance)
            registrations[ad.instance] = registration
            _state.value = AdvertiseState.Registering
            try {
                port.register(
                    NsdService(ad.instance, ad.serviceType, ad.port.toInt(), ad.txt.associate { it.key to it.value }),
                    registration,
                )
            } catch (e: Exception) {
                registrations.remove(ad.instance)
                _state.value = AdvertiseState.Failed(AdvertiseState.Reason.REGISTRATION_FAILED)
                throw e
            }
        }
        return ad.instance
    }

    /** Removes an advertisement (NSD sends goodbyes). */
    fun withdraw(instance: String) = onAdapterThread {
        val registration = registrations.remove(instance)
            ?: registrations.values.firstOrNull { it.actualName == instance }?.also { registrations.remove(it.requested) }
        registration?.let(::stop)
        // Nothing is advertised any more (a failed advertisement included).
        if (registrations.isEmpty() && gateOpen) _state.value = AdvertiseState.Idle
    }

    private fun stop(registration: Registration) {
        registration.active = false
        try {
            port.unregister(registration)
        } catch (_: Exception) {
            // Not registered (any more): nothing to withdraw.
        }
    }

    private inner class Registration(val requested: String) : NsdPort.RegistrationListener {
        var active = true
        var actualName: String? = null

        override fun registered(name: String) = hop {
            if (!active) return@hop
            actualName = name
            if (name == requested) {
                _state.value = AdvertiseState.Registered
            } else {
                // NSD renamed it after advertise() returned, so Rust's
                // self-filter does not know this name: withdraw it now and
                // drop browse results with it until the withdrawal is done.
                droppedName = name
                registrations.remove(requested)
                stop(this)
                _state.value = AdvertiseState.Failed(AdvertiseState.Reason.RENAMED)
            }
        }

        override fun registrationFailed(code: Int) = hop {
            if (!active) return@hop
            active = false
            registrations.remove(requested)
            _state.value = AdvertiseState.Failed(AdvertiseState.Reason.REGISTRATION_FAILED)
        }

        override fun unregistered() = hop { finishedUnregistering() }

        override fun unregistrationFailed(code: Int) = hop { finishedUnregistering() }

        private fun finishedUnregistering() {
            if (actualName != null && droppedName == actualName) droppedName = null
        }
    }

    // Browsing

    /** Starts a new browse session reporting to [sink]; an older session ends. */
    fun startBrowse(sink: BrowseSinkInterface) = onAdapterThread {
        session?.let(::end)
        val next = Session(++sessions, sink)
        session = next
        try {
            port.discover(SERVICE_TYPE, next)
        } catch (e: Exception) {
            session = null
            throw e
        }
    }

    fun stopBrowse() = onAdapterThread {
        session?.let(::end)
        session = null
    }

    private fun end(old: Session) {
        try {
            port.stopDiscovery(old)
        } catch (_: Exception) {
            // Already stopped.
        }
        for (callback in old.callbacks.values) unregisterQuietly(callback)
        old.callbacks.clear()
        old.queue.clear()
        old.resolving = null
    }

    private fun unregisterQuietly(callback: NsdPort.InfoCallback) {
        try {
            port.unregisterInfoCallback(callback)
        } catch (_: Exception) {
            // Already gone.
        }
    }

    private class Pending(val service: NsdService) {
        var retries = 0
    }

    private inner class Session(val id: Long, val sink: BrowseSinkInterface) : NsdPort.DiscoveryListener {
        val queue = ArrayDeque<Pending>()
        var resolving: Pending? = null
        val callbacks = LinkedHashMap<String, InfoCallback>()

        val isCurrent: Boolean get() = session === this

        override fun startFailed(code: Int) = hop {
            if (isCurrent) sink.failed("browsing for nearby devices failed (NSD error $code)")
        }

        override fun found(service: NsdService) = hop { if (isCurrent) onFound(this, service) }

        override fun lost(service: NsdService) = hop { if (isCurrent) onLost(this, service) }
    }

    private fun onFound(s: Session, service: NsdService) {
        if (service.name == droppedName) return
        if (port.hasServiceInfoCallback) {
            if (service.name in s.callbacks || s.callbacks.size >= MAX_PENDING) return
            val callback = InfoCallback(s, service.name)
            s.callbacks[service.name] = callback
            try {
                port.registerInfoCallback(service, callback)
            } catch (_: Exception) {
                s.callbacks.remove(service.name)
            }
        } else {
            val known = s.resolving?.service?.name == service.name || s.queue.any { it.service.name == service.name }
            val pending = s.queue.size + (if (s.resolving != null) 1 else 0)
            if (known || pending >= MAX_PENDING) return
            s.queue.addLast(Pending(service))
            resolveNext(s)
        }
    }

    private fun onLost(s: Session, service: NsdService) {
        s.queue.removeAll { it.service.name == service.name }
        s.callbacks.remove(service.name)?.let(::unregisterQuietly)
        s.sink.lost(service.name)
    }

    /** Below 34: starts the next resolve if none is in flight. */
    private fun resolveNext(s: Session) {
        while (s.resolving == null && s.queue.isNotEmpty()) {
            val next = s.queue.removeFirst()
            s.resolving = next
            try {
                port.resolve(next.service, Resolve(s, next))
            } catch (_: Exception) {
                s.resolving = null
            }
        }
    }

    private inner class Resolve(val s: Session, val pending: Pending) : NsdPort.ResolveListener {
        override fun resolved(service: NsdService) = hop {
            if (!s.isCurrent || s.resolving !== pending) return@hop
            s.resolving = null
            deliver(s, service)
            resolveNext(s)
        }

        override fun failed(service: NsdService, code: Int) = hop {
            if (!s.isCurrent || s.resolving !== pending) return@hop
            s.resolving = null
            if (code == NsdPort.FAILURE_ALREADY_ACTIVE && pending.retries < MAX_RETRIES) {
                pending.retries++
                s.queue.addFirst(pending)
            }
            resolveNext(s)
        }
    }

    private inner class InfoCallback(val s: Session, val name: String) : NsdPort.InfoCallback {
        private val isLive: Boolean get() = s.isCurrent && s.callbacks[name] === this

        override fun updated(service: NsdService) = hop { if (isLive) deliver(s, service.copy(name = name)) }

        override fun lost() = hop {
            if (!isLive) return@hop
            s.callbacks.remove(name)
            unregisterQuietly(this)
            s.sink.lost(name)
        }

        override fun registrationFailed(code: Int) = hop { if (isLive) s.callbacks.remove(name) }
    }

    private fun deliver(s: Session, service: NsdService) {
        if (service.name == droppedName || service.port !in 1..MAX_PORT) return
        val txt = service.attributes.map { (key, value) -> TxtEntry(key, value ?: ByteArray(0)) }
        s.sink.resolved(service.name, txt, service.addresses, service.port.toUShort())
    }

    // Threads

    /** Runs [body] on the adapter thread (an NSD callback); a failure is dropped there. */
    private fun hop(body: () -> Unit) {
        try {
            executor.execute {
                try {
                    body()
                } catch (_: Exception) {
                    // A failed callback (for example the core refusing a
                    // result) must not stop the adapter thread.
                }
            }
        } catch (_: Exception) {
            // The executor is shut down: nothing to do.
        }
    }

    /** Runs [body] on the adapter thread and waits for it (a call from the core). */
    private fun <T> onAdapterThread(body: () -> T): T {
        val future = executor.submit(Callable { body() })
        try {
            return future.get(CALL_LIMIT_SECONDS, TimeUnit.SECONDS)
        } catch (e: ExecutionException) {
            throw e.cause as? NsdFailure ?: NsdFailure(FAILED)
        }
    }

    /** Test hook: returns once every callback queued so far has run. */
    fun awaitIdle() {
        executor.submit {}.get(CALL_LIMIT_SECONDS, TimeUnit.SECONDS)
    }

    companion object {
        const val SERVICE_TYPE = "_dropumi._tcp"
        const val MAX_PENDING = 64
        const val MAX_RETRIES = 3
        const val THREAD_NAME = "dropumi-nsd"
        const val GATE_CLOSED = "advertising is off on this Android version"
        const val NO_IPV4 = "no IPv4 network"
        const val FAILED = "the NSD service failed"
        private const val CALL_LIMIT_SECONDS = 5L
        private const val MAX_PORT = 65_535

        fun newExecutor(): ExecutorService = Executors.newSingleThreadExecutor { runnable ->
            Thread(runnable, THREAD_NAME).apply { isDaemon = true }
        }
    }
}
