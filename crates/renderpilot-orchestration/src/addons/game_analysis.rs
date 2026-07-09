//! Assembling [`MatchFacts`] for a game by inspecting it on disk.
//!
//! This bridges an installed game and the pure matching layer
//! ([`crate::addons::matching`]): the game's rendering executable is resolved once
//! by the shared [`game_executable`] resolver (which also
//! reads its graphics API/architecture), and the engine is detected from
//! folder/exe markers. [`assemble_facts`] is pure given the resolved executable,
//! so the matcher logic stays platform-agnostic; only the resolver step is
//! Windows-specific.
//!
//! The fingerprint ([`MatchFacts::exe_sha256`]) is left unset here because hashing
//! a multi-hundred-megabyte executable on every scan is wasteful; the install flow
//! fills it in only when a title actually matches on a fingerprint rule.

use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_domain::{ExeGraphicsInfo, GameInstallation, PathRef};

use super::errors::invalid;
use crate::ServiceError;
use crate::addons::matching::{Engine, MatchFacts};
use crate::game_executable::{self, ResolvedExecutable};

/// Result of inspecting a game: the facts the matcher needs plus the chosen
/// rendering executable, whose folder is where ReShade and the add-on install.
#[derive(Debug, Clone)]
pub struct GameAnalysis {
    /// Facts to resolve against the manifest.
    pub facts: MatchFacts,
    /// The executable selected as the game's renderer, when one was found.
    pub primary_executable: Option<PathRef>,
}

/// Inspects an installed game and assembles its [`GameAnalysis`].
///
/// `override_path` is the user's pinned executable (if any); it wins over
/// auto-detection. Pass `None` to auto-detect.
#[must_use]
pub fn analyze_game(install: &GameInstallation, override_path: Option<&Path>) -> GameAnalysis {
    let install_dir = Path::new(install.install_path().as_str());
    let primary = game_executable::resolve_primary_executable(install_dir, override_path, true);
    let facts = assemble_facts(install, primary.as_ref());
    GameAnalysis {
        facts,
        primary_executable: primary.map(|resolved| resolved.path),
    }
}

/// The folder an add-on installs into: the resolved rendering executable's
/// directory. Shared by the install, update, and availability flows so they agree
/// on the target location.
pub fn install_target_dir(analysis: &GameAnalysis) -> Result<PathBuf, ServiceError> {
    let executable = analysis
        .primary_executable
        .as_ref()
        .ok_or_else(|| invalid("no rendering executable found for this game".to_owned()))?;
    Path::new(executable.as_str())
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| invalid("rendering executable has no parent directory".to_owned()))
}

/// Resolves the same install roots that install/update/exclusivity use for on-disk
/// scans: the rendering executable's parent plus any split `AddonPath` from
/// `ReShade.ini`. Returns `None` when no executable is known yet so callers can
/// fall back to a DB-only exclusivity check.
#[must_use]
pub fn install_roots_for_analysis(
    analysis: &GameAnalysis,
) -> Option<crate::addons::reshade::InstallRoots> {
    install_target_dir(analysis)
        .ok()
        .map(|dir| crate::addons::reshade::InstallRoots::resolve_from_ini(&dir))
}

/// Assembles [`MatchFacts`] from a game and its already-resolved primary
/// executable. Pure and platform-agnostic — the I/O lives in the resolver and in
/// the engine-marker probing below.
#[must_use]
pub fn assemble_facts(
    install: &GameInstallation,
    primary: Option<&ResolvedExecutable>,
) -> MatchFacts {
    let exe_file_name = primary.map(|resolved| resolved.file_name.clone());
    let graphics = primary
        .map(|resolved| resolved.graphics.clone())
        .unwrap_or_else(|| ExeGraphicsInfo::new(Vec::new(), None));

    // Engine detection reads every scanned exe name (e.g. `<Game>-Win64-Shipping`)
    // plus folder markers, independent of which exe was chosen as the renderer.
    let exe_names: Vec<String> = install
        .executable_candidates()
        .iter()
        .filter_map(|path| path.file_name())
        .map(str::to_owned)
        .collect();
    let engine = detect_engine(Path::new(install.install_path().as_str()), &exe_names);

    MatchFacts {
        launcher: install.identity().launcher(),
        external_id: install.identity().external_id().map(str::to_owned),
        exe_file_name,
        exe_sha256: None,
        engine,
        graphics,
    }
}

