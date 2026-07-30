use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_orchestration::application::{
    ArtifactRepository, ComponentRepository, GameRepository,
};
use renderpilot_orchestration::domain::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
    GameIdentity, GameInstallation, GameRuntime, Launcher, LibraryArtifact, LibraryTechnology,
    PathRef, Platform, Sha256Hash, Swappability, Version,
};
use renderpilot_storage_sqlite::SqliteStorage;

pub(super) fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[derive(Debug)]
pub(super) struct TempGameFolder {
    path: PathBuf,
}

pub(super) struct CatalogFixture {
    db_path: PathBuf,
    /// Direct storage handle on the same database, for test seeding and assertions.
    /// Commands under test open their own orchestration `Context` against `db_path`;
    /// only tests reach storage directly.
    storage: SqliteStorage,
}

impl TempGameFolder {
    pub(super) fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();

        Self {
            path: canonical_temp_dir().join(format!("renderpilot-{name}-{nanos}")),
        }
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl CatalogFixture {
    pub(super) fn new(name: &str) -> Self {
        let db_path = temp_db_path(name);
        let storage = open_storage(&db_path);

        Self { db_path, storage }
    }

    /// Opens a fresh orchestration `Context` on this fixture's database — the same
    /// seam the commands under test use, pointed at the fixture's `db_path`.
    fn open_context(
        &self,
    ) -> Result<renderpilot_orchestration::Context, renderpilot_orchestration::ServiceError> {
        renderpilot_orchestration::Context::open_at(&self.db_path)
    }

    pub(super) fn context(&self) -> renderpilot_orchestration::Context {
        self.open_context().expect("catalog sqlite should open")
    }

    pub(super) fn run<I>(&self, args: I) -> Result<String, crate::CliError>
    where
        I: IntoIterator<Item = OsString>,
    {
        crate::run_with_context(args, || self.open_context())
    }

    /// Direct storage handle for test seeding and assertions on the same database.
    pub(super) fn storage(&self) -> &SqliteStorage {
        &self.storage
    }

    pub(super) fn store_game(&self, game: &GameInstallation) {
        self.storage
            .upsert_game(game)
            .expect("game should be stored");
    }

    pub(super) fn store_components(
        &self,
        game_id: &GameId,
        components: &[renderpilot_orchestration::domain::LibraryComponent],
    ) {
        self.storage
            .replace_components_for_game(game_id, components)
            .expect("components should be stored");
    }

    pub(super) fn store_artifact(&self, artifact: &LibraryArtifact) {
        self.storage
            .upsert_artifact(artifact)
            .expect("artifact should be stored");
    }
}

impl Drop for TempGameFolder {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// Opens a direct storage handle on `db_path` for test seeding and assertions.
///
/// Production code never opens storage directly — it goes through `Context` — but
/// tests legitimately reach the infrastructure to set up state and verify it.
fn open_storage(db_path: &Path) -> SqliteStorage {
    SqliteStorage::open(db_path).expect("sqlite storage should open")
}

fn temp_db_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();

    canonical_temp_dir().join(format!("renderpilot-{name}-{nanos}.db"))
}

/// Resolves a possible Windows 8.3 alias in `%TEMP%` before scan tests persist paths.
///
/// The scanner persists canonical paths, so fixtures must start from the same
/// long form rather than compare it with an equivalent short alias.
fn canonical_temp_dir() -> PathBuf {
    let temp_dir = std::env::temp_dir();
    let canonical = temp_dir.canonicalize().unwrap_or(temp_dir);
    strip_verbatim_prefix(canonical)
}

fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();

    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = value.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
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
    technology: LibraryTechnology,
    swappability: Swappability,
    path: &str,
    version: Option<&str>,
    sha256: &str,
) -> renderpilot_orchestration::domain::LibraryComponent {
    let mut file = ComponentFile::new(PathRef::new(path).expect("component path should be valid"))
        .with_sha256(Sha256Hash::new(sha256).expect("sha256 should be valid"));

    if let Some(version) = version {
        file = file.with_version(Version::parse(version).expect("version should be valid"));
    }

    renderpilot_orchestration::domain::LibraryComponent::new(
        ComponentId::new(component_id).expect("component id should be valid"),
        GameId::new(game_id).expect("game id should be valid"),
        ComponentKind::NativeLibrary,
        technology,
        swappability,
    )
    .with_file(file)
}

/// Builds a multi-file component — e.g. a game already on the FSR 4 split set
/// (the loader installed as `amd_fidelityfx_dx12.dll`, plus the upscaler and frame
/// generation). Each `(path, version, sha256)` becomes one file, in order.
pub(super) fn sample_bundle_component(
    component_id: &str,
    game_id: &str,
    technology: LibraryTechnology,
    swappability: Swappability,
    files: &[(&str, Option<&str>, &str)],
) -> renderpilot_orchestration::domain::LibraryComponent {
    let mut component = renderpilot_orchestration::domain::LibraryComponent::new(
        ComponentId::new(component_id).expect("component id should be valid"),
        GameId::new(game_id).expect("game id should be valid"),
        ComponentKind::NativeLibrary,
        technology,
        swappability,
    );

    for (path, version, sha256) in files {
        let mut file =
            ComponentFile::new(PathRef::new(*path).expect("component path should be valid"))
                .with_sha256(Sha256Hash::new(*sha256).expect("sha256 should be valid"));
        if let Some(version) = *version {
            file = file.with_version(Version::parse(version).expect("version should be valid"));
        }
        component = component.with_file(file);
    }

    component
}

pub(super) fn sample_artifact(
    artifact_id: &str,
    technology: LibraryTechnology,
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
        vec![file],
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

pub(super) fn sample_bundle_artifact(
    artifact_id: &str,
    technology: LibraryTechnology,
    files: &[(&str, Option<&str>, &str)],
    source_game_id: Option<&str>,
) -> LibraryArtifact {
    let component_files = files
        .iter()
        .map(|(path, version, sha256)| {
            let mut file =
                ComponentFile::new(PathRef::new(*path).expect("artifact path should be valid"))
                    .with_sha256(Sha256Hash::new(*sha256).expect("sha256 should be valid"));
            if let Some(version) = version {
                file =
                    file.with_version(Version::parse(*version).expect("version should be valid"));
            }
            file
        })
        .collect::<Vec<_>>();
    let primary_path = files.first().expect("bundle artifact must have files").0;
    let primary_name = Path::new(primary_path)
        .file_name()
        .and_then(|name| name.to_str())
        .expect("artifact path should contain a file name");
    let artifact = LibraryArtifact::new(
        ArtifactId::new(artifact_id).expect("artifact id should be valid"),
        technology,
        primary_name,
        component_files,
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
