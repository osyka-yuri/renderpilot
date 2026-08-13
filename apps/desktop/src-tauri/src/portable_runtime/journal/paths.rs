use std::path::{Path, PathBuf};

use crate::portable_runtime::error::{PortableRuntimeError, Result};

pub(in crate::portable_runtime) fn journal_path(
    update_root: &Path,
    transaction_id: &str,
) -> PathBuf {
    update_root
        .join("transactions")
        .join(transaction_id)
        .join("journal.json")
}

pub(in crate::portable_runtime::journal) fn journal_identity(
    path: &Path,
) -> Result<(String, String)> {
    if path.file_name().and_then(|value| value.to_str()) != Some("journal.json") {
        return Err(PortableRuntimeError::new(
            "portable_journal_path",
            "journal path was not the canonical transaction journal leaf",
        ));
    }
    let transaction = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_journal_path",
                "journal had no canonical transaction id",
            )
        })?;
    if !is_digest(transaction) {
        return Err(PortableRuntimeError::new(
            "portable_journal_path",
            "journal transaction id was not a canonical random nonce",
        ));
    }
    Ok((transaction.to_owned(), format!("journal:{transaction}")))
}

pub(in crate::portable_runtime::journal) fn journal_update_root(path: &Path) -> Result<PathBuf> {
    path.parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_owned)
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_journal_path",
                "journal had no canonical update-root ancestry",
            )
        })
}

pub(in crate::portable_runtime::journal) fn is_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
