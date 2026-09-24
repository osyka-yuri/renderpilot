//! Read-only safety assessment for correcting a manual game root.
//!
//! A root correction keeps the stable game identity. State is therefore safe
//! to retain only when its concrete filesystem/executable ownership still
//! belongs to the prospective root. Component rollback baselines outside that
//! root are recoverable through the normal rollback workflow; other external
//! state requires the user to resolve it from the existing game card.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::ServiceError;
use renderpilot_application::{ComponentRepository, GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    ComponentRollbackBaseline, GameId, GameInstallation, InstalledAddon, LibraryComponent,
};

/// High-level result of assessing a root correction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootCorrectionStatus {
    /// All retained state is compatible with the prospective root.
    Ready,
    /// One or more managed inverse actions must complete first.
    CleanupRequired,
    /// State that has no safe inline rollback blocks the correction.
    Blocked,
}

/// One explicit managed inverse action required before root correction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootCorrectionCleanupAction {
    /// Restore one active component replacement.
    RollbackComponent {
        /// Component whose durable rollback path must be used.
        component_id: String,
    },
}

/// Managed state that prevents an inline component rollback from completing
/// the root correction safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RootCorrectionBlockerKind {
    /// A crash-recoverable filesystem mutation has not finished recovery.
    PendingRecovery,
    /// An installed add-on belongs outside the prospective game root.
    InstalledAddon,
    /// Managed NVAPI state belongs to a different executable scope.
    Nvapi,
    /// A persisted component baseline no longer has a component that can be
    /// rolled back through the normal component workflow.
    OrphanedComponentBaseline,
}

impl RootCorrectionBlockerKind {
    /// Stable diagnostic representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PendingRecovery => "pending_recovery",
            Self::InstalledAddon => "installed_addon",
            Self::Nvapi => "nvapi",
            Self::OrphanedComponentBaseline => "orphaned_component_baseline",
        }
    }
}

/// Structured capability returned with add-game inspection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootCorrectionAssessment {
    /// Stable identity whose install root would be corrected.
    pub game_id: String,
    /// Whether correction can proceed now.
    pub status: RootCorrectionStatus,
    /// Managed inverse actions required before correction.
    pub cleanup_actions: Vec<RootCorrectionCleanupAction>,
    /// Non-component state that requires separate resolution.
    pub blockers: Vec<RootCorrectionBlockerKind>,
}

impl RootCorrectionAssessment {
    /// True when the root can be corrected without first changing external
    /// state.
    #[must_use]
    pub const fn is_ready(&self) -> bool {
        matches!(self.status, RootCorrectionStatus::Ready)
    }
}

