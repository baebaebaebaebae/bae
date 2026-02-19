# bae-macos

Native macOS app for bae, built with SwiftUI.

## Build

1. Build the Rust bridge:

   cd bae-bridge
   ./build-macos.sh

2. Copy outputs to the macOS project:

   cp bae-bridge/BaeBridgeFFI.xcframework bae-macos/
   cp bae-bridge/swift-bindings/bae_bridge.swift bae-macos/bae/bae/

3. Generate Xcode project:

   cd bae-macos/bae
   xcodegen

4. Open in Xcode and build.

## Requirements

- macOS 14.0+
- Xcode 16+
- Rust toolchain with aarch64-apple-darwin target
- FFmpeg (for bae-core audio codec support)
