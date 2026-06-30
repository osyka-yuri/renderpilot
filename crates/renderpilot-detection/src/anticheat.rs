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
    /// Number of directory entries inspected.
    pub scanned_entry_count: usize,
    /// Whether the scan stopped because [`AntiCheatScanOptions::max_entries`] was reached.
    pub truncated: bool,
}

impl AntiCheatScanReport {
    fn empty() -> Self {
        Self {
            engines: Vec::new(),
            evidence: Vec::new(),
            scanned_entry_count: 0,
            truncated: false,
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
    /// Maximum number of directory entries to inspect before stopping.
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
/// Unreadable folders and entries are skipped. Symlinks/reparse-like entries are
/// not followed, so a scan cannot wander out of the game install tree.
#[must_use]
pub fn scan_anticheat_with_options(
    game_dir: &Path,
    options: AntiCheatScanOptions,
) -> AntiCheatScanReport {
    let mut report = AntiCheatScanReport::empty();
    let mut queue = VecDeque::new();
    queue.push_back(game_dir.to_path_buf());

    while let Some(dir) = queue.pop_front() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut entries = entries.flatten().collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
        for entry in entries {
            if report.scanned_entry_count >= options.max_entries {
                report.truncated = true;
                return report;
            }
            report.scanned_entry_count += 1;

            let Some(scanned) = scan_entry(&entry) else {
                continue;
            };
            if let Some(evidence) = scanned.evidence {
                report.push_evidence(evidence);
            }
            if scanned.descend {
                queue.push_back(scanned.path);
            }
        }
    }

    report
}

struct ScannedEntry {
    path: PathBuf,
    evidence: Option<AntiCheatEvidence>,
    descend: bool,
}

fn scan_entry(entry: &DirEntry) -> Option<ScannedEntry> {
    let path = entry.path();
    let metadata = fs::symlink_metadata(&path).ok()?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return None;
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

    Some(ScannedEntry {
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
