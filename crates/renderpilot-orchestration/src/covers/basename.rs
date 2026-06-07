//! Invariants for cover image basenames stored in `game_covers.file_name`.
//!
//! Keep this logic in sync with the SQLite `CHECK` constraint.

const TRAVERSAL_MARKER: &str = "..";

/// Returns `true` if `name` is safe to join under `covers/`.
///
/// A valid cover basename:
/// - is not empty;
/// - does not contain Unix or Windows path separators;
/// - does not contain `..`, matching the SQLite `CHECK` constraint;
/// - does not contain NUL bytes, which are invalid in filesystem paths.
#[must_use]
pub(crate) fn cover_basename_is_safe(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(TRAVERSAL_MARKER)
        && !name.bytes().any(is_path_separator_or_nul)
}

#[must_use]
fn is_path_separator_or_nul(byte: u8) -> bool {
    matches!(byte, b'/' | b'\\' | b'\0')
}

#[cfg(test)]
mod tests {
    use super::cover_basename_is_safe;

    #[test]
    fn accepts_plain_basenames() {
        assert!(cover_basename_is_safe("cover.jpg"));
        assert!(cover_basename_is_safe("game-cover_123.png"));
        assert!(cover_basename_is_safe("Обложка.webp"));
    }

    #[test]
    fn rejects_empty_name() {
        assert!(!cover_basename_is_safe(""));
    }

    #[test]
    fn rejects_unix_path_separator() {
        assert!(!cover_basename_is_safe("nested/cover.jpg"));
        assert!(!cover_basename_is_safe("/cover.jpg"));
        assert!(!cover_basename_is_safe("cover.jpg/"));
    }

    #[test]
    fn rejects_windows_path_separator() {
        assert!(!cover_basename_is_safe(r"nested\cover.jpg"));
        assert!(!cover_basename_is_safe(r"\cover.jpg"));
        assert!(!cover_basename_is_safe(r"cover.jpg\"));
    }

    #[test]
    fn rejects_traversal_marker_anywhere() {
        assert!(!cover_basename_is_safe(".."));
        assert!(!cover_basename_is_safe("../cover.jpg"));
        assert!(!cover_basename_is_safe("cover..jpg"));
        assert!(!cover_basename_is_safe("cover.jpg.."));
    }

    #[test]
    fn rejects_nul_byte() {
        assert!(!cover_basename_is_safe("cover\0.jpg"));
    }
}
