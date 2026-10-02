use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use renderpilot_orchestration::application::{ArtifactRepository, GameRepository};
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
        let mut args: Vec<OsString> = args.into_iter().collect();
        let is_apply = matches!(
            args.first().and_then(|argument| argument.to_str()),
            Some("apply" | "apply-operation")
        );
        if is_apply
            && !args.iter().any(|argument| {
                argument
                    .to_str()
                    .is_some_and(|argument| argument == "--safety-context-token")
            })
        {
            let game_id = args
                .windows(2)
                .find(|pair| pair[0].to_str() == Some("--game"))
                .and_then(|pair| pair[1].to_str())
                .and_then(|value| renderpilot_orchestration::domain::GameId::new(value).ok());
            if let Some(game_id) = game_id {
                let context = self.open_context()?;
                let assessment = renderpilot_orchestration::FileSafetyAuthority::new()
                    .issue_game_assessment(&context, &game_id)
                    .map_err(crate::CliError::from)?;
                args.push(OsString::from("--safety-context-token"));
                args.push(OsString::from(assessment.context_token));
            }
        }
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

    /// Publishes fixture components through the storage adapter's explicit
    /// test-only complete-scan path. Ordinary public component replacement is
    /// intentionally not a ready catalog projection.
    pub(super) fn store_complete_components(
        &self,
        game_id: &GameId,
        components: &[renderpilot_orchestration::domain::LibraryComponent],
    ) {
        let game = self
            .storage
            .require_game(game_id)
            .expect("game should be stored");
        self.storage
            .store_complete_components_for_test(&game, components)
            .expect("complete fixture components should be stored");
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

/// Builds a current ReShade Proxy host with the export evidence used by the
/// orchestration scanner. CLI status fixtures use this to distinguish a
/// physically present host from a persisted HostBinary origin receipt.
pub(super) fn compatible_reshade_proxy_host() -> Vec<u8> {
    let pe_offset = 0x80usize;
    let coff_offset = pe_offset + 4;
    let optional_header_offset = coff_offset + 20;
    let optional_header_size = 0xF0usize;
    let section_table_offset = optional_header_offset + optional_header_size;
    let headers_end = section_table_offset + 40;
    let section_rva = 0x1000u32;
    let section_raw_ptr = headers_end.div_ceil(0x200) * 0x200;
    let exports = [
        "ReShadeVersion",
        "ReShadeRegisterAddon",
        "ReShadeUnregisterAddon",
        "ReShadeRegisterEvent",
    ];

    let mut section = vec![0u8; 40 + exports.len() * 4 * 2 + exports.len() * 2];
    let functions_offset = 40;
    let names_offset = functions_offset + exports.len() * 4;
    let ordinals_offset = names_offset + exports.len() * 4;
    let mut name_rvas = Vec::with_capacity(exports.len());
    for name in exports {
        name_rvas.push(section_rva + section.len() as u32);
        section.extend_from_slice(name.as_bytes());
        section.push(0);
    }
    let function_stub_rva = section_rva + section.len() as u32;
    section.push(0xC3);

    for (index, name_rva) in name_rvas.iter().enumerate() {
        let name_offset = names_offset + index * 4;
        section[name_offset..name_offset + 4].copy_from_slice(&name_rva.to_le_bytes());
        let function_offset = functions_offset + index * 4;
        section[function_offset..function_offset + 4]
            .copy_from_slice(&function_stub_rva.to_le_bytes());
        let ordinal_offset = ordinals_offset + index * 2;
        section[ordinal_offset..ordinal_offset + 2].copy_from_slice(&(index as u16).to_le_bytes());
    }
    section[20..24].copy_from_slice(&(exports.len() as u32).to_le_bytes());
    section[24..28].copy_from_slice(&(exports.len() as u32).to_le_bytes());
    section[28..32].copy_from_slice(&(section_rva + functions_offset as u32).to_le_bytes());
    section[32..36].copy_from_slice(&(section_rva + names_offset as u32).to_le_bytes());
    section[36..40].copy_from_slice(&(section_rva + ordinals_offset as u32).to_le_bytes());

    let mut bytes = vec![0u8; section_raw_ptr + section.len()];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3C..0x40].copy_from_slice(&(pe_offset as u32).to_le_bytes());
    bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
    bytes[coff_offset..coff_offset + 2].copy_from_slice(&0x8664u16.to_le_bytes());
    bytes[coff_offset + 2..coff_offset + 4].copy_from_slice(&1u16.to_le_bytes());
    bytes[coff_offset + 16..coff_offset + 18]
        .copy_from_slice(&(optional_header_size as u16).to_le_bytes());
    bytes[optional_header_offset..optional_header_offset + 2]
        .copy_from_slice(&0x20Bu16.to_le_bytes());
    bytes[optional_header_offset + 108..optional_header_offset + 112]
        .copy_from_slice(&16u32.to_le_bytes());
    let export_entry = optional_header_offset + 112;
    bytes[export_entry..export_entry + 4].copy_from_slice(&section_rva.to_le_bytes());
    bytes[export_entry + 4..export_entry + 8].copy_from_slice(&40u32.to_le_bytes());
    bytes[section_table_offset..section_table_offset + 8].copy_from_slice(b".edata\0\0");
    bytes[section_table_offset + 8..section_table_offset + 12]
        .copy_from_slice(&(section.len() as u32).to_le_bytes());
    bytes[section_table_offset + 12..section_table_offset + 16]
        .copy_from_slice(&section_rva.to_le_bytes());
    bytes[section_table_offset + 16..section_table_offset + 20]
        .copy_from_slice(&(section.len() as u32).to_le_bytes());
    bytes[section_table_offset + 20..section_table_offset + 24]
        .copy_from_slice(&(section_raw_ptr as u32).to_le_bytes());
    bytes[section_raw_ptr..].copy_from_slice(&section);
    bytes
}

/// Builds a valid AMD64 PE header for a registered game's executable fixture.
pub(super) fn game_executable_pe() -> Vec<u8> {
    let pe_offset = 0x80usize;
    let coff_offset = pe_offset + 4;
    let optional_header_offset = coff_offset + 20;
    let optional_header_size = 0xF0usize;
    let mut bytes = vec![0u8; optional_header_offset + optional_header_size];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3C..0x40].copy_from_slice(&(pe_offset as u32).to_le_bytes());
    bytes[pe_offset..pe_offset + 4].copy_from_slice(b"PE\0\0");
    bytes[coff_offset..coff_offset + 2].copy_from_slice(&0x8664u16.to_le_bytes());
    bytes[coff_offset + 16..coff_offset + 18]
        .copy_from_slice(&(optional_header_size as u16).to_le_bytes());
    bytes[optional_header_offset..optional_header_offset + 2]
        .copy_from_slice(&0x20Bu16.to_le_bytes());
    bytes
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
