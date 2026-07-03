//! The single source of truth for a game's primary executable.
//!
//! NVAPI needs the game's profile target and RenoDX needs the renderer (the
//! install location plus the graphics API). In practice these are the same
//! binary, so this module resolves it once for both — and for the UI's manual
//! override — so they never disagree on which exe is "the game".
//!
//! Resolution priority:
//!   1. A manual override pinned by the user (highest — the user is always right).
//!   2. Otherwise the highest **combined score** over the install directory's
//!      scanned executables: a graphics importer (a real renderer, not a launcher
//!      stub) wins, then a match against the launcher's recorded launch executable,
//!      then DirectX (RenoDX only), then a known architecture, then a filesystem
//!      rank.
//!
//! The launcher's launch exe is a strong *signal*, not an override, because it is
//! not always the renderer: a launcher-wrapped game lists a stub, and a
//! dynamic-D3D game imports nothing. Combining the signals is robust where any one
//! alone is not — a normal game's launch exe also renders (wins on both), a
//! launcher stub loses to the importing renderer, and when nothing imports
//! graphics the launch exe still wins on the authoritative-match term.
//!
//! Detection (folder scan, PE import reading, launcher metadata) is Windows-only;
//! the pure scoring is shared and platform-agnostic (a non-Windows stub returns
//! `None`).

use std::path::Path;

use renderpilot_domain::{ExeGraphicsInfo, GraphicsApi, PathRef};

/// Where a resolved executable came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExeSource {
    /// Pinned by the user via the manual override.
    Override,
    /// Auto-detected by the scoring below.
    Auto,
}

/// A game's resolved primary executable: the file, its detected graphics, and how
/// it was chosen.
#[derive(Debug, Clone)]
pub struct ResolvedExecutable {
    /// Absolute path on disk (forward slashes).
    pub path: PathRef,
    /// Basename (e.g. `Game.exe`) — NVAPI's profile key.
    pub file_name: String,
    /// Detected graphics APIs + architecture of the chosen binary.
    pub graphics: ExeGraphicsInfo,
    /// Whether this came from the user's override or auto-detection.
    pub source: ExeSource,
}

// -----------------------------------------------------------------------------
// Scoring (pure, platform-agnostic)
// -----------------------------------------------------------------------------

/// A binary that imports any known graphics API is a renderer, not a
/// launcher/installer/helper. Dominates every other term.
const RENDERS_GRAPHICS: i64 = 1_000_000;
/// A candidate whose name equals the launcher's recorded launch executable. The
/// launcher's own truth outranks the DirectX preference heuristic.
const AUTHORITATIVE_MATCH: i64 = 100_000;
/// Score bump (RenoDX only) for a Direct3D importer when several renderers tie.
const PREFERS_DIRECTX: i64 = 10_000;
/// A binary whose architecture could be read (a valid PE).
const ARCHITECTURE_KNOWN: i64 = 100;

/// Combined primary-renderer score for one candidate; higher wins.
///
/// `fs_rank` is the filesystem-heuristic score (root proximity, normalized
/// folder-name match, size — see `executable_detection`), used only as the final
/// tiebreak. `authoritative_match` is set when the candidate's name equals the
/// launcher's recorded launch exe. Set `prefer_directx` for RenoDX, clear it for
/// NVAPI (a Vulkan game is still the game).
#[must_use]
pub fn primary_score(
    fs_rank: i32,
    graphics: &ExeGraphicsInfo,
    prefer_directx: bool,
    authoritative_match: bool,
) -> i64 {
    let renders = !graphics.apis().is_empty();
    let is_directx = graphics.apis().iter().any(|api| {
        matches!(
            api,
            GraphicsApi::D3D9 | GraphicsApi::D3D10 | GraphicsApi::D3D11 | GraphicsApi::D3D12
        )
    });
    let arch_known = graphics.architecture().is_some();

    i64::from(renders) * RENDERS_GRAPHICS
        + i64::from(authoritative_match) * AUTHORITATIVE_MATCH
        + if prefer_directx && is_directx {
            PREFERS_DIRECTX
        } else {
            0
        }
        + i64::from(arch_known) * ARCHITECTURE_KNOWN
        + i64::from(fs_rank)
}

// -----------------------------------------------------------------------------
// Resolution (Windows-only; non-Windows stub)
// -----------------------------------------------------------------------------

