#[cfg(windows)]
use renderpilot_application::{
    ComponentRepository, GameRepository, OperationItemRecord, OperationJournalEntry, OperationKind,
    OperationRecord, OperationRepository, OperationStatus, UnixTimestampMillis,
};
#[cfg(windows)]
use renderpilot_domain::{
    ComponentFile, ComponentId, ComponentKind, LibraryComponent, LibraryTechnology, OperationId,
    Swappability,
};
use renderpilot_domain::{GameIdentity, GameInstallation, GameRuntime, Platform};
#[cfg(windows)]
use renderpilot_storage_sqlite::BeginFileMutationPreparation;

use super::*;

mod correction;
mod relationship;
mod validation;

fn test_boundary(kind: InstallBoundaryKind) -> InstallBoundaryInspection {
    InstallBoundaryInspection {
        kind,
        completeness: TraversalCompleteness::Complete,
        candidate_roots: Vec::new(),
        evidence: Vec::new(),
    }
}

fn install_root(path: &str) -> renderpilot_domain::InstallRoot {
    renderpilot_domain::InstallRoot::new(PathRef::new(path).expect("valid install root"))
}

fn game(id: &str, path: &str, authority: RootAuthority) -> GameInstallation {
    GameInstallation::new(
        GameIdentity::new(GameId::new(id).expect("id"), "Game", Launcher::Manual)
            .expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new(path).expect("path"),
    )
    .with_root_authority(authority)
}

#[cfg(windows)]
fn canonical_path_text(path: &Path) -> Result<String, ServiceError> {
    // Windows runners may expose `%TEMP%` through an 8.3 alias such as
    // `RUNNER~1`. Fixtures must cross the same filesystem-identity boundary as
    // production inspection before they enter domain or storage values.
    let canonical =
        renderpilot_platform_windows::canonicalize_install_path(path).map_err(|error| {
            ServiceError::invalid_input(format!(
                "test fixture path could not be canonicalized: {} ({error})",
                path.display()
            ))
        })?;
    PathRef::new(canonical.to_string_lossy())
        .map(|path| path.as_str().to_owned())
        .map_err(|error| ServiceError::invalid_input(error.to_string()))
}

#[cfg(windows)]
fn seed_external_operation(
    context: &crate::Context,
    game: &GameInstallation,
    component_path: &Path,
) -> OperationId {
    std::fs::write(component_path, b"external component").expect("component file");
    let component_path =
        canonical_path_text(component_path).expect("canonical external component path");
    let component_id =
        ComponentId::new(format!("component:{}:external", game.id())).expect("component id");
    let component = LibraryComponent::new(
        component_id.clone(),
        game.id().clone(),
        ComponentKind::NativeLibrary,
        LibraryTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(ComponentFile::new(
        PathRef::new(&component_path).expect("component path"),
    ));
    context
        .storage()
        .replace_components_for_game(game.id(), &[component])
        .expect("component");

    let operation_id =
        OperationId::new(format!("operation:{}:external", game.id())).expect("operation id");
    let operation = OperationRecord::new(
        operation_id.clone(),
        game.id().clone(),
        OperationKind::Scan,
        OperationStatus::Completed,
        UnixTimestampMillis::new(1).expect("timestamp"),
    );
    let item = OperationItemRecord::new(
        operation_id.clone(),
        component_id,
        PathRef::new(component_path).expect("source path"),
        OperationStatus::Completed,
    );
    context
        .storage()
        .save_operation_entry(
            &OperationJournalEntry::try_new(operation, vec![item]).expect("entry"),
        )
        .expect("operation");
    operation_id
}
