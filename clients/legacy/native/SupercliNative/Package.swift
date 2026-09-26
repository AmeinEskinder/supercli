// swift-tools-version: 6.0
//
// SupercliNative — Phase 0 spike for the Swift + libghostty rewrite.
//
// libghostty-spm is VENDORED at ../vendor/libghostty-spm (tag 1.2.4 plus
// local patches, see SUPERCLI-PATCHES.md there). The libghostty public API
// is declared alpha upstream, so every bump is a deliberate event
// (budgeted in PRD §8): re-vendor explicitly, re-applying the patches.

import Foundation
import PackageDescription

let packageRoot = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let repoRoot = packageRoot
    .deletingLastPathComponent()
    .deletingLastPathComponent()
    .deletingLastPathComponent()
let nativeBridgeRoot = repoRoot
    .appendingPathComponent("crates/target/native-bridge", isDirectory: true)

let package = Package(
    name: "SupercliNative",
    platforms: [
        .macOS(.v13),
    ],
    dependencies: [
        // Cross-platform DTOs and pure logic shared by the Mac app and the
        // future iOS remote-controller app.
        .package(path: "../../shared/SupercliShared"),
        // Prebuilt GhosttyKit.xcframework (trimmed libghostty core: VT,
        // Metal renderer, CoreText shaping, exec termio) + the
        // GhosttyTerminal Swift wrapper (AppKit NSView, input handling,
        // display link, runtime callbacks).
        .package(path: "../vendor/libghostty-spm"),
        // Sparkle 2 handles signed appcast checks and app replacement for
        // Cloudflare/R2-hosted beta and stable updates.
        .package(url: "https://github.com/sparkle-project/Sparkle", from: "2.6.0"),
    ],
    targets: [
        .target(
            name: "CSupercliNativeBridge",
            path: "Sources/CSupercliNativeBridge",
            publicHeadersPath: "include"
        ),
        .executableTarget(
            name: "SupercliNative",
            dependencies: [
                "CSupercliNativeBridge",
                .product(name: "SupercliShared", package: "SupercliShared"),
                // Imported ONLY inside GhosttyBridge.swift (PRD §8 isolation
                // rule). Everything else in this target must stay
                // Ghostty-agnostic.
                .product(name: "GhosttyTerminal", package: "libghostty-spm"),
                .product(name: "Sparkle", package: "Sparkle"),
            ],
            path: "Sources/SupercliNative",
            resources: [
                // Dock icon for the bare-executable spike (no .app bundle,
                // so main.swift sets applicationIconImage at launch).
                .copy("Resources/AppIcon.png"),
                .copy("Resources/TestFlightIcon.png"),
                // Pixel mascot animation frames (13×13, extracted losslessly
                // from supercli-mascot/mascot-animated.webp; same assets as the
                // iOS app's MascotFrame0…3 imagesets).
                .copy("Resources/MascotFrame0.png"),
                .copy("Resources/MascotFrame1.png"),
                .copy("Resources/MascotFrame2.png"),
                .copy("Resources/MascotFrame3.png"),
            ],
            linkerSettings: [
                .unsafeFlags(
                    ["-L", nativeBridgeRoot.appendingPathComponent("debug").path,
                     "-lsupercli_native_bridge"],
                    .when(platforms: [.macOS], configuration: .debug)
                ),
                .unsafeFlags(
                    ["-L", nativeBridgeRoot.appendingPathComponent("release").path,
                     "-lsupercli_native_bridge"],
                    .when(platforms: [.macOS], configuration: .release)
                ),
            ]
        ),
        .testTarget(
            name: "SupercliNativeTests",
            dependencies: [
                "SupercliNative",
            ],
            path: "Tests/SupercliNativeTests"
        ),
    ]
)
