// swift-tools-version:5.5

import PackageDescription

let package = Package(
  name: "tauri-plugin-aroh-ios",
  platforms: [
    .macOS(.v10_15),
    .iOS(.v15),
  ],
  products: [
    .library(
      name: "tauri-plugin-aroh-ios",
      type: .static,
      targets: ["tauri-plugin-aroh-ios"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-aroh-ios",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
