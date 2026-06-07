//! Paths and size limits for cover files stored next to the catalog database.

use std::path::{Path, PathBuf};

pub(crate) const COVERS_DIR_NAME: &str = "covers";

const MIB: u64 = 1024 * 1024;

/// Maximum allowed cover file size: 10 MiB.
pub const MAX_COVER_BYTES: u64 = 10 * MIB;

pub(crate) fn covers_directory(catalog_db_path: &Path) -> PathBuf {
    catalog_directory(catalog_db_path).join(COVERS_DIR_NAME)
}

fn catalog_directory(catalog_db_path: &Path) -> &Path {
    catalog_db_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_covers_next_to_relative_database_path() {
        assert_eq!(
            covers_directory(Path::new("catalog.db")),
            PathBuf::from(".").join("covers")
        );
    }

    #[test]
    fn returns_covers_next_to_database_in_directory() {
        assert_eq!(
            covers_directory(Path::new("data/catalog.db")),
            PathBuf::from("data/covers")
        );
    }

    #[test]
    fn returns_covers_next_to_absolute_database_path() {
        assert_eq!(
            covers_directory(Path::new("/tmp/catalog.db")),
            PathBuf::from("/tmp/covers")
        );
    }

    #[test]
    fn max_cover_size_is_10_mib() {
        assert_eq!(MAX_COVER_BYTES, 10_485_760);
    }
}
