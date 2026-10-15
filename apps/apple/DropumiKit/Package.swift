// swift-tools-version: 6.0
// SPDX-License-Identifier: MPL-2.0
//
// DropumiKit: the Swift side of the Dropumi apps. At this stage it holds the
// Rust core (DropumiFFI.xcframework, built by scripts/ffi/build-apple.sh and
// not committed) and the generated UniFFI bindings (DropumiBindings).
//
// The minimum OS versions match scripts/ffi/apple.env. The apps target 27.0;
// these lower minimums only let CI test the package on older runners.

import PackageDescription

let package = Package(
    name: "DropumiKit",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [
        .library(name: "DropumiBindings", targets: ["DropumiBindings"]),
    ],
    targets: [
        .binaryTarget(name: "DropumiFFI", path: "Artifacts/DropumiFFI.xcframework"),
        .target(
            name: "DropumiBindings",
            dependencies: ["DropumiFFI"],
            path: "Sources/DropumiBindings",
            linkerSettings: [.linkedFramework("Security")]
        ),
        .testTarget(
            name: "DropumiBindingsTests",
            dependencies: ["DropumiBindings"],
            path: "Tests/DropumiBindingsTests"
        ),
    ]
)
