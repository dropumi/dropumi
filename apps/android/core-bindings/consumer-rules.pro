# SPDX-License-Identifier: MPL-2.0
# UniFFI's generated code uses JNA structures, callbacks and reflection;
# keep them and the bindings intact when an app shrinks its code.
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-keep class io.github.dropumi.ffi.** { *; }
-dontwarn java.awt.**
