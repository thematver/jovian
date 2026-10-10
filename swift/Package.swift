// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Jovian8",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "Jovian8Core", targets: ["Jovian8Core"]),
        .executable(name: "jovian8", targets: ["jovian8"]),
    ],
    targets: [
        .target(
            name: "Jovian8Core",
            swiftSettings: [.unsafeFlags(["-enforce-exclusivity=unchecked"])]
        ),
        .executableTarget(name: "jovian8", dependencies: ["Jovian8Core"]),
        .testTarget(name: "Jovian8CoreTests", dependencies: ["Jovian8Core"]),
    ],
    swiftLanguageModes: [.v5]
)
