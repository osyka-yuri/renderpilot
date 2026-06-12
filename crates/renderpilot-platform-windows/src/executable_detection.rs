//! Orchestrates the heuristic detection and classification of game executables.
//!
//! The NVIDIA Driver Settings (DRS) architecture exclusively indexes application profiles by
//! executable basename. Consequently, RenderPilot must deterministically identify a "primary"
//! executable for each game installation to guarantee accurate profile resolution and writes.
//! This module surfaces a rigorously ranked list of candidates evaluated against exclusion filters
//! and positive heuristic signals (e.g., proximity to root, stem matching, binary payload size).
//! This enables the upstream orchestration layer to either autonomously select the optimal target
//! or expose the ranked collection for manual user override.
//!
//! Detection execution is strictly bounded to a designated install directory and operates purely
//! via filesystem metadata—deliberately eschewing deep PE parsing. Version-specific PE introspection
//! lives in the global catalog (`renderpilot-detection`), not in this crate.

use std::path::{Path, PathBuf};

use crate::fs_walk;

// -----------------------------------------------------------------------------
// Filter lists
// -----------------------------------------------------------------------------

/// Exact filename matches (case-insensitive, with or without `.exe`) that
/// are definitely not the main game binary. Launchers, support apps.
const NON_GAME_EXE_NAMES: &[&str] = &[
    "steam",
    "steamservice",
    "steamerrorreporter",
    "epicgameslauncher",
    "origin",
    "eadesktop",
    "ubisoftconnect",
    "gog galaxy",
    "galaxyclient",
    "battle.net",
    "rockstargameslauncher",
    "playnite",
    "setup",
    "unins000",
    "unins001",
];

/// Filename suffixes (case-insensitive) that strongly imply a non-game
/// binary. Matched against the basename without the `.exe` extension.
const NON_GAME_EXE_SUFFIXES: &[&str] = &[
    "launcher",
    "setup",
    "install",
    "uninstall",
    "crashreport",
    "crashhandler",
    "updater",
    "update",
    "redist",
    "dxsetup",
    "vcredist",
    "configure",
    "settings",
    "benchmark",
    "server",
    "dedicated",
    "editor",
    "helper",
    "support",
    "tool",
];

/// Substrings (case-insensitive) anywhere in the filename that imply
/// a non-game binary. Catches names like `CrashHandler_x64.exe`.
const NON_GAME_EXE_SUBSTRINGS: &[&str] = &[
    "crash",
    "report",
    "redist",
    "helper",
    "support",
    "config",
    "setup",
    "install",
    "uninstall",
    "launcher",
    "updater",
    "dxsetup",
    "vcredist",
];

// -----------------------------------------------------------------------------
// Public types
// -----------------------------------------------------------------------------

/// Articulates the specific heuristic rationale for segregating a `.exe` from the pool of
/// primary game candidates. This classification is preserved alongside the candidate record,
/// enabling the frontend UI to transparently justify the rejection and facilitate manual
/// override workflows should the heuristic prove overly aggressive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectionReason {
    /// Filename matched an entry in `NON_GAME_EXE_NAMES`.
    NonGameName(String),
    /// Filename ended with an entry in `NON_GAME_EXE_SUFFIXES`.
    NonGameSuffix(String),
    /// Filename contained an entry in `NON_GAME_EXE_SUBSTRINGS`.
    NonGameSubstring(String),
}

impl RejectionReason {
    /// Stable wire string for serialization to the UI.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NonGameName(_) => "non_game_name",
            Self::NonGameSuffix(_) => "non_game_suffix",
            Self::NonGameSubstring(_) => "non_game_substring",
        }
    }

    /// The exact filter token that matched.
    pub fn token(&self) -> &str {
        match self {
            Self::NonGameName(s) | Self::NonGameSuffix(s) | Self::NonGameSubstring(s) => s,
        }
    }
}

