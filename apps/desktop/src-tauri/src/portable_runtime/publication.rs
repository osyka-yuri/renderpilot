use std::{
    io::Write,
    path::{Path, PathBuf},
};

use super::{
    error::{PortableRuntimeError, Result},
    signature::sha256_hex,
    win32::file::{
        NoReplacePublication, create_pending_file, discard_exact_file, publish_no_replace,
    },
};

/// Writes durable bytes outside the authoritative namespace, then publishes
/// them with one no-replace rename. A crash can leave only an app-owned
/// staging file; readers can never observe a partial final record.
pub fn publish_bytes_no_replace(
    destination: &Path,
    pending_root: &Path,
    bytes: &[u8],
) -> Result<NoReplacePublication> {
    let parent = destination.parent().ok_or_else(|| {
        PortableRuntimeError::new("portable_publication", "destination had no parent")
    })?;
    std::fs::create_dir_all(parent)?;
    std::fs::create_dir_all(pending_root)?;
    let pending = pending_path(pending_root, bytes);
    require_absent_pending_file(&pending)?;

    let mut file = create_pending_file(&pending)?;
    if let Err(error) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        return discard_after_failure(&file, error.into());
    }

    match publish_no_replace(&pending, destination) {
        Ok(NoReplacePublication::Published) => Ok(NoReplacePublication::Published),
        Ok(NoReplacePublication::Occupied) => {
            discard_exact_file(&file)?;
            Ok(NoReplacePublication::Occupied)
        }
        Err(error) => discard_after_failure(&file, error),
    }
}

fn discard_after_failure(
    file: &std::fs::File,
    operation_error: PortableRuntimeError,
) -> Result<NoReplacePublication> {
    match discard_exact_file(file) {
        Ok(()) => Err(operation_error),
        Err(cleanup_error) => Err(PortableRuntimeError::new(
            "portable_publication_cleanup",
            format!(
                "publication failed ({operation_error}); exact candidate cleanup also failed ({cleanup_error})"
            ),
        )),
    }
}

fn pending_path(root: &Path, bytes: &[u8]) -> PathBuf {
    root.join(format!("{}.pending", sha256_hex(bytes)))
}

fn require_absent_pending_file(path: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let _ = metadata;
    Err(PortableRuntimeError::new(
        "portable_publication",
        "existing pending publication was retained; no raw-path cleanup is authorized",
    ))
}
