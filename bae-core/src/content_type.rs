use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Content type for files stored in the library.
///
/// Wraps MIME types as an enum for type-safe comparisons.
/// Stored as MIME type strings in the database.
#[derive(Clone, Debug, PartialEq)]
pub enum ContentType {
    // Audio
    Flac,
    Mpeg,
    Ogg,
    Wav,
    Aac,
    Mp4Audio,
    // Images
    Jpeg,
    Png,
    Gif,
    Webp,
    Bmp,
    Svg,
    // Text
    PlainText,
    // Other
    Pdf,
    OctetStream,
    Other(String),
}

impl ContentType {
    /// MIME type string (e.g., "audio/flac", "image/jpeg").
    pub fn as_str(&self) -> &str {
        match self {
            Self::Flac => "audio/flac",
            Self::Mpeg => "audio/mpeg",
            Self::Ogg => "audio/ogg",
            Self::Wav => "audio/wav",
            Self::Aac => "audio/aac",
            Self::Mp4Audio => "audio/mp4",
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Gif => "image/gif",
            Self::Webp => "image/webp",
            Self::Bmp => "image/bmp",
            Self::Svg => "image/svg+xml",
            Self::PlainText => "text/plain",
            Self::Pdf => "application/pdf",
            Self::OctetStream => "application/octet-stream",
            Self::Other(s) => s,
        }
    }

    /// Parse from a MIME type string (as stored in the database).
    pub fn from_mime(s: &str) -> Self {
        match s {
            "audio/flac" => Self::Flac,
            "audio/mpeg" => Self::Mpeg,
            "audio/ogg" => Self::Ogg,
            "audio/wav" => Self::Wav,
            "audio/aac" => Self::Aac,
            "audio/mp4" => Self::Mp4Audio,
            "image/jpeg" => Self::Jpeg,
            "image/png" => Self::Png,
            "image/gif" => Self::Gif,
            "image/webp" => Self::Webp,
            "image/bmp" => Self::Bmp,
            "image/svg+xml" => Self::Svg,
            "text/plain" => Self::PlainText,
            "application/pdf" => Self::Pdf,
            "application/octet-stream" => Self::OctetStream,
            other => Self::Other(other.to_string()),
        }
    }

    /// Map a file extension to its content type.
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "flac" => Self::Flac,
            "mp3" => Self::Mpeg,
            "ogg" => Self::Ogg,
            "wav" => Self::Wav,
            "aac" => Self::Aac,
            "m4a" => Self::Mp4Audio,
            "jpg" | "jpeg" => Self::Jpeg,
            "png" => Self::Png,
            "gif" => Self::Gif,
            "webp" => Self::Webp,
            "bmp" => Self::Bmp,
            "svg" => Self::Svg,
            "txt" | "cue" | "log" | "nfo" | "m3u" | "m3u8" => Self::PlainText,
            "pdf" => Self::Pdf,
            _ => Self::OctetStream,
        }
    }

    pub fn is_audio(&self) -> bool {
        matches!(
            self,
            Self::Flac | Self::Mpeg | Self::Ogg | Self::Wav | Self::Aac | Self::Mp4Audio
        ) || matches!(self, Self::Other(s) if s.starts_with("audio/"))
    }

    pub fn is_image(&self) -> bool {
        matches!(
            self,
            Self::Jpeg | Self::Png | Self::Gif | Self::Webp | Self::Bmp | Self::Svg
        ) || matches!(self, Self::Other(s) if s.starts_with("image/"))
    }

    /// Short human-readable name for UI display (e.g., "FLAC", "JPEG").
    pub fn display_name(&self) -> &str {
        match self {
            Self::Flac => "FLAC",
            Self::Mpeg => "MP3",
            Self::Ogg => "OGG",
            Self::Wav => "WAV",
            Self::Aac => "AAC",
            Self::Mp4Audio => "M4A",
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::Gif => "GIF",
            Self::Webp => "WebP",
            Self::Bmp => "BMP",
            Self::Svg => "SVG",
            Self::PlainText => "Text",
            Self::Pdf => "PDF",
            Self::OctetStream => "Binary",
            Self::Other(s) => s,
        }
    }
}

impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for ContentType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ContentType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(ContentType::from_mime(&s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_extension_audio() {
        assert_eq!(ContentType::from_extension("flac"), ContentType::Flac);
        assert_eq!(ContentType::from_extension("FLAC"), ContentType::Flac);
        assert_eq!(ContentType::from_extension("mp3"), ContentType::Mpeg);
    }

    #[test]
    fn from_extension_image() {
        assert_eq!(ContentType::from_extension("jpg"), ContentType::Jpeg);
        assert_eq!(ContentType::from_extension("jpeg"), ContentType::Jpeg);
        assert_eq!(ContentType::from_extension("png"), ContentType::Png);
    }

    #[test]
    fn from_extension_text() {
        assert_eq!(ContentType::from_extension("cue"), ContentType::PlainText);
        assert_eq!(ContentType::from_extension("log"), ContentType::PlainText);
        assert_eq!(ContentType::from_extension("nfo"), ContentType::PlainText);
    }

    #[test]
    fn roundtrip() {
        let ct = ContentType::Flac;
        assert_eq!(ContentType::from_mime(ct.as_str()), ct);

        let ct = ContentType::Jpeg;
        assert_eq!(ContentType::from_mime(ct.as_str()), ct);
    }

    #[test]
    fn predicates() {
        assert!(ContentType::Flac.is_audio());
        assert!(!ContentType::Flac.is_image());
        assert!(ContentType::Jpeg.is_image());
        assert!(!ContentType::Jpeg.is_audio());
        assert!(!ContentType::PlainText.is_audio());
        assert!(!ContentType::PlainText.is_image());
    }

    #[test]
    fn display_name() {
        assert_eq!(ContentType::Flac.display_name(), "FLAC");
        assert_eq!(ContentType::Mpeg.display_name(), "MP3");
        assert_eq!(ContentType::Jpeg.display_name(), "JPEG");
    }
}
