//! Durable, atomically published recovery bundles for lossy consolidation.

use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_storage_sqlite::{ConsolidationConflictSummary, ConsolidationPlan, SqliteStorage};

use crate::ServiceError;

mod file_collection;
mod manifest;
mod publication;
mod workspace;

use file_collection::{
    copy_associated_files, copy_one_associated_file, relevant_cover_files, sha256_file,
};
use manifest::{ManagedCleanupRecoveryManifest, RecoveryManifest, RootCorrectionRecoveryManifest};
use publication::{publish_directory, sync_file, write_and_sync};
use workspace::{BundleWorkspace, RecoveryBundleKind};

/// Creates and atomically publishes a recovery bundle before a transaction
/// whose deterministic destination-wins policy would otherwise discard state.
pub(crate) fn create_consolidation_recovery_bundle(
    storage: &SqliteStorage,
    plan: &ConsolidationPlan,
    conflicts: &ConsolidationConflictSummary,
) -> Result<PathBuf, ServiceError> {
    BundleWorkspace::create(storage, RecoveryBundleKind::Consolidation)?.build(|workspace| {
        build_bundle(
            storage,
            plan,
            conflicts,
            workspace.catalog_path(),
            workspace.temporary(),
            workspace.published(),
            workspace.timestamp(),
        )
    })
}

/// Creates a durable database snapshot before root correction removes
/// operation history belonging to components outside the corrected root.
pub(crate) fn create_root_correction_recovery_bundle(
    storage: &SqliteStorage,
    game_id: &str,
    previous_root: &str,
    corrected_root: &str,
    archived_component_ids: &[String],
) -> Result<PathBuf, ServiceError> {
    BundleWorkspace::create(storage, RecoveryBundleKind::RootCorrection)?.build(|workspace| {
        build_root_correction_bundle(
            storage,
            game_id,
            previous_root,
            corrected_root,
            archived_component_ids,
            workspace.temporary(),
            workspace.published(),
            workspace.timestamp(),
        )
    })
}

/// Publishes a durable snapshot before reporting an ambiguous managed cleanup
/// graph. No inverse action may execute when this bundle is required.
pub(crate) fn create_managed_cleanup_recovery_bundle(
    storage: &SqliteStorage,
    game_id: &str,
    ambiguous_targets: &[String],
    associated_paths: &[PathBuf],
) -> Result<PathBuf, ServiceError> {
    BundleWorkspace::create(storage, RecoveryBundleKind::ManagedCleanup)?.build(|workspace| {
        build_managed_cleanup_bundle(
            storage,
            game_id,
            ambiguous_targets,
            associated_paths,
            workspace.temporary(),
            workspace.published(),
            workspace.timestamp(),
        )
    })
}

#[allow(
    clippy::too_many_arguments,
    reason = "bundle inputs are explicit durability boundaries"
)]
fn build_managed_cleanup_bundle(
    storage: &SqliteStorage,
    game_id: &str,
    ambiguous_targets: &[String],
    associated_paths: &[PathBuf],
    temporary: &Path,
    published: &Path,
    timestamp: u128,
) -> Result<PathBuf, ServiceError> {
    let database_snapshot = temporary.join("catalog.db");
    if !storage.copy_catalog_snapshot_to(&database_snapshot)? {
        return Err(ServiceError::command_failed(
            "cannot create a managed-cleanup recovery bundle for in-memory catalog storage",
        ));
    }

    let mut copied = Vec::new();
    let mut missing = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for path in associated_paths {
        if path.is_file() {
            copy_one_associated_file(path, temporary, &mut seen, &mut copied)?;
        } else {
            missing.push(path.to_string_lossy().into_owned());
        }
        if let Ok(sidecar) = crate::fs::backup_path(path)
            && sidecar.is_file()
        {
            copy_one_associated_file(&sidecar, temporary, &mut seen, &mut copied)?;
        }
    }
    copied.sort_by(|left, right| left.original_path.cmp(&right.original_path));
    missing.sort();
    missing.dedup();

    let manifest = ManagedCleanupRecoveryManifest {
        format_version: 1,
        created_at_unix_ms: timestamp,
        game_id: game_id.to_owned(),
        ambiguous_targets: ambiguous_targets.to_vec(),
        copied_associated_files: copied,
        missing_associated_files: missing,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| {
        ServiceError::command_failed(format!(
            "could not serialize managed-cleanup recovery manifest: {error}"
        ))
    })?;
    write_and_sync(&temporary.join("manifest.json"), &manifest_bytes)?;

    let mut protected = vec![PathBuf::from("catalog.db"), PathBuf::from("manifest.json")];
    protected.extend(
        manifest
            .copied_associated_files
            .iter()
            .map(|file| PathBuf::from(&file.bundle_path)),
    );
    protected.sort();
    let mut checksums = String::new();
    for relative in protected {
        checksums.push_str(&format!(
            "{}  {}\n",
            sha256_file(&temporary.join(&relative))?,
            relative.to_string_lossy().replace('\\', "/")
        ));
    }
    write_and_sync(&temporary.join("checksums.sha256"), checksums.as_bytes())?;

    publish_directory(temporary, published, "managed-cleanup recovery bundle")
}

