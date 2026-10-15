// SPDX-License-Identifier: MPL-2.0
//
// The app side of the interop harness against the CLI (plan §8 D). These
// tests need a CLI on the host and the `dropumi_cli_*` runner arguments, so
// both passes of scripts/android/ui-tests.sh leave them out (`-e
// notAnnotation`); only the interop harness runs them, one at a time with
// `-e class`.

package io.github.dropumi

@Target(AnnotationTarget.CLASS, AnnotationTarget.FUNCTION)
@Retention(AnnotationRetention.RUNTIME)
annotation class CliInterop
