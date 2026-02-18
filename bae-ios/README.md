# bae-ios

iOS app for bae.

## Structure

```
bae-ios/
  BaeCrypto/     Rust crate with encryption primitives (UniFFI -> Swift)
  bae/           SwiftUI iOS app
```

## Building the crypto library

```sh
cd BaeCrypto
./build-ios.sh
```

This produces:
- `BaeCryptoFFI.xcframework/` -- static library for iOS device + simulator
- `swift-bindings/bae_crypto.swift` -- generated Swift bindings

## Setting up the Xcode project

Install [xcodegen](https://github.com/yonaskolb/XcodeGen):

```sh
brew install xcodegen
```

Generate the Xcode project:

```sh
cd bae
xcodegen generate
open bae.xcodeproj
```

To use the crypto library, add `BaeCryptoFFI.xcframework` and `bae_crypto.swift` from the build step to the Xcode project.

## Running tests (crypto crate only)

```sh
cd BaeCrypto
cargo test
```
