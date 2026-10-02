//! Lock-scoped coordination between filesystem presence and catalog visibility.

use std::path::Path;

use renderpilot_application::GameRepository;
#[cfg(any(windows, test))]
use renderpilot_domain::GameId;
use renderpilot_domain::GameInstallation;
use renderpilot_storage_sqlite::AuthorityCas;

#[cfg(any(windows, test))]
use crate::mutation_boundary::enter_game_observation_boundary;
use crate::{Context, ServiceError};

use super::{
    payload_presence::{GamePayloadObservation, observe_game_payload},
    presence::{InstallationPresence, absolute_path_parts, probe_installation_root},
    residue::{RetiredRootObservation, observe_registered_root},
};

#[derive(Debug)]
pub(crate) enum RegisteredRootObservation {
    Present {
        was_absent: bool,
    },
    ConfirmedAbsent {
        #[cfg(any(windows, test))]
        newly_absent: bool,
        collection_error: Option<String>,
    },
    Indeterminate {
        reason: String,
    },
}

#[cfg(any(windows, test))]
#[derive(Debug, Default)]
pub(crate) struct RegisteredRootSweep {
    pub removed_game_ids: Vec<GameId>,
    pub errors: Vec<RegisteredRootError>,
}

#[cfg(any(windows, test))]
#[derive(Debug)]
pub(crate) struct RegisteredRootError {
    pub root: String,
    pub message: String,
}

/// Rechecks one installation while its observation guard is held. When the
/// registered root is confirmed absent, the active-state transition commits
/// before local metadata collection; collection failure therefore cannot make
/// the vanished game visible again.
pub(crate) fn observe_registered_root_under_lock(
    context: &Context,
    guard: &crate::game_mutation_lock::GameMutationGuard,
    expected_game: &GameInstallation,
) -> Result<RegisteredRootObservation, ServiceError> {
    if guard.game_id() != expected_game.id() {
        return Err(ServiceError::invalid_input(
            "observation guard does not match the requested game root",
        ));
    }

    let storage = context.storage();
    let registered_game = storage.find_game(expected_game.id())?;
    let probe_game = match registered_game.as_ref() {
        Some(stored) if stored.install_key() == expected_game.install_key() => stored,
        Some(_) => {
            return Err(ServiceError::command_failed(
                "installation root changed before its presence could be reconciled",
            ));
        }
        None => expected_game,
    };

    let Some(registered_game) = registered_game.as_ref() else {
        return Ok(
            match probe_installation_root(Path::new(probe_game.install_path().as_str())) {
                InstallationPresence::Present => {
                    RegisteredRootObservation::Present { was_absent: false }
                }
                InstallationPresence::Indeterminate { reason } => {
                    RegisteredRootObservation::Indeterminate { reason }
                }
                InstallationPresence::ConfirmedAbsent { .. } => {
                    RegisteredRootObservation::ConfirmedAbsent {
                        #[cfg(any(windows, test))]
                        newly_absent: false,
                        collection_error: None,
                    }
                }
            },
        );
    };

    let owners = storage.read_local_cleanup_owners(registered_game.id())?;
    if owners.game() != registered_game {
        return Err(ServiceError::command_failed(
            "installation identity changed while its local owners were observed",
        ));
    }
    let was_absent = owners.availability().is_absent();
    let observation = observe_registered_root(&owners);
    let (absence_evidence, mark_absent, collect_local) = match &observation {
        RetiredRootObservation::Missing(evidence) => (Some(evidence), true, true),
        RetiredRootObservation::ResidueOnly(root) if !was_absent && root.remnants().is_empty() => {
            return Ok(RegisteredRootObservation::Present { was_absent: false });
        }
        RetiredRootObservation::ResidueOnly(root) => (
            None,
            !root.remnants().is_empty(),
            was_absent && root.remnants().is_empty(),
        ),
        RetiredRootObservation::PresentNonResidue {
            native_root_identity,
        } => match observe_game_payload(&owners, native_root_identity) {
            GamePayloadObservation::Present => {
                return Ok(RegisteredRootObservation::Present { was_absent });
            }
            GamePayloadObservation::ConfirmedAbsent => (None, true, false),
            GamePayloadObservation::Indeterminate { reason } => {
                return Ok(RegisteredRootObservation::Indeterminate { reason });
            }
        },
        RetiredRootObservation::Indeterminate { reason } => {
            return Ok(RegisteredRootObservation::Indeterminate {
                reason: reason.clone(),
            });
        }
    };

    if let Some(evidence) = absence_evidence {
        let expected_root = Path::new(probe_game.install_path().as_str());
        let expected_anchor = absolute_path_parts(expected_root)
            .map(|(anchor, _)| anchor)
            .map_err(ServiceError::command_failed)?;
        if evidence.probed_root() != expected_root
            || evidence.readable_anchor() != expected_anchor.as_path()
            || !expected_root.starts_with(evidence.missing_segment())
        {
            return Ok(RegisteredRootObservation::Indeterminate {
                reason: "absence proof does not match the registered root and readable anchor"
                    .to_owned(),
            });
        }
    }

    if mark_absent {
        storage.mark_installation_absent(
            registered_game,
            AuthorityCas::new(owners.authority_epoch()),
        )?;
    }

    // Existing canonical local rows survive while exact files or recorded
    // directories remain. Missing roots and fully empty, already-absent roots
    // use the established ordered collection transaction.
    let collection_error = if collect_local {
        match storage.catalog_readiness(registered_game.id()) {
            Ok(readiness) => storage
                .collect_absent_installation(
                    registered_game,
                    AuthorityCas::new(readiness.authority_epoch()),
                )
                .err()
                .map(|error| error.to_string()),
            Err(error) => Some(error.to_string()),
        }
    } else {
        None
    };

    Ok(RegisteredRootObservation::ConfirmedAbsent {
        #[cfg(any(windows, test))]
        newly_absent: mark_absent && !was_absent,
        collection_error,
    })
}

