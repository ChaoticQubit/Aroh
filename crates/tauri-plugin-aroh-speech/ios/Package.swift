// swift-tools-version:5.5

import PackageDescription

let package = Package(
  name: "tauri-plugin-aroh-speech",
  platforms: [
    .macOS(.v10_15),
    .iOS(.v14),
  ],
  products: [
    .library(
      name: "tauri-plugin-aroh-speech",
      type: .static,
      targets: ["tauri-plugin-aroh-speech"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-aroh-speech",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
