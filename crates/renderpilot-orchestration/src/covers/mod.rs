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
use renderpilot_domain::{GameId, GameInstallation};
use renderpilot_storage_sqlite::SqliteStorage;
use serde::Serialize;

use self::http_client::http_client;
use self::policy::CoverRemotePolicy;
use crate::storage;
use crate::ServiceError;

/// Settings row key for the SteamGridDB API bearer token.
pub const STEAMGRIDDB_API_KEY_SETTING: &str = "steamgriddb_api_key";

/// Result of a cover mutation (fetch, set, or replace).
#[derive(Debug, Serialize)]
pub struct CoverMutationOutput {
    /// File name of the installed cover, relative to the covers directory.
    pub file_name: String,
    /// Epoch-millisecond timestamp of when the cover row was last updated.
    pub updated_at_ms: i64,
}

pub use fs_ops::{gc_orphan_cover_files, unlink_cover_file_best_effort};
pub use paths::MAX_COVER_BYTES;
pub use protocol::cover_protocol_http_response;

struct CoverCatalog<'a> {
    catalog_path: PathBuf,
    sqlite: &'a SqliteStorage,
}

impl<'a> CoverCatalog<'a> {
    fn new(context: &'a crate::Context) -> Result<Self, ServiceError> {
        let catalog_path = storage::catalog_database_path()?;
        Ok(Self {
            catalog_path,
            sqlite: context.storage(),
        })
    }

    fn require_game(&self, game_id: &GameId) -> Result<GameInstallation, ServiceError> {
        self.sqlite
            .find_game(game_id)?
            .ok_or_else(|| game_not_found(game_id))
    }

    fn install_cover(
        &self,
        game: &GameInstallation,
        bytes: &[u8],
    ) -> Result<CoverMutationOutput, ServiceError> {
        install::install_cover(
            self.sqlite,
            &self.catalog_path,
            game.id(),
            game.identity().title(),
            bytes,
        )
    }

    fn gc_orphans(&self) -> Result<(), ServiceError> {
        gc_orphan_cover_files(&self.catalog_path, self.sqlite)
    }
}

/// Removes orphan cover files from disk at application startup, best-effort.
pub fn gc_orphan_cover_files_startup(context: &crate::Context) {
    let _ = CoverCatalog::new(context).and_then(|catalog| catalog.gc_orphans());
}

/// Downloads cover artwork using the configured provider chain, then stores it for the game.
pub fn fetch_game_cover_auto(
    context: &crate::Context,
    game_id: GameId,
) -> Result<CoverMutationOutput, ServiceError> {
    let catalog = CoverCatalog::new(context)?;
    let game = catalog.require_game(&game_id)?;

    let client = http_client()?;

    let api_key = catalog
        .sqlite
        .get_setting(STEAMGRIDDB_API_KEY_SETTING)
        .map_err(ServiceError::from)?;

    let remote_policy = CoverRemotePolicy::load(catalog.sqlite)?;

    let bytes = providers::resolve_cover_bytes(&client, api_key.as_deref(), &remote_policy, &game)?;

    catalog.install_cover(&game, &bytes)
}

/// Copies a user-selected image into the catalog cover store after validation.
pub fn set_game_cover_from_file(
    context: &crate::Context,
    game_id: GameId,
    source: PathBuf,
) -> Result<CoverMutationOutput, ServiceError> {
    let catalog = CoverCatalog::new(context)?;
    let game = catalog.require_game(&game_id)?;

    let bytes = read_cover_source_file(&source)?;

    catalog.install_cover(&game, &bytes)
}

/// Removes stored cover metadata and deletes the associated cover file from disk.
pub fn clear_game_cover(context: &crate::Context, game_id: GameId) -> Result<(), ServiceError> {
    let catalog = CoverCatalog::new(context)?;
    catalog.require_game(&game_id)?;

    let existing = catalog.sqlite.find_game_cover(&game_id)?;

    catalog.sqlite.clear_game_cover_row(&game_id)?;

    if let Some(record) = existing {
        unlink_cover_file_best_effort(&catalog.catalog_path, Some(record.file_name));
    }

    catalog.gc_orphans()?;

    Ok(())
}

fn read_cover_source_file(source: &Path) -> Result<Vec<u8>, ServiceError> {
    let file = fs::File::open(source).map_err(|error| {
        ServiceError::CoverIo(format!("could not read cover source file: {error}"))
    })?;

    let meta = file.metadata().map_err(|error| {
        ServiceError::CoverIo(format!("could not read cover source file: {error}"))
    })?;

    if !meta.is_file() {
        return Err(ServiceError::CoverIo(
            "cover source path must be a regular file".into(),
        ));
    }

    if meta.len() > MAX_COVER_BYTES {
        return Err(cover_too_large());
    }

    let mut bytes = Vec::new();

    file.take(MAX_COVER_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| {
            ServiceError::CoverIo(format!("could not read cover source file: {error}"))
        })?;

    if cover_len_exceeds_limit(bytes.len()) {
        return Err(cover_too_large());
    }

    Ok(bytes)
}

fn cover_len_exceeds_limit(len: usize) -> bool {
    u64::try_from(len).map_or(true, |len| len > MAX_COVER_BYTES)
}

fn game_not_found(game_id: &GameId) -> ServiceError {
    ServiceError::GameNotFound(game_id.as_str().to_owned())
}

fn cover_too_large() -> ServiceError {
    ServiceError::CoverDownloadFailed("cover file exceeds maximum size".into())
}
