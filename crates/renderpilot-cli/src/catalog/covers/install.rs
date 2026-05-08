//! Crash-conscious cover installation.
//!
//! The file is written and durably renamed before catalog metadata is updated.
//! SQLite and the filesystem cannot participate in one shared atomic transaction,
//! so this module favors a state where the database never points to a file that
//! was not fully written.
//!
//! Failure behavior:
//! - validation failure: no filesystem or database mutation;
//! - temporary-file creation/write/sync failure: temporary file is removed best-effort;
//! - rename failure: temporary file is removed best-effort;
//! - database upsert failure after successful rename: newly installed file is removed best-effort;
//! - cleanup of old/orphan files is best-effort and does not make a successful install fail.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use renderpilot_domain::GameId;
use renderpilot_storage_sqlite::SqliteStorage;

use super::fs_ops::gc_orphan_cover_files;
use super::paths::covers_directory;
use super::validation::validate_cover_bytes;
use super::CoverMutationOutput;
use crate::error::CliError;

const MAX_SAFE_ID_FRAGMENT_LEN: usize = 80;

pub(super) fn install_cover(
    sqlite: &SqliteStorage,
    catalog_db_path: &Path,
    game_id: &GameId,
    bytes: &[u8],
) -> Result<CoverMutationOutput, CliError> {
    let format = validate_cover_bytes(bytes)?;
    let paths = CoverInstallPaths::new(catalog_db_path, game_id, format.extension())?;

    install_cover_file(bytes, &paths)?;

    if let Err(error) = sqlite.upsert_game_cover(game_id, &paths.file_name) {
        remove_file_and_sync_parent_best_effort(&paths.final_path);
        return Err(CliError::from(error));
    }

    let record = sqlite
        .find_game_cover(game_id)?
        .ok_or_else(|| CliError::CommandFailed("cover row missing after upsert".into()))?;

    // Old files are removed via GC instead of direct deletion. This keeps cleanup
    // safe even if the storage model later allows cover files to be shared.
    gc_orphan_cover_files_best_effort(catalog_db_path, sqlite);

    Ok(CoverMutationOutput {
        file_name: record.file_name,
        updated_at_ms: record.updated_at_ms,
    })
}

struct CoverInstallPaths {
    file_name: String,
    temp_path: PathBuf,
    final_path: PathBuf,
}

impl CoverInstallPaths {
    fn new(catalog_db_path: &Path, game_id: &GameId, extension: &str) -> Result<Self, CliError> {
        let covers_dir = covers_directory(catalog_db_path);

        fs::create_dir_all(&covers_dir).map_err(|error| {
            cover_io_error("could not create covers directory", &covers_dir, error)
        })?;

        let safe_game_id = safe_id_fragment(game_id.as_str());
        let ulid = ulid::Ulid::new().to_string();

        let file_name = format!("cover-{safe_game_id}-{ulid}.{extension}");
        let temp_path = covers_dir.join(format!(".tmp-cover-{ulid}.part"));
        let final_path = covers_dir.join(&file_name);

        Ok(Self {
            file_name,
            temp_path,
            final_path,
        })
    }
}

fn install_cover_file(bytes: &[u8], paths: &CoverInstallPaths) -> Result<(), CliError> {
    let mut temp_cleanup = RemoveFileOnDrop::new(paths.temp_path.clone());

    write_temp_file_durably(&paths.temp_path, bytes)?;

    fs::rename(&paths.temp_path, &paths.final_path).map_err(|error| {
        cover_io_error("could not finalize cover file", &paths.final_path, error)
    })?;

    temp_cleanup.disarm();

    sync_parent_directory_best_effort(&paths.final_path);

    Ok(())
}

fn write_temp_file_durably(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| cover_io_error("could not create temporary cover file", path, error))?;

    file.write_all(bytes)
        .map_err(|error| cover_io_error("could not write temporary cover file", path, error))?;

    file.sync_all()
        .map_err(|error| cover_io_error("could not sync temporary cover file", path, error))?;

    Ok(())
}

fn safe_id_fragment(game_id: &str) -> String {
    let mut out = String::with_capacity(game_id.len().min(MAX_SAFE_ID_FRAGMENT_LEN));
    let mut previous_was_dash = false;

    for ch in game_id.chars() {
        let safe_ch = match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => Some(ch),
            _ => None,
        };

        match safe_ch {
            Some(ch) => {
                out.push(ch);
                previous_was_dash = false;
            }
            None if !previous_was_dash => {
                out.push('-');
                previous_was_dash = true;
            }
            None => {}
        }

        if out.len() >= MAX_SAFE_ID_FRAGMENT_LEN {
            break;
        }
    }

    let trimmed = out.trim_matches('-');

    if trimmed.is_empty() {
        "game".to_string()
    } else {
        trimmed.to_string()
    }
}

fn gc_orphan_cover_files_best_effort(catalog_db_path: &Path, sqlite: &SqliteStorage) {
    let _ = gc_orphan_cover_files(catalog_db_path, sqlite);
}

fn remove_file_and_sync_parent_best_effort(path: &Path) {
    remove_file_best_effort(path);
    sync_parent_directory_best_effort(path);
}

fn remove_file_best_effort(path: &Path) {
    let _ = fs::remove_file(path);
}

fn sync_parent_directory_best_effort(path: &Path) {
    if let Some(parent) = path.parent() {
        sync_directory_best_effort(parent);
    }
}

#[cfg(not(windows))]
fn sync_directory_best_effort(path: &Path) {
    use std::fs::File;

    if let Ok(dir) = File::open(path) {
        let _ = dir.sync_all();
    }
}

#[cfg(windows)]
fn sync_directory_best_effort(path: &Path) {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    // FILE_FLAG_BACKUP_SEMANTICS — required to open directories on Windows for `sync_all`.
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    if let Ok(dir) = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
    {
        let _ = dir.sync_all();
    }
}

fn cover_io_error(action: &str, path: &Path, error: std::io::Error) -> CliError {
    CliError::CoverIo(format!("{action} '{}': {error}", path.display()))
}

struct RemoveFileOnDrop {
    path: PathBuf,
    armed: bool,
}

impl RemoveFileOnDrop {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for RemoveFileOnDrop {
    fn drop(&mut self) {
        if self.armed {
            remove_file_and_sync_parent_best_effort(&self.path);
        }
    }
}
