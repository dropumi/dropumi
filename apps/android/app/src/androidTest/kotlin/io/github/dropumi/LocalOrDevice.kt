// SPDX-License-Identifier: MPL-2.0
//
// Tests that start a foreground service or use NSD: they run on local
// emulators and devices, and the CI emulator job leaves them out
// (`-e notAnnotation io.github.dropumi.LocalOrDevice`; plan §8 C).

package io.github.dropumi

@Target(AnnotationTarget.CLASS, AnnotationTarget.FUNCTION)
@Retention(AnnotationRetention.RUNTIME)
annotation class LocalOrDevice
