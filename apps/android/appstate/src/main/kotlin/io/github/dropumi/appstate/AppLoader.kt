// SPDX-License-Identifier: MPL-2.0
//
// Opens the core and builds the app model, off the main thread: opening the
// core reads or creates the identity. The screens show Loading, the model,
// or "Dropumi could not start" with Try again.

package io.github.dropumi.appstate

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * @param open opens the core and builds the model; runs on [io].
 */
class AppLoader(
    private val scope: CoroutineScope,
    private val io: CoroutineDispatcher,
    private val open: () -> AppModel,
) {
    sealed interface State {
        data object Loading : State

        data class Ready(val model: AppModel) : State

        /** [message] comes from the error catalog; exception text is never shown or logged. */
        data class Failed(val message: String) : State
    }

    private val _state = MutableStateFlow<State>(State.Loading)
    val state: StateFlow<State> = _state.asStateFlow()
    private var loading = false

    /**
     * Whether the app was last seen in the background. A model created while
     * it is starts there, so it does not receive until the app is in front.
     */
    var isInBackground = false

    /** The model, once ready. */
    val model: AppModel? get() = (_state.value as? State.Ready)?.model

    /** Opens the core once; again only after a failure ([retry]). */
    fun load() {
        if (loading || _state.value is State.Ready) return
        loading = true
        _state.value = State.Loading
        scope.launch {
            try {
                val model = withContext(io) { open() }
                // The background state must be set before start().
                if (isInBackground) model.enterBackground()
                _state.value = State.Ready(model)
                model.start()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                _state.value = State.Failed(ErrorCatalog.message(e))
            } finally {
                loading = false
            }
        }
    }

    /** Try again after a failure. */
    fun retry() {
        if (_state.value is State.Failed) load()
    }

    /**
     * The app came to the front (ProcessLifecycleOwner's ON_START):
     * receiving matches the settings again. Call on [scope]'s thread.
     */
    fun processStarted() {
        isInBackground = false
        val model = model ?: return
        scope.launch { model.becomeActive() }
    }

    /**
     * The app left the screen (ProcessLifecycleOwner's ON_STOP, sent a
     * moment after the last activity stops, and never for a rotation or
     * another configuration change): new connections are refused; running
     * transfers finish. A model still loading starts in the background.
     * Call on [scope]'s thread.
     */
    fun processStopped() {
        isInBackground = true
        model?.enterBackground()
    }
}
