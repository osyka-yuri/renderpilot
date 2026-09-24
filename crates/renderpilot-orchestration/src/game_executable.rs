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

pub(crate) fn is_existing_executable_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
}

// -----------------------------------------------------------------------------
// Scoring (pure, platform-agnostic)
// -----------------------------------------------------------------------------

/// A binary that imports any known graphics API is a renderer, not a
/// launcher/installer/helper. Dominates every other term.
const RENDERS_GRAPHICS: i64 = 1_000_000;
/// A binary with a shipping configuration suffix bound to the authoritative
/// launcher executable (e.g. `FactoryGameSteam-Win64-Shipping.exe` for
/// launcher `FactoryGameSteam.exe`), representing a modular or production engine
/// renderer rather than a launcher/bootstrap stub. Outranks launcher stubs and
/// filesystem proximity when graphics imports are modular or dynamic.
const BOUND_SHIPPING_RENDERER: i64 = 500_000;
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
/// NVAPI (a Vulkan game is still the game). `is_bound_shipping` indicates a production
/// shipping configuration binary whose base stem matches the authoritative launcher
/// executable (e.g. `FactoryGameSteam-Win64-Shipping.exe` for launcher `FactoryGameSteam.exe`).
#[must_use]
pub fn primary_score(
    fs_rank: i32,
    graphics: &ExeGraphicsInfo,
    prefer_directx: bool,
    authoritative_match: bool,
    is_bound_shipping: bool,
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
        + i64::from(is_bound_shipping) * BOUND_SHIPPING_RENDERER
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
#[must_use]
pub fn resolve_primary_executable(
    install_dir: &Path,
    override_path: Option<&Path>,
    prefer_directx: bool,
) -> Option<ResolvedExecutable> {
    use renderpilot_detection::analyze_executable;
    use renderpilot_platform_windows::{detect_executable_candidates, is_bound_shipping_target};

    if let Some(over) = override_path.filter(|path| is_existing_executable_file(path)) {
        // Older catalog rows may retain a verbatim path written before the
        // selector began storing normal DOS/UNC spellings. DRS CreateApplication
        // rejects the `\\?\` drive form, so normalize the shared selection here
        // as well as when new overrides are written. Unsupported device
        // namespaces fail closed instead of silently choosing an auto candidate.
        let over = crate::paths::strip_windows_verbatim_prefix(over).ok()?;
        let path = PathRef::new(to_forward_slashes(&over)).ok()?;
        return Some(ResolvedExecutable {
            file_name: file_name_of(&over),
            graphics: analyze_executable(&over),
            path,
            source: ExeSource::Override,
        });
    }

    #[cfg(windows)]
    let launch_exe = renderpilot_platform_windows::launcher_launch_executable(install_dir);
    #[cfg(not(windows))]
    let launch_exe: Option<String> = None;

    let candidates: Vec<_> = detect_executable_candidates(install_dir)
        .into_iter()
        .filter(|candidate| candidate.rejection.is_none())
        .collect();

    let root_bootstrap_names: Vec<String> = candidates
        .iter()
        .filter(|c| {
            c.depth == 0
                && renderpilot_platform_windows::strip_shipping_suffix(&c.file_name).is_none()
        })
        .map(|c| c.file_name.clone())
        .collect();

    candidates
        .into_iter()
        .filter_map(|candidate| {
            let absolute_path =
                crate::paths::strip_windows_verbatim_prefix(&candidate.absolute_path).ok()?;
            let path = PathRef::new(to_forward_slashes(&absolute_path)).ok()?;
            let graphics = analyze_executable(&absolute_path);
            let authoritative = launch_exe
                .as_deref()
                .is_some_and(|name| name.eq_ignore_ascii_case(&candidate.file_name));
            let is_bound_to_launcher = launch_exe
                .as_deref()
                .is_some_and(|launcher| is_bound_shipping_target(&candidate.file_name, launcher));
            let is_bound_to_root = is_expected_renderer_topology(&candidate.relative_path)
                && root_bootstrap_names
                    .iter()
                    .any(|root_name| is_bound_shipping_target(&candidate.file_name, root_name));
            let is_bound_shipping = is_bound_to_launcher || is_bound_to_root;
            let score = primary_score(
                candidate.rank_score,
                &graphics,
                prefer_directx,
                authoritative,
                is_bound_shipping,
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

/// Returns whether `relative_path` matches the expected renderer topology for nested shipping binaries.
///
/// Specifically, the binary must reside in a `Binaries/Win64` or `Binaries/Win32` directory
/// (case-insensitive), e.g. `Engine/Binaries/Win64/FactoryGameSteam-Win64-Shipping.exe` or
/// `Binaries/Win64/Game-Win64-Shipping.exe`.
///
/// Unrelated directories such as `Tools/Game-Shipping.exe` return `false` to prevent arbitrary
/// tool executables from hijacking the primary executable when launcher metadata is absent.
fn is_expected_renderer_topology(relative_path: &str) -> bool {
    let path = Path::new(relative_path);
    let Some(parent) = path.parent() else {
        return false;
    };
    let Some(arch_dir) = parent.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if !arch_dir.eq_ignore_ascii_case("Win64") && !arch_dir.eq_ignore_ascii_case("Win32") {
        return false;
    }
    let Some(binaries_dir) = parent
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
    else {
        return false;
    };
    binaries_dir.eq_ignore_ascii_case("Binaries")
}

fn to_forward_slashes(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

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
            false,
        );
        let launcher = primary_score(60, &info(&[], None), true, false, false);
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
            false,
        );
        let other = primary_score(
            20,
            &info(&[GraphicsApi::D3D12], Some(Architecture::X64)),
            true,
            false,
            false,
        );
        assert!(authoritative > other);
    }

    #[test]
    fn an_importing_renderer_beats_an_authoritative_launcher_stub() {
        // Launcher-wrapped game: the launch exe is a stub that imports nothing; the
        // real renderer (imports graphics) must still win.
        let stub = primary_score(20, &info(&[], None), true, true, false);
        let renderer = primary_score(
            0,
            &info(&[GraphicsApi::D3D11], Some(Architecture::X64)),
            true,
            false,
            false,
        );
        assert!(renderer > stub);
    }

    #[test]
    fn authoritative_exe_wins_when_nothing_imports_graphics() {
        // Fully-dynamic-D3D game: no candidate imports graphics, so the launcher's
        // recorded launch exe is the best signal.
        let launch = primary_score(0, &info(&[], Some(Architecture::X64)), true, true, false);
        let other = primary_score(20, &info(&[], Some(Architecture::X64)), true, false, false);
        assert!(launch > other);
    }

    #[test]
    fn bound_shipping_renderer_beats_authoritative_launcher_stub_when_graphics_are_modular() {
        // When a shipping binary is bound to the authoritative launcher
        // (e.g. FactoryGameSteam-Win64-Shipping for FactoryGameSteam),
        // it represents the actual modular engine renderer and outranks the launcher stub.
        let authoritative_stub =
            primary_score(20, &info(&[], Some(Architecture::X64)), true, true, false);
        let bound_shipping_binary =
            primary_score(25, &info(&[], Some(Architecture::X64)), true, false, true);
        assert!(bound_shipping_binary > authoritative_stub);
    }

    #[test]
    fn authoritative_launcher_beats_unrelated_shipping_binary() {
        // Game.exe authoritative + Unrelated-Shipping.exe => Game.exe
        let authoritative_launcher =
            primary_score(20, &info(&[], Some(Architecture::X64)), true, true, false);
        let unrelated_shipping =
            primary_score(25, &info(&[], Some(Architecture::X64)), true, false, false);
        assert!(
            authoritative_launcher > unrelated_shipping,
            "Authoritative launcher must beat unrelated shipping binary"
        );
    }

    #[test]
    fn bound_shipping_renderer_beats_authoritative_launcher_and_unrelated_shipping_helper() {
        // FactoryGameSteam.exe authoritative
        // + CrashReportClient-Win64-Shipping.exe
        // + FactoryGameSteam-Win64-Shipping.exe
        // => FactoryGameSteam-Win64-Shipping.exe
        let authoritative_launcher =
            primary_score(20, &info(&[], Some(Architecture::X64)), true, true, false);
        let crash_report_helper =
            primary_score(25, &info(&[], Some(Architecture::X64)), true, false, false);
        let bound_shipping_target =
            primary_score(25, &info(&[], Some(Architecture::X64)), true, false, true);

        assert!(
            bound_shipping_target > authoritative_launcher,
            "Bound shipping target must beat authoritative launcher"
        );
        assert!(
            authoritative_launcher > crash_report_helper,
            "Authoritative launcher must beat unrelated shipping helper"
        );
        assert!(
            bound_shipping_target > crash_report_helper,
            "Bound shipping target must beat unrelated shipping helper"
        );
    }

    #[test]
    fn renodx_prefers_directx_over_vulkan_when_both_render() {
        let dx = primary_score(
            0,
            &info(&[GraphicsApi::D3D11], Some(Architecture::X64)),
            true,
            false,
            false,
        );
        let vk = primary_score(
            0,
            &info(&[GraphicsApi::Vulkan], Some(Architecture::X64)),
            true,
            false,
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
            false,
        );
        let dx_helper = primary_score(
            0,
            &info(&[GraphicsApi::D3D11], Some(Architecture::X64)),
            false,
            false,
            false,
        );
        assert!(vulkan_game > dx_helper);
    }

    #[test]
    fn resolve_primary_executable_returns_none_for_empty_dir() {
        let temp = tempfile::tempdir().expect("tempdir");
        assert!(resolve_primary_executable(temp.path(), None, true).is_none());
    }

    #[test]
    fn resolve_primary_executable_honors_existing_override() {
        let temp = tempfile::tempdir().expect("tempdir");
        let fake_exe = temp.path().join("custom_game.exe");
        std::fs::write(&fake_exe, b"MZ").expect("write fake exe");
        let resolved = resolve_primary_executable(temp.path(), Some(&fake_exe), true);
        assert!(resolved.is_some());
        let resolved = resolved.unwrap();
        assert_eq!(resolved.file_name, "custom_game.exe");
    }

    #[test]
    fn resolve_primary_executable_ignores_existing_non_executable_overrides() {
        let temp = tempfile::tempdir().expect("tempdir");
        let directory_override = temp.path().join("folder.exe");
        std::fs::create_dir(&directory_override).expect("create directory override");
        assert!(resolve_primary_executable(temp.path(), Some(&directory_override), true).is_none());

        let file_override = temp.path().join("game.txt");
        std::fs::write(&file_override, b"not an executable").expect("write non-exe file");
        assert!(resolve_primary_executable(temp.path(), Some(&file_override), true).is_none());
    }

    #[test]
    fn executable_file_check_accepts_case_insensitive_exe_extension() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("Game.EXE");
        std::fs::write(&path, b"test executable").expect("write exe fixture");
        assert!(super::is_existing_executable_file(&path));
    }

    #[cfg(windows)]
    #[test]
    fn resolve_primary_executable_normalizes_existing_verbatim_override() {
        let temp = tempfile::tempdir().expect("tempdir");
        let fake_exe = temp.path().join("custom_game.exe");
        std::fs::write(&fake_exe, b"MZ").expect("write fake exe");
        let canonical = std::fs::canonicalize(&fake_exe).expect("canonicalize override");
        let resolved = resolve_primary_executable(temp.path(), Some(&canonical), true)
            .expect("existing override resolves");

        assert_eq!(resolved.source, ExeSource::Override);
        assert!(!resolved.path.as_str().starts_with("//?/"));
        assert_eq!(
            renderpilot_domain::normalized_path_key(resolved.path.as_str()),
            renderpilot_domain::normalized_path_key(&canonical.to_string_lossy())
        );

        let persisted_wire = canonical.to_string_lossy().replace('\\', "/");
        let persisted =
            resolve_primary_executable(temp.path(), Some(Path::new(&persisted_wire)), true)
                .expect("persisted forward-slash override resolves");
        assert_eq!(persisted.source, ExeSource::Override);
        assert_eq!(persisted.path, resolved.path);
    }

    #[test]
    fn resolve_primary_executable_promotes_bound_shipping_and_rejects_unrelated() {
        let temp = tempfile::tempdir().expect("tempdir");
        let gog_info = temp.path().join("goggame-12345.info");
        std::fs::write(
            &gog_info,
            r#"{
                "playTasks": [
                    { "type": "FileTask", "isPrimary": true, "path": "FactoryGameSteam.exe" }
                ]
            }"#,
        )
        .expect("write gog info");

        let root_launcher = temp.path().join("FactoryGameSteam.exe");
        let helper = temp
            .path()
            .join("Engine/Binaries/Win64/CrashReportClient-Win64-Shipping.exe");
        let bound_shipping = temp
            .path()
            .join("Engine/Binaries/Win64/FactoryGameSteam-Win64-Shipping.exe");

        std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
        let pe_bytes = [0u8; 1024];
        std::fs::write(&root_launcher, pe_bytes).unwrap();
        std::fs::write(&helper, pe_bytes).unwrap();
        std::fs::write(&bound_shipping, pe_bytes).unwrap();

        let resolved = resolve_primary_executable(temp.path(), None, true)
            .expect("must resolve primary executable");
        assert_eq!(
            resolved.file_name, "FactoryGameSteam-Win64-Shipping.exe",
            "Bound shipping target must win over launcher and helper"
        );
    }

    #[test]
    fn resolve_primary_executable_authoritative_launcher_wins_over_unrelated_shipping() {
        let temp = tempfile::tempdir().expect("tempdir");
        let gog_info = temp.path().join("goggame-12345.info");
        std::fs::write(
            &gog_info,
            r#"{
                "playTasks": [
                    { "type": "FileTask", "isPrimary": true, "path": "Game.exe" }
                ]
            }"#,
        )
        .expect("write gog info");

        let root_launcher = temp.path().join("Game.exe");
        let unrelated_shipping = temp.path().join("Tools/Foo-Shipping.exe");
        std::fs::create_dir_all(unrelated_shipping.parent().unwrap()).unwrap();

        let pe_bytes = [0u8; 1024];
        std::fs::write(&root_launcher, pe_bytes).unwrap();
        std::fs::write(&unrelated_shipping, pe_bytes).unwrap();

        let resolved = resolve_primary_executable(temp.path(), None, true)
            .expect("must resolve primary executable");
        assert_eq!(
            resolved.file_name, "Game.exe",
            "Authoritative launcher must win over unrelated shipping binary"
        );
    }

    #[test]
    fn resolve_primary_executable_root_bootstrap_binds_shipping_without_launcher_metadata() {
        let temp = tempfile::tempdir().expect("tempdir");
        // No launcher metadata written! (Pure filesystem discovery, like Steam)
        let root_launcher = temp.path().join("FactoryGameSteam.exe");
        let helper = temp
            .path()
            .join("Engine/Binaries/Win64/CrashReportClient-Win64-Shipping.exe");
        let bound_shipping = temp
            .path()
            .join("Engine/Binaries/Win64/FactoryGameSteam-Win64-Shipping.exe");
        let unrelated_shipping = temp.path().join("Tools/Foo-Shipping.exe");

        std::fs::create_dir_all(helper.parent().unwrap()).unwrap();
        std::fs::create_dir_all(unrelated_shipping.parent().unwrap()).unwrap();
        let pe_bytes = [0u8; 1024];
        std::fs::write(&root_launcher, pe_bytes).unwrap();
        std::fs::write(&helper, pe_bytes).unwrap();
        std::fs::write(&bound_shipping, pe_bytes).unwrap();
        std::fs::write(&unrelated_shipping, pe_bytes).unwrap();

        let resolved = resolve_primary_executable(temp.path(), None, true)
            .expect("must resolve primary executable");
        assert_eq!(
            resolved.file_name, "FactoryGameSteam-Win64-Shipping.exe",
            "Nested shipping binary bound to root launcher must win even without launcher metadata"
        );
    }

    #[test]
    fn resolve_primary_executable_tools_shipping_with_same_stem_rejected_without_launcher_metadata()
    {
        let temp = tempfile::tempdir().expect("tempdir");
        // No launcher metadata written!
        let root_launcher = temp.path().join("Game.exe");
        let tools_shipping = temp.path().join("Tools/Game-Shipping.exe");

        std::fs::create_dir_all(tools_shipping.parent().unwrap()).unwrap();
        let pe_bytes = [0u8; 1024];
        std::fs::write(&root_launcher, pe_bytes).unwrap();
        std::fs::write(&tools_shipping, pe_bytes).unwrap();

        let resolved = resolve_primary_executable(temp.path(), None, true)
            .expect("must resolve primary executable");
        assert_eq!(
            resolved.file_name, "Game.exe",
            "Root launcher must win over Tools/Game-Shipping.exe when no launcher metadata is present"
        );
    }
}
