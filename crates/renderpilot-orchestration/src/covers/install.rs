//! Crash-conscious cover installation.
//!
//! The file is written and durably renamed before catalog metadata is updated.
//! SQLite and the filesystem cannot participate in one shared atomic transaction,
//! so this module favors a state where the database never points to a file that
//! was not fully written.
//!
//! Concurrency: a process-global mutex ([`COVER_INSTALL_LOCK`]) serializes the
//! whole install. Without it, two installs could interleave like this:
//!
//! 1. install A: writes `cover_A`, upserts row(`game_a` → `cover_A`).
//! 2. install B: writes `cover_B` via `rename`, so the GC-visible final file
//!    exists on disk before B's row hits the catalog.
//! 3. install A: runs the post-install GC. The DB only references `cover_A`
//!    (B has not upserted yet), so `cover_B` looks like an orphan and gets
//!    deleted.
//! 4. install B: upserts row(`game_b` → `cover_B`), but the file is gone.
//!
//! That race surfaced in the field as a card with a "broken image" placeholder
//! whose `rp-cover://` request returned 404 because the catalog row pointed at
//! a non-existent file. Background cover sync runs with concurrency = 2, so
//! overlapping installs are the common case, not the rare one.
//!
//! Holding the lock around the whole install also keeps the targeted
//! `gc_orphan_cover_files` step honest: by the time it scans the directory,
//! every concurrent install is either entirely done (its file is referenced)
//! or has not yet renamed its temp file (its `.tmp-...part` is filtered out by
//! the GC visibility predicate).
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
use std::sync::{Mutex, MutexGuard};

use renderpilot_domain::GameId;
use renderpilot_storage_sqlite::SqliteStorage;

use super::fs_ops::gc_orphan_cover_files;
use super::paths::covers_directory;
use super::validation::validate_cover_bytes;
use super::CoverMutationOutput;
use crate::ServiceError;

/// Upper bound on the human-readable portion of cover file names.
///
/// 40 ASCII characters is enough for typical titles ("Black Myth Wukong",
/// "The Callisto Protocol", "Cyberpunk 2077") while keeping the total file
/// name well under filesystem limits once the ULID and extension are appended.
const MAX_TITLE_FRAGMENT_LEN: usize = 40;

/// Fallback fragment used when a title sanitizes to an empty string (titles
/// composed entirely of non-ASCII letters, punctuation, etc.).
const TITLE_FRAGMENT_FALLBACK: &str = "game";

/// Serializes cover install side effects (temp write, rename, upsert,
/// post-install GC) across concurrent fetches.
///
/// See the module-level docs for the race this prevents. Lock contention is
/// negligible because each install is a small write plus a single SQLite
/// upsert; the network download happens before the lock is acquired.
static COVER_INSTALL_LOCK: Mutex<()> = Mutex::new(());

fn lock_cover_install() -> MutexGuard<'static, ()> {
    // A poisoned lock here only means a previous install panicked
    // mid-write; the catalog cannot be more inconsistent than the
    // crash-conscious flow below already tolerates, so we proceed with
    // the inner guard rather than fail the new install.
    COVER_INSTALL_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Persists `bytes` as the cover for `game_id`.
