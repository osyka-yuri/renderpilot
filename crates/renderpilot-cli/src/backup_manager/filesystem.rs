use std::{
    env,
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_application::{AppError, AppResult, OperationRecord, UnixTimestampMillis};
use renderpilot_domain::{PathRef, Sha256Hash};
use sha2::{Digest, Sha256};

pub(crate) const BACKUP_ROOT_DIR_ENV: &str = "RENDERPILOT_BACKUP_ROOT";

const HASH_BUFFER_SIZE: usize = 64 * 1024;

pub(super) fn backup_operation_root(operation: &OperationRecord) -> PathBuf {
    backup_root_dir()
        .join(sanitize_path_segment(operation.game_id.as_str()))
        .join(sanitize_path_segment(operation.id.as_str()))
}

pub(super) fn copy_backup_file_with_verification<F>(
    source_path: &Path,
    backup_path: &Path,
    post_copy: &F,
) -> AppResult<Sha256Hash>
where
    F: Fn(&Path) -> std::io::Result<()>,
{
    copy_file_with_verification(
        source_path,
        backup_path,
        post_copy,
        "backup sha256 mismatch",
    )
}

pub(super) fn copy_file_with_verification<F>(
    source_path: &Path,
    destination_path: &Path,
    post_copy: &F,
    mismatch_message: &str,
) -> AppResult<Sha256Hash>
where
    F: Fn(&Path) -> std::io::Result<()>,
{
    let original_sha256 = sha256_file(source_path)?;

    fs::copy(source_path, destination_path).map_err(|error| {
        file_system_error(
            format!(
                "failed to copy {} to {}",
                source_path.display(),
                destination_path.display()
            ),
            error,
        )
    })?;

    post_copy(destination_path).map_err(|error| {
        file_system_error(
            format!(
                "failed to finalize copied file {}",
                destination_path.display()
            ),
            error,
        )
    })?;

    let destination_sha256 = sha256_file(destination_path)?;

    if destination_sha256 != original_sha256 {
        return Err(AppError::provider_failed(format!(
            "{mismatch_message} for {}",
            source_path.display()
        )));
    }

    Ok(original_sha256)
}

pub(super) fn current_timestamp_millis() -> AppResult<UnixTimestampMillis> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AppError::storage_failed("system clock is before Unix epoch"))?;
    let millis = i64::try_from(duration.as_millis())
        .map_err(|_| AppError::storage_failed("system clock is too large to persist"))?;

    UnixTimestampMillis::new(millis)
}

pub(super) fn sanitize_path_segment(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());

    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }

    if sanitized.is_empty() {
        return "item".to_owned();
    }

    sanitized
}

pub(super) fn path_ref_from_path(path: &Path) -> AppResult<PathRef> {
    PathRef::new(path.to_string_lossy().into_owned())
        .map_err(|error| AppError::provider_failed(error.to_string()))
}

pub(super) fn file_system_error(message: impl Into<String>, error: std::io::Error) -> AppError {
    AppError::provider_failed(format!("{}: {error}", message.into()))
}

pub(super) fn ensure_file_exists(path: &Path, description: &str) -> AppResult<()> {
    fs::metadata(path).map_err(|error| {
        file_system_error(
            format!("{description} is not available: {}", path.display()),
            error,
        )
    })?;

    Ok(())
}

pub(super) fn ensure_file_is_writable(path: &Path, description: &str) -> AppResult<()> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| {
            file_system_error(
                format!(
                    "{description} is locked or not writable: {}",
                    path.display()
                ),
                error,
            )
        })?;

    Ok(())
}

fn backup_root_dir() -> PathBuf {
    if let Some(override_path) = env::var_os(BACKUP_ROOT_DIR_ENV) {
        return PathBuf::from(override_path);
    }

    if let Some(appdata) = env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("RenderPilot").join("backups");
    }

    PathBuf::from("RenderPilot").join("backups")
}

pub(super) fn sha256_file(path: &Path) -> AppResult<Sha256Hash> {
    let mut file = File::open(path).map_err(|error| {
        file_system_error(
            format!("could not open {} for hashing", path.display()),
            error,
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; HASH_BUFFER_SIZE];

    loop {
        let bytes_read = file.read(&mut buffer).map_err(|error| {
            file_system_error(format!("could not hash {}", path.display()), error)
        })?;

        if bytes_read == 0 {
            break;
        }

        hasher.update(&buffer[..bytes_read]);
    }

    Sha256Hash::new(hex_lower(&hasher.finalize()))
        .map_err(|error| AppError::provider_failed(error.to_string()))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        use std::fmt::Write as _;

        let _ = write!(hex, "{byte:02x}");
    }

    hex
}

pub(super) fn ensure_file_matches_sha256(
    path: &Path,
    expected_sha256: &Sha256Hash,
    mismatch_message: &str,
) -> AppResult<()> {
    let actual_sha256 = sha256_file(path)?;

    if &actual_sha256 != expected_sha256 {
        return Err(AppError::provider_failed(format!(
            "{mismatch_message} for {}",
            path.display()
        )));
    }

    Ok(())
}
