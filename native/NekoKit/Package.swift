// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "NekoNative",
    platforms: [.macOS(.v14)],
    products: [.library(name: "NekoKit", targets: ["NekoKit"]), .executable(name: "NekoNative", targets: ["NekoNative"])],
    targets: [.target(name: "NekoKit"), .executableTarget(name: "NekoNative", dependencies: ["NekoKit"]), .testTarget(name: "NekoKitTests", dependencies: ["NekoKit"]), .testTarget(name: "NekoNativeTests", dependencies: ["NekoNative", "NekoKit"])]
)