/// One executable discovered inside a game's install directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableCandidate {
    /// Absolute path on disk.
    pub absolute_path: PathBuf,
    /// Path relative to the install dir root, using forward slashes
    /// regardless of the host filesystem. E.g. `"bin/Game.exe"`.
    pub relative_path: String,
    /// Just the basename (e.g. `"Game.exe"`). NVAPI is keyed by this.
    pub file_name: String,
    /// File size in bytes. Used by the ranking heuristic.
    pub size_bytes: u64,
    /// Depth relative to the install dir root (0 = directly in the root).
    pub depth: u32,
    /// Ranking score: higher = more likely to be the main game binary.
    /// Only meaningful for candidates with `rejection: None`.
    pub rank_score: i32,
    /// `None` means "looks like a game binary". `Some` means a filter
    /// rejected it; the UI can still surface it as a "show more" option
    /// in case the heuristic was wrong.
    pub rejection: Option<RejectionReason>,
}

// -----------------------------------------------------------------------------
// Detection entry point
// -----------------------------------------------------------------------------

/// Scans `install_dir` for executables and returns them ranked.
///
/// Order:
///   1. `rejection: None` (game-binary candidates) first, sorted by
///      `rank_score DESC`, then by relative path ASC for stability.
///   2. `rejection: Some(_)` last, sorted by relative path ASC.
///
/// Returns an empty vector if the directory does not exist or cannot
/// be read; never panics on filesystem errors.
pub fn detect_executable_candidates(install_dir: &Path) -> Vec<ExecutableCandidate> {
    let install_dir_canonical = install_dir.to_path_buf();
    let install_dir_name = install_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_owned)
        .unwrap_or_default();

    let raw_candidates = collect_raw_candidates(&install_dir_canonical);

    let mut candidates: Vec<ExecutableCandidate> = raw_candidates
        .into_iter()
        .map(|raw| {
            let rejection = classify_filename(&raw.file_name_no_ext, &raw.file_name);
            let rank_score = compute_rank_score(&raw, &install_dir_name);
            ExecutableCandidate {
                absolute_path: raw.absolute_path,
                relative_path: raw.relative_path,
                file_name: raw.file_name,
                size_bytes: raw.size_bytes,
                depth: raw.depth,
                rank_score,
                rejection,
            }
        })
        .collect();

    candidates.sort_by(
        |a, b| match (a.rejection.is_some(), b.rejection.is_some()) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            _ => b
                .rank_score
                .cmp(&a.rank_score)
                .then_with(|| a.relative_path.cmp(&b.relative_path)),
        },
    );

    candidates
}

// -----------------------------------------------------------------------------
// Ranking constants
// -----------------------------------------------------------------------------

/// Bonus for executables in the install directory root (depth 0).
/// Root-level files are far more likely to be the main game binary.
const ROOT_DEPTH_BONUS: i32 = 20;

/// Bonus for executables one level below root (depth 1).
/// Paths like `bin/Game.exe` are still plausible primary targets.
const NEAR_ROOT_DEPTH_BONUS: i32 = 5;

/// Bonus when the executable stem matches the install directory name.
/// E.g. `Cyberpunk2077.exe` inside `Cyberpunk2077/`.
const FOLDER_NAME_MATCH_BONUS: i32 = 30;

/// Bonus for binaries larger than [`LARGE_BINARY_BYTES`].
/// Capped to avoid letting size dominate games with small engines.
const LARGE_BINARY_BONUS: i32 = 10;

/// Bonus for binaries larger than [`MEDIUM_BINARY_BYTES`] but not large.
const MEDIUM_BINARY_BONUS: i32 = 3;

const MEGABYTE: u64 = 1024 * 1024;

/// Size threshold for the large-binary bonus.
const LARGE_BINARY_BYTES: u64 = 100 * MEGABYTE;

/// Size threshold for the medium-binary bonus.
const MEDIUM_BINARY_BYTES: u64 = 10 * MEGABYTE;

// -----------------------------------------------------------------------------
// Internals
// -----------------------------------------------------------------------------