#[allow(
    clippy::too_many_arguments,
    reason = "the bundle manifest and publication paths are explicit durability inputs"
)]
fn build_root_correction_bundle(
    storage: &SqliteStorage,
    game_id: &str,
    previous_root: &str,
    corrected_root: &str,
    archived_component_ids: &[String],
    temporary: &Path,
    published: &Path,
    timestamp: u128,
) -> Result<PathBuf, ServiceError> {
    let database_snapshot = temporary.join("catalog.db");
    if !storage.copy_catalog_snapshot_to(&database_snapshot)? {
        return Err(ServiceError::command_failed(
            "cannot create a root-correction recovery bundle for in-memory catalog storage",
        ));
    }

    let manifest = RootCorrectionRecoveryManifest {
        format_version: 1,
        created_at_unix_ms: timestamp,
        game_id: game_id.to_owned(),
        previous_root: previous_root.to_owned(),
        corrected_root: corrected_root.to_owned(),
        archived_component_ids: archived_component_ids.to_vec(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| {
        ServiceError::command_failed(format!(
            "could not serialize root-correction recovery manifest: {error}"
        ))
    })?;
    write_and_sync(&temporary.join("manifest.json"), &manifest_bytes)?;

    let mut checksums = String::new();
    for relative in ["catalog.db", "manifest.json"] {
        let digest = sha256_file(&temporary.join(relative))?;
        checksums.push_str(&format!("{digest}  {relative}\n"));
    }
    write_and_sync(&temporary.join("checksums.sha256"), checksums.as_bytes())?;

    publish_directory(temporary, published, "root-correction recovery bundle")
}

fn build_bundle(
    storage: &SqliteStorage,
    plan: &ConsolidationPlan,
    conflicts: &ConsolidationConflictSummary,
    catalog_path: &Path,
    temporary: &Path,
    published: &Path,
    timestamp: u128,
) -> Result<PathBuf, ServiceError> {
    let database_snapshot = temporary.join("catalog.db");
    if !storage.copy_catalog_snapshot_to(&database_snapshot)? {
        return Err(ServiceError::command_failed(
            "cannot create a recovery bundle for in-memory catalog storage",
        ));
    }

    let relevant_covers = relevant_cover_files(storage, plan)?;
    let source_cover_dir = crate::covers::covers_directory(catalog_path);
    let bundle_cover_dir = temporary.join("covers");
    let mut copied_cover_files = Vec::new();
    let mut missing_cover_files = Vec::new();

    for file_name in relevant_covers {
        let source = source_cover_dir.join(&file_name);
        if !source.is_file() {
            missing_cover_files.push(file_name);
            continue;
        }
        fs::create_dir_all(&bundle_cover_dir).map_err(|error| {
            ServiceError::command_failed(format!(
                "could not create recovery cover directory {}: {error}",
                bundle_cover_dir.display()
            ))
        })?;
        let destination = bundle_cover_dir.join(&file_name);
        fs::copy(&source, &destination).map_err(|error| {
            ServiceError::command_failed(format!(
                "could not copy recovery cover {}: {error}",
                source.display()
            ))
        })?;
        sync_file(&destination)?;
        copied_cover_files.push(file_name);
    }

    copied_cover_files.sort();
    missing_cover_files.sort();
    let (copied_associated_files, missing_associated_files) =
        copy_associated_files(storage, plan, temporary)?;
    let manifest = RecoveryManifest {
        format_version: 1,
        created_at_unix_ms: timestamp,
        destination_game_id: plan.destination_game_id.as_str().to_owned(),
        source_game_ids: plan
            .sources
            .iter()
            .map(|source| source.source_game_id.as_str().to_owned())
            .collect(),
        conflict_tables: conflicts.recovery_tables(),
        copied_cover_files,
        missing_cover_files,
        copied_associated_files,
        missing_associated_files,
    };
    let manifest_path = temporary.join("manifest.json");
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| {
        ServiceError::command_failed(format!("could not serialize recovery manifest: {error}"))
    })?;
    write_and_sync(&manifest_path, &manifest_bytes)?;

    let mut protected_files = vec![PathBuf::from("catalog.db"), PathBuf::from("manifest.json")];
    protected_files.extend(
        manifest
            .copied_cover_files
            .iter()
            .map(|name| PathBuf::from("covers").join(name)),
    );
    protected_files.extend(
        manifest
            .copied_associated_files
            .iter()
            .map(|file| PathBuf::from(&file.bundle_path)),
    );
    protected_files.sort();
    let mut checksums = String::new();
    for relative in protected_files {
        let digest = sha256_file(&temporary.join(&relative))?;
        checksums.push_str(&format!(
            "{digest}  {}\n",
            relative.to_string_lossy().replace('\\', "/")
        ));
    }
    write_and_sync(&temporary.join("checksums.sha256"), checksums.as_bytes())?;

    publish_directory(temporary, published, "recovery bundle")
}

