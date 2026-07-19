use std::fs;
use std::path::Path;

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt as _;

use renderpilot_orchestration::domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, GameInstallation, GraphicsTechnology,
    LibraryArtifact, PathRef, Sha256Hash, Swappability, Version,
};

use crate::hash::sha256_hex;

use super::super::{
    CatalogFixture, TempGameFolder, args, path_string, sample_artifact, sample_bundle_component,
    sample_component, sample_game,
};

pub(super) const REPLACEMENT_SHA256: &str =
    "70bf69c13743b7193ffd7a3718caab18522b61d4643fe13ac80caa5301e2345a";
pub(super) const FSR_COMPONENT_ID: &str = "component:fsr";
pub(super) const FSR_ENTRY_POINT_FILE: &str = "amd_fidelityfx_dx12.dll";
pub(super) const ORIGINAL_BYTES: &[u8] = b"original-bytes";
pub(super) const REPLACEMENT_BYTES: &[u8] = b"replacement-bytes";
pub(super) fn write_bundle_artifact(
    folder: &Path,
    technology: GraphicsTechnology,
    files: &[(&str, &[u8], Option<&str>)],
) -> (LibraryArtifact, String) {
    let component_files: Vec<ComponentFile> = files
        .iter()
        .map(|(name, bytes, install_as)| {
            let path = folder.join(name);
            fs::write(&path, bytes).expect("artifact file should be written");
            let mut file =
                ComponentFile::new(PathRef::new(path_string(&path)).expect("artifact path valid"))
                    .with_sha256(Sha256Hash::new(sha256_hex(bytes)).expect("sha256 valid"))
                    .with_version(Version::parse("4.0.0").expect("version valid"));
            if let Some(install_as) = install_as {
                file = file.with_install_as(*install_as);
            }
            file
        })
        .collect();

    let id = ArtifactId::for_bundle(component_files.iter().filter_map(ComponentFile::sha256));
    let id_string = id.as_str().to_owned();
    let artifact = LibraryArtifact::new(
        id,
        technology,
        files[0].0,
        component_files,
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("bundle artifact should be valid");

    (artifact, id_string)
}

pub(super) fn write_fsr_bundle_artifact(
    folder: &Path,
    files: &[(&str, &[u8], Option<&str>)],
) -> (LibraryArtifact, String) {
    write_bundle_artifact(folder, GraphicsTechnology::AmdFsr, files)
}

pub(super) fn store_manual_game(
    fixture: &CatalogFixture,
    game_folder: &TempGameFolder,
    name: &str,
) -> GameInstallation {
    let install_path = path_string(game_folder.path());
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, name, &install_path);
    fixture.store_game(&game);
    game
}

pub(super) fn store_single_file_fsr_component(
    fixture: &CatalogFixture,
    game: &GameInstallation,
    path: &Path,
    version: &str,
    bytes: &[u8],
) {
    fixture.store_components(
        game.id(),
        &[sample_component(
            FSR_COMPONENT_ID,
            game.id().as_str(),
            GraphicsTechnology::AmdFsr,
            Swappability::BundleOnly,
            &path_string(path),
            Some(version),
            &sha256_hex(bytes),
        )],
    );
}

pub(super) fn write_versioned_component_members<'a>(
    folder: &Path,
    members: &[(&'a str, &[u8], &'a str)],
) -> Vec<(String, Option<&'a str>, String)> {
    let mut written = Vec::with_capacity(members.len());
    for (name, bytes, version) in members {
        let path = folder.join(name);
        fs::write(&path, bytes).expect("member written");
        written.push((path_string(&path), Some(*version), sha256_hex(bytes)));
    }
    written
}

pub(super) fn store_written_fsr_bundle_component<'a>(
    fixture: &CatalogFixture,
    game: &GameInstallation,
    written: &'a [(String, Option<&'a str>, String)],
) {
    let component_files: Vec<(&str, Option<&str>, &str)> = written
        .iter()
        .map(|(path, version, sha)| (path.as_str(), *version, sha.as_str()))
        .collect();

    fixture.store_components(
        game.id(),
        &[sample_bundle_component(
            FSR_COMPONENT_ID,
            game.id().as_str(),
            GraphicsTechnology::AmdFsr,
            Swappability::BundleOnly,
            &component_files,
        )],
    );
}

pub(super) fn dir_file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("directory should be readable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

pub(super) struct AppliedScenario {
    pub(super) fixture: CatalogFixture,
    pub(super) game_id: renderpilot_orchestration::domain::GameId,
    pub(super) source_path: std::path::PathBuf,
    pub(super) original_sha256: String,
    pub(super) _game_folder: TempGameFolder,
    pub(super) _artifact_folder: TempGameFolder,
}
pub(super) fn setup_applied_scenario(name: &str) -> AppliedScenario {
    let fixture = CatalogFixture::new(name);
    let game_folder = TempGameFolder::new(&format!("{name}-game"));
    let artifact_folder = TempGameFolder::new(&format!("{name}-artifact"));

    fs::create_dir_all(game_folder.path()).expect("game folder should be created");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder should be created");

    let source_path = game_folder.path().join("nvngx_dlss.dll");
    let artifact_path = artifact_folder.path().join("nvngx_dlss.dll");
    fs::write(&source_path, ORIGINAL_BYTES).expect("source file should be written");
    fs::write(&artifact_path, REPLACEMENT_BYTES).expect("artifact file should be written");

    let original_sha256 = sha256_hex(ORIGINAL_BYTES);
    let install_path = path_string(game_folder.path());
    let artifact_path_string = path_string(&artifact_path);
    let source_path_string = path_string(&source_path);
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, "Game A", &install_path);

    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:game-a:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &source_path_string,
            Some("3.5.0"),
            &original_sha256,
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &artifact_path_string,
        Some("3.7.0"),
        REPLACEMENT_SHA256,
        None,
    ));

    fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:game-a:dlss",
            "--artifact",
            "artifact:dlss-3.7",
        ]))
        .expect("apply should succeed");

    AppliedScenario {
        fixture,
        game_id: game.id().clone(),
        source_path,
        original_sha256,
        _game_folder: game_folder,
        _artifact_folder: artifact_folder,
    }
}

#[cfg(windows)]
pub(super) fn open_exclusive_file_lock(path: &Path) -> std::fs::File {
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).share_mode(0);

    options.open(path).expect("exclusive file lock should open")
}
