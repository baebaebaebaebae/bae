#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

echo "Adding iOS targets..."
rustup target add aarch64-apple-ios aarch64-apple-ios-sim 2>/dev/null || true

echo "Building for iOS device (arm64)..."
cargo build --release --target aarch64-apple-ios

echo "Building for iOS simulator (arm64)..."
cargo build --release --target aarch64-apple-ios-sim

echo "Generating Swift bindings..."
mkdir -p swift-bindings
cargo run --bin uniffi-bindgen generate \
    --library target/aarch64-apple-ios/release/libbae_crypto.a \
    --language swift \
    --out-dir swift-bindings/

echo "Creating XCFramework..."
rm -rf BaeCryptoFFI.xcframework

# The swift bindings generate a modulemap and header.
# We need to set up header directories for xcodebuild.
mkdir -p swift-bindings/headers
cp swift-bindings/bae_cryptoFFI.h swift-bindings/headers/
cp swift-bindings/bae_cryptoFFI.modulemap swift-bindings/headers/module.modulemap

xcodebuild -create-xcframework \
    -library target/aarch64-apple-ios/release/libbae_crypto.a \
    -headers swift-bindings/headers \
    -library target/aarch64-apple-ios-sim/release/libbae_crypto.a \
    -headers swift-bindings/headers \
    -output BaeCryptoFFI.xcframework

echo ""
echo "Done. Outputs:"
echo "  BaeCryptoFFI.xcframework/  (add to Xcode project)"
echo "  swift-bindings/bae_crypto.swift  (add to Swift sources)"
