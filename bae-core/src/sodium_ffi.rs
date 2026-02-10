//! Minimal FFI bindings to libsodium
//!
//! Requires libsodium system library:
//! - macOS: `brew install libsodium`
//! - Linux: `apt install libsodium-dev`

use libc::{c_int, c_uchar, c_ulonglong};

// XChaCha20-Poly1305 AEAD constants
pub const NPUBBYTES: usize = 24; // nonce size
pub const ABYTES: usize = 16; // auth tag size

// Ed25519 signing constants
pub const SIGN_PUBLICKEYBYTES: usize = 32;
pub const SIGN_SECRETKEYBYTES: usize = 64;
pub const SIGN_BYTES: usize = 64;

// X25519 / sealed box constants
pub const CURVE25519_PUBLICKEYBYTES: usize = 32;
pub const CURVE25519_SECRETKEYBYTES: usize = 32;
pub const SEALBYTES: usize = 48; // crypto_box_PUBLICKEYBYTES + crypto_box_MACBYTES = 32 + 16

extern "C" {
    pub fn sodium_init() -> c_int;

    // --- XChaCha20-Poly1305 AEAD ---

    pub fn crypto_aead_xchacha20poly1305_ietf_encrypt(
        c: *mut c_uchar,
        clen_p: *mut c_ulonglong,
        m: *const c_uchar,
        mlen: c_ulonglong,
        ad: *const c_uchar,
        adlen: c_ulonglong,
        nsec: *const c_uchar,
        npub: *const c_uchar,
        k: *const c_uchar,
    ) -> c_int;

    pub fn crypto_aead_xchacha20poly1305_ietf_decrypt(
        m: *mut c_uchar,
        mlen_p: *mut c_ulonglong,
        nsec: *mut c_uchar,
        c: *const c_uchar,
        clen: c_ulonglong,
        ad: *const c_uchar,
        adlen: c_ulonglong,
        npub: *const c_uchar,
        k: *const c_uchar,
    ) -> c_int;

    pub fn randombytes_buf(buf: *mut c_uchar, size: usize);

    // --- Ed25519 signing ---

    pub fn crypto_sign_ed25519_keypair(pk: *mut c_uchar, sk: *mut c_uchar) -> c_int;

    pub fn crypto_sign_ed25519_detached(
        sig: *mut c_uchar,
        siglen_p: *mut c_ulonglong,
        m: *const c_uchar,
        mlen: c_ulonglong,
        sk: *const c_uchar,
    ) -> c_int;

    pub fn crypto_sign_ed25519_verify_detached(
        sig: *const c_uchar,
        m: *const c_uchar,
        mlen: c_ulonglong,
        pk: *const c_uchar,
    ) -> c_int;

    // --- Ed25519 -> X25519 conversion ---

    pub fn crypto_sign_ed25519_sk_to_curve25519(
        curve25519_sk: *mut c_uchar,
        ed25519_sk: *const c_uchar,
    ) -> c_int;

    pub fn crypto_sign_ed25519_pk_to_curve25519(
        curve25519_pk: *mut c_uchar,
        ed25519_pk: *const c_uchar,
    ) -> c_int;

    // --- Sealed boxes (anonymous public-key encryption) ---

    pub fn crypto_box_seal(
        c: *mut c_uchar,
        m: *const c_uchar,
        mlen: c_ulonglong,
        pk: *const c_uchar,
    ) -> c_int;

    pub fn crypto_box_seal_open(
        m: *mut c_uchar,
        c: *const c_uchar,
        clen: c_ulonglong,
        pk: *const c_uchar,
        sk: *const c_uchar,
    ) -> c_int;
}