/// Optional prospective component set used by the final, locked assessment.
///
/// Inspection does not perform a full component scan, so it validates a
/// baseline against the currently catalogued component and concrete paths.
/// Before persistence, the full scan supplies `Some` and additionally proves
/// that the same stable component id is still present.
pub(super) fn assess(
    context: &crate::Context,
    game_id: &GameId,
    selected_root: &str,
    prospective_components: Option<&[LibraryComponent]>,
) -> Result<RootCorrectionAssessment, ServiceError> {
    let storage = context.storage();
    let current_components = storage
        .list_components_for_game(game_id)?
        .into_iter()
        .map(|component| (component.id().clone(), component))
        .collect::<HashMap<_, _>>();
    let prospective_ids = prospective_components.map(|components| {
        components
            .iter()
            .map(|component| component.id().clone())
            .collect::<HashSet<_>>()
    });

    let mut rollback_component_ids = BTreeSet::new();
    let mut blockers = BTreeSet::new();
    for (component_id, baseline) in storage.component_backups_for_game(game_id)? {
        let Some(component) = current_components.get(&component_id) else {
            blockers.insert(RootCorrectionBlockerKind::OrphanedComponentBaseline);
            continue;
        };
        let remains_in_prospective_scan = prospective_ids
            .as_ref()
            .is_none_or(|ids| ids.contains(&component_id));
        if !remains_in_prospective_scan
            || !component_belongs_to_root(component, selected_root)
            || !baseline_belongs_to_root(&baseline, selected_root)
        {
            rollback_component_ids.insert(component_id.as_str().to_owned());
        }
    }

    if !storage.pending_file_mutations_for_game(game_id)?.is_empty() {
        blockers.insert(RootCorrectionBlockerKind::PendingRecovery);
    }
    if let Some(addon) = storage.get_installed_addon(game_id)?
        && !addon_belongs_to_root(&addon, selected_root)
    {
        blockers.insert(RootCorrectionBlockerKind::InstalledAddon);
    }
    if (storage.game_has_nvapi_durable_state(game_id.as_str())?
        || storage
            .get_nvapi_executable_override(game_id.as_str())?
            .is_some())
        && !nvapi_belongs_to_root(storage, &storage.require_game(game_id)?, selected_root)?
    {
        blockers.insert(RootCorrectionBlockerKind::Nvapi);
    }

    let blockers = blockers.into_iter().collect::<Vec<_>>();
    let cleanup_actions = rollback_component_ids
        .into_iter()
        .map(|component_id| RootCorrectionCleanupAction::RollbackComponent { component_id })
        .collect::<Vec<_>>();
    let status = if !blockers.is_empty() {
        RootCorrectionStatus::Blocked
    } else if !cleanup_actions.is_empty() {
        RootCorrectionStatus::CleanupRequired
    } else {
        RootCorrectionStatus::Ready
    };

    Ok(RootCorrectionAssessment {
        game_id: game_id.as_str().to_owned(),
        status,
        cleanup_actions,
        blockers,
    })
}

fn component_belongs_to_root(component: &LibraryComponent, root: &str) -> bool {
    !component.files().is_empty()
        && component
            .files()
            .iter()
            .all(|file| path_belongs_to_root(file.path().as_str(), root))
}

fn baseline_belongs_to_root(baseline: &ComponentRollbackBaseline, root: &str) -> bool {
    baseline
        .files()
        .iter()
        .chain(baseline.expected_active_files())
        .all(|file| path_belongs_to_root(file.path().as_str(), root))
        && baseline.d3d12_executable().is_none_or(|executable| {
            path_belongs_to_root(executable.executable_path().as_str(), root)
        })
}

fn addon_belongs_to_root(addon: &InstalledAddon, root: &str) -> bool {
    if let Some(registered_executable) = addon.registered_exe_path() {
        // Shared-host payloads may intentionally live outside the game. Their
        // ownership is scoped by the exact registered game executable.
        return path_belongs_to_root(registered_executable.as_str(), root);
    }

    addon
        .created_files()
        .iter()
        .chain(addon.backed_up_files())
        .all(|path| path_belongs_to_root(path.as_str(), root))
        && addon
            .managed_files()
            .iter()
            .all(|file| path_belongs_to_root(file.path().as_str(), root))
}

fn nvapi_belongs_to_root(
    storage: &renderpilot_storage_sqlite::SqliteStorage,
    game: &GameInstallation,
    root: &str,
) -> Result<bool, ServiceError> {
    let owner = storage.get_nvapi_owned_profile(game.id().as_str())?;
    let claims = storage.list_nvapi_setting_claims_for_game(game.id().as_str())?;
    let targets = claims
        .iter()
        .map(|claim| claim.target_id.as_str())
        .chain(owner.iter().map(|profile| profile.profile_name.as_str()))
        .collect::<HashSet<_>>();
    if storage
        .list_pending_nvapi_operations()?
        .iter()
        .any(|operation| {
            operation.game_id.as_deref() == Some(game.id().as_str())
                || operation
                    .target_id
                    .as_deref()
                    .is_some_and(|target| targets.contains(target))
        })
    {
        return Ok(false);
    }
    if let Some(owner) = owner
        && !path_belongs_to_root(&owner.binding_path, root)
    {
        return Ok(false);
    }
    if claims
        .iter()
        .any(|claim| !path_belongs_to_root(&claim.executable_path, root))
    {
        return Ok(false);
    }
    if let Some(executable) = storage.get_nvapi_executable_override(game.id().as_str())?
        && !path_belongs_to_root(&executable.selected_path, root)
    {
        return Ok(false);
    }

    Ok(true)
}

