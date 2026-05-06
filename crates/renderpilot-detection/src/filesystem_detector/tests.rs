use std::path::{Path, PathBuf};

use renderpilot_application::ComponentDetector;
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, GraphicsTechnology, Launcher, PathRef,
    Platform, Swappability,
};

use super::{DetectedLibraryFile, DetectionConfidence, LibraryPatternComponentDetector};
use crate::VersionDetectionStatus;

const FIXTURE_NEWLINE_FILE_SHA256: &str =
    "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b";

#[test]
fn fixture_detects_known_graphics_libraries() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(fixture_path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("fixture detection should succeed");

    assert_detects(
        &libraries,
        "nvngx_dlss.dll",
        GraphicsTechnology::DlssSuperResolution,
    );
    assert_detects(
        &libraries,
        "nvngx_dlssg.dll",
        GraphicsTechnology::DlssFrameGeneration,
    );
    assert_detects(
        &libraries,
        "nvngx_dlssd.dll",
        GraphicsTechnology::DlssRayReconstruction,
    );
    assert_detects(
        &libraries,
        "sl.interposer.dll",
        GraphicsTechnology::NvidiaStreamline,
    );
    assert_detects(&libraries, "libxess.dll", GraphicsTechnology::IntelXeSs);
    assert_detects(
        &libraries,
        "amd_fidelityfx_framegeneration.dll",
        GraphicsTechnology::AmdFsrFrameGeneration,
    );
    assert_detects(
        &libraries,
        "some_fsr_unknown.dll",
        GraphicsTechnology::Unknown,
    );
}

#[test]
fn fixture_does_not_detect_garbage_dlls() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(fixture_path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("fixture detection should succeed");

    assert!(!libraries
        .iter()
        .any(|library| library.file_name() == "random.dll"));
    assert!(!libraries
        .iter()
        .any(|library| library.file_name() == "not_a_graphics.dll"));
}

#[test]
fn fixture_does_not_scan_system_directories() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(fixture_path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("fixture detection should succeed");

    assert!(!libraries
        .iter()
        .any(|library| library.file_path().as_str().contains("/Windows/")));
}

#[test]
fn detector_respects_max_recursion_depth() {
    let detector = LibraryPatternComponentDetector::windows_default()
        .expect("valid patterns")
        .with_max_depth(1);
    let game = game_installation(fixture_path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("fixture detection should succeed");

    assert_detects(
        &libraries,
        "nvngx_dlss.dll",
        GraphicsTechnology::DlssSuperResolution,
    );
    assert!(!libraries
        .iter()
        .any(|library| library.file_name() == "nvngx_dlssg.dll"));
}

#[test]
fn streamline_is_bundle_only() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(fixture_path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("fixture detection should succeed");
    let streamline = libraries
        .iter()
        .find(|library| library.file_name() == "sl.interposer.dll")
        .expect("streamline should be detected");

    assert_eq!(streamline.swappability(), Swappability::BundleOnly);
}

#[test]
fn exact_matches_have_high_confidence_and_unknown_globs_have_low_confidence() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(fixture_path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("fixture detection should succeed");
    let dlss = libraries
        .iter()
        .find(|library| library.file_name() == "nvngx_dlss.dll")
        .expect("DLSS should be detected");
    let unknown_fsr = libraries
        .iter()
        .find(|library| library.file_name() == "some_fsr_unknown.dll")
        .expect("unknown FSR should be detected");

    assert_eq!(dlss.detection_confidence(), DetectionConfidence::High);
    assert_eq!(unknown_fsr.detection_confidence(), DetectionConfidence::Low);
}

#[test]
fn detected_files_include_hash_unknown_version_status_and_cache_key() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(fixture_path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("fixture detection should succeed");
    let dlss = libraries
        .iter()
        .find(|library| library.file_name() == "nvngx_dlss.dll")
        .expect("DLSS should be detected");

    assert_eq!(dlss.sha256().as_str(), FIXTURE_NEWLINE_FILE_SHA256);
    assert_eq!(dlss.version(), None);
    assert_eq!(dlss.status(), VersionDetectionStatus::UnknownVersion);
    assert_eq!(dlss.cache_key().path(), dlss.file_path());
    assert_eq!(dlss.cache_key().size(), 1);
    assert_eq!(
        dlss.cache_key().sha256().as_str(),
        FIXTURE_NEWLINE_FILE_SHA256
    );
}

#[test]
fn component_detector_trait_maps_detected_files_to_domain_components() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(fixture_path());
    let components = detector
        .detect_components(&game)
        .expect("component detection should succeed");

    assert!(components
        .iter()
        .any(|component| component.technology() == GraphicsTechnology::DlssSuperResolution));
    assert!(components
        .iter()
        .any(|component| component.files().iter().any(|file| file.sha256().is_some())));
}

fn assert_detects(
    libraries: &[DetectedLibraryFile],
    file_name: &str,
    technology: GraphicsTechnology,
) {
    assert!(
        libraries
            .iter()
            .any(|library| library.file_name() == file_name && library.technology() == technology),
        "expected to detect {file_name} as {technology:?}; got {libraries:#?}"
    );
}

fn game_installation(folder: PathBuf) -> GameInstallation {
    let install_path = PathRef::new(folder.to_string_lossy().as_ref()).expect("valid path");
    let identity = GameIdentity::new(
        GameId::new(format!("manual:{}", install_path.as_str())).expect("valid id"),
        "Manual Game",
        Launcher::Manual,
    )
    .expect("valid identity");

    GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        install_path,
    )
}

fn fixture_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("game_with_dlls")
}
