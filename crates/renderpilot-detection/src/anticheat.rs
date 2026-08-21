//! Filesystem heuristics for detecting anti-cheat runtimes in a game install.
//!
//! This module is intentionally product-agnostic: it reports engines and
//! filesystem evidence only. Callers decide whether a detected engine is a
//! warning, a hard block, or merely an informational signal.

use std::collections::VecDeque;
use std::fs::{self, DirEntry};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Default upper bound on filesystem entries walked during an anti-cheat scan.
pub const DEFAULT_ANTICHEAT_MAX_SCANNED_ENTRIES: usize = 8192;

/// Anti-cheat engine identified from filesystem evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AntiCheatEngine {
    /// Easy Anti-Cheat.
    EasyAntiCheat,
    /// BattlEye.
    BattlEye,
}

/// Filesystem object kind for an anti-cheat marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AntiCheatEvidenceKind {
    /// Marker is a regular file.
    File,
    /// Marker is a directory.
    Directory,
    /// Marker is neither a regular file nor a directory.
    Other,
}

/// Completeness of a bounded anti-cheat filesystem scan.
///
/// `Limited` is deliberately conservative: it is returned when traversal hit
/// the configured entry bound or when any directory/entry could not be read.
/// An empty engine list is therefore never interpreted as proof that a game is
/// safe to modify.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AntiCheatScanCompleteness {
    /// Every visited directory and entry was observed within the configured bound.
    #[default]
    Complete,
    /// The scan could not fully observe the requested tree.
    Limited,
}

/// One anti-cheat marker found in a game folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AntiCheatEvidence {
    /// Engine identified by this marker.
    pub engine: AntiCheatEngine,
    /// Lower-cased marker name matched by the detector.
    pub matched_marker: String,
    /// Full path to the marker.
    pub path: PathBuf,
    /// Filesystem object kind for the marker.
    pub kind: AntiCheatEvidenceKind,
}

/// Result of scanning a game folder for anti-cheat markers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AntiCheatScanReport {
    /// Distinct engines found in first-seen order.
    pub engines: Vec<AntiCheatEngine>,
    /// Evidence entries found during the scan.
    pub evidence: Vec<AntiCheatEvidence>,
    /// Number of directory entries and iterator error records inspected.
    pub scanned_entry_count: usize,
    /// Whether the scan stopped because [`AntiCheatScanOptions::max_entries`] was reached.
    pub truncated: bool,
    /// Directories whose contents could not be enumerated.
    #[serde(default)]
    pub unreadable_directories: Vec<PathBuf>,
    /// Parent directories for directory entries whose metadata could not be read.
    /// Paths are sorted and deduplicated so they are stable fingerprint inputs.
    #[serde(default)]
    pub unreadable_entries: Vec<PathBuf>,
    /// Number of unreadable directory-entry records observed. This preserves
    /// multiplicity even when several iterator errors share one parent path.
    #[serde(default)]
    pub unreadable_entry_count: usize,
    /// Explicit conservative completeness classification.
    #[serde(default)]
    pub completeness: AntiCheatScanCompleteness,
}

impl AntiCheatScanReport {
    fn empty() -> Self {
        Self {
            engines: Vec::new(),
            evidence: Vec::new(),
            scanned_entry_count: 0,
            truncated: false,
            unreadable_directories: Vec::new(),
            unreadable_entries: Vec::new(),
            unreadable_entry_count: 0,
            completeness: AntiCheatScanCompleteness::Complete,
        }
    }

    fn mark_limited(&mut self) {
        self.completeness = AntiCheatScanCompleteness::Limited;
    }

    fn finish(&mut self) {
        self.unreadable_directories.sort();
        self.unreadable_directories.dedup();
        self.unreadable_entries.sort();
        self.unreadable_entries.dedup();
        if self.truncated
            || !self.unreadable_directories.is_empty()
            || !self.unreadable_entries.is_empty()
        {
            self.completeness = AntiCheatScanCompleteness::Limited;
        }
    }

    fn push_evidence(&mut self, evidence: AntiCheatEvidence) {
        if !self.engines.contains(&evidence.engine) {
            self.engines.push(evidence.engine);
        }
        self.evidence.push(evidence);
    }
}

/// Runtime options for anti-cheat filesystem detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AntiCheatScanOptions {
    /// Maximum number of directory entry records (including iterator errors)
    /// to inspect before stopping.
    pub max_entries: usize,
}

impl Default for AntiCheatScanOptions {
    fn default() -> Self {
        Self {
            max_entries: DEFAULT_ANTICHEAT_MAX_SCANNED_ENTRIES,
        }
    }
}

/// File and directory names (lowercased) that indicate Easy Anti-Cheat.
const EAC_MARKERS: &[&str] = &[
    "easyanticheat",
    "easyanticheat_eos",
    "easyanticheat_x64.dll",
    "easyanticheat_x86.dll",
    "eaclauncher.exe",
    "start_protected_game.exe",
];