/// Pre-ranking record gathered during the directory walk.
struct RawCandidate {
    absolute_path: PathBuf,
    relative_path: String,
    file_name: String,
    file_name_no_ext: String,
    size_bytes: u64,
    depth: u32,
}

fn collect_raw_candidates(root: &Path) -> Vec<RawCandidate> {
    let mut out = Vec::new();
    fs_walk::walk_files(root, 0, &mut |entry, path, depth| {
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            return;
        };
        let lower = file_name.to_ascii_lowercase();
        if !lower.ends_with(".exe") {
            return;
        }

        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let relative_path = relative_path_from(root, path);
        let file_name_no_ext = file_name
            .rsplit_once('.')
            .map(|(stem, _)| stem.to_owned())
            .unwrap_or_else(|| file_name.to_owned());

        out.push(RawCandidate {
            absolute_path: path.to_path_buf(),
            relative_path,
            file_name: file_name.to_owned(),
            file_name_no_ext,
            size_bytes,
            depth,
        });
    });
    out
}

fn relative_path_from(root: &Path, full: &Path) -> String {
    full.strip_prefix(root)
        .ok()
        .and_then(|rel| rel.to_str())
        .map(|s| s.replace('\\', "/"))
        .unwrap_or_else(|| {
            full.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_owned()
        })
}

fn classify_filename(name_no_ext: &str, full_name: &str) -> Option<RejectionReason> {
    let lower = name_no_ext.to_ascii_lowercase();

    for banned in NON_GAME_EXE_NAMES {
        if lower == *banned {
            return Some(RejectionReason::NonGameName((*banned).to_owned()));
        }
    }

    for suffix in NON_GAME_EXE_SUFFIXES {
        if lower.ends_with(suffix) && lower != *suffix {
            // Don't double-count when the whole name *is* the suffix
            // (covered by NON_GAME_EXE_NAMES match above).
            return Some(RejectionReason::NonGameSuffix((*suffix).to_owned()));
        }
    }

    let lower_full = full_name.to_ascii_lowercase();
    for needle in NON_GAME_EXE_SUBSTRINGS {
        if lower_full.contains(needle) {
            return Some(RejectionReason::NonGameSubstring((*needle).to_owned()));
        }
    }

    None
}

