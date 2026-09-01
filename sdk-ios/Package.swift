// swift-tools-version:5.5
import PackageDescription

let package = Package(
    name: "Stackhouse",
    platforms: [
        .iOS(.v13), .macOS(.v10_15)
    ],
    products: [
        .library(name: "Stackhouse", targets: ["Stackhouse"]),
    ],
    targets: [
        .target(name: "Stackhouse", dependencies: []),
        .testTarget(name: "StackhouseTests", dependencies: ["Stackhouse"]),
    ]
)