/// File and directory names (lowercased) that indicate BattlEye.
const BATTLEYE_MARKERS: &[&str] = &[
    "battleye",
    "beservice.exe",
    "beservice_x64.dll",
    "beclient_x64.dll",
    "beclient_x86.dll",
];

/// Scans `game_dir` for known anti-cheat artifacts using default options.
#[must_use]
pub fn scan_anticheat(game_dir: &Path) -> AntiCheatScanReport {
    scan_anticheat_with_options(game_dir, AntiCheatScanOptions::default())
}

/// Scans `game_dir` for known anti-cheat artifacts.
///
/// The walk is breadth-first and bounded by [`AntiCheatScanOptions::max_entries`].
/// Unreadable folders and entries are recorded and make the result limited.
/// Symlinks/reparse-like entries are not followed, so a scan cannot wander out
/// of the game install tree.
#[must_use]
pub fn scan_anticheat_with_options(
    game_dir: &Path,
    options: AntiCheatScanOptions,
) -> AntiCheatScanReport {
    let mut report = AntiCheatScanReport::empty();
    let mut queue = VecDeque::new();
    queue.push_back(game_dir.to_path_buf());

    while let Some(dir) = queue.pop_front() {
        if report.scanned_entry_count >= options.max_entries {
            report.truncated = true;
            report.mark_limited();
            report.finish();
            return report;
        }

        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => {
                report.unreadable_directories.push(dir);
                report.mark_limited();
                continue;
            }
        };
        let mut entries_within_bound = Vec::new();
        let mut hit_entry_bound = false;
        for entry in entries {
            if report.scanned_entry_count >= options.max_entries {
                report.truncated = true;
                report.mark_limited();
                hit_entry_bound = true;
                break;
            }

            report.scanned_entry_count += 1;
            match entry {
                Ok(entry) => entries_within_bound.push(entry),
                Err(_) => {
                    report.unreadable_entries.push(dir.clone());
                    report.unreadable_entry_count += 1;
                    report.mark_limited();
                }
            }
        }
        entries_within_bound.sort_by(|left, right| {
            left.file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .cmp(&right.file_name().to_string_lossy().to_ascii_lowercase())
                .then_with(|| {
                    left.file_name()
                        .to_string_lossy()
                        .cmp(&right.file_name().to_string_lossy())
                })
        });
        for entry in entries_within_bound {
            let scanned = match scan_entry(&entry) {
                Ok(scanned) => scanned,
                Err(_) => {
                    report.unreadable_entries.push(dir.clone());
                    report.unreadable_entry_count += 1;
                    report.mark_limited();
                    continue;
                }
            };
            if let Some(evidence) = scanned.evidence {
                report.push_evidence(evidence);
            }
            if scanned.descend {
                queue.push_back(scanned.path);
            }
        }

        if hit_entry_bound {
            report.finish();
            return report;
        }
    }

    report.finish();
    report
}

struct ScannedEntry {
    path: PathBuf,
    evidence: Option<AntiCheatEvidence>,
    descend: bool,
}

fn scan_entry(entry: &DirEntry) -> std::io::Result<ScannedEntry> {
    let path = entry.path();
    let metadata = fs::symlink_metadata(&path)?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Ok(ScannedEntry {
            path,
            evidence: None,
            descend: false,
        });
    }

    let kind = if file_type.is_dir() {
        AntiCheatEvidenceKind::Directory
    } else if file_type.is_file() {
        AntiCheatEvidenceKind::File
    } else {
        AntiCheatEvidenceKind::Other
    };
    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
    let engine = marker_engine(&name);
    let evidence = engine.map(|engine| AntiCheatEvidence {
        engine,
        matched_marker: name,
        path: path.clone(),
        kind,
    });

    Ok(ScannedEntry {
        path,
        evidence,
        descend: file_type.is_dir(),
    })
}

