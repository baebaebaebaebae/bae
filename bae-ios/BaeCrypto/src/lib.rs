use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chacha20poly1305::aead::generic_array::GenericArray;
use chacha20poly1305::{aead::Aead, KeyInit, XChaCha20Poly1305};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;

uniffi::setup_scaffolding!();

const NONCE_SIZE: usize = 24;
const TAG_SIZE: usize = 16;
const CHUNK_SIZE: usize = 65536;
const ENCRYPTED_CHUNK_SIZE: usize = CHUNK_SIZE + TAG_SIZE;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CryptoError {
    #[error("Decryption failed: {msg}")]
    Decryption { msg: String },
    #[error("Invalid key: {msg}")]
    InvalidKey { msg: String },
    #[error("Invalid input: {msg}")]
    InvalidInput { msg: String },
}

#[derive(Debug, uniffi::Record)]
pub struct DeviceLinkPayload {
    pub proxy_url: String,
    pub encryption_key: Vec<u8>,
    pub signing_key: Vec<u8>,
    pub library_id: String,
}

/// Decrypt an entire file encrypted with bae's chunked XChaCha20-Poly1305 format.
///
/// Format: [24-byte base_nonce][encrypted_chunk_0][encrypted_chunk_1]...
/// Each chunk: up to 65536 bytes plaintext + 16-byte Poly1305 auth tag.
#[uniffi::export]
pub fn decrypt_file(ciphertext: Vec<u8>, key: Vec<u8>) -> Result<Vec<u8>, CryptoError> {
    let key = validate_key(&key)?;
    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(&key));

    if ciphertext.len() < NONCE_SIZE {
        return Err(CryptoError::Decryption {
            msg: "ciphertext too short for nonce".to_string(),
        });
    }

    let base_nonce: [u8; NONCE_SIZE] = ciphertext[..NONCE_SIZE]
        .try_into()
        .map_err(|_| CryptoError::Decryption {
            msg: "invalid nonce".to_string(),
        })?;

    let data = &ciphertext[NONCE_SIZE..];
    let total_len = data.len();
    let num_full_chunks = total_len / ENCRYPTED_CHUNK_SIZE;
    let remainder = total_len % ENCRYPTED_CHUNK_SIZE;
    let total_chunks = num_full_chunks + if remainder > 0 { 1 } else { 0 };

    let mut plaintext = Vec::new();

    for i in 0..total_chunks {
        let chunk_start = i * ENCRYPTED_CHUNK_SIZE;
        let chunk_end = if i == total_chunks - 1 && remainder > 0 {
            chunk_start + remainder
        } else {
            chunk_start + ENCRYPTED_CHUNK_SIZE
        };

        let chunk_data = &data[chunk_start..chunk_end];
        let nonce = chunk_nonce(&base_nonce, i as u64);
        let nonce_arr = GenericArray::from_slice(&nonce);

        let decrypted =
            cipher
                .decrypt(nonce_arr, chunk_data)
                .map_err(|_| CryptoError::Decryption {
                    msg: format!("authentication failed at chunk {i}"),
                })?;

        plaintext.extend(decrypted);
    }

    Ok(plaintext)
}

