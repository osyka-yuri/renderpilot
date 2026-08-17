use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_application::{AppResult, ComponentDetector};
use renderpilot_domain::{
    GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, LibraryTechnology, PathRef,
    Platform, Swappability,
};

use super::{
    DetectedLibraryFile, LibraryPatternComponentDetector, ReusableFileMetadata,
    group_into_artifacts, group_into_components,
};
use crate::{
    FileIdentityProbeResult, FileObservation, FileObservationResult, FileObservationSource,
    StableFileSnapshot, StrongFileIdentity, VersionDetectionStatus, sha256_bytes,
};

const FIXTURE_NEWLINE_FILE_SHA256: &str =
    "01ba4719c80b6fe911b091a7c05124b64eeece964e09c058ef8f9805daca546b";
const TEMP_DLSS_NAME: &str = "nvngx_dlss.dll";

mod core;
mod fsr;

fn assert_detects(
    libraries: &[DetectedLibraryFile],
    file_name: &str,
    technology: LibraryTechnology,
) {
    assert!(
        libraries
            .iter()
            .any(|library| library.file_name() == file_name && library.technology() == technology),
        "expected to detect {file_name} as {technology:?}; got {libraries:#?}"
    );
}

fn game_installation(folder: &Path) -> GameInstallation {
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
