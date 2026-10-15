// SPDX-License-Identifier: MPL-2.0
//
// Foreground and background for the whole process (ADR-0013 D7), from
// ProcessLifecycleOwner: ON_START makes receiving match the settings again;
// ON_STOP stops accepting new connections while running transfers finish.
// ProcessLifecycleOwner sends ON_STOP a moment (about 700 ms) after the last
// activity stops, and not at all when an activity is recreated meanwhile,
// so rotation, fold, resize and theme changes never withdraw the advert.
package io.github.dropumi

import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner

class ProcessVisibility private constructor(private val host: AppGraphHost) : DefaultLifecycleObserver {
    override fun onStart(owner: LifecycleOwner) = host.graph.loader.processStarted()

    override fun onStop(owner: LifecycleOwner) = host.graph.loader.processStopped()

    companion object {
        /** Follows the process for [host]'s graph (whichever graph it holds then). Call from Application.onCreate. */
        fun install(host: AppGraphHost) {
            ProcessLifecycleOwner.get().lifecycle.addObserver(ProcessVisibility(host))
        }
    }
}