///
/// The on-disk basename has shape `cover-<title-slug>-<ulid>.<ext>`:
///
/// * `<title-slug>` is derived from `display_title` (the human-readable
///   game title) by lower-casing ASCII alphanumerics and collapsing runs of
///   anything else into single dashes (see [`safe_title_fragment`]). This
///   keeps `covers/` browsable and avoids embedding launcher metadata or
///   absolute paths into file names — earlier revisions encoded the full
///   `manual:<install_path>` game id, which produced names like
///   `cover-manual-D-SteamLibrary-steamapps-common-The-Callisto-Protocol-...`.
/// * `<ulid>` keeps each install unique (and time-sortable for debugging),
///   so concurrent or repeat installs never collide regardless of title.
/// * `<ext>` matches the validated image format (`png`/`jpg`/`webp`/`gif`).
///
/// `display_title` should be `GameInstallation::identity().title()`. Empty,
/// non-ASCII-only, or otherwise unprintable titles fall back to `game`,
/// guaranteeing a valid basename for any catalog row.
pub(super) fn install_cover(
    sqlite: &SqliteStorage,
    catalog_db_path: &Path,
    game_id: &GameId,
    display_title: &str,
    bytes: &[u8],
) -> Result<CoverMutationOutput, ServiceError> {
    let format = validate_cover_bytes(bytes)?;

    let _install_guard = lock_cover_install();

    let paths = CoverInstallPaths::new(catalog_db_path, display_title, format.extension())?;

    install_cover_file(bytes, &paths)?;

    if let Err(error) = sqlite.upsert_game_cover(game_id, &paths.file_name) {
        remove_file_and_sync_parent_best_effort(&paths.final_path);
        return Err(ServiceError::from(error));
    }

    let record = sqlite
        .find_game_cover(game_id)?
        .ok_or_else(|| ServiceError::CommandFailed("cover row missing after upsert".into()))?;

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
    fn new(
        catalog_db_path: &Path,
        display_title: &str,
        extension: &str,
    ) -> Result<Self, ServiceError> {
        let covers_dir = covers_directory(catalog_db_path);

        fs::create_dir_all(&covers_dir).map_err(|error| {
            cover_io_error("could not create covers directory", &covers_dir, error)
        })?;

        let title_fragment = safe_title_fragment(display_title);
        let ulid = ulid::Ulid::new().to_string();

        let file_name = format!("cover-{title_fragment}-{ulid}.{extension}");
        let temp_path = covers_dir.join(format!(".tmp-cover-{ulid}.part"));
        let final_path = covers_dir.join(&file_name);

        Ok(Self {
            file_name,
            temp_path,
            final_path,
        })
    }
}

fn install_cover_file(bytes: &[u8], paths: &CoverInstallPaths) -> Result<(), ServiceError> {
    let mut temp_cleanup = RemoveFileOnDrop::new(paths.temp_path.clone());

    write_temp_file_durably(&paths.temp_path, bytes)?;

    fs::rename(&paths.temp_path, &paths.final_path).map_err(|error| {
        cover_io_error("could not finalize cover file", &paths.final_path, error)
    })?;

    temp_cleanup.disarm();

    sync_parent_directory_best_effort(&paths.final_path);

    Ok(())
}