/// Detects the engine from executable naming and well-known folder markers.
///
/// Conservative on purpose: returns a known engine only on a strong signal, so
/// the matcher's generic engine fallbacks apply only when warranted.
fn detect_engine(install_dir: &Path, exe_names: &[String]) -> Option<Engine> {
    if exe_names.iter().any(|name| is_unreal_exe_name(name))
        || install_dir.join("Engine").join("Binaries").is_dir()
    {
        return Some(Engine::Unreal);
    }
    if install_dir.join("UnityPlayer.dll").is_file() || has_unity_data_dir(install_dir) {
        return Some(Engine::Unity);
    }
    None
}

/// Unreal shipping executables are named `<Game>-Win64-Shipping.exe` (or
/// `<Game>-Shipping.exe` for 32-bit), a reliable engine fingerprint.
fn is_unreal_exe_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with("-shipping.exe")
}

/// Unity games ship a `<Game>_Data` directory next to the executable.
fn has_unity_data_dir(install_dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(install_dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
            && entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with("_data")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_executable::ExeSource;
    use renderpilot_domain::{
        Architecture, GameIdentity, GameRuntime, GraphicsApi, Launcher, Platform,
    };
    use tempfile::tempdir;

    fn path(value: &str) -> PathRef {
        PathRef::new(value).expect("valid path")
    }

    fn resolved(path_str: &str, apis: &[GraphicsApi]) -> ResolvedExecutable {
        let file_name = Path::new(path_str)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        ResolvedExecutable {
            path: path(path_str),
            file_name,
            graphics: ExeGraphicsInfo::new(apis.to_vec(), Some(Architecture::X64)),
            source: ExeSource::Auto,
        }
    }

    fn install_in(dir: &Path, candidate: &str) -> GameInstallation {
        let identity = GameIdentity::new(
            renderpilot_domain::GameId::new("steam:1091500").expect("id"),
            "My Game",
            Launcher::Steam,
        )
        .expect("identity")
        .with_external_id("1091500")
        .expect("external id");

        GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            path(&dir.to_string_lossy()),
        )
        .with_executable_candidate(path(candidate))
    }

    #[test]
    fn assemble_facts_carries_identity_exe_and_graphics() {
        let dir = tempdir().expect("tempdir");
        let install = install_in(dir.path(), "Game.exe");
        let primary = resolved("C:/g/Game.exe", &[GraphicsApi::D3D12]);

        let facts = assemble_facts(&install, Some(&primary));
        assert_eq!(facts.launcher, Launcher::Steam);
        assert_eq!(facts.external_id.as_deref(), Some("1091500"));
        assert_eq!(facts.exe_file_name.as_deref(), Some("Game.exe"));
        assert_eq!(facts.graphics.apis(), &[GraphicsApi::D3D12]);
    }

    #[test]
    fn assemble_facts_without_primary_has_no_exe_or_graphics() {
        let dir = tempdir().expect("tempdir");
        let install = install_in(dir.path(), "Game.exe");

        let facts = assemble_facts(&install, None);
        assert!(facts.exe_file_name.is_none());
        assert!(facts.graphics.apis().is_empty());
        assert!(facts.graphics.architecture().is_none());
    }

    #[test]
    fn assemble_facts_detects_engine_from_candidate_names() {
        let dir = tempdir().expect("tempdir");
        let install = install_in(dir.path(), "MyGame-Win64-Shipping.exe");
        let facts = assemble_facts(&install, None);
        assert_eq!(facts.engine, Some(Engine::Unreal));
    }

    #[test]
    fn detects_unreal_from_shipping_exe_name() {
        let dir = tempdir().expect("tempdir");
        let engine = detect_engine(
            dir.path(),
            &[
                "MyGame-Win64-Shipping.exe".to_owned(),
                "launcher.exe".to_owned(),
            ],
        );
        assert_eq!(engine, Some(Engine::Unreal));
    }

    #[test]
    fn detects_unreal_from_engine_binaries_dir() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("Engine").join("Binaries")).expect("mkdir");
        assert_eq!(
            detect_engine(dir.path(), &["game.exe".to_owned()]),
            Some(Engine::Unreal)
        );
    }

    #[test]
    fn detects_unity_from_data_directory() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("MyGame_Data")).expect("mkdir");
        assert_eq!(
            detect_engine(dir.path(), &["MyGame.exe".to_owned()]),
            Some(Engine::Unity)
        );
    }

    #[test]
    fn detects_unity_from_player_dll() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join("UnityPlayer.dll"), b"stub").expect("write");
        assert_eq!(
            detect_engine(dir.path(), &["MyGame.exe".to_owned()]),
            Some(Engine::Unity)
        );
    }

    #[test]
    fn detects_no_engine_without_markers() {
        let dir = tempdir().expect("tempdir");
        assert_eq!(detect_engine(dir.path(), &["game.exe".to_owned()]), None);
    }
}
