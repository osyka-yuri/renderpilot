//! Raster cover magic-byte validation and MIME mapping.

use crate::ServiceError;

use super::paths::MAX_COVER_BYTES;

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1A\n";
const JPEG_MAGIC: &[u8] = b"\xFF\xD8\xFF";
const GIF87A_MAGIC: &[u8] = b"GIF87a";
const GIF89A_MAGIC: &[u8] = b"GIF89a";
const RIFF_MAGIC: &[u8] = b"RIFF";
const WEBP_FORM_TYPE: &[u8] = b"WEBP";

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoverFormat {
    Png,
    Jpeg,
    Webp,
    Gif,
}

impl CoverFormat {
    pub(crate) const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Gif => "gif",
        }
    }

    pub(crate) const fn mime(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Webp => "image/webp",
            Self::Gif => "image/gif",
        }
    }
}

pub(crate) fn validate_cover_bytes(bytes: &[u8]) -> Result<CoverFormat, ServiceError> {
    validate_cover_size(bytes)?;

    if looks_like_markup(bytes) {
        return Err(ServiceError::UnsupportedCoverImageType);
    }

    detect_cover_format(bytes).ok_or(ServiceError::UnsupportedCoverImageType)
}

pub(crate) fn mime_for_bytes(bytes: &[u8]) -> &'static str {
    validate_cover_bytes(bytes)
        .map(CoverFormat::mime)
        .unwrap_or("application/octet-stream")
}

fn validate_cover_size(bytes: &[u8]) -> Result<(), ServiceError> {
    let len = bytes.len() as u64;

    if len > MAX_COVER_BYTES {
        return Err(ServiceError::CoverDownloadFailed(
            "cover exceeds maximum size".into(),
        ));
    }

    if bytes.is_empty() {
        return Err(ServiceError::UnsupportedCoverImageType);
    }

    Ok(())
}

fn detect_cover_format(bytes: &[u8]) -> Option<CoverFormat> {
    if bytes.starts_with(PNG_MAGIC) {
        return Some(CoverFormat::Png);
    }

    if bytes.starts_with(JPEG_MAGIC) {
        return Some(CoverFormat::Jpeg);
    }

    if bytes.starts_with(GIF87A_MAGIC) || bytes.starts_with(GIF89A_MAGIC) {
        return Some(CoverFormat::Gif);
    }

    if is_webp(bytes) {
        return Some(CoverFormat::Webp);
    }

    None
}

fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && bytes.starts_with(RIFF_MAGIC) && bytes.get(8..12) == Some(WEBP_FORM_TYPE)
}

fn looks_like_markup(bytes: &[u8]) -> bool {
    trim_ascii_start(strip_utf8_bom(bytes)).first() == Some(&b'<')
}

fn strip_utf8_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes)
}

fn trim_ascii_start(mut bytes: &[u8]) -> &[u8] {
    while let Some((&first, rest)) = bytes.split_first() {
        if first.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }

    bytes
}

#[cfg(test)]
mod tests {
    use super::{mime_for_bytes, validate_cover_bytes, CoverFormat};

    #[test]
    fn validate_accepts_png_magic() {
        let mut bytes = Vec::from(b"\x89PNG\r\n\x1A\n" as &[u8]);
        bytes.extend_from_slice(&[0; 8]);

        assert_eq!(validate_cover_bytes(&bytes).expect("png"), CoverFormat::Png);
    }

    #[test]
    fn validate_accepts_jpeg_magic() {
        assert_eq!(
            validate_cover_bytes(b"\xFF\xD8\xFF\xE0").expect("jpeg"),
            CoverFormat::Jpeg
        );
    }

    #[test]
    fn validate_accepts_gif87a_magic() {
        assert_eq!(
            validate_cover_bytes(b"GIF87a").expect("gif"),
            CoverFormat::Gif
        );
    }

    #[test]
    fn validate_accepts_gif89a_magic() {
        assert_eq!(
            validate_cover_bytes(b"GIF89a").expect("gif"),
            CoverFormat::Gif
        );
    }

    #[test]
    fn validate_accepts_webp_magic() {
        let bytes = b"RIFF\x00\x00\x00\x00WEBPVP8 ";
        assert_eq!(
            validate_cover_bytes(bytes).expect("webp"),
            CoverFormat::Webp
        );
    }

    #[test]
    fn validate_rejects_empty_bytes() {
        assert!(validate_cover_bytes(b"").is_err());
    }

    #[test]
    fn validate_rejects_leading_angle_bracket() {
        assert!(validate_cover_bytes(b"<svg ").is_err());
    }

    #[test]
    fn validate_rejects_markup_after_ascii_whitespace() {
        assert!(validate_cover_bytes(b" \n\t<html").is_err());
    }

    #[test]
    fn validate_rejects_markup_after_utf8_bom() {
        assert!(validate_cover_bytes(b"\xEF\xBB\xBF<svg").is_err());
    }

    #[test]
    fn validate_rejects_png_magic_after_whitespace() {
        assert!(validate_cover_bytes(b" \x89PNG\r\n\x1A\n").is_err());
    }

    #[test]
    fn mime_returns_detected_image_mime() {
        assert_eq!(mime_for_bytes(b"GIF89a"), "image/gif");
    }

    #[test]
    fn mime_falls_back_for_unknown_bytes() {
        assert_eq!(mime_for_bytes(b"not an image"), "application/octet-stream");
    }
}
