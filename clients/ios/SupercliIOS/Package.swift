// swift-tools-version: 6.0

import PackageDescription

let package = Package(
    name: "SupercliIOS",
    platforms: [
        .iOS(.v17),
    ],
    products: [
        .library(
            name: "SupercliIOS",
            targets: ["SupercliIOS"]
        ),
    ],
    dependencies: [
        .package(path: "../../shared/SupercliShared"),
        .package(path: "../../native/vendor/libghostty-spm"),
    ],
    targets: [
        .target(
            name: "SupercliIOS",
            dependencies: [
                .product(name: "SupercliShared", package: "SupercliShared"),
                .product(name: "GhosttyTerminal", package: "libghostty-spm"),
            ],
            path: "Sources/SupercliIOS"
        ),
        .testTarget(
            name: "SupercliIOSTests",
            dependencies: ["SupercliIOS"],
            path: "Tests/SupercliIOSTests"
        ),
    ]
)
