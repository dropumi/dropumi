// SPDX-License-Identifier: MPL-2.0
//
// The generated UniFFI bindings of dropumi-ffi (committed; regenerate with
// scripts/ffi/bindings.sh) and, once scripts/ffi/build-android.sh has run,
// the native library for the device ABIs.

plugins {
    alias(libs.plugins.android.library)
}

android {
    namespace = "io.github.dropumi.ffi"
    compileSdk = 36
    ndkVersion = "27.1.12297006"

    defaultConfig {
        minSdk = 26
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets {
        getByName("main") {
            // Written by scripts/ffi/build-android.sh.
            jniLibs.directories.add("build/rust/jniLibs")
        }
    }

    testOptions {
        unitTests.all { test ->
            // The JVM tests load the host build of the Rust library.
            val hostLibDir = providers.gradleProperty("dropumi.hostLibDir").get()
            test.systemProperty("jna.library.path", rootDir.resolve(hostLibDir).canonicalPath)
            test.systemProperty("dropumi.test.tmp", layout.buildDirectory.dir("test-tmp").get().asFile.path)
        }
    }
}

kotlin {
    jvmToolchain(17)
}

dependencies {
    // JNA 5.17.0 and later build jnidispatch for 16 KB pages.
    api("${libs.jna.get()}@aar")
    // The desktop JNA jar carries the host jnidispatch for the JVM tests.
    testImplementation(libs.jna)
    testImplementation(libs.junit)
}