#[cfg(test)]
mod tests {
    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        GameId, GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
        RootAuthority,
    };
    use renderpilot_storage_sqlite::{ConsolidationPlan, ConsolidationSource, SqliteStorage};

    use super::{create_consolidation_recovery_bundle, create_root_correction_recovery_bundle};

    #[test]
    fn bundle_is_published_beside_the_actual_custom_catalog() {
        let temp = tempfile::tempdir().expect("temp");
        let catalog_path = temp.path().join("custom-catalog.db");
        let storage = SqliteStorage::open(&catalog_path).expect("storage");
        let (destination, source, plan) = seed_cover_conflict(&storage);
        let covers_dir = crate::covers::covers_directory(&catalog_path);
        std::fs::create_dir_all(&covers_dir).expect("covers directory");
        std::fs::write(covers_dir.join("destination.webp"), b"destination").expect("cover");
        std::fs::write(covers_dir.join("source.webp"), b"source").expect("cover");
        let associated = temp.path().join("managed.dll");
        let sidecar = temp.path().join("managed.dll.bak");
        std::fs::write(&associated, b"managed").expect("associated file");
        std::fs::write(&sidecar, b"original").expect("associated sidecar");
        rusqlite::Connection::open(&catalog_path)
            .expect("fixture connection")
            .execute(
                "INSERT INTO installed_addons (
                    game_id, kind, addon_file, created_files_json,
                    backed_up_files_json, managed_files_json, tracked_sources_json
                ) VALUES (:game_id, 'RenoDx', :file, '[]', '[]', '[]', '[]')",
                rusqlite::named_params! {
                    ":game_id": source.id().as_str(),
                    ":file": associated.to_string_lossy(),
                },
            )
            .expect("associated state");
        let conflicts = storage
            .inspect_consolidation_conflicts(&plan)
            .expect("conflicts");

        let bundle =
            create_consolidation_recovery_bundle(&storage, &plan, &conflicts).expect("bundle");

        assert_eq!(
            bundle.parent(),
            Some(temp.path().join("recovery").as_path())
        );
        assert!(bundle.join("catalog.db").is_file());
        assert!(bundle.join("manifest.json").is_file());
        assert!(bundle.join("checksums.sha256").is_file());
        assert!(bundle.join("covers/destination.webp").is_file());
        assert!(bundle.join("covers/source.webp").is_file());
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(bundle.join("manifest.json")).expect("manifest bytes"),
        )
        .expect("manifest JSON");
        assert_eq!(
            manifest["copiedAssociatedFiles"]
                .as_array()
                .expect("associated files")
                .len(),
            2,
            "live managed file and classic sidecar must both be recoverable",
        );
        assert!(
            storage
                .find_game(destination.id())
                .expect("destination")
                .is_some()
        );
        assert!(storage.find_game(source.id()).expect("source").is_some());
    }

    #[test]
    fn publication_failure_leaves_catalog_unchanged() {
        let temp = tempfile::tempdir().expect("temp");
        let catalog_path = temp.path().join("custom-catalog.db");
        let storage = SqliteStorage::open(&catalog_path).expect("storage");
        let (destination, source, plan) = seed_cover_conflict(&storage);
        std::fs::write(temp.path().join("recovery"), b"blocks directory").expect("blocker");
        let conflicts = storage
            .inspect_consolidation_conflicts(&plan)
            .expect("conflicts");

        let error = create_consolidation_recovery_bundle(&storage, &plan, &conflicts)
            .expect_err("bundle publication must fail");

        assert!(error.to_string().contains("recovery directory"));
        assert!(
            storage
                .find_game(destination.id())
                .expect("destination")
                .is_some()
        );
        assert!(storage.find_game(source.id()).expect("source").is_some());
    }

    #[test]
    fn root_correction_bundle_archives_catalog_history_with_checksums() {
        let temp = tempfile::tempdir().expect("temp");
        let catalog_path = temp.path().join("catalog.db");
        let storage = SqliteStorage::open(&catalog_path).expect("storage");
        let game = game("game:root-correction", "C:/Games");
        storage.upsert_game(&game).expect("game");

        let bundle = create_root_correction_recovery_bundle(
            &storage,
            game.id().as_str(),
            "C:/Games",
            "C:/Games/Selected",
            &["component:sibling".to_owned()],
        )
        .expect("bundle");

        assert!(bundle.join("catalog.db").is_file());
        assert!(bundle.join("checksums.sha256").is_file());
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(bundle.join("manifest.json")).expect("manifest bytes"),
        )
        .expect("manifest");
        assert_eq!(manifest["gameId"], game.id().as_str());
        assert_eq!(manifest["previousRoot"], "C:/Games");
        assert_eq!(manifest["correctedRoot"], "C:/Games/Selected");
        assert_eq!(
            manifest["archivedComponentIds"],
            serde_json::json!(["component:sibling"])
        );
    }

    #[test]
    fn root_correction_bundle_failure_does_not_mutate_the_catalog() {
        let temp = tempfile::tempdir().expect("temp");
        let catalog_path = temp.path().join("catalog.db");
        let storage = SqliteStorage::open(&catalog_path).expect("storage");
        let game = game("game:root-correction-failure", "C:/Games");
        storage.upsert_game(&game).expect("game");
        std::fs::write(temp.path().join("recovery"), b"blocks recovery directory")
            .expect("blocker");

        create_root_correction_recovery_bundle(
            &storage,
            game.id().as_str(),
            "C:/Games",
            "C:/Games/Selected",
            &["component:sibling".to_owned()],
        )
        .expect_err("bundle publication must fail");

        assert_eq!(
            storage
                .find_game(game.id())
                .expect("game query")
                .expect("game")
                .install_path()
                .as_str(),
            "C:/Games"
        );
    }

    fn seed_cover_conflict(
        storage: &SqliteStorage,
    ) -> (GameInstallation, GameInstallation, ConsolidationPlan) {
        let destination = game("game:destination", "C:/Games/Example");
        let source = game("manual:child", "C:/Games/Example/D3D12");
        storage.upsert_game(&destination).expect("destination");
        storage.upsert_game(&source).expect("source");
        storage
            .upsert_game_cover(destination.id(), "destination.webp")
            .expect("destination cover");
        storage
            .upsert_game_cover(source.id(), "source.webp")
            .expect("source cover");
        let plan = ConsolidationPlan {
            destination_game_id: destination.id().clone(),
            sources: vec![ConsolidationSource {
                source_game_id: source.id().clone(),
                component_rekeys: Vec::new(),
            }],
        };
        (destination, source, plan)
    }

    fn game(id: &str, path: &str) -> GameInstallation {
        GameInstallation::new(
            GameIdentity::new(GameId::new(id).expect("id"), "Game", Launcher::Manual)
                .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(path).expect("path"),
        )
        .with_root_authority(RootAuthority::Legacy)
    }
}