/// Resolves a game's primary executable from its install directory.
///
/// An existing `override_path` wins outright; otherwise the best-scoring scanned
/// executable is returned (or `None` when the directory holds no game binary).
#[cfg(windows)]
#[must_use]
pub fn resolve_primary_executable(
    install_dir: &Path,
    override_path: Option<&Path>,
    prefer_directx: bool,
) -> Option<ResolvedExecutable> {
    use renderpilot_detection::analyze_executable;
    use renderpilot_platform_windows::{detect_executable_candidates, launcher_launch_executable};

    if let Some(over) = override_path.filter(|path| path.exists())
        && let Ok(path) = PathRef::new(to_forward_slashes(over))
    {
        return Some(ResolvedExecutable {
            file_name: file_name_of(over),
            graphics: analyze_executable(over),
            path,
            source: ExeSource::Override,
        });
    }

    let launch_exe = launcher_launch_executable(install_dir);
    detect_executable_candidates(install_dir)
        .into_iter()
        .filter(|candidate| candidate.rejection.is_none())
        .filter_map(|candidate| {
            let path = PathRef::new(to_forward_slashes(&candidate.absolute_path)).ok()?;
            let graphics = analyze_executable(&candidate.absolute_path);
            let authoritative = launch_exe
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(&candidate.file_name));
            let score = primary_score(
                candidate.rank_score,
                &graphics,
                prefer_directx,
                authoritative,
            );
            Some((
                score,
                ResolvedExecutable {
                    path,
                    file_name: candidate.file_name,
                    graphics,
                    source: ExeSource::Auto,
                },
            ))
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, resolved)| resolved)
}

/// Non-Windows stub: executable resolution relies on Windows-only detection.
#[cfg(not(windows))]
#[must_use]
pub fn resolve_primary_executable(
    _install_dir: &Path,
    _override_path: Option<&Path>,
    _prefer_directx: bool,
) -> Option<ResolvedExecutable> {
    None
}

#[cfg(windows)]
fn to_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(windows)]
fn file_name_of(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_domain::Architecture;

    fn info(apis: &[GraphicsApi], arch: Option<Architecture>) -> ExeGraphicsInfo {
        ExeGraphicsInfo::new(apis.to_vec(), arch)
    }

    #[test]
    fn a_renderer_outranks_a_non_renderer_regardless_of_fs_rank() {
        let renderer = primary_score(
            0,
            &info(&[GraphicsApi::D3D12], Some(Architecture::X64)),
            true,
            false,
        );
        let launcher = primary_score(60, &info(&[], None), true, false);
        assert!(renderer > launcher);
    }

    #[test]
    fn authoritative_match_breaks_a_tie_between_two_renderers() {
        // Two DirectX renderers; the one the launcher actually runs wins.
        let authoritative = primary_score(
            0,
            &info(&[GraphicsApi::D3D12], Some(Architecture::X64)),
            true,
            true,
        );
        let other = primary_score(
            20,
            &info(&[GraphicsApi::D3D12], Some(Architecture::X64)),
            true,
            false,
        );
        assert!(authoritative > other);
    }

    #[test]
    fn an_importing_renderer_beats_an_authoritative_launcher_stub() {
        // Launcher-wrapped game: the launch exe is a stub that imports nothing; the
        // real renderer (imports graphics) must still win.
        let stub = primary_score(20, &info(&[], None), true, true);
        let renderer = primary_score(
            0,
            &info(&[GraphicsApi::D3D11], Some(Architecture::X64)),
            true,
            false,
        );
        assert!(renderer > stub);
    }

    #[test]
    fn authoritative_exe_wins_when_nothing_imports_graphics() {
        // Fully-dynamic-D3D game: no candidate imports graphics, so the launcher's
        // recorded launch exe is the best signal.
        let launch = primary_score(0, &info(&[], Some(Architecture::X64)), true, true);
        let other = primary_score(20, &info(&[], Some(Architecture::X64)), true, false);
        assert!(launch > other);
    }

    #[test]
    fn renodx_prefers_directx_over_vulkan_when_both_render() {
        let dx = primary_score(
            0,
            &info(&[GraphicsApi::D3D11], Some(Architecture::X64)),
            true,
            false,
        );
        let vk = primary_score(
            0,
            &info(&[GraphicsApi::Vulkan], Some(Architecture::X64)),
            true,
            false,
        );
        assert!(dx > vk);
    }

    #[test]
    fn nvapi_does_not_demote_a_vulkan_game_for_a_directx_helper() {
        let vulkan_game = primary_score(
            20,
            &info(&[GraphicsApi::Vulkan], Some(Architecture::X64)),
            false,
            false,
        );
        let dx_helper = primary_score(
            0,
            &info(&[GraphicsApi::D3D11], Some(Architecture::X64)),
            false,
            false,
        );
        assert!(vulkan_game > dx_helper);
    }
}
