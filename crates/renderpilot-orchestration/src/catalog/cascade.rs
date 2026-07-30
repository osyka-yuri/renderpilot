//! Compound catalog rollback driven by disappearing owned managed paths.
//!
//! Feature-neutral: any tool that records owned [`ManagedAddonFile`] paths can
//! request whole-component restore when those paths leave the game tree.
//! Tool-specific composition (which owned paths disappear) stays in the tool.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use renderpilot_application::{
    AppError, AppResult, ComponentRepository, InstalledAddonRepository, OperationKind,
};
use renderpilot_domain::{
    ComponentFile, ComponentId, ComponentRollbackBaseline, GameId, LibraryComponent,
    component_version_report, fsr,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::catalog::execute::{
    JournalEntryItem, JournalEntryParams, ROLLBACK_TARGET_LABEL, record_operation_journal_entry,
    revert_to_baseline_fs,
};

/// Full catalog component rollback selected by an owned managed-file intersection.
#[derive(Debug)]
pub(crate) struct ValidatedRollbackPlan {
    component: LibraryComponent,
    rollback_baseline: ComponentRollbackBaseline,
}

/// Named result of [`cascade_for_owned_paths`].
pub(crate) struct CascadeResult {
    pub(crate) rollback_specs: Vec<ValidatedRollbackPlan>,
    pub(crate) next_components: Vec<LibraryComponent>,
    pub(crate) mutation_paths: Vec<PathBuf>,
}

impl ValidatedRollbackPlan {
    pub(crate) fn component_id(&self) -> &ComponentId {
        self.component.id()
    }

    pub(crate) fn contains_path(&self, path: &Path) -> bool {
        let path = crate::paths::normalized_key(path);
        self.component
            .files()
            .iter()
            .chain(self.rollback_baseline.files())
            .any(|file| crate::paths::normalized_key(Path::new(file.path().as_str())) == path)
    }

    fn files(&self) -> &[ComponentFile] {
        self.rollback_baseline.files()
    }
}

/// Selects whole component bundles when any active member intersects an owned path.
pub(crate) fn cascade_rollback_specs(
    storage: &SqliteStorage,
    game_id: &GameId,
    owned_paths: &[PathBuf],
) -> AppResult<Vec<ValidatedRollbackPlan>> {
    if owned_paths.is_empty() {
        return Ok(Vec::new());
    }
    let game_root = crate::catalog::game_root_for_mutation(
        storage,
        game_id,
        owned_paths
            .first()
            .and_then(|path| path.parent().map(Path::to_path_buf)),
    )?;
    let managed_files =
        crate::coordinated_files::managed_files_of(storage.get_installed_addon(game_id)?.as_ref())
            .to_vec();
    let owned: HashSet<String> = owned_paths
        .iter()
        .map(|path| crate::paths::normalized_key(path))
        .collect();
    let mut specs = Vec::new();
    for component in storage.list_components_for_game(game_id)? {
        let Some(baseline) =
            crate::coordinated_files::load_component_backup_availability(storage, &component)?
                .into_available()
        else {
            continue;
        };
        let intersects = component
            .files()
            .iter()
            .chain(baseline.files())
            .map(|file| crate::paths::normalized_key(Path::new(file.path().as_str())))
            .any(|path| owned.contains(&path));
        if intersects {
            if baseline.d3d12_executable().is_some() {
                return Err(AppError::invalid_input(format!(
                    "component {} has auxiliary rollback state; fully roll it back before an add-on can consume its managed files",
                    component.id().as_str()
                )));
            }
            let component =
                crate::coordinated_files::current_component_snapshot(&component, &managed_files)
                    .map_err(|error| {
                        AppError::invalid_input(format!(
                            "cannot validate active component {} for cascade rollback: {error}",
                            component.id().as_str()
                        ))
                    })?
                    .into_component();
            let resolved_files = crate::coordinated_files::resolve_component_baseline(
                &game_root,
                component.technology(),
                component.files(),
                Some(baseline.files()),
                &managed_files,
            )
            .map_err(|error| {
                AppError::invalid_input(format!(
                    "cannot validate baseline for cascade rollback {}: {error}",
                    component.id().as_str()
                ))
            })?;
            let rollback_baseline = ComponentRollbackBaseline::new(resolved_files);
            specs.push(ValidatedRollbackPlan {
                component,
                rollback_baseline,
            });
        }
    }
    Ok(specs)
}

pub(crate) fn cascade_next_components(
    storage: &SqliteStorage,
    game_id: &GameId,
    specs: &[ValidatedRollbackPlan],
) -> AppResult<Vec<LibraryComponent>> {
    let mut components = storage.list_components_for_game(game_id)?;
    for spec in specs {
        if spec.files().is_empty() {
            components.retain(|component| component.id() != spec.component.id());
            continue;
        }
        let mut restored = spec.files().to_vec();
        fsr::sort_representative_first(&mut restored);
        let rebuilt = spec.component.rebuild_with_files(restored);
        if let Some(component) = components
            .iter_mut()
            .find(|component| component.id() == rebuilt.id())
        {
            *component = rebuilt;
        }
    }
    Ok(components)
}

pub(crate) fn cascade_mutation_paths(specs: &[ValidatedRollbackPlan]) -> Vec<PathBuf> {
    crate::catalog::execute::mutation_paths_from_component_files(
        specs
            .iter()
            .flat_map(|spec| spec.component.files().iter().chain(spec.files())),
    )
}

/// Plans cascade rollback for any owned managed paths that are about to leave
/// the game tree: validated specs, the post-cascade component set, and
/// live/sidecar mutation paths.
///
/// Path selection is the caller's responsibility:
/// - full uninstall → all [`crate::addons::records::owned_managed_paths`]
/// - Luma update when payload drops DLSS →
///   [`crate::addons::luma::dlss::cascade_for_disappearing_owned`]
/// - mutation-path snapshotting may intentionally use a wider owned set than
///   the apply-time cascade plan
pub(crate) fn cascade_for_owned_paths(
    storage: &SqliteStorage,
    game_id: &GameId,
    owned_paths: &[PathBuf],
) -> AppResult<CascadeResult> {
    let rollback_specs = cascade_rollback_specs(storage, game_id, owned_paths)?;
    let next_components = cascade_next_components(storage, game_id, &rollback_specs)?;
    let mutation_paths = cascade_mutation_paths(&rollback_specs);
    Ok(CascadeResult {
        rollback_specs,
        next_components,
        mutation_paths,
    })
}

pub(crate) fn apply_cascade_rollback_fs(specs: &[ValidatedRollbackPlan]) -> AppResult<()> {
    for spec in specs {
        revert_to_baseline_fs(spec.component.files(), spec.files())?;
    }
    Ok(())
}

pub(crate) fn record_cascade_rollback_journal(
    storage: &SqliteStorage,
    game_id: &GameId,
    specs: &[ValidatedRollbackPlan],
) {
    for spec in specs {
        // Owned so the version string outlives the temporary version report.
        let to_version = cascade_rollback_to_version(spec);
        record_operation_journal_entry(
            storage,
            JournalEntryParams {
                game_id,
                component_id: spec.component.id(),
                kind: OperationKind::RollbackComponent,
                component: &spec.component,
                to_version: Some(to_version.as_str()),
                items: cascade_rollback_journal_items(spec),
                d3d12_executable_action: None,
            },
        );
    }
}

fn cascade_rollback_to_version(spec: &ValidatedRollbackPlan) -> String {
    component_version_report(spec.files(), spec.component.technology())
        .known_version()
        .map(|version| version.as_str().to_owned())
        .unwrap_or_else(|| ROLLBACK_TARGET_LABEL.to_owned())
}

fn cascade_rollback_journal_items(spec: &ValidatedRollbackPlan) -> Vec<JournalEntryItem<'_>> {
    spec.files()
        .iter()
        .map(|file| JournalEntryItem::component_file(file.path(), None))
        .collect()
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{ComponentRepository, GameRepository};
    use renderpilot_domain::{
        ComponentFile, ComponentId, ComponentKind, ComponentRollbackBaseline,
        D3d12ExecutableBaseline, D3d12ExecutableIdentity, GameId, GameIdentity, GameInstallation,
        GameRuntime, Launcher, LibraryComponent, LibraryTechnology, PathRef, Platform,
        Swappability,
    };
    use renderpilot_storage_sqlite::SqliteStorage;

    use super::cascade_rollback_specs;

    #[test]
    fn cascade_never_consumes_a_component_with_auxiliary_rollback_state() {
        let root = tempfile::tempdir().expect("root");
        let runtime = root.path().join("D3D12Core.dll");
        let executable = root.path().join("game.exe");
        std::fs::write(&runtime, b"original runtime").expect("runtime");
        std::fs::write(&executable, b"original executable").expect("executable");

        let game = GameInstallation::new(
            GameIdentity::new(
                GameId::new("manual:cascade-d3d12").expect("game id"),
                "Cascade D3D12",
                Launcher::Manual,
            )
            .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            path_ref(root.path()),
        )
        .with_executable_candidate(path_ref(&executable));
        let component_id = ComponentId::new("component:cascade-d3d12").expect("component id");
        let runtime_file = ComponentFile::new(path_ref(&runtime))
            .with_sha256(renderpilot_detection::sha256_file(&runtime).expect("runtime hash"));
        let component = LibraryComponent::new(
            component_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::D3D12Agility,
            Swappability::Swappable,
        )
        .with_file(runtime_file.clone());
        let executable_hash =
            renderpilot_detection::sha256_file(&executable).expect("executable hash");
        let baseline = ComponentRollbackBaseline::new(vec![runtime_file]).with_d3d12_executable(
            D3d12ExecutableBaseline::new(
                path_ref(&executable),
                D3d12ExecutableIdentity::new(606, executable_hash.clone()),
                D3d12ExecutableIdentity::new(606, executable_hash),
            ),
        );

        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_game(&game).expect("game");
        storage
            .replace_components_for_game(game.id(), std::slice::from_ref(&component))
            .expect("component");
        storage
            .recover_component_rollback_baseline(game.id(), &component_id, &baseline)
            .expect("baseline");

        let error = cascade_rollback_specs(&storage, game.id(), &[runtime])
            .expect_err("cascade must not discard auxiliary state");
        assert!(
            error.message().contains("fully roll it back"),
            "unexpected error: {error}"
        );
        assert_eq!(
            storage
                .get_component_backup(&component_id)
                .expect("query")
                .as_ref(),
            Some(&baseline),
            "rejected cascade must preserve the complete aggregate"
        );
    }

    fn path_ref(path: &std::path::Path) -> PathRef {
        PathRef::new(path.to_string_lossy().into_owned()).expect("path")
    }
}
