//! Detecting whether a game qualifies for the DLSS-Fix companion add-on and
//! resolving the DLL paths its `ReShade.ini` configuration needs.
//!
//! The DLSS-Fix is installed only when the game has all three NVIDIA technologies
//! detected on disk: Frame Generation (`nvngx_dlssg.dll`), DLSS Super Resolution
//! (`nvngx_dlss.dll`), and Streamline (`sl.interposer.dll`). The resolved paths
//! are converted to Windows-native backslash form for the INI.

use std::path::PathBuf;

use renderpilot_application::ComponentRepository;
use renderpilot_domain::{ComponentFile, GameId, GraphicsComponent, GraphicsTechnology};

use crate::ServiceError;

/// The resolved DLSS-Fix configuration: Windows-native paths to the two DLLs the
/// `[RENODX-DLSSFIX]` INI section points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DlssFixRequest {
    /// Windows-native (backslash) path to `nvngx_dlss.dll`.
    pub dlss_path: String,
    /// Windows-native (backslash) path to `sl.interposer.dll`.
    pub streamline_path: String,
}

/// Checks whether the game has all three NVIDIA technologies (Frame Generation,
/// DLSS Super Resolution, and Streamline with `sl.interposer.dll`) and returns
/// the resolved DLL paths if so. Returns `None` when any of the three is missing
/// — the DLSS-Fix is not offered in that case.
pub(crate) fn resolve_dlss_fix(
    storage: &dyn ComponentRepository,
    game_id: &GameId,
) -> Result<Option<DlssFixRequest>, ServiceError> {
    let components = storage.list_components_for_game(game_id)?;

    let dlss_path = find_dll_path(
        &components,
        GraphicsTechnology::DlssSuperResolution,
        "nvngx_dlss.dll",
    );
    let streamline_path = find_dll_path(
        &components,
        GraphicsTechnology::NvidiaStreamline,
        "sl.interposer.dll",
    );
    let has_frame_gen = components
        .iter()
        .any(|c| c.technology() == GraphicsTechnology::DlssFrameGeneration);

    // All three are required: FG triggers the need, DLSS SR and Streamline provide
    // the paths the [RENODX-DLSSFIX] section must point at.
    if !has_frame_gen {
        return Ok(None);
    }
    let (Some(dlss_path), Some(streamline_path)) = (dlss_path, streamline_path) else {
        return Ok(None);
    };

    Ok(Some(DlssFixRequest {
        dlss_path: to_windows_path(&dlss_path),
        streamline_path: to_windows_path(&streamline_path),
    }))
}

/// Finds the first file named `dll_name` within the component for `technology`.
fn find_dll_path(
    components: &[GraphicsComponent],
    technology: GraphicsTechnology,
    dll_name: &str,
) -> Option<PathBuf> {
    components
        .iter()
        .find(|c| c.technology() == technology)
        .and_then(|c| c.files().iter().find(|f| is_named(f, dll_name)))
        .map(|f| PathBuf::from(f.path().as_str()))
}

/// Returns whether `file`'s file name matches `name` (case-insensitive).
fn is_named(file: &ComponentFile, name: &str) -> bool {
    file.path()
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case(name))
}

/// Converts a forward-slash `PathRef` string to a Windows-native backslash string.
fn to_windows_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('/', "\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_application::AppResult;
    use renderpilot_domain::{
        ComponentFile, ComponentId, ComponentKind, GameId, GraphicsComponent, GraphicsTechnology,
        PathRef, Swappability,
    };

    /// A minimal in-memory `ComponentRepository` for testing `resolve_dlss_fix`.
    struct FakeRepo {
        components: Vec<GraphicsComponent>,
    }

    impl ComponentRepository for FakeRepo {
        fn replace_components_for_game(
            &self,
            _game_id: &GameId,
            _components: &[GraphicsComponent],
        ) -> AppResult<()> {
            Ok(())
        }
        fn list_components_for_game(&self, _game_id: &GameId) -> AppResult<Vec<GraphicsComponent>> {
            Ok(self.components.clone())
        }
    }

    fn game_id() -> GameId {
        GameId::new("steam:1091500").expect("id")
    }

    fn component(technology: GraphicsTechnology, file_names: &[&str]) -> GraphicsComponent {
        let id =
            ComponentId::new(format!("component:game:{:?}:dir", technology)).expect("component id");
        let game = game_id();
        let mut component = GraphicsComponent::new(
            id,
            game,
            ComponentKind::NativeLibrary,
            technology,
            Swappability::Swappable,
        );
        for name in file_names {
            component = component.with_file(ComponentFile::new(
                PathRef::new(format!("C:/Games/Game/{name}")).expect("path"),
            ));
        }
        component
    }

    #[test]
    fn returns_paths_when_all_three_technologies_present() {
        let repo = FakeRepo {
            components: vec![
                component(
                    GraphicsTechnology::DlssFrameGeneration,
                    &["nvngx_dlssg.dll"],
                ),
                component(GraphicsTechnology::DlssSuperResolution, &["nvngx_dlss.dll"]),
                component(
                    GraphicsTechnology::NvidiaStreamline,
                    &["sl.interposer.dll", "sl.common.dll"],
                ),
            ],
        };
        let request = resolve_dlss_fix(&repo, &game_id())
            .expect("resolve")
            .expect("should return a request");
        assert_eq!(request.dlss_path, r"C:\Games\Game\nvngx_dlss.dll");
        assert_eq!(request.streamline_path, r"C:\Games\Game\sl.interposer.dll");
    }

    #[test]
    fn returns_none_when_frame_generation_missing() {
        let repo = FakeRepo {
            components: vec![
                component(GraphicsTechnology::DlssSuperResolution, &["nvngx_dlss.dll"]),
                component(GraphicsTechnology::NvidiaStreamline, &["sl.interposer.dll"]),
            ],
        };
        assert!(
            resolve_dlss_fix(&repo, &game_id())
                .expect("resolve")
                .is_none()
        );
    }

    #[test]
    fn returns_none_when_dlss_sr_missing() {
        let repo = FakeRepo {
            components: vec![
                component(
                    GraphicsTechnology::DlssFrameGeneration,
                    &["nvngx_dlssg.dll"],
                ),
                component(GraphicsTechnology::NvidiaStreamline, &["sl.interposer.dll"]),
            ],
        };
        assert!(
            resolve_dlss_fix(&repo, &game_id())
                .expect("resolve")
                .is_none()
        );
    }

    #[test]
    fn returns_none_when_sl_interposer_missing() {
        // Streamline component exists but has no sl.interposer.dll (only sl.common.dll).
        let repo = FakeRepo {
            components: vec![
                component(
                    GraphicsTechnology::DlssFrameGeneration,
                    &["nvngx_dlssg.dll"],
                ),
                component(GraphicsTechnology::DlssSuperResolution, &["nvngx_dlss.dll"]),
                component(GraphicsTechnology::NvidiaStreamline, &["sl.common.dll"]),
            ],
        };
        assert!(
            resolve_dlss_fix(&repo, &game_id())
                .expect("resolve")
                .is_none()
        );
    }

    #[test]
    fn returns_none_when_no_components() {
        let repo = FakeRepo { components: vec![] };
        assert!(
            resolve_dlss_fix(&repo, &game_id())
                .expect("resolve")
                .is_none()
        );
    }
}
