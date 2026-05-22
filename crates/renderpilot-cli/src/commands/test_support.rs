use std::{
    ffi::{OsString, OsString as PlatformString},
    fs,
    path::{Path, PathBuf},
    sync::MutexGuard,
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_application::{ArtifactRepository, ComponentRepository, GameRepository};
use renderpilot_domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
    GameIdentity, GameInstallation, GameRuntime, GraphicsTechnology, Launcher, LibraryArtifact,
    PathRef, Platform, Sha256Hash, Swappability, Version,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::catalog::CATALOG_DB_PATH_ENV;
use crate::test_env::lock_process_env;

pub(super) fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[derive(Debug)]
pub(super) struct TempGameFolder {
    path: PathBuf,
}

pub(super) struct CatalogEnvironmentGuard {
    previous: Option<PlatformString>,
    _lock: MutexGuard<'static, ()>,
}

pub(super) struct CatalogFixture {
    _catalog_env: CatalogEnvironmentGuard,
    pub(super) storage: SqliteStorage,
}

impl TempGameFolder {
    pub(super) fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();

        Self {
            path: std::env::temp_dir().join(format!("renderpilot-{name}-{nanos}")),
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl CatalogEnvironmentGuard {
    pub(super) fn new(path: PathBuf) -> Self {
        let lock = lock_process_env();
        let previous = std::env::var_os(CATALOG_DB_PATH_ENV);
        std::env::set_var(CATALOG_DB_PATH_ENV, &path);

        Self {
            previous,
            _lock: lock,
        }
    }
}

impl CatalogFixture {
    pub(super) fn new(name: &str) -> Self {
        let db_path = temp_db_path(name);
        let catalog_env = CatalogEnvironmentGuard::new(db_path.clone());
        let storage = SqliteStorage::open(&db_path).expect("sqlite storage should open");

        Self {
            _catalog_env: catalog_env,
            storage,
        }
    }

    pub(super) fn store_game(&self, game: &GameInstallation) {
        self.storage
            .upsert_game(game)
            .expect("game should be stored");
    }

    pub(super) fn store_components(
        &self,
        game_id: &GameId,
        components: &[renderpilot_domain::GraphicsComponent],
    ) {
        self.storage
            .replace_components_for_game(game_id, components)
            .expect("components should be stored");
    }

    pub(super) fn store_artifact(&self, artifact: LibraryArtifact) {
        self.storage
            .upsert_artifact(&artifact)
            .expect("artifact should be stored");
    }
}

impl Drop for CatalogEnvironmentGuard {
    fn drop(&mut self) {
        if let Some(previous) = &self.previous {
            std::env::set_var(CATALOG_DB_PATH_ENV, previous);
        }
    }
}

impl Drop for TempGameFolder {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

pub(super) fn temp_db_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();

    std::env::temp_dir().join(format!("renderpilot-{name}-{nanos}.db"))
}

pub(super) fn sample_game(id: &str, title: &str, install_path: &str) -> GameInstallation {
    let identity = GameIdentity::new(
        GameId::new(id).expect("game id should be valid"),
        title,
        Launcher::Manual,
    )
    .expect("game identity should be valid");

    GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(install_path).expect("install path should be valid"),
    )
}

/// Normalizes a platform path to forward slashes (same convention as domain `PathRef` paths / scan).
pub(super) fn path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(super) fn sample_component(
    component_id: &str,
    game_id: &str,
    technology: GraphicsTechnology,
    swappability: Swappability,
    path: &str,
    version: Option<&str>,
    sha256: &str,
) -> renderpilot_domain::GraphicsComponent {
    let mut file = ComponentFile::new(PathRef::new(path).expect("component path should be valid"))
        .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

    if let Some(version) = version {
        file = file.with_version(Version::parse(version).expect("version should be valid"));
    }

    renderpilot_domain::GraphicsComponent::new(
        ComponentId::new(component_id).expect("component id should be valid"),
        GameId::new(game_id).expect("game id should be valid"),
        ComponentKind::NativeLibrary,
        technology,
        swappability,
    )
    .with_file(file)
}

pub(super) fn sample_artifact(
    artifact_id: &str,
    technology: GraphicsTechnology,
    path: &str,
    version: Option<&str>,
    sha256: &str,
    source_game_id: Option<&str>,
) -> LibraryArtifact {
    let file_name = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .expect("artifact path should contain a file name");
    let mut file = ComponentFile::new(PathRef::new(path).expect("artifact path should be valid"))
        .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

    if let Some(version) = version {
        file = file.with_version(Version::parse(version).expect("version should be valid"));
    }

    let artifact = LibraryArtifact::new(
        ArtifactId::new(artifact_id).expect("artifact id should be valid"),
        technology,
        file_name,
        file,
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact should be valid")
    .with_source("scan-folder")
    .expect("source should be valid");

    match source_game_id {
        Some(source_game_id) => artifact.with_source_game_id(
            GameId::new(source_game_id).expect("source game id should be valid"),
        ),
        None => artifact,
    }
}
