#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

# Use sccache if available
if command -v sccache &> /dev/null; then
    export RUSTC_WRAPPER=sccache
fi

echo "Building for macOS (arm64)..."
cargo build --release --target aarch64-apple-darwin -p bae-bridge

echo "Generating Swift bindings..."
mkdir -p bae-bridge/swift-bindings
cargo run --bin uniffi-bindgen generate \
    --library target/aarch64-apple-darwin/release/libbae_bridge.a \
    --language swift \
    --out-dir bae-bridge/swift-bindings/

echo "Creating XCFramework..."
rm -rf bae-bridge/BaeBridgeFFI.xcframework

mkdir -p bae-bridge/swift-bindings/headers
cp bae-bridge/swift-bindings/bae_bridgeFFI.h bae-bridge/swift-bindings/headers/
cp bae-bridge/swift-bindings/bae_bridgeFFI.modulemap bae-bridge/swift-bindings/headers/module.modulemap

xcodebuild -create-xcframework \
    -library target/aarch64-apple-darwin/release/libbae_bridge.a \
    -headers bae-bridge/swift-bindings/headers \
    -output bae-bridge/BaeBridgeFFI.xcframework

echo ""
echo "Done. Outputs:"
echo "  bae-bridge/BaeBridgeFFI.xcframework/  (add to Xcode project)"
echo "  bae-bridge/swift-bindings/bae_bridge.swift  (add to Swift sources)"
