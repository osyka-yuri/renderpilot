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
use crate::ServiceError;
use crate::storage;

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

/// Result of a successful cover clear. The mutation itself is durable even
/// when its best-effort orphan pass fails, and the caller owns the one allowed
/// warning/diagnostic observation of that soft issue.
#[derive(Debug)]
pub struct ClearGameCoverObservation {
    /// A nonfatal orphan-cleanup failure after the durable clear completed.
    pub cleanup_issue: Option<ServiceError>,
}

pub use fs_ops::{gc_orphan_cover_files, unlink_cover_file_best_effort};
pub use paths::MAX_COVER_BYTES;
pub(crate) use paths::covers_directory;
pub use protocol::{cover_protocol_http_response, cover_unavailable_response};

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
            .ok_or_else(|| crate::addons::records::game_not_found(game_id))
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

/// Attempts the startup orphan pass. The caller decides how its soft failure
/// is observed, while the legacy wrapper keeps its void best-effort contract.
pub fn try_gc_orphan_cover_files_startup(context: &crate::Context) -> Result<(), ServiceError> {
    CoverCatalog::new(context).and_then(|catalog| catalog.gc_orphans())
}

/// Removes orphan cover files from disk at application startup, best-effort.
pub fn gc_orphan_cover_files_startup(context: &crate::Context) {
    if let Err(error) = try_gc_orphan_cover_files_startup(context) {
        log::warn!("startup cover orphan cleanup failed: {error}");
    }
}

/// Downloads cover artwork using the configured provider chain, then stores it for the game.
pub fn fetch_game_cover_auto(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<CoverMutationOutput, ServiceError> {
    let catalog = CoverCatalog::new(context)?;
    let game = catalog.require_game(game_id)?;

    let client = http_client()?;

    let api_key = catalog
        .sqlite
        .get_setting(STEAMGRIDDB_API_KEY_SETTING)
        .map_err(ServiceError::from)?;

    let remote_policy = CoverRemotePolicy::load(catalog.sqlite)?;

    let bytes = providers::resolve_cover_bytes(&client, api_key.as_deref(), &remote_policy, &game)?;

    let output = catalog.install_cover(&game, &bytes)?;
    context.patch_catalog_cover(game_id, Some(output.updated_at_ms));
    Ok(output)
}

/// Copies a user-selected image into the catalog cover store after validation.
pub fn set_game_cover_from_file(
    context: &crate::Context,
    game_id: &GameId,
    source: &Path,
) -> Result<CoverMutationOutput, ServiceError> {
    let catalog = CoverCatalog::new(context)?;
    let game = catalog.require_game(game_id)?;

    let bytes = read_cover_source_file(source)?;

    let output = catalog.install_cover(&game, &bytes)?;
    context.patch_catalog_cover(game_id, Some(output.updated_at_ms));
    Ok(output)
}

/// Removes stored cover metadata and deletes the associated cover file from disk.
pub fn clear_game_cover_with_observation(
    context: &crate::Context,
    game_id: &GameId,
) -> Result<ClearGameCoverObservation, ServiceError> {
    let catalog = CoverCatalog::new(context)?;
    catalog.require_game(game_id)?;

    let existing = catalog.sqlite.find_game_cover(game_id)?;

    catalog.sqlite.clear_game_cover_row(game_id)?;
    context.patch_catalog_cover(game_id, None);

    if let Some(record) = existing {
        unlink_cover_file_best_effort(&catalog.catalog_path, Some(record.file_name.as_str()));
    }

    let cleanup_issue = catalog.gc_orphans().err();

    Ok(ClearGameCoverObservation { cleanup_issue })
}

/// Legacy clear contract. Existing callers retain its result shape and its
/// console detail; new callers can observe the soft issue without duplicate
/// durable logging.
pub fn clear_game_cover(context: &crate::Context, game_id: &GameId) -> Result<(), ServiceError> {
    let observation = clear_game_cover_with_observation(context, game_id)?;
    if let Some(error) = observation.cleanup_issue {
        log::warn!("cover was cleared but orphan cleanup failed: {error}");
    }
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

fn cover_too_large() -> ServiceError {
    ServiceError::CoverDownloadFailed("cover file exceeds maximum size".into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn observation_and_legacy_cover_cleanup_contracts_remain_additive() {
        let source = include_str!("mod.rs");
        assert!(source.contains("pub fn try_gc_orphan_cover_files_startup("));
        assert!(source.contains("pub fn gc_orphan_cover_files_startup("));
        assert!(source.contains("pub struct ClearGameCoverObservation"));
        assert!(source.contains("pub fn clear_game_cover_with_observation("));
        assert!(source.contains("pub fn clear_game_cover(context:"));
    }
}
