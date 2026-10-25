// SPDX-License-Identifier: MPL-2.0
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}

rootProject.name = "dropumi"
include(":core-bindings")
include(":core")
include(":appstate")
include(":testsupport")
include(":app")
// Test only: draws an overlay window for the R23 tests. Never shipped.
include(":overlayhelper")