#[cfg(any(windows, test))]
/// Reconciles every stored root independently of launcher-provider output.
/// Per-game lock, proof, CAS, or collection failures are returned alongside
/// successful removals so one unavailable install cannot stop other games.
pub(crate) fn reconcile_registered_roots(
    context: &Context,
) -> Result<RegisteredRootSweep, ServiceError> {
    let games = context.storage().list_games()?;
    let mut sweep = RegisteredRootSweep::default();

    for game in games {
        let guard = enter_game_observation_boundary(game.id());
        match observe_registered_root_under_lock(context, &guard, &game) {
            Ok(RegisteredRootObservation::Present { .. }) => {}
            Ok(RegisteredRootObservation::ConfirmedAbsent {
                newly_absent,
                collection_error,
            }) => {
                if newly_absent {
                    sweep.removed_game_ids.push(game.id().clone());
                }
                if let Some(message) = collection_error {
                    sweep.errors.push(RegisteredRootError {
                        root: game.install_path().as_str().to_owned(),
                        message,
                    });
                }
            }
            Ok(RegisteredRootObservation::Indeterminate { reason }) => {
                sweep.errors.push(RegisteredRootError {
                    root: game.install_path().as_str().to_owned(),
                    message: format!("installation root presence is indeterminate: {reason}"),
                });
            }
            Err(error) => sweep.errors.push(RegisteredRootError {
                root: game.install_path().as_str().to_owned(),
                message: error.to_string(),
            }),
        }
    }

    sweep.removed_game_ids.sort();
    sweep.removed_game_ids.dedup();
    Ok(sweep)
}

#[cfg(test)]
mod tests {
    use renderpilot_application::GameRepository;
    use renderpilot_application::InstalledAddonRepository;
    use renderpilot_domain::{
        AddonKind, GameId, GameIdentity, GameInstallation, GameRuntime, InstalledAddon, Launcher,
        ManagedAddonFile, ManagedFileBaseline, PathRef, Platform, RootAuthority,
    };

    use super::{
        RegisteredRootObservation, observe_registered_root_under_lock, reconcile_registered_roots,
    };

