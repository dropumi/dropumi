// SPDX-License-Identifier: MPL-2.0
//
// Test only: a tiny app whose OverlayService draws a TYPE_APPLICATION_OVERLAY
// window for the overlay-protection tests (threat model R23). It is
// installed by the UI test script before a run and uninstalled after it;
// it is never shipped, and it has no release build.

plugins {
    alias(libs.plugins.android.application)
}

android {
    namespace = "io.github.dropumi.overlayhelper"
    compileSdk = 36

    defaultConfig {
        applicationId = "io.github.dropumi.overlayhelper"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    lint {
        lintConfig = rootProject.file("lint.xml")
    }
}

androidComponents {
    // Debug only: the helper is never built for release or signed for it.
    beforeVariants(selector().withBuildType("release")) { it.enable = false }
}

kotlin {
    jvmToolchain(17)
}

dependencies {
    implementation(libs.androidx.core.ktx)
}
