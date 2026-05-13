use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_application::ComponentDetector;
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, GraphicsTechnology, Launcher, PathRef,
    Platform, Swappability,
};

use super::{DetectedLibraryFile, DetectionConfidence, LibraryPatternComponentDetector};
use crate::{
    file_metadata::{
        reset_sha256_file_call_count_for_tests, sha256_file_call_count_for_tests, FileHashCache,
    },
    VersionDetectionStatus,
};

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

const TEMP_DLSS_NAME: &str = "nvngx_dlss.dll";

fn file_hash_cache_from_libraries(libraries: &[DetectedLibraryFile]) -> FileHashCache {
    let mut cache = FileHashCache::with_capacity(libraries.len());

    for library in libraries {
        cache.insert(
            library.file_path().as_str().to_owned(),
            library.cache_key().size(),
            library.cache_key().modified_at(),
            library.sha256().clone(),
            library.version().cloned(),
        );
    }

    cache
}

#[test]
fn cache_hit_avoids_sha256_when_size_and_mtime_match() {
    let folder = temp_dlss_folder(b"hello");
    let game = game_installation(folder.clone());
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");

    reset_sha256_file_call_count_for_tests();
    let libraries = detector
        .detect_library_files(&game)
        .expect("detection should succeed");
    assert!(
        sha256_file_call_count_for_tests() >= 1,
        "cold scan should hash at least one library file",
    );

    let cache = file_hash_cache_from_libraries(&libraries);

    reset_sha256_file_call_count_for_tests();
    let cached_libs = detector
        .detect_library_files_with_cache(&game, &cache)
        .expect("cached detection should succeed");

    assert_eq!(
        sha256_file_call_count_for_tests(),
        0,
        "warm cache should skip SHA-256 when size and mtime match",
    );
    assert_eq!(libraries.len(), cached_libs.len());
    assert_eq!(
        libraries[0].sha256().as_str(),
        cached_libs[0].sha256().as_str(),
    );
}

#[test]
fn fast_cached_detection_report_contains_detectable_count() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(fixture_path());
    let full_libraries = detector
        .detect_library_files(&game)
        .expect("full detection should succeed");
    let cache = file_hash_cache_from_libraries(&full_libraries);

    let report = detector
        .detect_library_files_fast_cached_with_evidence(&game, &cache)
        .expect("fast report should succeed");

    assert_eq!(
        report.detectable_count(),
        detector
            .count_detectable_library_files(&game)
            .expect("detectable count should be available"),
    );
    assert_eq!(report.libraries().len(), full_libraries.len());
}

#[test]
fn stale_cache_entry_triggers_fresh_sha256_after_file_change() {
    let folder = temp_dlss_folder(b"a");
    let game = game_installation(folder.clone());
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");

    let libraries = detector
        .detect_library_files(&game)
        .expect("detection should succeed");

    let cache = file_hash_cache_from_libraries(&libraries);

    let dll_path = folder.join(TEMP_DLSS_NAME);
    fs::write(&dll_path, b"ab").expect("test file should update");

    reset_sha256_file_call_count_for_tests();
    let refreshed = detector
        .detect_library_files_with_cache(&game, &cache)
        .expect("detection after change should succeed");

    assert!(
        sha256_file_call_count_for_tests() >= 1,
        "stale cache should trigger SHA-256 after content change",
    );
    assert_ne!(
        libraries[0].sha256().as_str(),
        refreshed[0].sha256().as_str(),
    );
}

#[test]
fn fast_cached_scan_can_be_partial_when_new_dlls_are_added_after_cache_warmup() {
    let folder = temp_dlss_folder(b"intel");
    let game = game_installation(folder.clone());
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");

    fs::rename(folder.join(TEMP_DLSS_NAME), folder.join("libxess.dll"))
        .expect("fixture should rename to intel dll");

    let baseline = detector
        .detect_library_files(&game)
        .expect("baseline detection should succeed");
    assert_eq!(baseline.len(), 1, "baseline should only include intel");

    let cache = file_hash_cache_from_libraries(&baseline);

    fs::write(folder.join("amd_fidelityfx_framegeneration.dll"), b"amd")
        .expect("amd dll should be written");
    fs::write(folder.join("nvngx_dlss.dll"), b"nvidia").expect("nvidia dll should be written");

    let fast = detector
        .detect_library_files_fast_cached(&game, &cache)
        .expect("fast cached scan should succeed");
    let detectable_count = detector
        .count_detectable_library_files(&game)
        .expect("detectable count should succeed");
    let full = detector
        .detect_library_files_with_cache(&game, &cache)
        .expect("full cached scan should succeed");

    assert_eq!(fast.len(), 1, "fast cache sees only cached file paths");
    assert_eq!(
        detectable_count, 3,
        "on-disk detectable DLL count should include newly added vendors",
    );
    assert_eq!(full.len(), 3, "full cached scan should recover all files");
}

