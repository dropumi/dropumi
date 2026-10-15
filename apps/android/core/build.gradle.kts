// SPDX-License-Identifier: MPL-2.0
//
// The app's bridge to the Rust core: the core client, the platform adapters
// the core calls back into (NSD discovery, content:// files with UriGuard),
// app paths, local addresses and the export of received files.

plugins {
    alias(libs.plugins.android.library)
}

android {
    namespace = "io.github.dropumi.core"
    compileSdk = 36

    defaultConfig {
        minSdk = 26
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    testOptions {
        unitTests.all { test ->
            // JVM tests never load the native library: any accidental load
            // looks for this name and fails loudly (docs/testing.md).
            test.systemProperty("uniffi.component.dropumi_ffi.libraryOverride", "dropumi_ffi_must_not_load")
        }
    }
}

kotlin {
    jvmToolchain(17)
}

dependencies {
    api(project(":core-bindings"))
    api(libs.kotlinx.coroutines.android)
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
}
