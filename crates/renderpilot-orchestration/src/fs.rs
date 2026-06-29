//! Filesystem primitives shared across orchestration features — crash-durable
//! writes, directory-entry durability, and bare-file-name safety. Provider- and
//! domain-neutral: the library swapper and the add-on installers both build on it.
//!
//! Writing a file with `fs::write` only schedules the data for the OS page cache; a
//! crash before the cache is flushed can leave a torn file. [`write_file_atomically`]
//! makes a write **content-durable** (temp file + `sync_all` + atomic rename).
//! Directory-entry durability (so a freshly created/renamed file survives a crash)
//! is a *separate, explicit* step — [`sync_directory_best_effort`] — that callers
//! invoke once over the dirs they touched, rather than paying a parent-dir fsync on
//! every single write.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::ServiceError;

fn err(message: impl Into<String>) -> ServiceError {
    ServiceError::CommandFailed(message.into())
}

// ---------------------------------------------------------------------------
// Durability
// ---------------------------------------------------------------------------

/// Best-effort flush of the directory entry for `path` (i.e. its parent).
///
/// Renames and freshly created files only become durable once the *directory* is
/// fsynced. Failures are swallowed: the data write has already been made durable by
/// [`write_file_atomically`], and a parent-dir sync failure must not turn an
/// otherwise-successful operation into an error.
pub(crate) fn sync_parent_directory_best_effort(path: &Path) {
    if let Some(parent) = path.parent() {
        sync_directory_best_effort(parent);
    }
}

#[cfg(not(windows))]
pub(crate) fn sync_directory_best_effort(path: &Path) {
    if let Ok(dir) = fs::File::open(path) {
        let _ = dir.sync_all();
    }
}

#[cfg(windows)]
pub(crate) fn sync_directory_best_effort(path: &Path) {
    use std::os::windows::fs::OpenOptionsExt;

    // FILE_FLAG_BACKUP_SEMANTICS — required to open a directory handle on Windows
    // so that `sync_all` (FlushFileBuffers) can be issued against it.
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    if let Ok(dir) = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
    {
        let _ = dir.sync_all();
    }
}

// ---------------------------------------------------------------------------
// Atomic write
// ---------------------------------------------------------------------------

/// Writes `bytes` to `path` content-durably: into a temp file (synced), then an
/// atomic rename over the destination, creating parent directories as needed.
///
/// Does **not** fsync the parent directory — that is a separate, batched
/// [`sync_directory_best_effort`] callers invoke once per group of writes.
pub(crate) fn write_file_atomically(path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
    let parent = path.parent().ok_or_else(|| {
        err(format!(
            "cannot write file `{}` because it has no parent directory",
            path.display()
        ))
    })?;

    fs::create_dir_all(parent).map_err(|error| {
        err(format!(
            "failed to create directory `{}`: {error}",
            parent.display()
        ))
    })?;

    let temp_path = temporary_file_path(path, "tmp");
    write_temp_file(&temp_path, bytes)?;

    replace_with_temp_file(&temp_path, path)
}

/// Copies `source` onto `dest` content-durably and crash-atomically: streams into a
/// temp file **in the destination directory** (synced), then atomically replaces
/// `dest`. Unlike a bare `fs::copy`, a crash can never leave a torn/partial file at
/// `dest` (it is always either the old file or the complete new one). Creates parent
/// directories as needed; a no-op when `source` and `dest` are the same file.
///
/// Does **not** fsync the parent directory — that is a separate, batched
/// [`sync_directory_best_effort`] callers invoke once per group of writes.
pub(crate) fn copy_file_atomically(source: &Path, dest: &Path) -> Result<(), ServiceError> {
    // Copying a file onto itself would destroy it through the temp/replace dance.
    if is_same_file(source, dest) {
        return Ok(());
    }

    let parent = dest.parent().ok_or_else(|| {
        err(format!(
            "cannot copy onto `{}` because it has no parent directory",
            dest.display()
        ))
    })?;

    fs::create_dir_all(parent).map_err(|error| {
        err(format!(
            "failed to create directory `{}`: {error}",
            parent.display()
        ))
    })?;

    // The temp lives in the destination directory so the replace stays a
    // same-volume (atomic) rename rather than a cross-volume copy.
    let temp_path = temporary_file_path(dest, "copy");
    copy_into_temp(source, &temp_path)?;

    replace_with_temp_file(&temp_path, dest)
}

