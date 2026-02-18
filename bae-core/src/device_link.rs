use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use qrcode::render::svg;
use qrcode::QrCode;
use serde::{Deserialize, Serialize};

/// Contains the library encryption key and full Ed25519 secret key.
/// Treat as highly sensitive -- do not log or expose in debug output.
#[derive(Serialize, Deserialize)]
struct DeviceLinkPayload {
    proxy_url: String,
    encryption_key: String,
    signing_key: String,
    library_id: String,
}

/// Generate a QR code SVG string containing the device link payload.
///
/// The QR code encodes a JSON object with base64url-encoded keys so a mobile
/// device can scan it and bootstrap into the same library.
pub fn generate_qr_svg(
    proxy_url: &str,
    encryption_key: &[u8],
    signing_key: &[u8],
    library_id: &str,
) -> Result<String, String> {
    let payload = DeviceLinkPayload {
        proxy_url: proxy_url.to_string(),
        encryption_key: URL_SAFE_NO_PAD.encode(encryption_key),
        signing_key: URL_SAFE_NO_PAD.encode(signing_key),
        library_id: library_id.to_string(),
    };

    let json = serde_json::to_string(&payload).map_err(|e| format!("JSON serialization: {e}"))?;

    let code = QrCode::new(json.as_bytes()).map_err(|e| format!("QR generation: {e}"))?;

    let svg = code
        .render::<svg::Color>()
        .min_dimensions(256, 256)
        .quiet_zone(true)
        .build();

    Ok(svg)
}

/// Decode a device link payload from a JSON string.
///
/// Returns (proxy_url, encryption_key_bytes, signing_key_bytes, library_id).
pub fn decode(json: &str) -> Result<(String, Vec<u8>, Vec<u8>, String), String> {
    let payload: DeviceLinkPayload =
        serde_json::from_str(json).map_err(|e| format!("Invalid device link JSON: {e}"))?;

    let encryption_key = URL_SAFE_NO_PAD
        .decode(&payload.encryption_key)
        .map_err(|e| format!("Invalid encryption key encoding: {e}"))?;

    let signing_key = URL_SAFE_NO_PAD
        .decode(&payload.signing_key)
        .map_err(|e| format!("Invalid signing key encoding: {e}"))?;

    if encryption_key.len() != 32 {
        return Err(format!(
            "Encryption key must be 32 bytes, got {}",
            encryption_key.len()
        ));
    }

    if signing_key.len() != 64 {
        return Err(format!(
            "Signing key must be 64 bytes, got {}",
            signing_key.len()
        ));
    }

    Ok((
        payload.proxy_url,
        encryption_key,
        signing_key,
        payload.library_id,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encode_decode() {
        let encryption_key = [0xAB_u8; 32];
        let signing_key = [0xCD_u8; 64];
        let proxy_url = "https://alice.bae.fm";
        let library_id = "lib-abc-123";

        let svg = generate_qr_svg(proxy_url, &encryption_key, &signing_key, library_id).unwrap();

        // SVG should contain valid markup (starts with XML declaration then <svg>)
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<rect"));

        // Verify decode works on the same payload JSON
        let payload = DeviceLinkPayload {
            proxy_url: proxy_url.to_string(),
            encryption_key: URL_SAFE_NO_PAD.encode(encryption_key),
            signing_key: URL_SAFE_NO_PAD.encode(signing_key),
            library_id: library_id.to_string(),
        };
        let json = serde_json::to_string(&payload).unwrap();

        let (decoded_url, decoded_enc, decoded_sign, decoded_lib) = decode(&json).unwrap();
        assert_eq!(decoded_url, proxy_url);
        assert_eq!(decoded_enc, encryption_key);
        assert_eq!(decoded_sign, signing_key);
        assert_eq!(decoded_lib, library_id);
    }

    #[test]
    fn svg_output_is_valid() {
        let svg =
            generate_qr_svg("https://example.com", &[0x01; 32], &[0x02; 64], "lib-1").unwrap();
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<rect"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn decode_invalid_json() {
        let result = decode("not valid json");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid device link JSON"));
    }

    #[test]
    fn decode_wrong_key_lengths() {
        // 16-byte encryption key (too short)
        let short_enc = URL_SAFE_NO_PAD.encode([0xAA_u8; 16]);
        let valid_sign = URL_SAFE_NO_PAD.encode([0xBB_u8; 64]);
        let json = format!(
            r#"{{"proxy_url":"x","encryption_key":"{short_enc}","signing_key":"{valid_sign}","library_id":"y"}}"#
        );
        let result = decode(&json);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("Encryption key must be 32 bytes"));

        // 32-byte signing key (too short)
        let valid_enc = URL_SAFE_NO_PAD.encode([0xAA_u8; 32]);
        let short_sign = URL_SAFE_NO_PAD.encode([0xBB_u8; 32]);
        let json = format!(
            r#"{{"proxy_url":"x","encryption_key":"{valid_enc}","signing_key":"{short_sign}","library_id":"y"}}"#
        );
        let result = decode(&json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Signing key must be 64 bytes"));
    }

    #[test]
    fn decode_invalid_key_encoding() {
        // Valid JSON but bad base64 in encryption_key
        let json =
            r#"{"proxy_url":"x","encryption_key":"!!!","signing_key":"AAAA","library_id":"y"}"#;
        let result = decode(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid encryption key"));
    }
}
