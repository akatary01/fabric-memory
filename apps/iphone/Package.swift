// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "FabricPhoneCore",
    platforms: [.iOS(.v17), .macOS(.v14)],
    products: [.library(name: "FabricPhoneCore", targets: ["FabricCore"])],
    dependencies: [
        .package(url: "https://github.com/microsoft/onnxruntime-swift-package-manager", exact: "1.24.2")
    ],
    targets: [
        .systemLibrary(name: "CSQLite"),
        .binaryTarget(name:"llama",url:"https://github.com/ggml-org/llama.cpp/releases/download/b12211/llama-b12211-xcframework.zip",checksum:"1f701c47b0e2f9410e1441960105e7d7ad1757f8a49e4063c6e792df31348a7a"),
        .target(name: "FabricCore", dependencies: ["CSQLite", "llama", .product(name: "onnxruntime", package: "onnxruntime-swift-package-manager")]),
        .testTarget(name: "FabricCoreTests", dependencies: ["FabricCore"])
    ]
)
