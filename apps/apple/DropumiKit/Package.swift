// swift-tools-version: 6.0
// SPDX-License-Identifier: MPL-2.0
//
// DropumiKit: the Swift side of the Dropumi apps.
//
// - DropumiFFI: the Rust core (DropumiFFI.xcframework, built by
//   scripts/ffi/build-apple.sh and not committed).
// - DropumiBindings: the generated UniFFI bindings.
// - DropumiShare: the share inbox, shared with the Share Extension. It does
//   not link the Rust core (the extension never loads it).
// - DropumiCore: the app's view of the core, Bonjour, paths.
// - DropumiAppState: UI-free, observable app state.
// - DropumiDesign: tokens, fonts and components (no Rust either).
// - DropumiTestSupport: a fake core for tests and UI state tests.
//
// The minimum OS versions match scripts/ffi/apple.env. The apps target
// 27.0; these lower minimums only let CI test the package on older runners.

import PackageDescription

let package = Package(
    name: "DropumiKit",
    defaultLocalization: "en",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "DropumiBindings", targets: ["DropumiBindings"]),
        .library(name: "DropumiShare", targets: ["DropumiShare"]),
        .library(name: "DropumiCore", targets: ["DropumiCore"]),
        .library(name: "DropumiAppState", targets: ["DropumiAppState"]),
        .library(name: "DropumiDesign", targets: ["DropumiDesign"]),
        .library(name: "DropumiTestSupport", targets: ["DropumiTestSupport"]),
    ],
    targets: [
        .binaryTarget(name: "DropumiFFI", path: "Artifacts/DropumiFFI.xcframework"),
        .target(
            name: "DropumiBindings",
            dependencies: ["DropumiFFI"],
            path: "Sources/DropumiBindings",
            linkerSettings: [.linkedFramework("Security")]
        ),
        .target(name: "DropumiShare"),
        .target(name: "DropumiCore", dependencies: ["DropumiBindings", "DropumiShare"]),
        .target(name: "DropumiAppState", dependencies: ["DropumiCore"]),
        .target(name: "DropumiDesign", resources: [.copy("Resources/Fonts")]),
        .target(name: "DropumiTestSupport", dependencies: ["DropumiAppState"]),
        .testTarget(
            name: "DropumiBindingsTests",
            dependencies: ["DropumiBindings"],
            path: "Tests/DropumiBindingsTests"
        ),
        .testTarget(name: "DropumiShareTests", dependencies: ["DropumiShare"]),
        .testTarget(name: "DropumiCoreTests", dependencies: ["DropumiCore"]),
        .testTarget(name: "DropumiAppStateTests", dependencies: ["DropumiAppState", "DropumiTestSupport"]),
    ]
)
