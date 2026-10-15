# SPDX-License-Identifier: MPL-2.0
#
# Extra R8 rules for the app APK of the qa build only (the build
# scripts/android/interop.sh runs); release never uses them. The test APK
# runs in the app's process and calls into it, and AGP leaves out of the
# test APK every class the app APK already has, so whatever the tests call
# must survive R8 in the app. Only the CLI interop harness runs on qa (the
# UI state and real-core tests run on debug), so this covers what its run
# uses: DropumiTestRunner, TestApplication's interop graph and
# AndroidInteropTest. Keep it next to those sources and update it with
# them: a stripped class or member shows up in an interop run as
# NoClassDefFoundError or NoSuchMethodError.

# The app's own classes that TestApplication and AndroidInteropTest use.
-keep class io.github.dropumi.AppGraph { *; }
-keep class io.github.dropumi.AppGraphHost { *; }
-keep class io.github.dropumi.LiveGraph { *; }
-keep class io.github.dropumi.ProcessVisibility { *; }
-keep class io.github.dropumi.ProcessVisibility$Companion { *; }
-keep class io.github.dropumi.AndroidServiceStarter { *; }
-keep class io.github.dropumi.ReceivedFileActions { *; }

# :appstate and :core: the tests use most of both (AppModel, AppLoader,
# MemorySettingsStore, AppPaths, LiveCoreClient, CoreClient, UriGuard,
# ContentUriFile, AndroidDocumentSource, and the types in their
# signatures). The bindings and JNA keep their release rules
# (core-bindings/consumer-rules.pro), which this build checks.
-keep class io.github.dropumi.appstate.** { *; }
-keep class io.github.dropumi.core.** { *; }

# The Kotlin standard library and kotlinx.coroutines: the tests use the
# app's copies (suspend functions, runBlocking, withTimeout, Flow.first),
# and R8 removes or merges what the app itself does not call.
-keep class kotlin.** { *; }
-keep class kotlinx.coroutines.** { *; }

# androidx.tracing and androidx.concurrent: AndroidJUnitRunner and
# androidx.test's core use the app's copies.
-keep class androidx.tracing.** { *; }
-keep class androidx.concurrent.futures.** { *; }
