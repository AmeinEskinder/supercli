// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "SupercliShared",
    platforms: [
        .iOS(.v16),
        .macOS(.v13),
    ],
    products: [
        .library(
            name: "SupercliShared",
            targets: ["SupercliShared"]
        ),
    ],
    targets: [
        .target(
            name: "SupercliShared",
            path: "Sources/SupercliShared"
        ),
        .testTarget(
            name: "SupercliSharedTests",
            dependencies: ["SupercliShared"],
            path: "Tests/SupercliSharedTests"
        ),
    ]
)
