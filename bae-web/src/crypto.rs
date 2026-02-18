use chacha20poly1305::aead::generic_array::GenericArray;
use chacha20poly1305::{aead::Aead, KeyInit, XChaCha20Poly1305};

const NONCE_SIZE: usize = 24;
const TAG_SIZE: usize = 16;
const CHUNK_SIZE: usize = 65536;
const ENCRYPTED_CHUNK_SIZE: usize = CHUNK_SIZE + TAG_SIZE;

/// Decrypt data encrypted with bae-core's chunked XChaCha20-Poly1305 format.
///
/// Format: [24-byte base_nonce][encrypted_chunk_0][encrypted_chunk_1]...
/// Each chunk: up to 65536 bytes plaintext + 16-byte auth tag.
/// Chunk nonce: base_nonce XOR chunk_index (little-endian).
pub fn decrypt(key: &[u8; 32], ciphertext: &[u8]) -> Result<Vec<u8>, String> {
    if ciphertext.len() < NONCE_SIZE {
        return Err("ciphertext too short for nonce".to_string());
    }

    let base_nonce: [u8; NONCE_SIZE] = ciphertext[..NONCE_SIZE]
        .try_into()
        .map_err(|_| "invalid nonce".to_string())?;

    let cipher = XChaCha20Poly1305::new(GenericArray::from_slice(key));

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

        let decrypted = cipher
            .decrypt(nonce_arr, chunk_data)
            .map_err(|_| format!("decryption failed at chunk {i}"))?;

        plaintext.extend(decrypted);
    }

    Ok(plaintext)
}

/// Derive chunk nonce: base_nonce XOR chunk_index (little-endian).
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

    /// Encrypt using the same chunked format as bae-core.
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

    #[test]
    fn decrypt_small() {
        let key = [0x42u8; 32];
        let plaintext = b"hello world";
        let ciphertext = encrypt_chunked(&key, plaintext);
        let decrypted = decrypt(&key, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_empty() {
        let key = [0x42u8; 32];
        let ciphertext = encrypt_chunked(&key, b"");
        let decrypted = decrypt(&key, &ciphertext).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn decrypt_multi_chunk() {
        let key = [0x42u8; 32];
        // 2.5 chunks
        let plaintext: Vec<u8> = (0..CHUNK_SIZE * 2 + CHUNK_SIZE / 2)
            .map(|i| (i % 256) as u8)
            .collect();
        let ciphertext = encrypt_chunked(&key, &plaintext);
        let decrypted = decrypt(&key, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_exact_chunk() {
        let key = [0x42u8; 32];
        let plaintext = vec![0xAA; CHUNK_SIZE];
        let ciphertext = encrypt_chunked(&key, &plaintext);
        let decrypted = decrypt(&key, &ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn decrypt_wrong_key_fails() {
        let key = [0x42u8; 32];
        let wrong_key = [0x43u8; 32];
        let ciphertext = encrypt_chunked(&key, b"secret");
        assert!(decrypt(&wrong_key, &ciphertext).is_err());
    }

    #[test]
    fn decrypt_tampered_fails() {
        let key = [0x42u8; 32];
        let mut ciphertext = encrypt_chunked(&key, b"secret");
        // Flip a byte in the ciphertext (after nonce)
        ciphertext[NONCE_SIZE + 3] ^= 0xFF;
        assert!(decrypt(&key, &ciphertext).is_err());
    }

    #[test]
    fn decrypt_too_short_fails() {
        let key = [0x42u8; 32];
        assert!(decrypt(&key, &[0u8; 10]).is_err());
    }
}
