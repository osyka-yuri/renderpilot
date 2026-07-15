//! Basic file read/remove and byte helpers.

use std::fs;
use std::io;
use std::path::Path;

use crate::ServiceError;

/// Reads a file, mapping I/O errors to a [`ServiceError`].
pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>, ServiceError> {
    fs::read(path).map_err(|error| {
        crate::failed(format!("failed to read file `{}`: {error}", path.display()))
    })
}

/// Deletes a file, treating "not found" as success.
pub(crate) fn remove_file_if_exists(path: &Path) -> Result<(), ServiceError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(crate::failed(format!(
            "failed to delete file `{}`: {error}",
            path.display()
        ))),
    }
}

/// Returns `bytes` without a leading UTF-8 byte-order mark.
///
/// Published JSON documents are sometimes produced by tooling that prepends a BOM
/// which `serde_json` rejects; stripping it at the read boundary keeps parsing
/// independent of how the publisher encoded the file.
pub(crate) fn strip_utf8_bom(bytes: &[u8]) -> &[u8] {
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_utf8_bom_removes_leading_bom_only() {
        assert_eq!(strip_utf8_bom(b"\xEF\xBB\xBF{}"), b"{}");
        assert_eq!(strip_utf8_bom(b"{}"), b"{}");
        assert_eq!(strip_utf8_bom(b"\xEF\xBB{}"), b"\xEF\xBB{}");
        assert_eq!(strip_utf8_bom(b"{\xEF\xBB\xBF}"), b"{\xEF\xBB\xBF}");
        assert_eq!(strip_utf8_bom(b""), b"");
    }
}