fn path_belongs_to_root(path: &str, root: &str) -> bool {
    // Domain paths are normally PathRef-normalized, but NVAPI rows and legacy
    // catalog state may still contain native Windows separators.
    let Ok(path) = renderpilot_domain::PathRef::new(path) else {
        return false;
    };
    let Ok(root) = renderpilot_domain::PathRef::new(root) else {
        return false;
    };
    renderpilot_domain::InstallRoot::new(root).contains_path(&path)
}

#[cfg(test)]
mod tests {
    use renderpilot_application::{ComponentRepository, GameRepository, InstalledAddonRepository};
    use renderpilot_domain::{
        AddonKind, ComponentFile, ComponentId, ComponentKind, ComponentRollbackBaseline,
        D3d12ExecutableBaseline, D3d12ExecutableIdentity, GameId, GameIdentity, GameInstallation,
        GameRuntime, InstalledAddon, Launcher, LibraryComponent, LibraryTechnology, PathRef,
        Platform, RootAuthority, Sha256Hash, Swappability,
    };
    use renderpilot_storage_sqlite::{
        BeginFileMutationPreparation, NvapiGameSettingPreparation, NvapiSettingOperationCompletion,
        NvapiSettingOperationScope, NvapiSettingState,
    };

    use super::{
        RootCorrectionBlockerKind, RootCorrectionCleanupAction, RootCorrectionStatus, assess,
        path_belongs_to_root,
    };

    #[test]
    fn containment_is_separator_and_case_insensitive_without_prefix_collisions() {
        assert!(path_belongs_to_root(
            r"d:\Games\Example\bin\game.dll",
            "D:/Games/Example"
        ));
        assert!(!path_belongs_to_root(
            "D:/Games/Example 2/game.dll",
            "D:/Games/Example"
        ));
    }

    #[test]
    fn compatible_component_baseline_requires_the_same_prospective_component() {
        let fixture = fixture(&["Selected/Game.exe"]);
        let component = component(
            fixture.game.id(),
            "component:selected",
            "C:/Games/Selected/nvngx_dlss.dll",
        );
        fixture
            .context
            .storage()
            .replace_components_for_game(fixture.game.id(), std::slice::from_ref(&component))
            .expect("component");
        fixture
            .context
            .storage()
            .recover_component_rollback_baseline(
                fixture.game.id(),
                component.id(),
                &ComponentRollbackBaseline::new(vec![component_file(
                    "C:/Games/Selected/nvngx_dlss.dll",
                )]),
            )
            .expect("baseline");

        let ready = assessment(&fixture, std::slice::from_ref(&component));
        assert_eq!(ready.status, RootCorrectionStatus::Ready);
        assert!(ready.cleanup_actions.is_empty());

        let missing = assessment(&fixture, &[]);
        assert_eq!(missing.status, RootCorrectionStatus::CleanupRequired);
        assert_eq!(
            missing.cleanup_actions,
            vec![RootCorrectionCleanupAction::RollbackComponent {
                component_id: "component:selected".to_owned(),
            }]
        );
    }

    #[test]
    fn expected_active_component_files_must_remain_inside_the_corrected_root() {
        let fixture = fixture(&["Selected/Game.exe", "Sibling/Other.exe"]);
        let component = component(
            fixture.game.id(),
            "component:selected-active-scope",
            "C:/Games/Selected/nvngx_dlss.dll",
        );
        fixture
            .context
            .storage()
            .replace_components_for_game(fixture.game.id(), std::slice::from_ref(&component))
            .expect("component");
        let baseline = ComponentRollbackBaseline::new(vec![component_file(
            "C:/Games/Selected/nvngx_dlss.dll",
        )])
        .with_expected_active_files(vec![component_file("C:/Games/Sibling/nvngx_dlss.dll")]);
        fixture
            .context
            .storage()
            .recover_component_rollback_baseline(fixture.game.id(), component.id(), &baseline)
            .expect("baseline");

        let result = assessment(&fixture, std::slice::from_ref(&component));

        assert_eq!(result.status, RootCorrectionStatus::CleanupRequired);
        assert_eq!(
            result.cleanup_actions,
            vec![RootCorrectionCleanupAction::RollbackComponent {
                component_id: "component:selected-active-scope".to_owned(),
            }]
        );
    }

