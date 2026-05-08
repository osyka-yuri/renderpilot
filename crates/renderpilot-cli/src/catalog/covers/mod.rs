//! Local cover images beside `catalog.db`, remote fetch orchestration, and orphan GC.

mod basename;
mod fs_ops;
mod http_client;
mod install;
mod paths;
mod policy;
mod protocol;
mod providers;
mod validation;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use renderpilot_application::GameRepository;
use renderpilot_domain::GameId;
use renderpilot_storage_sqlite::SqliteStorage;
use serde::Serialize;

use self::http_client::http_client;
use self::policy::CoverRemotePolicy;
use super::{open_catalog_storage, storage};
use crate::error::CliError;

/// Settings row key for the SteamGridDB API bearer token.
pub(crate) const STEAMGRIDDB_API_KEY_SETTING: &str = "steamgriddb_api_key";

#[derive(Debug, Serialize)]
pub(crate) struct CoverMutationOutput {
    pub(crate) file_name: String,
    pub(crate) updated_at_ms: i64,
}

pub(crate) use fs_ops::{gc_orphan_cover_files, unlink_cover_file_best_effort};
pub(crate) use paths::MAX_COVER_BYTES;
pub(crate) use protocol::cover_protocol_http_response;

struct CoverCatalog {
    catalog_path: PathBuf,
    sqlite: SqliteStorage,
}

impl CoverCatalog {
    fn open() -> Result<Self, CliError> {
        let (catalog_path, sqlite) = open_catalog_for_covers()?;

        Ok(Self {
            catalog_path,
            sqlite,
        })
    }

    fn require_game_exists(&self, game_id: &GameId) -> Result<(), CliError> {
        if self.sqlite.find_game(game_id)?.is_some() {
            Ok(())
        } else {
            Err(game_not_found(game_id))
        }
    }

    fn install_cover(
        &self,
        game_id: &GameId,
        bytes: &[u8],
    ) -> Result<CoverMutationOutput, CliError> {
        install::install_cover(&self.sqlite, &self.catalog_path, game_id, bytes)
    }

    fn gc_orphans(&self) -> Result<(), CliError> {
        gc_orphan_cover_files(&self.catalog_path, &self.sqlite)
    }
}

pub(crate) fn gc_orphan_cover_files_startup() {
    let _ = CoverCatalog::open().and_then(|catalog| catalog.gc_orphans());
}

fn open_catalog_for_covers() -> Result<(PathBuf, SqliteStorage), CliError> {
    let catalog_path = storage::catalog_database_path()?;
    let sqlite = open_catalog_storage()?;

    Ok((catalog_path, sqlite))
}

pub(crate) fn fetch_game_cover_auto(game_id: GameId) -> Result<CoverMutationOutput, CliError> {
    let catalog = CoverCatalog::open()?;

    let game = catalog
        .sqlite
        .find_game(&game_id)?
        .ok_or_else(|| game_not_found(&game_id))?;

    let client = http_client()?;

    let api_key = catalog
        .sqlite
        .get_setting(STEAMGRIDDB_API_KEY_SETTING)
        .map_err(CliError::from)?;

    let remote_policy = CoverRemotePolicy::load(&catalog.sqlite)?;

    let bytes = providers::resolve_cover_bytes(&client, api_key.as_deref(), &remote_policy, &game)?;

    catalog.install_cover(&game_id, &bytes)
}

pub(crate) fn set_game_cover_from_file(
    game_id: GameId,
    source: PathBuf,
) -> Result<CoverMutationOutput, CliError> {
    let catalog = CoverCatalog::open()?;

    catalog.require_game_exists(&game_id)?;

    let bytes = read_cover_source_file(&source)?;

    catalog.install_cover(&game_id, &bytes)
}

pub(crate) fn clear_game_cover(game_id: GameId) -> Result<(), CliError> {
    let catalog = CoverCatalog::open()?;

    catalog.require_game_exists(&game_id)?;

    let existing = catalog.sqlite.find_game_cover(&game_id)?;

    catalog.sqlite.clear_game_cover_row(&game_id)?;

    if let Some(record) = existing {
        unlink_cover_file_best_effort(&catalog.catalog_path, Some(record.file_name));
    }

    catalog.gc_orphans()?;

    Ok(())
}

fn read_cover_source_file(source: &Path) -> Result<Vec<u8>, CliError> {
    let file = fs::File::open(source)
        .map_err(|error| CliError::CoverIo(format!("could not read cover source file: {error}")))?;

    let meta = file
        .metadata()
        .map_err(|error| CliError::CoverIo(format!("could not read cover source file: {error}")))?;

    if !meta.is_file() {
        return Err(CliError::CoverIo(
            "cover source path must be a regular file".into(),
        ));
    }

    if meta.len() > MAX_COVER_BYTES {
        return Err(cover_too_large());
    }

    let mut bytes = Vec::new();

    file.take(MAX_COVER_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| CliError::CoverIo(format!("could not read cover source file: {error}")))?;

    if cover_len_exceeds_limit(bytes.len()) {
        return Err(cover_too_large());
    }

    Ok(bytes)
}

fn cover_len_exceeds_limit(len: usize) -> bool {
    u64::try_from(len).map_or(true, |len| len > MAX_COVER_BYTES)
}

fn game_not_found(game_id: &GameId) -> CliError {
    CliError::GameNotFound(game_id.as_str().to_owned())
}

fn cover_too_large() -> CliError {
    CliError::CoverDownloadFailed("cover file exceeds maximum size".into())
}
