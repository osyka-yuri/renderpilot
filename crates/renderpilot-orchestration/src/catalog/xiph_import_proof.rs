//! Whole-install static-import proof for vendor-suffixed Xiph deployments.
//!
//! Vendor suffixes are a loader contract of one installation, not a catalog
//! naming convention.  This module deliberately has one job: establish the
//! exact external static/delay import aliases before a transition is resolved,
//! and make that observation repeatable at the mutation boundary.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use renderpilot_application::{AppError, AppResult, ExternalAliasRequirements};
use renderpilot_detection::{
    FileIdentityProbeResult, FileObservationResult, FileObservationSource, InstallTreeWalker,
    PeImportInspection, StableFileSnapshot, StrongFileCacheKey, SystemFileObservationSource,
    inspect_pe_bytes,
};
use renderpilot_domain::{ComponentFile, LibraryComponent, xiph};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct XiphImportProof {
    root: PathBuf,
    bindings: Vec<XiphImportBinding>,
    aliases: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ImportKind {
    Regular,
    Delay,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct XiphImportBinding {
    importer: PathBuf,
    sha256: renderpilot_domain::Sha256Hash,
    cache_identity: Option<(String, String, String, u64)>,
    kind: ImportKind,
    alias: String,
}

impl XiphImportProof {
    pub(super) fn requirements(&self) -> ExternalAliasRequirements {
        ExternalAliasRequirements::Proven(self.aliases.clone())
    }
}

/// Request-local import inspection reuse. Every proof still performs a fresh
/// strict tree walk, canonical containment check, and identity probe. Only an
/// equal strong identity may reuse a previously read digest and PE inspection.
#[derive(Debug, Default)]
pub(super) struct XiphImportProofCache {
    entries: Vec<CachedImporterInspection>,
}

#[derive(Debug, Clone)]
struct CachedImporterInspection {
    key: StrongFileCacheKey,
    sha256: renderpilot_domain::Sha256Hash,
    inspection: PeImportInspection,
}

#[derive(Debug)]
enum ImporterInspection<'a> {
    Cached(&'a CachedImporterInspection),
    Observed {
        sha256: renderpilot_domain::Sha256Hash,
        cache_key: Option<StrongFileCacheKey>,
        inspection: PeImportInspection,
    },
}

impl ImporterInspection<'_> {
    fn sha256(&self) -> &renderpilot_domain::Sha256Hash {
        match self {
            Self::Cached(cached) => &cached.sha256,
            Self::Observed { sha256, .. } => sha256,
        }
    }

    fn cache_key(&self) -> Option<&StrongFileCacheKey> {
        match self {
            Self::Cached(cached) => Some(&cached.key),
            Self::Observed { cache_key, .. } => cache_key.as_ref(),
        }
    }

    fn inspection(&self) -> &PeImportInspection {
        match self {
            Self::Cached(cached) => &cached.inspection,
            Self::Observed { inspection, .. } => inspection,
        }
    }
}

/// Proves the exact aliases required by external (non-component) static PE
/// importers.  A readable non-PE is irrelevant; every other uncertainty fails
/// closed because a loader binding could otherwise be silently missed.
pub(super) fn prove_external_aliases(
    game_root: &Path,
    component: &LibraryComponent,
    baseline: &[ComponentFile],
    cache: &mut XiphImportProofCache,
) -> AppResult<Option<XiphImportProof>> {
    prove_external_aliases_impl(
        game_root,
        component,
        baseline,
        &SystemFileObservationSource,
        cache,
    )
}

#[cfg(test)]
pub(super) fn prove_external_aliases_with_source(
    game_root: &Path,
    component: &LibraryComponent,
    baseline: &[ComponentFile],
    source: &dyn FileObservationSource,
) -> AppResult<Option<XiphImportProof>> {
    prove_external_aliases_with_source_and_cache(
        game_root,
        component,
        baseline,
        source,
        &mut XiphImportProofCache::default(),
    )
}

#[cfg(test)]
pub(super) fn prove_external_aliases_with_source_and_cache(
    game_root: &Path,
    component: &LibraryComponent,
    baseline: &[ComponentFile],
    source: &dyn FileObservationSource,
    cache: &mut XiphImportProofCache,
) -> AppResult<Option<XiphImportProof>> {
    prove_external_aliases_impl(game_root, component, baseline, source, cache)
}

fn prove_external_aliases_impl(
    game_root: &Path,
    component: &LibraryComponent,
    baseline: &[ComponentFile],
    source: &dyn FileObservationSource,
    cache: &mut XiphImportProofCache,
) -> AppResult<Option<XiphImportProof>> {
    let aliases = vendor_aliases(component.files().iter().chain(baseline));
    if aliases.is_empty() {
        return Ok(None);
    }

    let root = std::fs::canonicalize(game_root).map_err(|error| {
        AppError::invalid_input(format!(
            "cannot establish canonical game root for Xiph import proof {}: {error}",
            game_root.display()
        ))
    })?;
    let excluded = component
        .files()
        .iter()
        .chain(baseline)
        .map(|file| PathBuf::from(file.path().as_str()))
        .collect::<Vec<_>>();
    let report = InstallTreeWalker::full_strict().walk_filtered(&root, is_executable_or_dll)?;
    if !report.diagnostics().is_empty() {
        return Err(AppError::invalid_input(
            "Xiph import proof requires a complete strict walk of the game root",
        ));
    }

    let mut bindings = Vec::new();
    for path in report.files() {
        if excluded
            .iter()
            .any(|excluded| crate::paths::same_path(path, excluded))
        {
            continue;
        }
        let canonical = canonical_importer_within_root(&root, path)?;
        let inspection = inspect_importer(source, cache, &canonical)?;
        append_import_bindings(&mut bindings, &aliases, &canonical, &inspection)?;
    }
    bindings.sort_by(|left, right| {
        (
            crate::paths::normalized_key(&left.importer),
            left.kind,
            &left.alias,
            left.sha256.as_str(),
        )
            .cmp(&(
                crate::paths::normalized_key(&right.importer),
                right.kind,
                &right.alias,
                right.sha256.as_str(),
            ))
    });
    let required = bindings
        .iter()
        .map(|binding| binding.alias.clone())
        .collect();
    Ok(Some(XiphImportProof {
        root,
        bindings,
        aliases: required,
    }))
}

/// Observes enough current state to prove one external importer, reusing only
/// an exact strong identity established earlier in this request.
fn inspect_importer<'a>(
    source: &dyn FileObservationSource,
    cache: &'a mut XiphImportProofCache,
    path: &Path,
) -> AppResult<ImporterInspection<'a>> {
    match source.probe_identity(path)? {
        FileIdentityProbeResult::Available(key) => {
            if let Some(entry_index) = cache.entries.iter().position(|cached| cached.key == key) {
                return Ok(ImporterInspection::Cached(&cache.entries[entry_index]));
            }
            let snapshot = observe_importer(source, path)?;
            if snapshot.cache_key.as_ref() != Some(&key) {
                return Err(AppError::invalid_input(format!(
                    "Xiph import proof importer identity changed during observation: {}",
                    path.display()
                )));
            }
            let inspection = inspection_for_snapshot(&snapshot);
            let entry_index = cache.entries.len();
            cache.entries.push(CachedImporterInspection {
                key,
                sha256: snapshot.sha256,
                inspection,
            });
            Ok(ImporterInspection::Cached(&cache.entries[entry_index]))
        }
        FileIdentityProbeResult::Uncacheable => {
            let snapshot = observe_importer(source, path)?;
            let inspection = inspection_for_snapshot(&snapshot);
            Ok(ImporterInspection::Observed {
                sha256: snapshot.sha256,
                cache_key: snapshot.cache_key,
                inspection,
            })
        }
        FileIdentityProbeResult::Missing | FileIdentityProbeResult::Unavailable => {
            Err(AppError::invalid_input(format!(
                "Xiph import proof cannot stably observe importer {}",
                path.display()
            )))
        }
    }
}