    #[test]
    fn sibling_component_baseline_requires_explicit_rollback() {
        let fixture = fixture(&["Selected/Game.exe", "Sibling/Other.exe"]);
        let component = component(
            fixture.game.id(),
            "component:sibling",
            "C:/Games/Sibling/nvngx_dlss.dll",
        );
        fixture
            .context
            .storage()
            .replace_components_for_game(fixture.game.id(), std::slice::from_ref(&component))
            .expect("component");
        fixture
            .context
            .storage()
            .recover_component_rollback_baseline(
                fixture.game.id(),
                component.id(),
                &ComponentRollbackBaseline::new(vec![component_file(
                    "C:/Games/Sibling/nvngx_dlss.dll",
                )]),
            )
            .expect("baseline");

        let result = assessment(&fixture, &[]);
        assert_eq!(result.status, RootCorrectionStatus::CleanupRequired);
        assert_eq!(
            result.cleanup_actions,
            vec![RootCorrectionCleanupAction::RollbackComponent {
                component_id: "component:sibling".to_owned(),
            }]
        );
        assert!(result.blockers.is_empty());
    }

    #[test]
    fn d3d12_executable_outside_the_selected_root_requires_rollback() {
        let fixture = fixture(&["Selected/Game.exe", "Sibling/Other.exe"]);
        let component = component(
            fixture.game.id(),
            "component:selected-d3d12",
            "C:/Games/Selected/D3D12Core.dll",
        );
        fixture
            .context
            .storage()
            .replace_components_for_game(fixture.game.id(), std::slice::from_ref(&component))
            .expect("component");
        let identity =
            D3d12ExecutableIdentity::new(610, Sha256Hash::new("a".repeat(64)).expect("hash"));
        let baseline =
            ComponentRollbackBaseline::new(vec![component_file("C:/Games/Selected/D3D12Core.dll")])
                .with_d3d12_executable(D3d12ExecutableBaseline::new(
                    PathRef::new("C:/Games/Sibling/Other.exe").expect("executable"),
                    identity.clone(),
                    identity,
                ));
        fixture
            .context
            .storage()
            .recover_component_rollback_baseline(fixture.game.id(), component.id(), &baseline)
            .expect("baseline");

        let result = assessment(&fixture, std::slice::from_ref(&component));
        assert_eq!(result.status, RootCorrectionStatus::CleanupRequired);
        assert_eq!(
            result.cleanup_actions,
            vec![RootCorrectionCleanupAction::RollbackComponent {
                component_id: "component:selected-d3d12".to_owned(),
            }]
        );
    }

    #[test]
    fn nvapi_claim_bound_outside_prospective_root_is_blocked() {
        let fixture = fixture(&["Selected/Game.exe", "Sibling/Game.exe"]);
        seed_nvapi_setting_claim(&fixture, "C:/Games/Sibling/Game.exe");

        let result = assessment(&fixture, &[]);
        assert_eq!(result.status, RootCorrectionStatus::Blocked);
        assert_eq!(result.blockers, vec![RootCorrectionBlockerKind::Nvapi]);
    }

    #[test]
    fn nvapi_claim_bound_to_selected_executable_is_preserved() {
        let fixture = fixture(&["Selected/Game.exe"]);
        seed_nvapi_setting_claim(&fixture, "C:/Games/Selected/Game.exe");

        assert_eq!(
            assessment(&fixture, &[]).status,
            RootCorrectionStatus::Ready
        );
    }

