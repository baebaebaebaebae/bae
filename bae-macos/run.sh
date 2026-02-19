#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

./bae-bridge/build-macos.sh
cp bae-bridge/swift-bindings/bae_bridge.swift bae-macos/bae/bae/bae_bridge.swift
cd bae-macos/bae && xcodegen && cd ../..
xcodebuild -project bae-macos/bae/bae.xcodeproj -scheme bae -configuration Debug -derivedDataPath build build
open build/Build/Products/Debug/bae.app
