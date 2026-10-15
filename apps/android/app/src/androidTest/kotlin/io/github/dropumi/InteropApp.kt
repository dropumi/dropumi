// SPDX-License-Identifier: MPL-2.0
//
// Tests that need TestApplication's interop graph: the app's own live graph
// (the real Rust core) on temporary folders under
// noBackupFilesDir/interop-<uuid>. They run only in the second pass of
// scripts/android/ui-tests.sh, with the runner argument `dropumi_app
// interop` (`-e annotation io.github.dropumi.InteropApp`); the first pass
// leaves them out (`-e notAnnotation`). A runner argument applies to the
// whole run, so the two kinds never share a pass (plan §8 B).

package io.github.dropumi

@Target(AnnotationTarget.CLASS, AnnotationTarget.FUNCTION)
@Retention(AnnotationRetention.RUNTIME)
annotation class InteropApp
