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

use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use renderpilot_detection::{InstallTreeCompleteness, InstallTreeWalker, WalkDiagnosticKind};

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
    "eosbootstrapper",
    "easyanticheat",
    "easyanticheat_setup",
    "battleye",
    "anticheatexpert",
    "activationui",
    "touchup",
    "oalinst",
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
    "anticheat",
    "bootstrapper",
    "prereqsetup",
    "diag",
    "reporter",
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
    "anticheat",
    "battleye",
    "bootstrapper",
    "prereq",
    "cleanup",
];

/// Directory names (case-insensitive, exact segment match) that hold installer,
/// redistributable, or prerequisite binaries rather than the game itself. Any
/// executable located under one of these — at any depth — is not the game, no
/// matter what it is named (e.g. `__Installer/Cleanup.exe`, `_CommonRedist/.../
/// vc_redist.exe`). Matching the parent folder catches generically-named helpers
/// the filename filters miss.
const NON_GAME_DIR_SEGMENTS: &[&str] = &[
    "__installer",
    "_commonredist",
    "commonredist",
    "redist",
    "_redist",
    "redistributable",
    "redistributables",
    "directx",
    "vcredist",
    "dotnet",
    "prerequisites",
    "installers",
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
    /// Located under an entry in `NON_GAME_DIR_SEGMENTS` (installer/redist folder).
    NonGameLocation(String),
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
            Self::NonGameLocation(_) => "non_game_location",
            Self::NonGameName(_) => "non_game_name",
            Self::NonGameSuffix(_) => "non_game_suffix",
            Self::NonGameSubstring(_) => "non_game_substring",
        }
    }

    /// The exact filter token that matched.
    pub fn token(&self) -> &str {
        match self {
            Self::NonGameLocation(s)
            | Self::NonGameName(s)
            | Self::NonGameSuffix(s)
            | Self::NonGameSubstring(s) => s,
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

/// Read-only executable probe for one installation tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableDetectionReport {
    candidates: Vec<ExecutableCandidate>,
    structural_files: Vec<PathBuf>,
    completeness: InstallTreeCompleteness,
    diagnostics: Vec<String>,
    visited_entries: usize,
}

impl ExecutableDetectionReport {
    /// Ranked executable candidates.
    pub fn candidates(&self) -> &[ExecutableCandidate] {
        &self.candidates
    }

    /// Non-executable distribution files observed by the same traversal.
    ///
    /// These paths are boundary evidence only; their contents are never read
    /// or hashed by executable inspection.
    pub fn structural_files(&self) -> &[PathBuf] {
        &self.structural_files
    }

    /// Whether every reachable directory was enumerated.
    pub fn completeness(&self) -> InstallTreeCompleteness {
        self.completeness
    }

    /// Recoverable filesystem/cancellation diagnostics.
    ///
    /// An intentional advisory depth limit is reflected by [`Self::completeness`]
    /// but is not an error and therefore is not included here.
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Directory entries consumed by this inspection.
    pub fn visited_entries(&self) -> usize {
        self.visited_entries
    }
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
    inspect_executable_candidates(install_dir).candidates
}

/// Probes executable candidates and preserves traversal completeness evidence.
#[must_use]
pub fn inspect_executable_candidates(install_dir: &Path) -> ExecutableDetectionReport {
    inspect_executable_candidates_with_walker(install_dir, InstallTreeWalker::probe())
}

/// Performs a complete, non-hashing executable walk for an installation
/// boundary decision.
///
/// Advisory probes intentionally trade coverage for latency. A root
/// recommendation, however, must know whether another executable branch exists
/// before it can call a parent one installation. This variant traverses every
/// reachable non-reparse directory and preserves incomplete diagnostics.
#[must_use]
pub fn inspect_executable_candidates_complete(install_dir: &Path) -> ExecutableDetectionReport {
    inspect_executable_candidates_with_walker(install_dir, InstallTreeWalker::full())
}

/// Performs a complete executable walk with an explicit entry budget and
/// cooperative cancellation.
///
/// Budget exhaustion and cancellation both produce an incomplete report;
/// callers must not use such a report as authoritative boundary evidence.
#[must_use]
pub fn inspect_executable_candidates_bounded(
    install_dir: &Path,
    max_entries: usize,
    is_cancelled: impl Fn() -> bool,
) -> ExecutableDetectionReport {
    inspect_executable_candidates_with_walker_and_cancel(
        install_dir,
        InstallTreeWalker::full().with_entry_budget(max_entries),
        is_cancelled,
    )
}

fn inspect_executable_candidates_with_walker(
    install_dir: &Path,
    walker: InstallTreeWalker,
) -> ExecutableDetectionReport {
    inspect_executable_candidates_with_walker_and_cancel(install_dir, walker, || false)
}

fn inspect_executable_candidates_with_walker_and_cancel(
    install_dir: &Path,
    walker: InstallTreeWalker,
    is_cancelled: impl Fn() -> bool,
) -> ExecutableDetectionReport {
    let install_dir_canonical = install_dir.to_path_buf();
    let install_dir_name = install_dir
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_owned)
        .unwrap_or_default();

    let (raw_candidates, structural_files, completeness, diagnostics, visited_entries) =
        collect_raw_candidates(&install_dir_canonical, walker, is_cancelled);

    let mut candidates: Vec<ExecutableCandidate> = raw_candidates
        .into_iter()
        .map(|raw| {
            let rejection = classify(&raw.relative_path, &raw.file_name_no_ext, &raw.file_name);
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

    ExecutableDetectionReport {
        candidates,
        structural_files,
        completeness,
        diagnostics,
        visited_entries,
    }
}

/// Returns whether `path` is a readable Windows PE executable.
///
/// This intentionally validates only the DOS and PE signatures required to
/// distinguish a real executable from an arbitrary file named `*.exe`.
#[must_use]
pub fn is_readable_windows_pe_executable(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut dos_header = [0_u8; 64];
    if file.read_exact(&mut dos_header).is_err() || &dos_header[..2] != b"MZ" {
        return false;
    }
    let pe_offset = u32::from_le_bytes([
        dos_header[0x3c],
        dos_header[0x3d],
        dos_header[0x3e],
        dos_header[0x3f],
    ]);
    if file.seek(SeekFrom::Start(u64::from(pe_offset))).is_err() {
        return false;
    }
    let mut signature = [0_u8; 4];
    file.read_exact(&mut signature).is_ok() && signature == *b"PE\0\0"
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

/// Bonus when the executable stem matches the install directory name after
/// normalization (non-alphanumerics stripped, lowercased). E.g. `Cyberpunk2077.exe`
/// inside `Cyberpunk 2077/`. Tolerant of spacing/punctuation differences the way a
/// launcher names a folder vs the game binary.
const FOLDER_NAME_MATCH_BONUS: i32 = 30;

/// Weaker bonus when one normalized name merely *contains* the other (e.g. an
/// `re2.exe` inside `Resident Evil 2/`, or a `WitcherLauncher` folder). Lower than
/// an exact match so the precise binary still wins.
const FOLDER_NAME_PARTIAL_BONUS: i32 = 12;

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

fn collect_raw_candidates(
    root: &Path,
    walker: InstallTreeWalker,
    is_cancelled: impl Fn() -> bool,
) -> (
    Vec<RawCandidate>,
    Vec<PathBuf>,
    InstallTreeCompleteness,
    Vec<String>,
    usize,
) {
    let mut out = Vec::new();
    let mut structural_files = Vec::new();
    let Ok(report) = walker.walk_filtered_cancellable(
        root,
        |file_name| {
            let lower = file_name.to_ascii_lowercase();
            lower.ends_with(".exe") || is_structural_file_name(&lower)
        },
        is_cancelled,
    ) else {
        return (
            out,
            structural_files,
            InstallTreeCompleteness::Incomplete,
            vec![format!("could not inspect {}", root.display())],
            0,
        );
    };
    let completeness = report.completeness();
    let visited_entries = report.visited_entries();
    let diagnostics = report
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.kind() != WalkDiagnosticKind::DepthLimit)
        .map(|diagnostic| format!("{}: {}", diagnostic.path().display(), diagnostic.message()))
        .collect();

    for path in report.files() {
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if is_structural_file_name(&file_name.to_ascii_lowercase()) {
            structural_files.push(path.clone());
            continue;
        }
        let size_bytes = path.metadata().map(|m| m.len()).unwrap_or(0);
        let relative_path = relative_path_from(root, path);
        let depth = Path::new(&relative_path)
            .parent()
            .map(|parent| parent.components().count() as u32)
            .unwrap_or(0);
        let file_name_no_ext = file_name
            .rsplit_once('.')
            .map(|(stem, _)| stem.to_owned())
            .unwrap_or_else(|| file_name.to_owned());

        out.push(RawCandidate {
            absolute_path: path.clone(),
            relative_path,
            file_name: file_name.to_owned(),
            file_name_no_ext,
            size_bytes,
            depth,
        });
    }
    (
        out,
        structural_files,
        completeness,
        diagnostics,
        visited_entries,
    )
}

fn is_structural_file_name(lower_file_name: &str) -> bool {
    [".dll", ".pak", ".utoc", ".ucas", ".archive", ".bundle"]
        .iter()
        .any(|extension| lower_file_name.ends_with(extension))
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

/// Returns the parent-directory segment of `relative_path` (forward-slash,
/// relative to the install root) that matches a known installer/redist folder,
/// case-insensitively. The final segment (the file name) is never considered.
fn non_game_dir_segment(relative_path: &str) -> Option<String> {
    let mut segments: Vec<&str> = relative_path.split('/').collect();
    segments.pop(); // drop the file name; only parent folders count
    segments.into_iter().find_map(|segment| {
        let lower = segment.to_ascii_lowercase();
        NON_GAME_DIR_SEGMENTS
            .iter()
            .find(|&&dir| dir == lower)
            .map(|&dir| dir.to_owned())
    })
}

fn classify(relative_path: &str, name_no_ext: &str, full_name: &str) -> Option<RejectionReason> {
    // A parent installer/redist folder rejects the binary regardless of its name —
    // the strongest signal that an executable is not the game.
    if let Some(segment) = non_game_dir_segment(relative_path) {
        return Some(RejectionReason::NonGameLocation(segment));
    }

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

/// Normalizes a name for tolerant comparison: keeps only ASCII alphanumerics,
/// lowercased. So `"Cyberpunk 2077"` and `"Cyberpunk2077"` compare equal — matching
/// how a launcher names an install folder versus the game's binary. Mirrors NVIDIA
/// Profile Inspector's `NormalizeName`.
fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// True when one normalized name contains the other and the contained name is
/// substantial enough to be a meaningful signal (avoids matching on a 2–3 char
/// fragment). Backs the partial folder-name bonus.
fn folder_name_overlaps(stem: &str, dir: &str) -> bool {
    const MIN_OVERLAP: usize = 4;
    (dir.len() >= MIN_OVERLAP && stem.contains(dir))
        || (stem.len() >= MIN_OVERLAP && dir.contains(stem))
}

fn compute_rank_score(raw: &RawCandidate, install_dir_name: &str) -> i32 {
    let mut score: i32 = 0;

    if raw.depth == 0 {
        score += ROOT_DEPTH_BONUS;
    } else if raw.depth == 1 {
        score += NEAR_ROOT_DEPTH_BONUS;
    }

    let normalized_dir = normalize_name(install_dir_name);
    let normalized_stem = normalize_name(&raw.file_name_no_ext);
    if !normalized_dir.is_empty() && !normalized_stem.is_empty() {
        if normalized_stem == normalized_dir {
            score += FOLDER_NAME_MATCH_BONUS;
        } else if folder_name_overlaps(&normalized_stem, &normalized_dir) {
            score += FOLDER_NAME_PARTIAL_BONUS;
        }
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
    fn normalized_folder_name_match_tolerates_spacing_and_punctuation() {
        let tmp = TempDir::new().unwrap();
        // Folder carries a space the binary omits; the exact (pre-normalization)
        // comparison would miss this, the normalized one must not.
        let game_dir = tmp.path().join("Cyberpunk 2077");
        fs::create_dir_all(&game_dir).unwrap();
        write_file(&game_dir.join("Cyberpunk2077.exe"), &[0u8; 1024]);
        write_file(&game_dir.join("RandomOther.exe"), &[0u8; 1024]);

        let results = detect_executable_candidates(&game_dir);
        let kept: Vec<&ExecutableCandidate> =
            results.iter().filter(|c| c.rejection.is_none()).collect();
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].file_name, "Cyberpunk2077.exe");
    }

    #[test]
    fn rejects_executables_inside_installer_and_redist_folders() {
        let tmp = TempDir::new().unwrap();
        // The real game binary sits deep under the engine tree; installer/redist
        // helpers sit in well-known folders and must be filtered by location even
        // when their name (e.g. `Cleanup.exe`, `vc_redist.x64.exe`) passes the
        // filename filters.
        write_file(
            &tmp.path().join("SwGame/Binaries/Win64/JediSurvivor.exe"),
            &[0u8; 1024],
        );
        write_file(&tmp.path().join("__Installer/Cleanup.exe"), &[0u8; 1024]);
        write_file(
            &tmp.path()
                .join("Engine/Extras/Redist/en-us/UEPrereqSetup_x64.exe"),
            &[0u8; 1024],
        );

        let results = detect_executable_candidates(tmp.path());
        let kept: Vec<&str> = results
            .iter()
            .filter(|c| c.rejection.is_none())
            .map(|c| c.file_name.as_str())
            .collect();
        assert_eq!(kept, ["JediSurvivor.exe"]);

        let cleanup = results
            .iter()
            .find(|c| c.file_name == "Cleanup.exe")
            .expect("cleanup present");
        assert_eq!(
            cleanup.rejection.as_ref().map(RejectionReason::kind),
            Some("non_game_location")
        );
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
    fn pe_validation_rejects_extension_only_and_accepts_signatures() {
        let tmp = TempDir::new().unwrap();
        let fake = tmp.path().join("Fake.exe");
        write_file(&fake, b"not a PE");
        assert!(!is_readable_windows_pe_executable(&fake));

        let pe = tmp.path().join("Game.exe");
        let mut bytes = vec![0_u8; 0x84];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        write_file(&pe, &bytes);
        assert!(is_readable_windows_pe_executable(&pe));
    }

    #[test]
    fn intentional_probe_depth_limit_is_not_a_filesystem_diagnostic() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("a/b/c/d/e")).expect("deep directory");

        let report = inspect_executable_candidates(tmp.path());

        assert_eq!(report.completeness(), InstallTreeCompleteness::Incomplete);
        assert!(
            report.diagnostics().is_empty(),
            "a bounded advisory probe is not an access failure"
        );
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