    #[test]
    fn missing_root_is_removed_from_active_catalog_while_other_errors_are_isolated() {
        let temp = tempfile::tempdir().expect("temp");
        let missing_root = temp.path().join("MissingGame");
        std::fs::create_dir_all(&missing_root).expect("missing-game root");
        let non_directory = temp.path().join("ordinary-file");
        std::fs::write(&non_directory, b"file").expect("write regular file");
        let indeterminate_root = non_directory.join("Game");

        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let missing = game("game:missing-root", &missing_root);
        let indeterminate = game("game:indeterminate-root", &indeterminate_root);
        context
            .storage()
            .upsert_game(&missing)
            .expect("seed missing game");
        context
            .storage()
            .upsert_game(&indeterminate)
            .expect("seed indeterminate game");
        std::fs::remove_dir(&missing_root).expect("remove game root");

        let result = reconcile_registered_roots(&context).expect("reconcile roots");

        assert_eq!(result.removed_game_ids, vec![missing.id().clone()]);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].root, indeterminate.install_path().as_str());
        assert!(
            result.errors[0]
                .message
                .starts_with("installation root presence is indeterminate: ")
                && result.errors[0]
                    .message
                    .contains("installation path contains a non-directory component"),
            "unexpected indeterminate-root error: {}",
            result.errors[0].message
        );
        assert!(
            context
                .storage()
                .is_installation_absent(missing.id())
                .expect("read missing availability")
        );
        assert!(
            context
                .storage()
                .find_game(missing.id())
                .expect("raw missing registration")
                .is_some()
        );
        assert!(
            context
                .storage()
                .find_active_game(missing.id())
                .expect("active missing game")
                .is_none()
        );
        assert!(
            !context
                .storage()
                .is_installation_absent(indeterminate.id())
                .expect("read indeterminate availability")
        );
        assert!(
            context
                .storage()
                .find_active_game(indeterminate.id())
                .expect("active indeterminate game")
                .is_some()
        );
    }

    #[test]
    fn active_root_with_only_exact_receipts_becomes_absent_and_retains_owners() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("Receipt Only Game");
        std::fs::create_dir_all(&install).expect("install root");
        let managed_file = install.join("owned.dll");
        std::fs::write(&managed_file, b"owned bytes").expect("managed file");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game("game:active-receipt-only", &install);
        context
            .storage()
            .upsert_game(&game)
            .expect("seed active game");
        let addon = InstalledAddon::new(
            game.id().clone(),
            AddonKind::RenoDx,
            path_ref(&install.join("renderpilot.addon64")),
        )
        .try_with_managed_files(vec![ManagedAddonFile::owned(
            path_ref(&managed_file),
            ManagedFileBaseline::Absent,
            renderpilot_detection::sha256_file(&managed_file).expect("managed file hash"),
        )])
        .expect("managed add-on receipt");
        context
            .storage()
            .upsert_installed_addon(&addon)
            .expect("seed managed receipt");

        let result = reconcile_registered_roots(&context).expect("reconcile root");

        assert_eq!(result.removed_game_ids, vec![game.id().clone()]);
        assert!(result.errors.is_empty());
        assert!(
            context
                .storage()
                .is_installation_absent(game.id())
                .expect("absence state")
        );
        assert!(
            context
                .storage()
                .find_active_game(game.id())
                .expect("active game")
                .is_none()
        );
        assert!(
            context
                .storage()
                .get_installed_addon(game.id())
                .expect("managed addon")
                .is_some()
        );
        assert_eq!(
            std::fs::read(&managed_file).expect("receipt bytes remain"),
            b"owned bytes"
        );
    }

    #[test]
    fn active_empty_root_is_reported_present_to_scan_caller() {
        let temp = tempfile::tempdir().expect("temp");
        let install = temp.path().join("Empty Game");
        std::fs::create_dir_all(&install).expect("install root");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let game = game("game:active-empty-root", &install);
        context
            .storage()
            .upsert_game(&game)
            .expect("seed active game");
        let guard = crate::mutation_boundary::enter_game_observation_boundary(game.id());

        let observation = observe_registered_root_under_lock(&context, &guard, &game)
            .expect("observe empty active root");

        assert!(matches!(
            observation,
            RegisteredRootObservation::Present { was_absent: false }
        ));
        assert!(
            !context
                .storage()
                .is_installation_absent(game.id())
                .expect("active availability")
        );
    }

    fn game(id: &str, root: &std::path::Path) -> GameInstallation {
        let identity = GameIdentity::new(GameId::new(id).expect("id"), "Game", Launcher::Manual)
            .expect("identity");
        GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(root.to_string_lossy().replace('\\', "/")).expect("path"),
        )
        .with_root_authority(RootAuthority::UserConfirmed)
    }

    fn path_ref(path: &std::path::Path) -> PathRef {
        PathRef::new(path.to_string_lossy().replace('\\', "/")).expect("path")
    }
}