#[test]
fn detector_scans_intel_xell_runtime_files_from_disk() {
    let folder = temp_dlss_folder(b"intel-xell");
    let game = game_installation(folder.clone());
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");

    fs::rename(folder.join(TEMP_DLSS_NAME), folder.join("libxell.dll"))
        .expect("fixture should rename to XeLL dll");
    fs::write(folder.join("libxell_dx11.dll"), b"intel-xell-dx11")
        .expect("XeLL dx11 dll should be written");

    let libraries = detector
        .detect_library_files(&game)
        .expect("detection should succeed");

    assert_detects(&libraries, "libxell.dll", GraphicsTechnology::IntelXeLl);
    assert_detects(&libraries, "libxell_dx11.dll", GraphicsTechnology::IntelXeLl);
}

#[test]
fn detector_scans_amd_denoiser_loader_and_upscaler_runtime_files_from_disk() {
    let folder = temp_dlss_folder(b"amd-denoiser");
    let game = game_installation(folder.clone());
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");

    fs::remove_file(folder.join(TEMP_DLSS_NAME)).expect("temporary dlss file should be removed");
    fs::write(folder.join("amd_fidelityfx_denoiser.dll"), b"amd-denoiser")
        .expect("denoiser dll should be written");
    fs::write(folder.join("amd_fidelityfx_denoiser_dx12.dll"), b"amd-denoiser-dx12")
        .expect("denoiser dx12 dll should be written");
    fs::write(folder.join("amd_fidelityfx_loader_dx12.dll"), b"amd-loader")
        .expect("loader dll should be written");
    fs::write(folder.join("amd_fidelityfx_upscaler.dll"), b"amd-upscaler")
        .expect("upscaler dll should be written");

    let libraries = detector
        .detect_library_files(&game)
        .expect("detection should succeed");

    assert_detects(
        &libraries,
        "amd_fidelityfx_denoiser.dll",
        GraphicsTechnology::AmdFsrRayRegeneration,
    );
    assert_detects(
        &libraries,
        "amd_fidelityfx_denoiser_dx12.dll",
        GraphicsTechnology::AmdFsrRayRegeneration,
    );
    assert_detects(
        &libraries,
        "amd_fidelityfx_loader_dx12.dll",
        GraphicsTechnology::Unknown,
    );
    assert_detects(
        &libraries,
        "amd_fidelityfx_upscaler.dll",
        GraphicsTechnology::AmdFsr,
    );
}

#[test]
fn default_detector_depth_finds_deeply_nested_nvidia_runtime_dlls() {
    let root = temp_dlss_folder(b"root");
    let nested = root
        .join("Engine")
        .join("Plugins")
        .join("Runtime")
        .join("Nvidia")
        .join("DLSS")
        .join("Binaries")
        .join("ThirdParty")
        .join("Win64");
    fs::create_dir_all(&nested).expect("nested runtime path should be created");
    fs::write(nested.join("nvngx_dlss.dll"), b"deep-nvidia").expect("deep nvidia dll");

    let game = game_installation(root);
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let libraries = detector
        .detect_library_files(&game)
        .expect("deep detection should succeed");

    assert!(libraries.iter().any(|library| {
        library.file_name() == "nvngx_dlss.dll"
            && library.technology() == GraphicsTechnology::DlssSuperResolution
    }));
}

fn temp_dlss_folder(contents: &[u8]) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();

    let dir = std::env::temp_dir().join(format!("renderpilot-detect-cache-{nanos}"));
    fs::create_dir_all(&dir).expect("temp game folder should be created");

    fs::write(dir.join(TEMP_DLSS_NAME), contents).expect("temp dlss file should be written");

    dir
}
