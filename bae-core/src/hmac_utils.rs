use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Compute HMAC-SHA256 over `data` using `secret`.
pub fn hmac_sign(secret: &[u8; 32], data: &[u8]) -> [u8; 32] {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// Verify an HMAC-SHA256 signature over `data`.
pub fn hmac_verify(secret: &[u8; 32], data: &[u8], signature: &[u8]) -> bool {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(data);
    mac.verify_slice(signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_roundtrip() {
        let secret = [0x42u8; 32];
        let data = b"hello world";
        let sig = hmac_sign(&secret, data);
        assert!(hmac_verify(&secret, data, &sig));
    }

    #[test]
    fn wrong_data_fails() {
        let secret = [0x42u8; 32];
        let sig = hmac_sign(&secret, b"hello");
        assert!(!hmac_verify(&secret, b"world", &sig));
    }

    #[test]
    fn wrong_secret_fails() {
        let secret_a = [0x42u8; 32];
        let secret_b = [0x99u8; 32];
        let sig = hmac_sign(&secret_a, b"data");
        assert!(!hmac_verify(&secret_b, b"data", &sig));
    }

    #[test]
    fn truncated_signature_fails() {
        let secret = [0x42u8; 32];
        let sig = hmac_sign(&secret, b"data");
        assert!(!hmac_verify(&secret, b"data", &sig[..16]));
    }
}