/// Streams `source` into a freshly created (never-clobbered) temp file and flushes
/// it to durable storage, removing the temp on any failure.
fn copy_into_temp(source: &Path, temp_path: &Path) -> Result<(), ServiceError> {
    let mut reader = fs::File::open(source).map_err(|error| {
        err(format!(
            "failed to open source file `{}`: {error}",
            source.display()
        ))
    })?;

    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)
        .map_err(|error| {
            err(format!(
                "failed to create temporary file `{}`: {error}",
                temp_path.display()
            ))
        })?;

    io::copy(&mut reader, &mut temp_file).map_err(|error| {
        let _ = fs::remove_file(temp_path);
        err(format!(
            "failed to copy `{}` into `{}`: {error}",
            source.display(),
            temp_path.display()
        ))
    })?;

    temp_file.sync_all().map_err(|error| {
        let _ = fs::remove_file(temp_path);
        err(format!(
            "failed to flush temporary file `{}`: {error}",
            temp_path.display()
        ))
    })?;

    Ok(())
}

/// Whether two paths resolve to the same existing file. Conservatively `false` when
/// either path cannot be canonicalized (e.g. `dest` does not exist yet — the common
/// install case), so a real copy proceeds.
fn is_same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn write_temp_file(temp_path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
    let mut temp_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)
        .map_err(|error| {
            err(format!(
                "failed to create temporary file `{}`: {error}",
                temp_path.display()
            ))
        })?;

    temp_file.write_all(bytes).map_err(|error| {
        let _ = fs::remove_file(temp_path);
        err(format!(
            "failed to write temporary file `{}`: {error}",
            temp_path.display()
        ))
    })?;

    temp_file.sync_all().map_err(|error| {
        let _ = fs::remove_file(temp_path);
        err(format!(
            "failed to flush temporary file `{}`: {error}",
            temp_path.display()
        ))
    })?;

    Ok(())
}

/// Atomically replaces `destination_path` with the freshly written `temp_path` via a
/// same-directory rename. `std::fs::rename` replaces an existing destination on every
/// supported platform — on Windows it maps to `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`
/// — so this overwrites cleanly. Rename *durability* is the caller's separate
/// [`sync_directory_best_effort`] step, not a per-rename flush.
fn replace_with_temp_file(temp_path: &Path, destination_path: &Path) -> Result<(), ServiceError> {
    fs::rename(temp_path, destination_path).map_err(|error| {
        let _ = fs::remove_file(temp_path);
        err(format!(
            "failed to move temporary file `{}` to `{}`: {error}",
            temp_path.display(),
            destination_path.display()
        ))
    })
}

fn temporary_file_path(path: &Path, marker: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    path.with_file_name(format!(
        "{file_name}.{marker}-{}-{timestamp}",
        std::process::id()
    ))
}

// ---------------------------------------------------------------------------
// File-name safety
// ---------------------------------------------------------------------------

/// Returns `true` if `value` is a safe bare file name: non-empty, no path
/// separators, no parent-directory references, no trailing dots or spaces, and not
/// a Windows reserved device name.
pub(crate) fn is_safe_file_name(value: &str) -> bool {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
        || value.ends_with('.')
        || value.ends_with(' ')
    {
        return false;
    }

    let stem = value.split('.').next().unwrap_or(value);
    !is_windows_reserved_name(stem)
}

/// Sanitizes an arbitrary string into a safe bare path component (used to derive
/// storage directory names from untrusted identifiers).
pub(crate) fn sanitize_path_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => character,
            _ => '_',
        })
        .collect();

    let sanitized = sanitized
        .trim_matches(|c| c == '.' || c == ' ' || c == '_')
        .to_owned();

    if sanitized.is_empty() {
        return "unknown".to_owned();
    }

    let stem = sanitized.split('.').next().unwrap_or_default();
    if is_windows_reserved_name(stem) {
        format!("_{sanitized}")
    } else {
        sanitized
    }
}

