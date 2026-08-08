//! Content-addressed local storage for catalog v1.

use std::fs;
use std::path::{Path, PathBuf};

use crate::ServiceError;

use super::library_error;

const LIBRARIES_DIR_NAME: &str = "libraries";
const V1_DIR_NAME: &str = "v1";
const CONTENT_MARKER: &str = ".content-layout-ready";
// Compatibility boundary for the pre-catalog-v1 cache file name.
const LEGACY_LIBRARY_CACHE_FILE: &str = "libraries_manifest.json";
const LOCAL_DLSS_FILES: &[&str] = &[
    "dlss_presets.json",
    "dlss_g_presets.json",
    "dlss_d_presets.json",
    "dlss_settings.json",
];
// These are the only directories created by the pre-catalog-v1 library cache.
// Do not sweep arbitrary entries from `libraries`: users may keep unrelated
// files there and content-addressed blobs must remain reusable.
const LEGACY_LIBRARY_GROUP_DIRS: &[&str] = &[
    "dlss",
    "dlss_g",
    "dlss_d",
    "fsr_31_dx12",
    "fsr_31_vk",
    "fsr_loader_dx12",
    "fsr_upscaler_dx12",
    "fsr_framegeneration_dx12",
    "fsr_denoiser_dx12",
    "fsr_radiancecache_dx12",
    "xell",
    "xess",
    "xess_dx11",
    "xess_fg",
    "other",
];

/// Owns every catalog-v1 and legacy path below the application library root.
#[derive(Debug, Clone)]
pub(super) struct LibraryStorage {
    root: PathBuf,
}

impl LibraryStorage {
    pub(super) fn discover() -> Result<Self, ServiceError> {
        Ok(Self {
            root: crate::portable::runtime_paths().map_or_else(
                || crate::app_dir::app_dir().map(|path| path.join(LIBRARIES_DIR_NAME)),
                |paths| Ok(paths.libraries_root.clone()),
            )?,
        })
    }

    #[cfg(test)]
    pub(super) fn from_root(root: PathBuf) -> Self {
        Self { root }
    }

    fn v1_dir(&self) -> PathBuf {
        self.root.join(V1_DIR_NAME)
    }

    pub(super) fn catalog_cache_path(&self) -> PathBuf {
        self.v1_dir().join("catalog.json")
    }

    pub(super) fn local_dlss_document_path(
        &self,
        file_name: &str,
    ) -> Result<PathBuf, ServiceError> {
        if !LOCAL_DLSS_FILES.contains(&file_name) {
            return Err(library_error(format!(
                "unsupported local DLSS document file name: `{file_name}`"
            )));
        }
        Ok(self.root.join(file_name))
    }

    pub(super) fn local_archive_path(&self, transport_sha256: &str) -> PathBuf {
        self.v1_dir()
            .join("blobs")
            .join(format!("{transport_sha256}.dll.zst"))
    }

    pub(super) fn local_dll_path(&self, dll_sha256: &str, file_name: &str) -> PathBuf {
        self.v1_dir()
            .join("artifacts")
            .join(dll_sha256)
            .join(file_name)
    }

    pub(super) fn ensure_content_layout_v1(&self) -> Result<(), ServiceError> {
        let marker = self.v1_dir().join(CONTENT_MARKER);
        if marker.is_file() {
            return Ok(());
        }

        if self.root.is_dir() {
            remove_known_legacy_entries(&self.root)?;
        }

        let app_root = self.root.parent().ok_or_else(|| {
            library_error("library storage root has no application directory parent")
        })?;
        crate::fs::remove_file_if_exists(&app_root.join(LEGACY_LIBRARY_CACHE_FILE))?;
        crate::fs::write_file_atomically(&marker, b"catalog-v1\n")
    }
}

/// Returns the path for a known locally stored DLSS support document.
pub fn local_dlss_document_path(file_name: &str) -> Result<PathBuf, ServiceError> {
    LibraryStorage::discover()?.local_dlss_document_path(file_name)
}

fn remove_known_legacy_entries(root: &Path) -> Result<(), ServiceError> {
    for legacy_name in LEGACY_LIBRARY_GROUP_DIRS {
        let path = root.join(legacy_name);
        if fs::symlink_metadata(&path).is_ok() {
            remove_legacy_entry(root, &path)?;
        }
    }
    Ok(())
}

fn remove_legacy_entry(root: &Path, path: &Path) -> Result<(), ServiceError> {
    if path.parent() != Some(root) {
        return Err(library_error(
            "refusing to remove cache entry outside library root",
        ));
    }
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(library_error(format!(
                "failed to inspect legacy cache entry `{}`: {error}",
                path.display()
            )));
        }
    };
    let result = if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(library_error(format!(
            "failed to remove legacy cache entry `{}`: {error}",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{LibraryStorage, local_dlss_document_path, remove_known_legacy_entries};

    #[test]
    fn preset_path_rejects_unknown_and_non_basename_values() {
        assert!(local_dlss_document_path("unknown.json").is_err());
        assert!(local_dlss_document_path("../dlss_presets.json").is_err());
        assert!(local_dlss_document_path(r"nested\dlss_presets.json").is_err());
    }

    #[test]
    fn legacy_cleanup_removes_only_known_cache_directories() {
        let root = std::env::temp_dir().join(format!(
            "renderpilot-library-cleanup-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("dlss")).expect("known legacy directory");
        fs::write(root.join("dlss").join("cached.dll"), b"legacy").expect("legacy file");
        fs::create_dir_all(root.join("user-assets")).expect("unmanaged directory");
        fs::write(root.join("user-assets").join("keep.dll"), b"user").expect("unmanaged file");
        fs::write(root.join("notes.txt"), b"keep").expect("unmanaged root file");

        remove_known_legacy_entries(&root).expect("cleanup");

        assert!(!root.join("dlss").exists());
        assert!(root.join("user-assets").join("keep.dll").is_file());
        assert!(root.join("notes.txt").is_file());
        fs::remove_dir_all(root).expect("temp cleanup");
    }

    #[test]
    fn storage_paths_are_confined_to_the_configured_root() {
        let root = std::env::temp_dir().join("renderpilot-library-storage-paths");
        let storage = LibraryStorage::from_root(root.clone());

        assert_eq!(storage.catalog_cache_path(), root.join("v1/catalog.json"));
        assert_eq!(
            storage.local_archive_path("abc"),
            root.join("v1/blobs/abc.dll.zst")
        );
        assert_eq!(
            storage.local_dll_path("def", "runtime.dll"),
            root.join("v1/artifacts/def/runtime.dll")
        );
    }
}
