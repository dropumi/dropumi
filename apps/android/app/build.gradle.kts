// SPDX-License-Identifier: MPL-2.0
//
// The Dropumi app for Android phones and tablets (ADR-0013).

import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// Release signing comes only from the gitignored apps/android/keystore.properties
// (storeFile, storePassword, keyAlias, keyPassword). Without it the release
// APK is unsigned. CI never signs.
val keystoreProperties = rootProject.file("keystore.properties").takeIf { it.isFile }?.let { file ->
    Properties().apply { file.inputStream().use { load(it) } }
}

// The commit the app was built from, shown in Settings next to the version.
// "unknown" outside a git checkout (or without git); only a hex hash is used.
val gitCommit: String = try {
    providers.exec {
        commandLine("git", "rev-parse", "--short=12", "HEAD")
        workingDir = rootProject.projectDir
        isIgnoreExitValue = true
    }.standardOutput.asText.get().trim()
} catch (_: Exception) {
    ""
}.takeIf { Regex("[0-9a-f]{7,40}").matches(it) } ?: "unknown"

// The ABIs the APK ships. The ABI filter and the core check below use this
// one list, so an APK never carries JNA's libjnidispatch for an ABI that has
// no libdropumi_ffi.
val coreAbis = listOf("arm64-v8a", "x86_64")

android {
    namespace = "io.github.dropumi"
    compileSdk = 36

    defaultConfig {
        applicationId = "io.github.dropumi"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
        buildConfigField("String", "COMMIT", "\"$gitCommit\"")
        // Installs the test Application, never DropumiApplication (ADR-0013 D9).
        testInstrumentationRunner = "io.github.dropumi.DropumiTestRunner"
        // The Rust core is built for these two ABIs only. JNA also ships
        // 32-bit libjnidispatch; without the filter the APK could install on
        // a 32-bit device that has no libdropumi_ffi.
        ndk {
            abiFilters += coreAbis
        }
    }

    signingConfigs {
        if (keystoreProperties != null) {
            create("release") {
                storeFile = rootProject.file(keystoreProperties.getProperty("storeFile"))
                storePassword = keystoreProperties.getProperty("storePassword")
                keyAlias = keystoreProperties.getProperty("keyAlias")
                keyPassword = keystoreProperties.getProperty("keyPassword")
            }
        }
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        jniLibs {
            // Stored uncompressed and page-aligned, so the libraries load
            // straight from the APK on 16 KB-page devices.
            useLegacyPackaging = false
        }
    }

    lint {
        lintConfig = rootProject.file("lint.xml")
        checkDependencies = true
        abortOnError = true
    }

    // MockContentResolver (android.test.mock) for the instrumented tests.
    useLibrary("android.test.mock")

    testOptions {
        unitTests.all { test ->
            // JVM tests never load the native library (see :core).
            test.systemProperty("uniffi.component.dropumi_ffi.libraryOverride", "dropumi_ffi_must_not_load")
            // TokensTest checks the colour tokens against design.md itself.
            val designMd = rootProject.file("../../design.md")
            test.systemProperty("dropumi.designMd", designMd.path)
            test.inputs.file(designMd).withPathSensitivity(PathSensitivity.NONE)
        }
    }
}

kotlin {
    jvmToolchain(17)
}

// Every variant refuses a missing or stale Rust core, or one that lacks a
// library for any ABI in coreAbis, and never runs cargo
// (scripts/android/check-core-artifacts.sh). The check gets the variant's
// build type, so a release build with a dev-profile core prints a warning.
val checkCoreScript = rootProject.file("../../scripts/android/check-core-artifacts.sh").path
androidComponents {
    onVariants { variant ->
        val suffix = variant.name.replaceFirstChar { it.uppercase() }
        val coreBuildType = if (variant.buildType == "release") "release" else "debug"
        val check = tasks.register<Exec>("checkCoreArtifacts$suffix") {
            description = "Checks that libdropumi_ffi.so exists for ${coreAbis.joinToString()} " +
                "and was built from the current Rust sources."
            commandLine(
                "bash", checkCoreScript,
                "--abis", coreAbis.joinToString(","),
                "--build-type", coreBuildType,
            )
        }
        tasks.named { it == "pre${suffix}Build" }.configureEach { dependsOn(check) }
    }
}

dependencies {
    implementation(project(":appstate"))
    implementation(project(":core"))

    implementation(platform(libs.compose.bom))
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.lifecycle.process)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.kotlinx.coroutines.android)

    // The fake core reaches the test APKs only, never an app APK (ADR-0013 D9).
    testImplementation(project(":testsupport"))
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test)

    androidTestImplementation(project(":testsupport"))
    androidTestImplementation(platform(libs.compose.bom))
    androidTestImplementation(libs.compose.ui.test.junit4)
    // ui-test-junit4's espresso-core 3.5.0 fails on API 36 (InputManager.getInstance).
    androidTestImplementation(libs.androidx.test.espresso.core)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.rules)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.uiautomator)
}