/// Decrypt a single chunk from bae's chunked encrypted format.
///
/// `ciphertext` must include the 24-byte nonce header followed by all encrypted chunks.
/// `chunk_index` is the zero-based chunk to decrypt.
#[uniffi::export]
pub fn decrypt_chunk(
    ciphertext: Vec<u8>,
    chunk_index: u32,
    key: Vec<u8>,
) -> Result<Vec<u8>, CryptoError> {
    let key = validate_key(&key)?;
    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(&key));
    let chunk_index = chunk_index as usize;

    if ciphertext.len() < NONCE_SIZE {
        return Err(CryptoError::Decryption {
            msg: "ciphertext too short for nonce".to_string(),
        });
    }

    let base_nonce: [u8; NONCE_SIZE] = ciphertext[..NONCE_SIZE]
        .try_into()
        .map_err(|_| CryptoError::Decryption {
            msg: "invalid nonce".to_string(),
        })?;

    let data = &ciphertext[NONCE_SIZE..];
    let total_len = data.len();
    let num_full_chunks = total_len / ENCRYPTED_CHUNK_SIZE;
    let remainder = total_len % ENCRYPTED_CHUNK_SIZE;
    let total_chunks = num_full_chunks + if remainder > 0 { 1 } else { 0 };

    if chunk_index >= total_chunks {
        return Err(CryptoError::Decryption {
            msg: format!(
                "chunk index {} out of range (total chunks: {})",
                chunk_index, total_chunks
            ),
        });
    }

    let chunk_start = chunk_index * ENCRYPTED_CHUNK_SIZE;
    let chunk_end = if chunk_index == total_chunks - 1 && remainder > 0 {
        chunk_start + remainder
    } else {
        chunk_start + ENCRYPTED_CHUNK_SIZE
    };

    let chunk_data = &data[chunk_start..chunk_end];
    let nonce = chunk_nonce(&base_nonce, chunk_index as u64);
    let nonce_arr = GenericArray::from_slice(&nonce);

    let decrypted =
        cipher
            .decrypt(nonce_arr, chunk_data)
            .map_err(|_| CryptoError::Decryption {
                msg: format!("authentication failed at chunk {chunk_index}"),
            })?;

    Ok(decrypted)
}

/// Derive a per-release 32-byte encryption key from a master key.
///
/// Uses the same derivation as bae-core:
///   salt = HMAC-SHA256(master_key, "bae-hkdf-salt-v1")
///   derived = HKDF-SHA256(salt=salt, ikm=master_key, info="bae-release-v1:{release_id}")
#[uniffi::export]
pub fn derive_release_key(
    master_key: Vec<u8>,
    release_id: String,
) -> Result<Vec<u8>, CryptoError> {
    let key = validate_key(&master_key)?;

    let mut mac =
        <Hmac<Sha256> as Mac>::new_from_slice(&key).expect("HMAC accepts any key length");
    mac.update(b"bae-hkdf-salt-v1");
    let salt = mac.finalize().into_bytes();

    let hk = Hkdf::<Sha256>::new(Some(&salt), &key);
    let info = format!("bae-release-v1:{release_id}");
    let mut okm = [0u8; 32];
    hk.expand(info.as_bytes(), &mut okm)
        .expect("32 bytes is a valid HKDF output length");

    Ok(okm.to_vec())
}

/// Parse a device link JSON payload (scanned from QR code).
///
/// JSON format: {"proxy_url":"...","encryption_key":"<base64url>","signing_key":"<base64url>","library_id":"..."}
#[uniffi::export]
pub fn parse_device_link(json: String) -> Result<DeviceLinkPayload, CryptoError> {
    #[derive(serde::Deserialize)]
    struct RawPayload {
        proxy_url: String,
        encryption_key: String,
        signing_key: String,
        library_id: String,
    }

    let raw: RawPayload =
        serde_json::from_str(&json).map_err(|e| CryptoError::InvalidInput {
            msg: format!("invalid device link JSON: {e}"),
        })?;

    let encryption_key = URL_SAFE_NO_PAD
        .decode(&raw.encryption_key)
        .map_err(|e| CryptoError::InvalidInput {
            msg: format!("invalid encryption key encoding: {e}"),
        })?;

    let signing_key = URL_SAFE_NO_PAD
        .decode(&raw.signing_key)
        .map_err(|e| CryptoError::InvalidInput {
            msg: format!("invalid signing key encoding: {e}"),
        })?;

    if encryption_key.len() != 32 {
        return Err(CryptoError::InvalidKey {
            msg: format!(
                "encryption key must be 32 bytes, got {}",
                encryption_key.len()
            ),
        });
    }

    if signing_key.len() != 64 {
        return Err(CryptoError::InvalidKey {
            msg: format!(
                "signing key must be 64 bytes, got {}",
                signing_key.len()
            ),
        });
    }

    Ok(DeviceLinkPayload {
        proxy_url: raw.proxy_url,
        encryption_key,
        signing_key,
        library_id: raw.library_id,
    })
}

fn validate_key(key: &[u8]) -> Result<[u8; 32], CryptoError> {
    key.try_into().map_err(|_| CryptoError::InvalidKey {
        msg: format!("key must be 32 bytes, got {}", key.len()),
    })
}