fn write_temp_file_durably(path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
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

/// Lower-case ASCII slug derived from a human-readable title.
///
/// Rules:
///
/// * ASCII letters → lower-case;
/// * ASCII digits → kept verbatim;
/// * any other code point (whitespace, punctuation, non-ASCII letters such as
///   `:` in `Black Myth: Wukong`, Cyrillic, emoji, ...) → replaced with a
///   single `-`, with consecutive replacements collapsed;
/// * leading/trailing dashes are trimmed;
/// * the slug counts only **useful** characters (ASCII letters and digits)
///   toward [`MAX_TITLE_FRAGMENT_LEN`]; separator dashes do not consume the
///   budget, and no trailing dash is emitted when the limit is reached on a
///   useful character;
/// * empty / fully-stripped output falls back to [`TITLE_FRAGMENT_FALLBACK`].
///
/// Examples:
///
/// | input                              | output                       |
/// |------------------------------------|------------------------------|
/// | `Black Myth: Wukong`               | `black-myth-wukong`          |
/// | `The Callisto Protocol`            | `the-callisto-protocol`      |
/// | `Cyberpunk 2077`                   | `cyberpunk-2077`             |
/// | `Tony Hawk's Pro Skater 1+2`       | `tony-hawk-s-pro-skater-1-2` |
/// | `Ведьмак 3` (Cyrillic + digit)     | `3`                          |
fn safe_title_fragment(title: &str) -> String {
    let mut out = String::with_capacity(title.len().min(MAX_TITLE_FRAGMENT_LEN));
    let mut previous_was_dash = false;
    let mut useful_count = 0usize;

    for ch in title.chars() {
        if useful_count >= MAX_TITLE_FRAGMENT_LEN {
            break;
        }

        let safe_ch = match ch {
            'a'..='z' | '0'..='9' => Some(ch),
            'A'..='Z' => Some(ch.to_ascii_lowercase()),
            _ => None,
        };

        match safe_ch {
            Some(ch) => {
                out.push(ch);
                previous_was_dash = false;
                useful_count += 1;
            }
            None if !previous_was_dash => {
                out.push('-');
                previous_was_dash = true;
            }
            None => {}
        }
    }

    let trimmed = out.trim_matches('-');

    if trimmed.is_empty() {
        TITLE_FRAGMENT_FALLBACK.to_string()
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

fn cover_io_error(action: &str, path: &Path, error: std::io::Error) -> ServiceError {
    ServiceError::CoverIo(format!("{action} '{}': {error}", path.display()))
}

/// Best-effort cleanup guard: if the contained path is still set when the
/// guard is dropped, the file is removed and the parent directory is synced.
/// Calling [`Self::disarm`] before drop signals success and suppresses the
/// cleanup. The state lives in the `Option` itself, so "armed" and
/// "disarmed" cannot drift apart from the path.
struct RemoveFileOnDrop {
    path: Option<PathBuf>,
}

impl RemoveFileOnDrop {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for RemoveFileOnDrop {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            remove_file_and_sync_parent_best_effort(&path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };
    use renderpilot_storage_sqlite::SqliteStorage;

    use super::*;

    /// Minimal byte sequence accepted by [`validate_cover_bytes`] as PNG.
    /// The full PNG file structure is not required — magic bytes plus a few
    /// bytes of body suffice for install / GC behavior under test.
    const FAKE_PNG_PREFIX: &[u8] = b"\x89PNG\r\n\x1A\n";

    fn fake_png_bytes(seed: u8) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(FAKE_PNG_PREFIX.len() + 64);
        bytes.extend_from_slice(FAKE_PNG_PREFIX);
        bytes.extend(std::iter::repeat_n(seed, 64));
        bytes
    }

    fn sample_game(game_id: &str, title: &str, install_path: &str) -> GameInstallation {
        let identity = GameIdentity::new(
            GameId::new(game_id).expect("game id should be valid"),
            title,
            Launcher::Manual,
        )
        .expect("game identity should be valid");

        GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(install_path).expect("install path should be valid"),
        )
    }

    /// Regression test for the GC race that produced "broken image" cards in
    /// the field: parallel `install_cover` calls (concurrency = 2 in the
    /// background cover sync) used to delete each other's freshly-renamed
    /// files because the per-install GC ran without coordination.
    ///
    /// Without [`COVER_INSTALL_LOCK`], one of the workers consistently lost
    /// its file before its DB row was upserted, and that game ended up with a
    /// catalog row pointing at a missing file.
    #[test]
    fn parallel_installs_keep_every_referenced_cover_file_on_disk() {
        let storage = Arc::new(SqliteStorage::in_memory().expect("sqlite should open"));
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let catalog_db_path: Arc<Path> = Arc::from(temp.path().join("catalog.db"));

        let games = (0..8)
            .map(|index| {
                sample_game(
                    &format!("game:cover-race-{index}"),
                    &format!("Game {index}"),
                    &format!("C:/Games/Game{index}"),
                )
            })
            .collect::<Vec<_>>();

        for game in &games {
            storage.upsert_game(game).expect("game should store");
        }

        let mut handles = Vec::with_capacity(games.len());

        for (index, game) in games.iter().enumerate() {
            let storage = Arc::clone(&storage);
            let catalog_db_path = Arc::clone(&catalog_db_path);
            let game_id = game.id().clone();
            let bytes = fake_png_bytes(u8::try_from(index).unwrap_or(0));

            let title = format!("Game {index}");

            handles.push(thread::spawn(move || {
                install_cover(&storage, &catalog_db_path, &game_id, &title, &bytes)
                    .expect("install cover should succeed")
            }));
        }

        let outputs = handles
            .into_iter()
            .map(|handle| handle.join().expect("install thread should not panic"))
            .collect::<Vec<_>>();

        let covers_dir = covers_directory(&catalog_db_path);

        for (game, output) in games.iter().zip(outputs.iter()) {
            let record = storage
                .find_game_cover(game.id())
                .expect("find_game_cover should succeed")
                .expect("cover row should exist for every game");
            assert_eq!(
                record.file_name,
                output.file_name,
                "DB row must agree with install output for {}",
                game.id().as_str(),
            );

            let cover_path = covers_dir.join(&record.file_name);
            assert!(
                cover_path.is_file(),
                "cover file must exist on disk for {} ({})",
                game.id().as_str(),
                cover_path.display(),
            );
        }
    }

    /// When the same game id is installed multiple times, only the most
    /// recent file is referenced and earlier files are GC'd. This guards the
    /// re-fetch path (e.g. user clicks "Fetch cover" twice) while the lock is
    /// in place.
    #[test]
    fn sequential_installs_for_same_game_keep_only_latest_file() {
        let storage = SqliteStorage::in_memory().expect("sqlite should open");
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let catalog_db_path = temp.path().join("catalog.db");

        let game = sample_game("game:replace-cover", "Replace Cover", "C:/Games/Replace");
        storage.upsert_game(&game).expect("game should store");

        let title = game.identity().title();
        let first = install_cover(
            &storage,
            &catalog_db_path,
            game.id(),
            title,
            &fake_png_bytes(1),
        )
        .expect("first install should succeed");
        let second = install_cover(
            &storage,
            &catalog_db_path,
            game.id(),
            title,
            &fake_png_bytes(2),
        )
        .expect("second install should succeed");

        assert!(
            first.file_name.starts_with("cover-replace-cover-"),
            "file name should embed the title slug, got {}",
            first.file_name,
        );
        assert!(
            !first.file_name.contains("manual"),
            "file name must not leak launcher/game-id metadata, got {}",
            first.file_name,
        );

        assert_ne!(first.file_name, second.file_name);

        let covers_dir = covers_directory(&catalog_db_path);
        assert!(
            !covers_dir.join(&first.file_name).exists(),
            "previous cover file should be removed by post-install GC",
        );
        assert!(
            covers_dir.join(&second.file_name).is_file(),
            "newly installed cover file should remain on disk",
        );
    }

    #[test]
    fn safe_title_fragment_caps_long_titles_within_limit_without_trailing_dash() {
        let very_long = "A".repeat(200);
        let fragment = safe_title_fragment(&very_long);
        assert_eq!(fragment.len(), MAX_TITLE_FRAGMENT_LEN);
        assert!(fragment.chars().all(|c| c == 'a'));
        assert!(!fragment.ends_with('-'));
    }

    #[test]
    fn safe_title_fragment_stops_at_limit_without_separator_dash_after_last_char() {
        let base = "a".repeat(MAX_TITLE_FRAGMENT_LEN);
        let fragment = safe_title_fragment(&format!("{base}!!!"));
        assert_eq!(fragment, base);
        assert!(!fragment.ends_with('-'));
    }

    #[test]
    fn safe_title_fragment_lowercases_and_collapses_separators() {
        assert_eq!(
            safe_title_fragment("Black Myth: Wukong"),
            "black-myth-wukong",
        );
        assert_eq!(
            safe_title_fragment("The Callisto Protocol"),
            "the-callisto-protocol",
        );
        assert_eq!(safe_title_fragment("Cyberpunk 2077"), "cyberpunk-2077");
    }

    #[test]
    fn safe_title_fragment_collapses_runs_of_punctuation() {
        assert_eq!(
            safe_title_fragment("Tony Hawk's   Pro+++Skater 1+2"),
            "tony-hawk-s-pro-skater-1-2",
        );
    }

    #[test]
    fn safe_title_fragment_strips_leading_and_trailing_separators() {
        assert_eq!(safe_title_fragment("  --Halo--  "), "halo");
        assert_eq!(safe_title_fragment("!!"), TITLE_FRAGMENT_FALLBACK);
    }

    #[test]
    fn safe_title_fragment_falls_back_for_non_ascii_only_titles() {
        assert_eq!(safe_title_fragment(""), TITLE_FRAGMENT_FALLBACK);
        assert_eq!(safe_title_fragment("Ведьмак 3"), "3");
        assert_eq!(safe_title_fragment("ゼルダ"), TITLE_FRAGMENT_FALLBACK);
    }
}