    fn seed_nvapi_setting_claim(fixture: &Fixture, executable_path: &str) {
        let storage = fixture.context.storage();
        let profile_name = "NVIDIA Test Profile";
        let setting_id = 0x10B3_292C;
        let identity = r#"{"name":"NVIDIA Test Profile"}"#;
        let witness = serde_json::json!({"app_name": executable_path}).to_string();
        storage
            .prepare_nvapi_setting_operation(NvapiGameSettingPreparation {
                op_id: "root-correction-setting",
                game_id: fixture.game.id().as_str(),
                profile_name,
                target_kind: "external",
                profile_is_predefined: false,
                profile_identity_json: identity,
                executable_path,
                application_witness_json: &witness,
                setting_id,
                original: NvapiSettingState {
                    present: false,
                    value: None,
                },
                before: NvapiSettingState {
                    present: false,
                    value: None,
                },
                after: NvapiSettingState {
                    present: true,
                    value: Some(1),
                },
            })
            .expect("prepare setting claim");
        storage
            .finish_nvapi_setting_operation(NvapiSettingOperationCompletion {
                op_id: "root-correction-setting",
                scope: NvapiSettingOperationScope::Game {
                    game_id: fixture.game.id().as_str(),
                },
                target_id: profile_name,
                setting_id,
                expected: NvapiSettingState {
                    present: true,
                    value: Some(1),
                },
                profile_identity_json: identity,
                composition_json: None,
            })
            .expect("finish setting claim");
    }

    #[test]
    fn pending_recovery_and_external_addon_are_reported_as_distinct_blockers() {
        let fixture = fixture(&["Selected/Game.exe"]);
        fixture
            .context
            .storage()
            .begin_file_mutation_preparation(&BeginFileMutationPreparation {
                id: "mutation:pending".to_owned(),
                game_id: fixture.game.id().clone(),
                feature: "test".to_owned(),
                subject_id: None,
                initial_manifest_json: "{}".to_owned(),
            })
            .expect("pending mutation");
        fixture
            .context
            .storage()
            .upsert_installed_addon(&InstalledAddon::new(
                fixture.game.id().clone(),
                AddonKind::Luma,
                PathRef::new("C:/Games/Sibling/Luma.addon64").expect("add-on path"),
            ))
            .expect("add-on");

        let result = assessment(&fixture, &[]);
        assert_eq!(result.status, RootCorrectionStatus::Blocked);
        assert_eq!(
            result.blockers,
            vec![
                RootCorrectionBlockerKind::PendingRecovery,
                RootCorrectionBlockerKind::InstalledAddon,
            ]
        );
    }

    struct Fixture {
        _temp: tempfile::TempDir,
        context: crate::Context,
        game: GameInstallation,
    }

    fn fixture(executables: &[&str]) -> Fixture {
        let temp = tempfile::tempdir().expect("temp");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let mut game = GameInstallation::new(
            GameIdentity::new(
                GameId::new("game:oversized").expect("game id"),
                "Oversized",
                Launcher::Manual,
            )
            .expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new("C:/Games").expect("root"),
        )
        .with_root_authority(RootAuthority::UserConfirmed);
        for executable in executables {
            game = game.with_executable_candidate(PathRef::new(*executable).expect("executable"));
        }
        context.storage().upsert_game(&game).expect("game");
        Fixture {
            _temp: temp,
            context,
            game,
        }
    }

    fn assessment(
        fixture: &Fixture,
        prospective_components: &[LibraryComponent],
    ) -> super::RootCorrectionAssessment {
        assess(
            &fixture.context,
            fixture.game.id(),
            "C:/Games/Selected",
            Some(prospective_components),
        )
        .expect("assessment")
    }

    fn component(game_id: &GameId, id: &str, path: &str) -> LibraryComponent {
        LibraryComponent::new(
            ComponentId::new(id).expect("component id"),
            game_id.clone(),
            ComponentKind::NativeLibrary,
            LibraryTechnology::DlssSuperResolution,
            Swappability::Swappable,
        )
        .with_file(component_file(path))
    }

    fn component_file(path: &str) -> ComponentFile {
        ComponentFile::new(PathRef::new(path).expect("component path"))
    }
}
