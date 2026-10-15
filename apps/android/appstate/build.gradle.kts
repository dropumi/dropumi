// SPDX-License-Identifier: MPL-2.0
//
// The app's state: AppModel and everything the screens render. Main code
// makes no Android framework calls, so it is tested on the host JVM.

plugins {
    alias(libs.plugins.android.library)
}

android {
    namespace = "io.github.dropumi.appstate"
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
    api(project(":core"))
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)
}
