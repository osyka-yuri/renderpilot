//! Fail-closed admission for detected Xiph closures.
//!
//! This module owns legacy rollback ownership and classic sidecar checks before
//! a scan can replace the catalog projection. Managed successor identity
//! reconciliation lives in [`super::xiph_lineage`].

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use renderpilot_application::{ComponentRepository, InstalledAddonRepository};
use renderpilot_domain::{
    ComponentFile, GameInstallation, LibraryComponent, LibraryTechnology, normalized_path_key, xiph,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::ServiceError;

use super::recovery::recover_bak_file_for_technology;

fn is_cross_xiph(component: &LibraryComponent) -> bool {
    component.technology() == LibraryTechnology::XiphVorbis
        && component.spans_multiple_parent_directories()
}

/// Checks all Xiph candidates against current durable rollback ownership before
/// the aggregate scan write. This applies to same-directory and split layouts.
pub(super) fn admit_xiph_components(
    storage: &SqliteStorage,
    game: &GameInstallation,
    components: &[LibraryComponent],
) -> Result<(), ServiceError> {
    let baselines = storage.component_backups_for_game(game.id())?;
    let installed_addon = storage.get_installed_addon(game.id())?;
    let managed_files = crate::coordinated_files::managed_files_of(installed_addon.as_ref());
    let previous_components = storage.list_components_for_game(game.id())?;
    let mut previous_by_id = HashMap::new();
    for component in &previous_components {
        previous_by_id
            .entry(component.id().as_str())
            .or_insert(component);
    }
    let mut protected_paths_by_id = Vec::with_capacity(baselines.len());
    for (old_id, baseline) in &baselines {
        let mut protected = normalized_paths(baseline.files());
        protected.extend(normalized_paths(baseline.expected_active_files()));
        if let Some(component) = previous_by_id.get(old_id.as_str()) {
            protected.extend(normalized_paths(component.files()));
        }
        protected_paths_by_id.push((old_id, protected));
    }

    for candidate in components
        .iter()
        .filter(|component| component.technology() == LibraryTechnology::XiphVorbis)
    {
        let has_established_same_id_baseline = if let Some(same_id) = baselines.get(candidate.id())
        {
            crate::coordinated_files::validate_recorded_xiph_baseline(
                candidate.technology(),
                candidate.files(),
                same_id.files(),
                managed_files,
            )
            .map_err(|error| {
                ServiceError::command_failed(format!(
                    "cannot admit Xiph component {}: existing rollback baseline is unsafe: {error}",
                    candidate.id()
                ))
            })?;
            true
        } else {
            false
        };

        let candidate_paths = normalized_paths(candidate.files());
        for (old_id, protected) in &protected_paths_by_id {
            if *old_id == candidate.id() {
                continue;
            }
            if candidate_paths.iter().any(|path| protected.contains(path)) {
                return Err(ServiceError::command_failed(format!(
                    "cannot admit Xiph component {} because it overlaps legacy rollback state for {}; restore or roll back {} before scanning",
                    candidate.id(),
                    old_id,
                    old_id
                )));
            }
        }

        // An established same-ID baseline has already bound its immutable
        // originals, including vendor-named reserved paths. Looking only for
        // sidecars beside the new canonical active paths would misclassify
        // that valid state as an orphaned partial set.
        if !has_established_same_id_baseline && is_cross_xiph(candidate) {
            admit_classic_sidecars(candidate)?;
        }
    }
    Ok(())
}

/// Reappearance has already established that the stored game was durably
/// absent. Its component baselines, installed add-on bindings, and prior
/// component identities may be stale and are cleared atomically with Complete
/// publication. Validate the fresh cross-directory Xiph sidecar evidence
/// without consulting or importing those retired catalog records.
pub(super) fn admit_reactivated_xiph_components(
    components: &[LibraryComponent],
) -> Result<(), ServiceError> {
    for candidate in components
        .iter()
        .filter(|component| is_cross_xiph(component))
    {
        admit_classic_sidecars(candidate)?;
    }
    Ok(())
}

fn normalized_paths(files: &[ComponentFile]) -> BTreeSet<String> {
    files
        .iter()
        .map(|file| normalized_path_key(file.path().as_str()))
        .collect()
}

fn admit_classic_sidecars(candidate: &LibraryComponent) -> Result<(), ServiceError> {
    let sidecars = candidate
        .files()
        .iter()
        .map(|file| {
            crate::fs::backup_path(Path::new(file.path().as_str())).map_err(|error| {
                ServiceError::command_failed(format!(
                    "cannot derive Xiph rollback sidecar for {}: {error}",
                    candidate.id()
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let present = sidecars.iter().filter(|path| path.exists()).count();
    if present == 0 {
        return Ok(());
    }
    if present != sidecars.len() {
        return Err(ServiceError::command_failed(format!(
            "cannot admit cross-directory Xiph component {}: only part of its classic rollback sidecar set exists; restore or remove the incomplete legacy state first",
            candidate.id()
        )));
    }

    let recovered = candidate
        .files()
        .iter()
        .zip(sidecars)
        .map(|(file, sidecar)| {
            recover_bak_file_for_technology(
                &sidecar,
                file.path().as_str(),
                LibraryTechnology::XiphVorbis,
            )
            .map_err(ServiceError::from)?
            .ok_or_else(|| {
                ServiceError::command_failed(format!(
                    "Xiph rollback sidecar disappeared during admission for {}",
                    candidate.id()
                ))
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let Some(layout) = xiph::detect_layout(&recovered) else {
        return Err(ServiceError::command_failed(format!(
            "cannot admit cross-directory Xiph component {}: classic rollback sidecars are not one complete valid Xiph layout",
            candidate.id()
        )));
    };
    let expected = xiph_members(candidate.files())?;
    let actual = layout.members().collect::<BTreeSet<_>>();
    if actual != expected {
        return Err(ServiceError::command_failed(format!(
            "cannot admit cross-directory Xiph component {}: classic rollback sidecars do not cover its exact semantic members",
            candidate.id()
        )));
    }
    Ok(())
}

fn xiph_members(files: &[ComponentFile]) -> Result<BTreeSet<xiph::XiphMember>, ServiceError> {
    let mut members = BTreeSet::new();
    for file in files {
        let name = file
            .install_as()
            .or_else(|| file.path().file_name())
            .ok_or_else(|| {
                ServiceError::command_failed("Xiph component file has no runtime basename")
            })?;
        let member = xiph::parse_runtime_file_name(name)
            .ok()
            .flatten()
            .ok_or_else(|| {
                ServiceError::command_failed(format!(
                    "Xiph component contains an unsupported runtime basename: {name}"
                ))
            })?
            .member();
        if !members.insert(member) {
            return Err(ServiceError::command_failed(format!(
                "Xiph component contains duplicate semantic member {}",
                member.as_slug()
            )));
        }
    }
    Ok(members)
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{ComponentRepository, GameRepository};
    use renderpilot_domain::{
        ComponentId, ComponentKind, ComponentRollbackBaseline, GameId, GameIdentity, GameRuntime,
        Launcher, PathRef, Platform, Swappability,
    };

    use super::*;
    use crate::catalog::scan::xiph_test_support::{
        complete_split_component, file, split_component, split_component_at, vendor_xiph_files,
    };

    #[test]
    fn legacy_baseline_overlap_blocks_admission_before_catalog_mutation() {
        let game = GameInstallation::new(
            GameIdentity::new(
                GameId::new("manual:xiph-admission").expect("game id"),
                "Xiph admission",
                Launcher::Manual,
            )
            .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new("C:/Game").expect("root"),
        );
        let old_id = ComponentId::new("component:legacy-xiph").expect("old id");
        let old_file = file("C:/Game/Plugin/vorbisfile.dll", 'a');
        let old_component = LibraryComponent::new(
            old_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::XiphVorbis,
            Swappability::BundleOnly,
        )
        .with_file(old_file.clone());
        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_game(&game).expect("game");
        storage
            .replace_components_for_game(game.id(), std::slice::from_ref(&old_component))
            .expect("old component");
        storage
            .recover_component_rollback_baseline(
                game.id(),
                &old_id,
                &ComponentRollbackBaseline::new(vec![old_file]),
            )
            .expect("baseline");
        let candidate = split_component(&game);

        let error = admit_xiph_components(&storage, &game, std::slice::from_ref(&candidate))
            .expect_err("legacy overlap must block");
        assert!(error.to_string().contains(old_id.as_str()));
        assert_eq!(
            storage
                .list_components_for_game(game.id())
                .expect("components"),
            vec![old_component],
            "admission performs no catalog write"
        );
    }

    #[test]
    fn reactivated_install_ignores_retired_xiph_baselines_but_checks_fresh_sidecars() {
        let temp = tempfile::tempdir().expect("root");
        let root = temp.path().join("Game");
        let game = GameInstallation::new(
            GameIdentity::new(
                GameId::new("manual:xiph-reactivated").expect("game id"),
                "Xiph reappeared",
                Launcher::Manual,
            )
            .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(root.to_string_lossy().replace('\\', "/")).expect("root"),
        );
        let candidate = split_component_at(
            game.id().as_str(),
            &root.to_string_lossy().replace('\\', "/"),
        );
        let stale_id = ComponentId::new("component:retired-xiph").expect("stale id");
        let stale_component = LibraryComponent::new(
            stale_id.clone(),
            game.id().clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::XiphVorbis,
            Swappability::BundleOnly,
        )
        .with_file(candidate.files()[0].clone());
        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_game(&game).expect("game");
        storage
            .replace_components_for_game(game.id(), std::slice::from_ref(&stale_component))
            .expect("retired component");
        storage
            .recover_component_rollback_baseline(
                game.id(),
                &stale_id,
                &ComponentRollbackBaseline::new(vec![candidate.files()[0].clone()]),
            )
            .expect("retired baseline");

        assert!(admit_xiph_components(&storage, &game, std::slice::from_ref(&candidate)).is_err());
        admit_reactivated_xiph_components(std::slice::from_ref(&candidate))
            .expect("retired catalog baseline does not veto current scan");

        let first_sidecar = std::path::Path::new(candidate.files()[0].path().as_str());
        std::fs::create_dir_all(first_sidecar.parent().expect("parent"))
            .expect("create first member parent");
        std::fs::write(
            crate::fs::backup_path(first_sidecar).expect("first sidecar"),
            b"only one retired backup",
        )
        .expect("write partial backup");
        assert!(
            admit_reactivated_xiph_components(std::slice::from_ref(&candidate)).is_err(),
            "fresh sidecar validation still rejects incomplete split Xiph backups"
        );
    }

    #[test]
    fn no_classic_sidecars_admits_a_split_xiph_component() {
        let root = tempfile::tempdir().expect("root");
        let component = split_component_at(
            "game:sidecar-none",
            &format!("{}/Game", root.path().to_string_lossy()),
        );

        admit_classic_sidecars(&component).expect("no sidecars is a fresh baseline");
    }

    #[test]
    fn partial_classic_sidecars_are_rejected() {
        let root = tempfile::tempdir().expect("root");
        let component = split_component_at(
            "game:sidecar-partial",
            &format!("{}/Game", root.path().to_string_lossy()),
        );
        let first = std::path::Path::new(component.files()[0].path().as_str());
        std::fs::create_dir_all(first.parent().expect("parent")).expect("parent dirs");
        std::fs::write(
            crate::fs::backup_path(first).expect("sidecar"),
            b"orphaned-original",
        )
        .expect("sidecar");

        assert!(admit_classic_sidecars(&component).is_err());
    }

    #[test]
    fn established_same_id_vendor_baseline_does_not_reinspect_canonical_sidecars() {
        let root = tempfile::tempdir().expect("root");
        let game = GameInstallation::new(
            GameIdentity::new(
                GameId::new("manual:xiph-established").expect("game id"),
                "Established Xiph",
                Launcher::Manual,
            )
            .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(root.path().to_string_lossy().into_owned()).expect("root"),
        );
        let candidate = complete_split_component(&game);
        let first = std::path::Path::new(candidate.files()[0].path().as_str());
        std::fs::create_dir_all(first.parent().expect("parent")).expect("parent dirs");
        // This would be a partial sidecar set if interpreted as an orphaned
        // canonical deployment. The established vendor baseline makes that
        // inference invalid: its reserved originals use vendor paths.
        std::fs::write(
            crate::fs::backup_path(first).expect("sidecar"),
            b"unrelated",
        )
        .expect("sidecar");

        let storage = SqliteStorage::in_memory().expect("storage");
        storage.upsert_game(&game).expect("game");
        storage
            .replace_components_for_game(game.id(), std::slice::from_ref(&candidate))
            .expect("component");
        storage
            .recover_component_rollback_baseline(
                game.id(),
                candidate.id(),
                &ComponentRollbackBaseline::new(vendor_xiph_files(game.install_path().as_str())),
            )
            .expect("baseline");
        admit_xiph_components(&storage, &game, std::slice::from_ref(&candidate))
            .expect("same-id vendor baseline is established state");
    }
}