fn observe_importer(
    source: &dyn FileObservationSource,
    path: &Path,
) -> AppResult<StableFileSnapshot> {
    match source.observe(path)? {
        FileObservationResult::Available(snapshot) => Ok(snapshot),
        FileObservationResult::Missing | FileObservationResult::Unavailable => {
            Err(AppError::invalid_input(format!(
                "Xiph import proof cannot stably observe importer {}",
                path.display()
            )))
        }
    }
}

/// Re-resolves each walker result immediately before observation.  A directory
/// reparse point can change between the strict walk and this stable read, so a
/// previously in-root lexical path is not sufficient authority for the read.
fn canonical_importer_within_root(root: &Path, path: &Path) -> AppResult<PathBuf> {
    let canonical = std::fs::canonicalize(path).map_err(|error| {
        AppError::invalid_input(format!(
            "Xiph import proof cannot canonicalize importer {}: {error}",
            path.display()
        ))
    })?;
    if !crate::paths::is_within(&canonical, root) {
        return Err(AppError::invalid_input(format!(
            "Xiph import proof importer escaped canonical game root: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

pub(super) fn require_same_external_alias_proof(
    expected: Option<&XiphImportProof>,
    game_root: &Path,
    component: &LibraryComponent,
    baseline: &[ComponentFile],
    cache: &mut XiphImportProofCache,
) -> AppResult<()> {
    let observed = prove_external_aliases(game_root, component, baseline, cache)?;
    if observed.as_ref() != expected {
        return Err(AppError::invalid_input(
            "Xiph external-import proof changed during swap; retry after the installation is stable",
        ));
    }
    Ok(())
}

fn vendor_aliases<'a>(files: impl Iterator<Item = &'a ComponentFile>) -> BTreeSet<String> {
    files
        .filter_map(|file| file.path().file_name())
        .filter_map(|name| xiph::parse_runtime_file_name(name).ok().flatten())
        .filter(|name| name.is_vendor())
        .map(|name| name.normalized_name().to_owned())
        .collect()
}

fn add_bindings(
    bindings: &mut Vec<XiphImportBinding>,
    aliases: &BTreeSet<String>,
    importer: &Path,
    sha256: &renderpilot_domain::Sha256Hash,
    cache_key: Option<&StrongFileCacheKey>,
    kind: ImportKind,
    imports: &[String],
) {
    for imported_name in imports {
        let Some(alias) = aliases
            .iter()
            .find(|alias| alias.eq_ignore_ascii_case(imported_name))
        else {
            continue;
        };
        bindings.push(XiphImportBinding {
            importer: importer.to_path_buf(),
            sha256: sha256.clone(),
            cache_identity: cache_key.map(|key| {
                (
                    key.kind.clone(),
                    key.object_identity.clone(),
                    key.change_token.clone(),
                    key.size,
                )
            }),
            kind,
            alias: alias.clone(),
        });
    }
}

#[cfg(test)]
fn append_import_bindings_from_snapshot(
    bindings: &mut Vec<XiphImportBinding>,
    aliases: &BTreeSet<String>,
    importer: &Path,
    snapshot: &StableFileSnapshot,
    inspection: &PeImportInspection,
) -> AppResult<()> {
    append_import_bindings_parts(
        bindings,
        aliases,
        importer,
        &snapshot.sha256,
        snapshot.cache_key.as_ref(),
        inspection,
    )
}

fn inspection_for_snapshot(snapshot: &StableFileSnapshot) -> PeImportInspection {
    inspect_pe_bytes(&snapshot.bytes).import_inspection()
}

fn append_import_bindings(
    bindings: &mut Vec<XiphImportBinding>,
    aliases: &BTreeSet<String>,
    importer: &Path,
    observation: &ImporterInspection<'_>,
) -> AppResult<()> {
    append_import_bindings_parts(
        bindings,
        aliases,
        importer,
        observation.sha256(),
        observation.cache_key(),
        observation.inspection(),
    )
}

fn append_import_bindings_parts(
    bindings: &mut Vec<XiphImportBinding>,
    aliases: &BTreeSet<String>,
    importer: &Path,
    sha256: &renderpilot_domain::Sha256Hash,
    cache_key: Option<&StrongFileCacheKey>,
    inspection: &PeImportInspection,
) -> AppResult<()> {
    match inspection {
        PeImportInspection::NotPe => Ok(()),
        PeImportInspection::Malformed(_) => Err(AppError::invalid_input(format!(
            "Xiph import proof found malformed PE imports in {}",
            importer.display()
        ))),
        PeImportInspection::Complete(imports) => {
            add_bindings(
                bindings,
                aliases,
                importer,
                sha256,
                cache_key,
                ImportKind::Regular,
                imports.regular.names(),
            );
            add_bindings(
                bindings,
                aliases,
                importer,
                sha256,
                cache_key,
                ImportKind::Delay,
                imports.delay.names(),
            );
            Ok(())
        }
    }
}

fn is_executable_or_dll(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        extension.eq_ignore_ascii_case("exe") || extension.eq_ignore_ascii_case("dll")
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use renderpilot_application::AppResult;
    use renderpilot_detection::{
        FileIdentityProbeResult, FileObservationResult, FileObservationSource, PeImportError,
        StableFileSnapshot, StrongFileCacheKey, sha256_bytes,
    };
    use renderpilot_domain::{
        ComponentFile, ComponentId, ComponentKind, GameId, LibraryComponent, LibraryTechnology,
        PathRef, PeImportProfile, PeImportSet, Swappability,
    };

    use super::{
        ImportKind, XiphImportBinding, XiphImportProof, XiphImportProofCache,
        append_import_bindings_from_snapshot, canonical_importer_within_root,
        prove_external_aliases_with_source, prove_external_aliases_with_source_and_cache,
    };

    fn vendor_component(root: &Path) -> LibraryComponent {
        LibraryComponent::new(
            ComponentId::new("component:xiph-proof").expect("component"),
            GameId::new("game:xiph-proof").expect("game"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::XiphVorbis,
            Swappability::BundleOnly,
        )
        .with_file(ComponentFile::new(
            PathRef::new(
                root.join("vorbisfile_vs2010_x64_rwdi.dll")
                    .to_string_lossy()
                    .into_owned(),
            )
            .expect("path"),
        ))
    }

    fn snapshot(bytes: &[u8]) -> StableFileSnapshot {
        StableFileSnapshot {
            cache_key: None,
            sha256: sha256_bytes(bytes).expect("hash"),
            bytes: bytes.to_vec(),
        }
    }

    #[test]
    fn static_and_delay_imports_bind_only_exact_vendor_aliases() {
        let aliases = BTreeSet::from(["vorbisfile_vs2010_x64_rwdi.dll".to_owned()]);
        let importer = PathBuf::from("C:/Game/engine.exe");
        let profile = PeImportProfile {
            regular: PeImportSet::from_observed_names(vec![
                "KERNEL32.dll".to_owned(),
                "vorbisfile_vs2010_x64_rwdi.dll".to_owned(),
            ])
            .expect("regular"),
            delay: PeImportSet::from_observed_names(vec![
                "vorbisfile_vs2010_x64_rwdi.dll".to_owned(),
            ])
            .expect("delay"),
        };
        let mut bindings = Vec::new();
        let inspection = renderpilot_detection::PeImportInspection::Complete(profile);
        append_import_bindings_from_snapshot(
            &mut bindings,
            &aliases,
            &importer,
            &snapshot(b"one stable importer"),
            &inspection,
        )
        .expect("complete proof");
        assert_eq!(bindings.len(), 2);
        assert!(
            bindings
                .iter()
                .any(|binding| binding.kind == ImportKind::Regular)
        );
        assert!(
            bindings
                .iter()
                .any(|binding| binding.kind == ImportKind::Delay)
        );
    }

    #[test]
    fn definite_non_pe_is_ignored_but_malformed_pe_blocks_the_proof() {
        let aliases = BTreeSet::from(["vorbisfile_vs2010_x64_rwdi.dll".to_owned()]);
        let importer = PathBuf::from("C:/Game/engine.exe");
        let mut bindings = Vec::new();
        let non_pe = renderpilot_detection::PeImportInspection::NotPe;
        append_import_bindings_from_snapshot(
            &mut bindings,
            &aliases,
            &importer,
            &snapshot(b"not PE"),
            &non_pe,
        )
        .expect("non-PE is irrelevant");
        assert!(bindings.is_empty());
        assert!(
            append_import_bindings_from_snapshot(
                &mut bindings,
                &aliases,
                &importer,
                &snapshot(b"malformed"),
                &renderpilot_detection::PeImportInspection::Malformed(PeImportError),
            )
            .is_err()
        );
    }

    #[test]
    fn strict_walk_includes_normally_excluded_git_descendants() {
        let root = tempfile::tempdir().expect("root");
        fs::create_dir(root.path().join(".git")).expect("git dir");
        fs::write(root.path().join("game.exe"), b"non-pe").expect("exe");
        fs::write(root.path().join(".git").join("loader.dll"), b"non-pe").expect("dll");
        let component = vendor_component(root.path());
        let error = prove_external_aliases_with_source(
            root.path(),
            &component,
            component.files(),
            &GitRejectingSource,
        )
        .expect_err("strict proof must observe .git/loader.dll");
        assert!(error.to_string().contains("cannot stably observe importer"));
    }

    #[test]
    fn canonical_importer_must_remain_within_the_canonical_game_root() {
        let root = tempfile::tempdir().expect("root");
        let outside = tempfile::tempdir().expect("outside");
        let escaped = outside.path().join("loader.dll");
        fs::write(&escaped, b"outside importer").expect("outside importer");
        let canonical_root = fs::canonicalize(root.path()).expect("canonical root");

        let error = canonical_importer_within_root(&canonical_root, &escaped)
            .expect_err("out-of-root importer must be rejected before observation");
        assert!(error.to_string().contains("escaped canonical game root"));
    }

    #[test]
    fn proof_equality_binds_root_and_snapshot_identity() {
        let binding = XiphImportBinding {
            importer: PathBuf::from("C:/Game/engine.exe"),
            sha256: snapshot(b"a").sha256,
            cache_identity: None,
            kind: ImportKind::Regular,
            alias: "vorbisfile_vs2010_x64_rwdi.dll".to_owned(),
        };
        let first = XiphImportProof {
            root: PathBuf::from("C:/Game"),
            bindings: vec![binding.clone()],
            aliases: BTreeSet::from([binding.alias.clone()]),
        };
        let changed_root = XiphImportProof {
            root: PathBuf::from("D:/Game"),
            bindings: vec![binding.clone()],
            aliases: first.aliases.clone(),
        };
        let changed_binding = XiphImportProof {
            root: first.root.clone(),
            bindings: vec![XiphImportBinding {
                sha256: snapshot(b"b").sha256,
                ..binding
            }],
            aliases: first.aliases.clone(),
        };
        assert_ne!(first, changed_root);
        assert_ne!(first, changed_binding);
    }

    #[test]
    fn equal_strong_importer_identity_reuses_only_the_prior_inspection() {
        let root = tempfile::tempdir().expect("root");
        fs::write(root.path().join("engine.exe"), b"loader").expect("loader");
        let component = vendor_component(root.path());
        let source = StrongSource::new();
        let mut cache = XiphImportProofCache::default();

        let first = prove_external_aliases_with_source_and_cache(
            root.path(),
            &component,
            component.files(),
            &source,
            &mut cache,
        )
        .expect("first proof");
        let second = prove_external_aliases_with_source_and_cache(
            root.path(),
            &component,
            component.files(),
            &source,
            &mut cache,
        )
        .expect("fresh reproof");

        assert_eq!(first, second);
        assert_eq!(source.observations.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn changed_strong_importer_identity_forces_a_fresh_observation() {
        let root = tempfile::tempdir().expect("root");
        fs::write(root.path().join("engine.exe"), b"loader").expect("loader");
        let component = vendor_component(root.path());
        let source = StrongSource::new();
        let mut cache = XiphImportProofCache::default();

        prove_external_aliases_with_source_and_cache(
            root.path(),
            &component,
            component.files(),
            &source,
            &mut cache,
        )
        .expect("first proof");
        source.advance_generation();
        prove_external_aliases_with_source_and_cache(
            root.path(),
            &component,
            component.files(),
            &source,
            &mut cache,
        )
        .expect("changed-key reproof");

        assert_eq!(source.observations.load(Ordering::Relaxed), 2);
    }

    struct GitRejectingSource;

    impl FileObservationSource for GitRejectingSource {
        fn observe(&self, path: &Path) -> AppResult<FileObservationResult> {
            if path
                .components()
                .any(|component| component.as_os_str().eq_ignore_ascii_case(".git"))
            {
                return Ok(FileObservationResult::Unavailable);
            }
            Ok(FileObservationResult::Available(snapshot(
                b"definitely not pe",
            )))
        }
    }

    struct StrongSource {
        generation: AtomicUsize,
        observations: AtomicUsize,
    }

    impl StrongSource {
        fn new() -> Self {
            Self {
                generation: AtomicUsize::new(0),
                observations: AtomicUsize::new(0),
            }
        }

        fn key(&self) -> StrongFileCacheKey {
            StrongFileCacheKey {
                kind: "test".to_owned(),
                object_identity: "object".to_owned(),
                change_token: self.generation.load(Ordering::Relaxed).to_string(),
                size: 7,
            }
        }

        fn advance_generation(&self) {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
    }

    impl FileObservationSource for StrongSource {
        fn observe(&self, _path: &Path) -> AppResult<FileObservationResult> {
            self.observations.fetch_add(1, Ordering::Relaxed);
            Ok(FileObservationResult::Available(StableFileSnapshot {
                cache_key: Some(self.key()),
                sha256: sha256_bytes(b"not pe!").expect("hash"),
                bytes: b"not pe!".to_vec(),
            }))
        }

        fn probe_identity(&self, _path: &Path) -> AppResult<FileIdentityProbeResult> {
            Ok(FileIdentityProbeResult::Available(self.key()))
        }
    }
}
