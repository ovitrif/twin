// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "imb",
    platforms: [.macOS(.v14)],
    targets: [
        .target(name: "IMBCore", swiftSettings: [.swiftLanguageMode(.v5)]),
        .executableTarget(name: "imb", dependencies: ["IMBCore"], swiftSettings: [.swiftLanguageMode(.v5)]),
        .target(name: "MirrorUI", dependencies: ["IMBCore"], swiftSettings: [.swiftLanguageMode(.v5)]),
        .executableTarget(name: "IOSMirror", dependencies: ["MirrorUI"], swiftSettings: [.swiftLanguageMode(.v5)]),
        .testTarget(name: "IMBCoreTests", dependencies: ["IMBCore"], swiftSettings: [.swiftLanguageMode(.v5)]),
        .testTarget(name: "MirrorUITests", dependencies: ["MirrorUI"], swiftSettings: [.swiftLanguageMode(.v5)]),
    ]
)
