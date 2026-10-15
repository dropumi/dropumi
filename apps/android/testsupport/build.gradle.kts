// SPDX-License-Identifier: MPL-2.0
//
// Test support: the fake core, fixtures and UI scenarios. :app uses it only
// as testImplementation and androidTestImplementation, so no app APK
// (debug or release) contains it. Its own JVM tests hold the AppModel tests
// that need the fake, so :appstate never depends on this module.

plugins {
    alias(libs.plugins.android.library)
}

android {
    namespace = "io.github.dropumi.testsupport"
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
            // JVM tests never load the native library (see :core).
            test.systemProperty("uniffi.component.dropumi_ffi.libraryOverride", "dropumi_ffi_must_not_load")
        }
    }
}

kotlin {
    jvmToolchain(17)
}

dependencies {
    api(project(":appstate"))
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
}