fn compute_rank_score(raw: &RawCandidate, install_dir_name: &str) -> i32 {
    let mut score: i32 = 0;

    if raw.depth == 0 {
        score += ROOT_DEPTH_BONUS;
    } else if raw.depth == 1 {
        score += NEAR_ROOT_DEPTH_BONUS;
    }

    if !install_dir_name.is_empty() && raw.file_name_no_ext.eq_ignore_ascii_case(install_dir_name) {
        score += FOLDER_NAME_MATCH_BONUS;
    }

    if raw.size_bytes > LARGE_BINARY_BYTES {
        score += LARGE_BINARY_BONUS;
    } else if raw.size_bytes > MEDIUM_BINARY_BYTES {
        score += MEDIUM_BINARY_BONUS;
    }

    score
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::io::Write;

    use tempfile::TempDir;

    use super::*;

    fn write_file(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        let mut f = File::create(path).expect("create file");
        f.write_all(bytes).expect("write contents");
    }

    #[test]
    fn returns_empty_for_missing_directory() {
        let path = std::env::temp_dir().join("renderpilot-no-such-folder-91823");
        assert!(detect_executable_candidates(&path).is_empty());
    }

    #[test]
    fn returns_empty_when_no_exe_files() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp.path().join("readme.txt"), b"hi");
        assert!(detect_executable_candidates(tmp.path()).is_empty());
    }

    #[test]
    fn ranks_root_exe_above_nested_one_of_same_size() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp.path().join("Game.exe"), &[0u8; 1024]);
        write_file(&tmp.path().join("bin/Game.exe"), &[0u8; 1024]);

        let results = detect_executable_candidates(tmp.path());
        let game_only: Vec<&ExecutableCandidate> =
            results.iter().filter(|c| c.rejection.is_none()).collect();
        assert_eq!(game_only.len(), 2);
        assert_eq!(game_only[0].depth, 0);
        assert_eq!(game_only[1].depth, 1);
    }

    #[test]
    fn rejects_launcher_and_setup_exes() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp.path().join("Game.exe"), &[0u8; 1024]);
        write_file(&tmp.path().join("GameLauncher.exe"), &[0u8; 1024]);
        write_file(&tmp.path().join("Setup.exe"), &[0u8; 1024]);

        let results = detect_executable_candidates(tmp.path());
        let kept: Vec<&ExecutableCandidate> =
            results.iter().filter(|c| c.rejection.is_none()).collect();
        let rejected: Vec<&ExecutableCandidate> =
            results.iter().filter(|c| c.rejection.is_some()).collect();

        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].file_name, "Game.exe");
        assert_eq!(rejected.len(), 2);
        // Order: kept first, rejected after.
        assert_eq!(results[0].file_name, "Game.exe");
    }

    #[test]
    fn rejection_reasons_carry_matched_token() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp.path().join("CrashHandler_x64.exe"), &[0u8; 1024]);

        let results = detect_executable_candidates(tmp.path());
        assert_eq!(results.len(), 1);
        let r = results[0].rejection.as_ref().expect("should be rejected");
        // "crash" is in NON_GAME_EXE_SUBSTRINGS.
        assert_eq!(r.kind(), "non_game_substring");
        assert_eq!(r.token(), "crash");
    }

    #[test]
    fn folder_name_match_promotes_main_binary() {
        let tmp = TempDir::new().unwrap();
        let game_dir = tmp.path().join("Cyberpunk2077");
        fs::create_dir_all(&game_dir).unwrap();
        // Two equally-sized exe's, neither at root. The one whose
        // stem matches the install folder name should rank higher.
        write_file(&game_dir.join("Cyberpunk2077.exe"), &[0u8; 1024]);
        write_file(&game_dir.join("RandomOther.exe"), &[0u8; 1024]);

        let results = detect_executable_candidates(&game_dir);
        let kept: Vec<&ExecutableCandidate> =
            results.iter().filter(|c| c.rejection.is_none()).collect();
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].file_name, "Cyberpunk2077.exe");
    }

    #[test]
    fn relative_path_uses_forward_slashes() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp.path().join("bin/win64/Game.exe"), &[0u8; 1024]);

        let results = detect_executable_candidates(tmp.path());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].relative_path, "bin/win64/Game.exe");
    }

    #[test]
    fn skips_dotted_and_backup_directories() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp.path().join("Game.exe"), &[0u8; 1024]);
        write_file(&tmp.path().join(".git/Decoy.exe"), &[0u8; 1024]);
        write_file(
            &tmp.path().join("_renderpilot_backups/Decoy.exe"),
            &[0u8; 1024],
        );

        let results = detect_executable_candidates(tmp.path());
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_name, "Game.exe");
    }

    #[test]
    fn detects_case_insensitive_exe_extension() {
        let tmp = TempDir::new().unwrap();
        write_file(&tmp.path().join("Game.EXE"), &[0u8; 1024]);
        write_file(&tmp.path().join("OTHER.Exe"), &[0u8; 1024]);

        let results = detect_executable_candidates(tmp.path());
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn larger_binaries_outrank_tiny_ones_when_other_signals_tie() {
        let tmp = TempDir::new().unwrap();
        // Both at root, same level, neither matches folder name.
        write_file(&tmp.path().join("BigGame.exe"), &[0u8; 110 * 1024 * 1024]);
        write_file(&tmp.path().join("TinyGame.exe"), &[0u8; 1024]);

        let results = detect_executable_candidates(tmp.path());
        let kept: Vec<&ExecutableCandidate> =
            results.iter().filter(|c| c.rejection.is_none()).collect();
        assert!(kept.len() >= 2);
        assert_eq!(kept[0].file_name, "BigGame.exe");
    }
}
