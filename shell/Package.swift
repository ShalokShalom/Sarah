// swift-tools-version: 5.9
// CounterShell — thin SwiftUI wrapper around the Rust counter core.
//
// This Swift package is the SHELL layer of the Phase 1 canonical example.
// All business logic lives in the Rust crate at ../core/.
// This package only handles:
//   - SwiftUI view rendering
//   - User interaction dispatch
//   - Binding the generated UniFFI Swift package (CoreFFI)
//
// Build the Rust core first:
//   cd ../core && cargo build
//
// Then run `cargo run --bin uniffi-bindgen generate` to produce the Swift
// bindings package, and add it as a local dependency below.

import PackageDescription

let package = Package(
    name: "CounterShell",
    platforms: [
        .iOS(.v17),
        .macOS(.v14),
    ],
    products: [
        .library(name: "CounterShell", targets: ["CounterShell"]),
    ],
    dependencies: [
        // The UniFFI-generated Swift package is produced by:
        //   cargo run --bin uniffi-bindgen generate \
        //     --library target/debug/libswift_rust_core.dylib \
        //     --language swift \
        //     --out-dir ../shell/Sources/CoreFFI
        //
        // Once generated, uncomment and adjust the path:
        // .package(path: "../bindings/swift"),
    ],
    targets: [
        .target(
            name: "CounterShell",
            dependencies: [
                // "SwiftRustCore",  // uncomment after generating UniFFI bindings
            ],
            path: "Sources/CounterApp"
        ),
        .testTarget(
            name: "CounterShellTests",
            dependencies: ["CounterShell"],
            path: "Tests/CounterShellTests"
        ),
    ]
)
