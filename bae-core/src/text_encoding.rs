//! Text encoding detection and decoding.
//!
//! Handles BOM detection (UTF-8, UTF-16 LE/BE), validates UTF-8,
//! and falls back to chardetng for legacy encodings (Windows-1252, Shift-JIS, etc.).

use std::path::Path;

/// Decode raw bytes to a String, detecting encoding automatically.
///
/// 1. BOM: UTF-8 (EF BB BF), UTF-16 LE (FF FE), UTF-16 BE (FE FF)
/// 2. No BOM: try UTF-8
/// 3. Not valid UTF-8: chardetng detection, decode via encoding_rs
pub fn decode_text(bytes: &[u8]) -> String {
    // UTF-8 BOM
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(&bytes[3..]).into_owned();
    }

    // UTF-16 LE BOM
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (decoded, _, _) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
        return decoded.into_owned();
    }

    // UTF-16 BE BOM
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (decoded, _, _) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
        return decoded.into_owned();
    }

    // Try UTF-8
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }

    // Fallback: detect legacy encoding
    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let (decoded, _, _) = encoding.decode(bytes);
    decoded.into_owned()
}

/// Read a file and decode its contents, detecting encoding automatically.
pub fn read_text_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(decode_text(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_no_bom() {
        let input = "Hello, world!".as_bytes();
        assert_eq!(decode_text(input), "Hello, world!");
    }

    #[test]
    fn utf8_with_bom() {
        let mut input = vec![0xEF, 0xBB, 0xBF];
        input.extend_from_slice("Hello, world!".as_bytes());
        assert_eq!(decode_text(&input), "Hello, world!");
    }

    #[test]
    fn utf16_le_with_bom() {
        let text = "Hello";
        let mut input = vec![0xFF, 0xFE]; // BOM
        for ch in text.encode_utf16() {
            input.extend_from_slice(&ch.to_le_bytes());
        }
        assert_eq!(decode_text(&input), "Hello");
    }

    #[test]
    fn utf16_be_with_bom() {
        let text = "Hello";
        let mut input = vec![0xFE, 0xFF]; // BOM
        for ch in text.encode_utf16() {
            input.extend_from_slice(&ch.to_be_bytes());
        }
        assert_eq!(decode_text(&input), "Hello");
    }

    #[test]
    fn windows_1252_fallback() {
        // "caf\xe9" = "cafe" with e-acute in Windows-1252
        let input = b"caf\xe9";
        let result = decode_text(input);
        assert!(result.contains("caf"), "should decode the ASCII portion");
        // chardetng should detect this as a Latin encoding and decode the e-acute
        assert!(
            result.contains('\u{00e9}') || result.len() == 4,
            "should decode e-acute"
        );
    }

    #[test]
    fn shift_jis_fallback() {
        // Encode a known Shift-JIS string: "テスト" (test)
        let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode("テスト");
        let result = decode_text(&encoded);
        assert_eq!(result, "テスト");
    }
}
