// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "dmb",
    platforms: [.macOS(.v14)],
    targets: [
        .target(name: "DMBCore", swiftSettings: [.swiftLanguageMode(.v5)]),
        .executableTarget(name: "dmb", dependencies: ["DMBCore"], swiftSettings: [.swiftLanguageMode(.v5)]),
        .target(name: "MirrorUI", dependencies: ["DMBCore"], swiftSettings: [.swiftLanguageMode(.v5)]),
        .executableTarget(name: "Twin", dependencies: ["MirrorUI"], swiftSettings: [.swiftLanguageMode(.v5)]),
        .testTarget(name: "DMBCoreTests", dependencies: ["DMBCore"], swiftSettings: [.swiftLanguageMode(.v5)]),
        .testTarget(name: "MirrorUITests", dependencies: ["MirrorUI"], swiftSettings: [.swiftLanguageMode(.v5)]),
    ]
)
