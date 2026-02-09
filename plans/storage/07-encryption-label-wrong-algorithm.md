# Encryption label says AES-256, actual algorithm is XChaCha20-Poly1305

## Problem

The storage profile editor's encryption checkbox description says "AES-256 encryption" but the actual implementation uses XChaCha20-Poly1305 via libsodium's `crypto_secretstream`.

## Where

`bae-ui/src/components/settings/storage_profiles.rs:811`:
```rust
"AES-256 encryption. Data is unreadable without your key."
```

The encryption subsection in settings correctly identifies XChaCha20-Poly1305. This is just the inline help text on the profile editor checkbox.

## Fix

Change the text to either:
- "XChaCha20-Poly1305 encryption. Data is unreadable without your key."
- "Encrypted at rest. Data is unreadable without your key." (avoids naming the algorithm in two places)