fn marker_engine(name: &str) -> Option<AntiCheatEngine> {
    if EAC_MARKERS.contains(&name) {
        return Some(AntiCheatEngine::EasyAntiCheat);
    }
    if BATTLEYE_MARKERS.contains(&name) {
        return Some(AntiCheatEngine::BattlEye);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn detects_easy_anticheat_directory_and_file_markers() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("EasyAntiCheat")).expect("mkdir");
        fs::write(dir.path().join("EasyAntiCheat_x64.dll"), b"stub").expect("write");

        let report = scan_anticheat(dir.path());

        assert_eq!(report.engines, vec![AntiCheatEngine::EasyAntiCheat]);
        assert_eq!(report.evidence.len(), 2);
        assert!(
            report
                .evidence
                .iter()
                .any(|evidence| evidence.kind == AntiCheatEvidenceKind::Directory)
        );
        assert!(
            report
                .evidence
                .iter()
                .any(|evidence| evidence.kind == AntiCheatEvidenceKind::File)
        );
    }

    #[test]
    fn detects_battleye_service_in_nested_folder() {
        let dir = tempdir().expect("tempdir");
        let sub = dir.path().join("bin");
        fs::create_dir(&sub).expect("mkdir");
        fs::write(sub.join("BEService_x64.dll"), b"stub").expect("write");

        let report = scan_anticheat(dir.path());

        assert_eq!(report.engines, vec![AntiCheatEngine::BattlEye]);
        assert_eq!(report.evidence.len(), 1);
        assert_eq!(
            report.evidence[0].matched_marker,
            "beservice_x64.dll".to_owned()
        );
    }

    #[test]
    fn collects_multiple_engines_and_evidence_entries() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("EasyAntiCheat")).expect("mkdir");
        fs::create_dir(dir.path().join("BattlEye")).expect("mkdir");

        let report = scan_anticheat(dir.path());

        assert_eq!(report.engines.len(), 2);
        assert!(report.engines.contains(&AntiCheatEngine::EasyAntiCheat));
        assert!(report.engines.contains(&AntiCheatEngine::BattlEye));
        assert_eq!(report.evidence.len(), 2);
    }

    #[test]
    fn clean_folder_reports_no_engines() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("game.exe"), b"stub").expect("write");

        let report = scan_anticheat(dir.path());

        assert!(report.engines.is_empty());
        assert!(report.evidence.is_empty());
        assert!(!report.truncated);
        assert_eq!(report.completeness, AntiCheatScanCompleteness::Complete);
        assert!(report.unreadable_directories.is_empty());
        assert!(report.unreadable_entries.is_empty());
    }

    #[test]
    fn marks_scan_truncated_at_entry_cap() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("game.exe"), b"stub").expect("write");
        fs::write(dir.path().join("EasyAntiCheat_x64.dll"), b"stub").expect("write");

        let report =
            scan_anticheat_with_options(dir.path(), AntiCheatScanOptions { max_entries: 1 });

        assert_eq!(report.scanned_entry_count, 1);
        assert!(report.truncated);
        assert_eq!(report.completeness, AntiCheatScanCompleteness::Limited);
    }

    #[test]
    fn zero_entry_bound_is_limited_without_reading_entries() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("EasyAntiCheat_x64.dll"), b"stub").expect("write");

        let report =
            scan_anticheat_with_options(dir.path(), AntiCheatScanOptions { max_entries: 0 });

        assert_eq!(report.scanned_entry_count, 0);
        assert!(report.truncated);
        assert_eq!(report.completeness, AntiCheatScanCompleteness::Limited);
        assert!(report.evidence.is_empty());
    }

    #[test]
    fn wide_directory_is_bounded_while_reading_and_remains_deterministic() {
        let dir = tempdir().expect("tempdir");
        for index in 0..4096 {
            fs::write(dir.path().join(format!("entry_{index:04}.bin")), b"stub").expect("write");
        }

        let options = AntiCheatScanOptions { max_entries: 8 };
        let first = scan_anticheat_with_options(dir.path(), options);
        let second = scan_anticheat_with_options(dir.path(), options);

        assert_eq!(first, second);
        assert_eq!(first.scanned_entry_count, options.max_entries);
        assert!(first.truncated);
        assert_eq!(first.completeness, AntiCheatScanCompleteness::Limited);
        assert!(first.evidence.is_empty());
    }

    #[test]
    fn missing_root_is_an_explicit_limited_observation() {
        let dir = tempdir().expect("tempdir");
        let missing = dir.path().join("missing");

        let report = scan_anticheat(&missing);

        assert_eq!(report.engines, Vec::<AntiCheatEngine>::new());
        assert_eq!(report.completeness, AntiCheatScanCompleteness::Limited);
        assert_eq!(report.unreadable_directories, vec![missing]);
        assert!(report.unreadable_entries.is_empty());
        assert_eq!(report.unreadable_entry_count, 0);
    }

    #[test]
    fn skips_symlinked_directories() {
        let dir = tempdir().expect("tempdir");
        let external = tempdir().expect("external tempdir");
        fs::create_dir(external.path().join("EasyAntiCheat")).expect("mkdir");
        let link = dir.path().join("linked");

        if create_dir_symlink(external.path(), &link).is_err() {
            return;
        }

        let report = scan_anticheat(dir.path());

        assert!(report.engines.is_empty());
        assert!(report.evidence.is_empty());
    }

    #[cfg(unix)]
    fn create_dir_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(target, link)
    }

    #[cfg(windows)]
    fn create_dir_symlink(target: &Path, link: &Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_dir(target, link)
    }
}