fn is_windows_reserved_name(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

// ---------------------------------------------------------------------------
// Read / remove
// ---------------------------------------------------------------------------

/// Reads a file, mapping I/O errors to a [`ServiceError`].
pub(crate) fn read_file(path: &Path) -> Result<Vec<u8>, ServiceError> {
    fs::read(path)
        .map_err(|error| err(format!("failed to read file `{}`: {error}", path.display())))
}

/// Best-effort mtime stamping helper for installed files whose upstream date is
/// meaningful to users. Callers decide whether a failure is fatal.
pub(crate) fn set_file_mtime(path: &Path, modified: SystemTime) -> Result<(), ServiceError> {
    filetime::set_file_mtime(path, filetime::FileTime::from_system_time(modified)).map_err(
        |error| {
            err(format!(
                "failed to set modified time for `{}`: {error}",
                path.display()
            ))
        },
    )
}

/// Parses an HTTP-date header into a [`SystemTime`].
pub(crate) fn parse_http_date(value: &str) -> Result<SystemTime, ServiceError> {
    httpdate::parse_http_date(value)
        .map_err(|error| err(format!("failed to parse HTTP date `{value}`: {error}")))
}

/// Formats a [`SystemTime`] as IMF-fixdate for DTO compatibility with existing UI
/// date parsing.
#[must_use]
pub(crate) fn format_http_date(time: SystemTime) -> String {
    httpdate::fmt_http_date(time)
}

/// Treat file mtimes far in the future as unreliable metadata.
#[must_use]
pub(crate) fn is_reasonable_file_mtime(time: SystemTime) -> bool {
    let future_slop = Duration::from_secs(60 * 60 * 24);
    time <= SystemTime::now()
        .checked_add(future_slop)
        .unwrap_or(SystemTime::now())
}

/// Best-effort mtime stamp for an installed file, from its upstream `Last-Modified`
/// HTTP-date (preferred) or a `fallback` time. A missing/unparseable date with no
/// fallback is a no-op; a stamping failure is logged, never fatal — the file's
/// bytes are already in place, only the displayed date is affected.
pub(crate) fn stamp_mtime_best_effort(
    path: &Path,
    last_modified: Option<&str>,
    fallback: Option<SystemTime>,
) {
    let Some(time) = last_modified
        .and_then(|value| parse_http_date(value).ok())
        .or(fallback)
    else {
        return;
    };
    if let Err(error) = set_file_mtime(path, time) {
        log::warn!("mtime stamp skipped for `{}`: {error}", path.display());
    }
}

/// Deletes a file, treating "not found" as success.
pub(crate) fn remove_file_if_exists(path: &Path) -> Result<(), ServiceError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(err(format!(
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
    fn rejects_unsafe_file_names() {
        assert!(is_safe_file_name("dxgi.dll"));
        assert!(!is_safe_file_name(""));
        assert!(!is_safe_file_name("a/b"));
        assert!(!is_safe_file_name("a\\b"));
        assert!(!is_safe_file_name(".."));
        assert!(!is_safe_file_name("trailing."));
        assert!(!is_safe_file_name("CON"));
        assert!(!is_safe_file_name("nul.txt"));
    }

    #[test]
    fn strip_utf8_bom_removes_leading_bom_only() {
        assert_eq!(strip_utf8_bom(b"\xEF\xBB\xBF{}"), b"{}");
        assert_eq!(strip_utf8_bom(b"{}"), b"{}");
        assert_eq!(strip_utf8_bom(b"\xEF\xBB{}"), b"\xEF\xBB{}");
        assert_eq!(strip_utf8_bom(b"{\xEF\xBB\xBF}"), b"{\xEF\xBB\xBF}");
        assert_eq!(strip_utf8_bom(b""), b"");
    }

    #[test]
    fn atomic_write_replaces_existing_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("payload.bin");
        fs::write(&path, b"old").expect("seed file");

        write_file_atomically(&path, b"new").expect("replace file");

        assert_eq!(fs::read(&path).expect("read replaced file"), b"new");
    }

    #[test]
    fn copy_file_atomically_replaces_existing_dest() {
        let dir = tempfile::tempdir().expect("temp dir");
        let source = dir.path().join("src.bin");
        let dest = dir.path().join("dest.bin");
        fs::write(&source, b"new-content").expect("seed source");
        fs::write(&dest, b"old").expect("seed dest");

        copy_file_atomically(&source, &dest).expect("atomic copy");

        assert_eq!(fs::read(&dest).expect("read dest"), b"new-content");
    }

    #[test]
    fn copy_file_atomically_failure_leaves_dest_unchanged() {
        let dir = tempfile::tempdir().expect("temp dir");
        let dest = dir.path().join("dest.bin");
        fs::write(&dest, b"original").expect("seed dest");
        let missing = dir.path().join("does-not-exist.bin");

        let result = copy_file_atomically(&missing, &dest);

        assert!(result.is_err(), "copying a missing source must fail");
        assert_eq!(
            fs::read(&dest).expect("read dest"),
            b"original",
            "a failed copy must leave the existing destination untouched"
        );
    }

    #[test]
    fn copy_file_atomically_is_a_noop_for_the_same_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("f.bin");
        fs::write(&path, b"data").expect("seed file");

        copy_file_atomically(&path, &path).expect("same-file no-op");

        assert_eq!(fs::read(&path).expect("read file"), b"data");
    }
}