/// Derive chunk nonce: base_nonce XOR chunk_index (little-endian in first 8 bytes).
fn chunk_nonce(base_nonce: &[u8; NONCE_SIZE], chunk_index: u64) -> [u8; NONCE_SIZE] {
    let mut nonce = *base_nonce;
    let index_bytes = chunk_index.to_le_bytes();
    for i in 0..8 {
        nonce[i] ^= index_bytes[i];
    }
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;
    use chacha20poly1305::aead::OsRng;
    use chacha20poly1305::AeadCore;

    fn test_key() -> Vec<u8> {
        vec![
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b,
            0x1c, 0x1d, 0x1e, 0x1f,
        ]
    }

    /// Encrypt using the same chunked format as bae-core.
    /// This is the reference implementation for tests -- produces data that
    /// decrypt_file and decrypt_chunk must be able to handle.
    fn encrypt_chunked(key: &[u8; 32], plaintext: &[u8]) -> Vec<u8> {
        let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(key));
        let base_nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let mut output = base_nonce.to_vec();

        if plaintext.is_empty() {
            let nonce = chunk_nonce(base_nonce.as_slice().try_into().unwrap(), 0);
            let nonce_arr = GenericArray::from_slice(&nonce);
            let ct = cipher.encrypt(nonce_arr, &[][..]).unwrap();
            output.extend(ct);
            return output;
        }

        for (i, chunk) in plaintext.chunks(CHUNK_SIZE).enumerate() {
            let nonce = chunk_nonce(base_nonce.as_slice().try_into().unwrap(), i as u64);
            let nonce_arr = GenericArray::from_slice(&nonce);
            let ct = cipher.encrypt(nonce_arr, chunk).unwrap();
            output.extend(ct);
        }

        output
    }

    // ---- decrypt_file tests ----

    #[test]
    fn decrypt_file_small() {
        let key = test_key();
        let key_arr: [u8; 32] = key.clone().try_into().unwrap();
        let plaintext = b"hello world";
        let ciphertext = encrypt_chunked(&key_arr, plaintext);
        let decrypted = decrypt_file(ciphertext, key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_file_empty() {
        let key = test_key();
        let key_arr: [u8; 32] = key.clone().try_into().unwrap();
        let ciphertext = encrypt_chunked(&key_arr, b"");
        let decrypted = decrypt_file(ciphertext, key).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn decrypt_file_exact_chunk() {
        let key = test_key();
        let key_arr: [u8; 32] = key.clone().try_into().unwrap();
        let plaintext = vec![0xAA; CHUNK_SIZE];
        let ciphertext = encrypt_chunked(&key_arr, &plaintext);
        let decrypted = decrypt_file(ciphertext, key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_file_multi_chunk() {
        let key = test_key();
        let key_arr: [u8; 32] = key.clone().try_into().unwrap();
        // 2.5 chunks
        let plaintext: Vec<u8> = (0..CHUNK_SIZE * 2 + CHUNK_SIZE / 2)
            .map(|i| (i % 256) as u8)
            .collect();
        let ciphertext = encrypt_chunked(&key_arr, &plaintext);
        let decrypted = decrypt_file(ciphertext, key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_file_single_byte() {
        let key = test_key();
        let key_arr: [u8; 32] = key.clone().try_into().unwrap();
        let plaintext = b"x";
        let ciphertext = encrypt_chunked(&key_arr, plaintext);
        let decrypted = decrypt_file(ciphertext, key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_file_wrong_key_fails() {
        let key = test_key();
        let key_arr: [u8; 32] = key.try_into().unwrap();
        let ciphertext = encrypt_chunked(&key_arr, b"secret");
        let wrong_key = vec![0xFF; 32];
        assert!(decrypt_file(ciphertext, wrong_key).is_err());
    }

    #[test]
    fn decrypt_file_tampered_fails() {
        let key = test_key();
        let key_arr: [u8; 32] = key.clone().try_into().unwrap();
        let mut ciphertext = encrypt_chunked(&key_arr, b"secret");
        ciphertext[NONCE_SIZE + 3] ^= 0xFF;
        assert!(decrypt_file(ciphertext, key).is_err());
    }

    #[test]
    fn decrypt_file_too_short_fails() {
        let key = test_key();
        assert!(decrypt_file(vec![0u8; 10], key).is_err());
    }

    #[test]
    fn decrypt_file_invalid_key_length() {
        assert!(decrypt_file(vec![0u8; 100], vec![0u8; 16]).is_err());
    }

    // ---- decrypt_chunk tests ----

    #[test]
    fn decrypt_chunk_random_access() {
        let key = test_key();
        let key_arr: [u8; 32] = key.clone().try_into().unwrap();

        // 3 chunks with distinct fill bytes
        let mut plaintext = vec![0x00u8; CHUNK_SIZE];
        plaintext.extend(vec![0x11u8; CHUNK_SIZE]);
        plaintext.extend(vec![0x22u8; CHUNK_SIZE]);

        let ciphertext = encrypt_chunked(&key_arr, &plaintext);

        let chunk0 = decrypt_chunk(ciphertext.clone(), 0, key.clone()).unwrap();
        assert_eq!(chunk0, vec![0x00u8; CHUNK_SIZE]);

        let chunk1 = decrypt_chunk(ciphertext.clone(), 1, key.clone()).unwrap();
        assert_eq!(chunk1, vec![0x11u8; CHUNK_SIZE]);

        let chunk2 = decrypt_chunk(ciphertext, 2, key).unwrap();
        assert_eq!(chunk2, vec![0x22u8; CHUNK_SIZE]);
    }

    #[test]
    fn decrypt_chunk_partial_last() {
        let key = test_key();
        let key_arr: [u8; 32] = key.clone().try_into().unwrap();

        let mut plaintext = vec![0xAAu8; CHUNK_SIZE];
        plaintext.extend(vec![0xBBu8; 100]);

        let ciphertext = encrypt_chunked(&key_arr, &plaintext);

        let chunk0 = decrypt_chunk(ciphertext.clone(), 0, key.clone()).unwrap();
        assert_eq!(chunk0, vec![0xAAu8; CHUNK_SIZE]);

        let chunk1 = decrypt_chunk(ciphertext, 1, key).unwrap();
        assert_eq!(chunk1, vec![0xBBu8; 100]);
    }

    #[test]
    fn decrypt_chunk_out_of_range() {
        let key = test_key();
        let key_arr: [u8; 32] = key.clone().try_into().unwrap();
        let plaintext = vec![0u8; CHUNK_SIZE]; // exactly 1 chunk
        let ciphertext = encrypt_chunked(&key_arr, &plaintext);
        assert!(decrypt_chunk(ciphertext.clone(), 0, key.clone()).is_ok());
        assert!(decrypt_chunk(ciphertext, 1, key).is_err());
    }

    // ---- chunk nonce derivation tests ----

    #[test]
    fn chunk_nonce_zero_index_is_identity() {
        let base = [0xAB; NONCE_SIZE];
        let nonce = chunk_nonce(&base, 0);
        assert_eq!(nonce, base);
    }

    #[test]
    fn chunk_nonce_xors_first_8_bytes() {
        let base = [0u8; NONCE_SIZE];
        let nonce = chunk_nonce(&base, 1);
        // 1u64 in little-endian: [1, 0, 0, 0, 0, 0, 0, 0]
        assert_eq!(nonce[0], 1);
        for byte in &nonce[1..] {
            assert_eq!(*byte, 0);
        }
    }

    #[test]
    fn chunk_nonce_leaves_last_16_bytes() {
        let mut base = [0u8; NONCE_SIZE];
        // Set bytes 8-23 to non-zero
        base[8..].fill(0xFF);
        let nonce = chunk_nonce(&base, 0xDEADBEEF);
        // Bytes 8-23 should be unchanged
        for (i, &byte) in nonce[8..].iter().enumerate() {
            assert_eq!(byte, 0xFF, "byte {} should be unchanged", i + 8);
        }
    }

    #[test]
    fn chunk_nonce_deterministic() {
        let base = [0x42; NONCE_SIZE];
        let n1 = chunk_nonce(&base, 7);
        let n2 = chunk_nonce(&base, 7);
        assert_eq!(n1, n2);
    }

    // ---- key derivation tests ----

    #[test]
    fn derive_release_key_deterministic() {
        let key = test_key();
        let derived1 = derive_release_key(key.clone(), "rel-123".to_string()).unwrap();
        let derived2 = derive_release_key(key, "rel-123".to_string()).unwrap();
        assert_eq!(derived1, derived2);
    }

    #[test]
    fn derive_release_key_different_releases() {
        let key = test_key();
        let key_a = derive_release_key(key.clone(), "rel-aaa".to_string()).unwrap();
        let key_b = derive_release_key(key, "rel-bbb".to_string()).unwrap();
        assert_ne!(key_a, key_b);
    }

    #[test]
    fn derive_release_key_different_master_keys() {
        let key1 = vec![0u8; 32];
        let key2 = vec![1u8; 32];
        let derived1 = derive_release_key(key1, "rel-123".to_string()).unwrap();
        let derived2 = derive_release_key(key2, "rel-123".to_string()).unwrap();
        assert_ne!(derived1, derived2);
    }

    #[test]
    fn derive_release_key_output_is_32_bytes() {
        let key = test_key();
        let derived = derive_release_key(key, "any-release".to_string()).unwrap();
        assert_eq!(derived.len(), 32);
    }

    #[test]
    fn derive_release_key_invalid_key_length() {
        assert!(derive_release_key(vec![0u8; 16], "rel-1".to_string()).is_err());
    }

    #[test]
    fn derive_release_key_then_decrypt() {
        let master_key = test_key();

        // Derive a release key and encrypt with it
        let release_key = derive_release_key(master_key.clone(), "rel-456".to_string()).unwrap();
        let release_arr: [u8; 32] = release_key.clone().try_into().unwrap();

        let plaintext = b"audio data for this release";
        let ciphertext = encrypt_chunked(&release_arr, plaintext);

        // Should decrypt with derived key
        let decrypted = decrypt_file(ciphertext.clone(), release_key).unwrap();
        assert_eq!(decrypted, plaintext);

        // Should NOT decrypt with master key
        assert!(decrypt_file(ciphertext.clone(), master_key).is_err());

        // Should NOT decrypt with wrong release key
        let wrong_key = derive_release_key(vec![0u8; 32], "rel-456".to_string()).unwrap();
        assert!(decrypt_file(ciphertext, wrong_key).is_err());
    }

    // ---- device link parsing tests ----

    #[test]
    fn parse_device_link_valid() {
        let enc_key = [0xAB_u8; 32];
        let sign_key = [0xCD_u8; 64];
        let json = format!(
            r#"{{"proxy_url":"https://test.bae.fm","encryption_key":"{}","signing_key":"{}","library_id":"lib-abc-123"}}"#,
            URL_SAFE_NO_PAD.encode(enc_key),
            URL_SAFE_NO_PAD.encode(sign_key),
        );

        let payload = parse_device_link(json).unwrap();
        assert_eq!(payload.proxy_url, "https://test.bae.fm");
        assert_eq!(payload.encryption_key, enc_key.to_vec());
        assert_eq!(payload.signing_key, sign_key.to_vec());
        assert_eq!(payload.library_id, "lib-abc-123");
    }

    #[test]
    fn parse_device_link_invalid_json() {
        let result = parse_device_link("not json".to_string());
        assert!(result.is_err());
        match result.unwrap_err() {
            CryptoError::InvalidInput { msg } => {
                assert!(msg.contains("invalid device link JSON"));
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn parse_device_link_wrong_encryption_key_length() {
        let json = format!(
            r#"{{"proxy_url":"x","encryption_key":"{}","signing_key":"{}","library_id":"y"}}"#,
            URL_SAFE_NO_PAD.encode([0xAA_u8; 16]), // too short
            URL_SAFE_NO_PAD.encode([0xBB_u8; 64]),
        );
        let result = parse_device_link(json);
        assert!(result.is_err());
        match result.unwrap_err() {
            CryptoError::InvalidKey { msg } => {
                assert!(msg.contains("encryption key must be 32 bytes"));
            }
            other => panic!("expected InvalidKey, got {other:?}"),
        }
    }

    #[test]
    fn parse_device_link_wrong_signing_key_length() {
        let json = format!(
            r#"{{"proxy_url":"x","encryption_key":"{}","signing_key":"{}","library_id":"y"}}"#,
            URL_SAFE_NO_PAD.encode([0xAA_u8; 32]),
            URL_SAFE_NO_PAD.encode([0xBB_u8; 32]), // too short
        );
        let result = parse_device_link(json);
        assert!(result.is_err());
        match result.unwrap_err() {
            CryptoError::InvalidKey { msg } => {
                assert!(msg.contains("signing key must be 64 bytes"));
            }
            other => panic!("expected InvalidKey, got {other:?}"),
        }
    }

    #[test]
    fn parse_device_link_invalid_base64() {
        let json =
            r#"{"proxy_url":"x","encryption_key":"!!!invalid!!!","signing_key":"AAAA","library_id":"y"}"#;
        let result = parse_device_link(json.to_string());
        assert!(result.is_err());
        match result.unwrap_err() {
            CryptoError::InvalidInput { msg } => {
                assert!(msg.contains("invalid encryption key encoding"));
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn parse_device_link_missing_field() {
        let json = r#"{"proxy_url":"x","encryption_key":"AAAA"}"#;
        let result = parse_device_link(json.to_string());
        assert!(result.is_err());
    }

    // ---- cross-compatibility: encrypt then decrypt with different chunk sizes ----

    #[test]
    fn encrypt_decrypt_consistency_across_sizes() {
        let key = test_key();
        let key_arr: [u8; 32] = key.clone().try_into().unwrap();

        // Test various sizes including boundaries
        let sizes = [
            0,
            1,
            100,
            CHUNK_SIZE - 1,
            CHUNK_SIZE,
            CHUNK_SIZE + 1,
            CHUNK_SIZE * 2,
            CHUNK_SIZE * 2 + 500,
            CHUNK_SIZE * 3,
        ];

        for &size in &sizes {
            let plaintext: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
            let ciphertext = encrypt_chunked(&key_arr, &plaintext);
            let decrypted = decrypt_file(ciphertext, key.clone()).unwrap();
            assert_eq!(
                decrypted, plaintext,
                "roundtrip failed for size {size}"
            );
        }
    }

    #[test]
    fn ciphertext_format_has_correct_structure() {
        let key: [u8; 32] = test_key().try_into().unwrap();

        // Empty plaintext: nonce + one auth tag
        let ct_empty = encrypt_chunked(&key, b"");
        assert_eq!(ct_empty.len(), NONCE_SIZE + TAG_SIZE);

        // Single byte: nonce + 1 byte ciphertext + auth tag
        let ct_one = encrypt_chunked(&key, b"x");
        assert_eq!(ct_one.len(), NONCE_SIZE + 1 + TAG_SIZE);

        // Exact chunk: nonce + CHUNK_SIZE bytes ciphertext + auth tag
        let ct_exact = encrypt_chunked(&key, &vec![0u8; CHUNK_SIZE]);
        assert_eq!(ct_exact.len(), NONCE_SIZE + ENCRYPTED_CHUNK_SIZE);

        // Chunk + 1 byte: nonce + full encrypted chunk + 1 byte ciphertext + auth tag
        let ct_plus_one = encrypt_chunked(&key, &vec![0u8; CHUNK_SIZE + 1]);
        assert_eq!(
            ct_plus_one.len(),
            NONCE_SIZE + ENCRYPTED_CHUNK_SIZE + 1 + TAG_SIZE
        );
    }

    #[test]
    fn different_encryptions_produce_different_ciphertext() {
        let key: [u8; 32] = test_key().try_into().unwrap();
        let plaintext = b"same message";
        let ct1 = encrypt_chunked(&key, plaintext);
        let ct2 = encrypt_chunked(&key, plaintext);
        // Different random nonces -> different ciphertext
        assert_ne!(ct1, ct2);
        // Both decrypt to the same plaintext
        assert_eq!(
            decrypt_file(ct1, key.to_vec()).unwrap(),
            decrypt_file(ct2, key.to_vec()).unwrap(),
        );
    }
}
