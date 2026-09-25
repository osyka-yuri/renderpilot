use renderpilot_application::{
    GameRepository, InstalledAddonRepository, OptiScalerStateRepository, ProxyTopologyRepository,
};
use renderpilot_domain::{
    AddonKind, CleanupState, ControlNamespaceBinding, DeleteEffect, DeleteState,
    DurableObservation, Endpoint, ExpectedAfter, FileReceipt, GameId, GameIdentity,
    GameInstallation, GameProxyTopology, GameRuntime, InstalledAddon, Launcher, ManagedAddonFile,
    ManagedFileBaseline, MaterializationState, NamespaceCapability, OperationEffect,
    OperationEndpoint, OperationRecord, OptiScalerAdoptionState, OptiScalerConfigurationBaseline,
    OptiScalerFileCleanup, OptiScalerFileReceipt, OptiScalerFileRole, OptiScalerInstallState,
    OptiScalerJournal, PathRef, Platform, Preimage, PrivateArtifactSlots, PrivateWorkspaceBinding,
    ProxyImplementation, ProxyLink, ProxyRootPrestate, RelocateEffect, RelocateState, Sha256Hash,
    VerifyEffect, VerifyState, WriteEffect, WriteState,
};
use rusqlite::OptionalExtension;

use super::{
    OptiScalerJournalAggregateBegin, OptiScalerJournalAggregateCommit,
    PendingFileMutationRecoveryCandidate, PreparedOptiScalerJournalAggregate,
    PreparingOptiScalerJournalAggregate, RecoveringOptiScalerJournalAggregate,
};
use crate::peer_runtime::{AggregateBefore, PeerStorageRuntime};
use crate::repositories::peer_aggregate_reservations::{
    PeerAggregateKind, PeerAggregateReservationState, read_within_transaction,
};
use crate::repositories::{
    OptiScalerAggregateMutation, OptiScalerPeerMutation, SqliteStorage, pending_file_mutations,
};

fn game_id(label: &str) -> GameId {
    GameId::new(format!("journal-aggregate:{label}")).expect("game id")
}

fn path(value: &str) -> PathRef {
    PathRef::new(value).expect("path")
}

fn capability() -> NamespaceCapability {
    NamespaceCapability::new("a".repeat(64)).expect("capability")
}

fn journal() -> OptiScalerJournal {
    let capability = capability();
    let endpoint = OperationEndpoint::new(
        Endpoint::Single,
        "game/dxgi.dll",
        Preimage::Initial {
            observation: DurableObservation::Absent,
            receipt: None,
            owned_basis: None,
        },
        ExpectedAfter::Pending,
    )
    .expect("endpoint");
    let operation = OperationRecord::new(
        0,
        Vec::new(),
        Some(0),
        PrivateArtifactSlots::new(
            DurableObservation::Absent,
            DurableObservation::Absent,
            DurableObservation::Absent,
        )
        .expect("slots"),
        OperationEffect::Write(
            WriteEffect::new(endpoint, WriteState::Planned).expect("write effect"),
        ),
    )
    .expect("operation");
    let workspace = PrivateWorkspaceBinding::new(
        0,
        0,
        format!(
            "game/.renderpilot-optiscaler-workspace-journal-0-{}",
            capability.as_str()
        ),
        None,
        capability.clone(),
    )
    .expect("private workspace");
    OptiScalerJournal::new(
        vec!["game".to_owned()],
        ControlNamespaceBinding::new(
            format!("game/control-journal-{}", capability.as_str()),
            None,
            capability,
        )
        .expect("control namespace"),
        vec![workspace],
        vec![operation],
    )
    .expect("journal")
}

fn prepared_journal() -> String {
    let mut journal = journal();
    let live = DurableObservation::File {
        identity: "live-0".to_owned(),
        digest: Sha256Hash::new("b".repeat(64)).expect("digest"),
    };
    if let OperationEffect::Write(effect) = journal.operations_mut()[0].effect_mut() {
        effect
            .endpoint_mut()
            .set_expected_after(ExpectedAfter::Known(live.clone()));
        *effect.state_mut() = WriteState::Applied {
            live,
            custody: DurableObservation::Absent,
        };
    }
    journal
        .control_namespace_mut()
        .set_identity("control-0")
        .expect("control identity");
    journal.private_workspaces_mut()[0]
        .set_identity("participant-0")
        .expect("workspace identity");
    journal.set_materialization(MaterializationState::Ready);
    journal.set_cleanup(CleanupState::Inactive);
    serde_json::to_string(&journal).expect("prepared journal json")
}

fn seed(label: &str) -> (PeerStorageRuntime, GameId, String) {
    let storage = SqliteStorage::in_memory().expect("storage");
    let id = game_id(label);
    storage
        .upsert_game(&GameInstallation::new(
            GameIdentity::new(id.clone(), "Journal aggregate", Launcher::Steam).expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            path("C:/game"),
        ))
        .expect("game");
    (
        PeerStorageRuntime::new(storage),
        id,
        serde_json::to_string(&journal()).expect("json"),
    )
}

fn begin(
    runtime: &PeerStorageRuntime,
    id: &GameId,
    operation_id: &str,
    initial: &str,
) -> PreparingOptiScalerJournalAggregate {
    begin_for(
        runtime,
        id,
        operation_id,
        renderpilot_domain::mutation_features::OPTISCALER_INSTALL,
        Some("optiscaler"),
        initial,
    )
}

fn begin_for(
    runtime: &PeerStorageRuntime,
    id: &GameId,
    operation_id: &str,
    feature: &str,
    subject_id: Option<&str>,
    initial: &str,
) -> PreparingOptiScalerJournalAggregate {
    runtime
        .begin_optiscaler_journal_aggregate(
            OptiScalerJournalAggregateBegin::new(
                operation_id,
                id.clone(),
                feature,
                subject_id.map(str::to_owned),
                initial,
            )
            .expect("begin input"),
        )
        .expect("begin")
}

fn reservation(
    runtime: &PeerStorageRuntime,
    id: &GameId,
    operation_id: &str,
) -> Option<crate::repositories::peer_aggregate_reservations::PeerAggregateReservation> {
    runtime
        .repositories()
        .with_transaction(|transaction| read_within_transaction(transaction, id, operation_id))
        .expect("reservation read")
}

fn pending_tuple(
    runtime: &PeerStorageRuntime,
    operation_id: &str,
) -> (String, String, Option<i64>) {
    runtime
        .repositories()
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT state, aggregate_kind, aggregate_revision
                     FROM pending_file_mutations WHERE id = ?1",
                    [operation_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
                .map(|row| row.expect("pending row"))
                .map_err(crate::error::storage_error)
        })
        .expect("pending tuple")
}

fn pending_identity(runtime: &PeerStorageRuntime, operation_id: &str) -> (i64, i64, i64, String) {
    runtime
        .repositories()
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT rowid, created_at, updated_at, manifest_json
                     FROM pending_file_mutations WHERE id = ?1",
                    [operation_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .map_err(crate::error::storage_error)
        })
        .expect("pending identity")
}

fn game_revision(runtime: &PeerStorageRuntime, id: &GameId) -> i64 {
    runtime
        .repositories()
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT peer_aggregate_revision FROM games WHERE id = ?1",
                    [id.as_str()],
                    |row| row.get(0),
                )
                .map_err(crate::error::storage_error)
        })
        .expect("game revision")
}

fn journal_with_materialization(
    json: &str,
    materialization: MaterializationState,
    control_identity: Option<&str>,
) -> String {
    let mut journal: OptiScalerJournal = serde_json::from_str(json).expect("journal json");
    if let Some(identity) = control_identity {
        journal
            .control_namespace_mut()
            .set_identity(identity)
            .expect("control identity");
    }
    journal.set_materialization(materialization);
    serde_json::to_string(&journal).expect("materialization json")
}

fn prepared_reverse_json(json: &str) -> String {
    let mut journal: OptiScalerJournal = serde_json::from_str(json).expect("prepared json");
    let operation = journal
        .operations_mut()
        .last_mut()
        .expect("prepared operation");
    if let OperationEffect::Write(effect) = operation.effect_mut() {
        let (postimage, custody) = match effect.state() {
            WriteState::Applied { live, custody } => (live.clone(), custody.clone()),
            state => panic!("expected applied write, got {state:?}"),
        };
        *effect.state_mut() = WriteState::DiscardIntent { postimage, custody };
    } else {
        panic!("expected write effect");
    }
    serde_json::to_string(&journal).expect("reverse json")
}

fn rollback_terminal_json(json: &str) -> String {
    let mut journal: OptiScalerJournal = serde_json::from_str(json).expect("prepared json");
    for operation in journal.operations_mut() {
        match operation.effect_mut() {
            OperationEffect::Write(effect) => *effect.state_mut() = WriteState::Preserved,
            OperationEffect::Delete(effect) => *effect.state_mut() = DeleteState::Preserved,
            OperationEffect::Verify(effect) => *effect.state_mut() = VerifyState::Preserved,
            OperationEffect::Relocate(effect) => *effect.state_mut() = RelocateState::Preserved,
            OperationEffect::CreateDirectory(effect) => {
                *effect.state_mut() = renderpilot_domain::CreateDirectoryState::Preserved;
            }
            OperationEffect::PostCommitRemoveDirectory(_) => {}
        }
    }
    journal.set_cleanup(CleanupState::Complete);
    serde_json::to_string(&journal).expect("rollback terminal json")
}

fn committed_terminal_json(json: &str) -> String {
    let mut journal: OptiScalerJournal = serde_json::from_str(json).expect("committed json");
    journal.set_cleanup(CleanupState::Complete);
    serde_json::to_string(&journal).expect("committed terminal json")
}

fn replace_pending_json(runtime: &PeerStorageRuntime, operation_id: &str, json: &str) {
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE pending_file_mutations SET manifest_json = ?1 WHERE id = ?2",
                    [json, operation_id],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("replace pending journal json");
}

#[derive(Clone, Copy)]
enum PersistedBindingPath {
    Control,
    Private,
}

fn assert_recovery_rejects_noncanonical_binding_path(
    fixture: &str,
    operation_id: &str,
    binding_path: PersistedBindingPath,
) {
    let (runtime, id, initial) = seed(fixture);
    begin(&runtime, &id, operation_id, &initial);
    let mut manifest: serde_json::Value =
        serde_json::from_str(&pending_identity(&runtime, operation_id).3)
            .expect("stored preparing journal");
    let path = match binding_path {
        PersistedBindingPath::Control => &mut manifest["control_namespace"]["path"],
        PersistedBindingPath::Private => &mut manifest["private_workspaces"][0]["path"],
    };
    let canonical = path.as_str().expect("canonical binding path").to_owned();
    *path = serde_json::Value::String(format!(
        "{}\\",
        canonical.to_ascii_uppercase().replace('/', "\\")
    ));
    let injected = serde_json::to_string(&manifest).expect("noncanonical stored journal");
    replace_pending_json(&runtime, operation_id, &injected);

    let before_identity = pending_identity(&runtime, operation_id);
    let before_tuple = pending_tuple(&runtime, operation_id);
    let before_reservation =
        reservation(&runtime, &id, operation_id).expect("reservation before recovery");

    let error = runtime
        .recover_pending_file_mutation_candidates_for_game(&id, |_| true)
        .expect_err("noncanonical durable spelling must fail before recovery CAS");
    assert!(error.to_string().contains("canonical durable spelling"));
    assert_eq!(pending_identity(&runtime, operation_id), before_identity);
    assert_eq!(pending_tuple(&runtime, operation_id), before_tuple);
    assert_eq!(
        reservation(&runtime, &id, operation_id),
        Some(before_reservation)
    );
}

fn retarget_preparing(
    runtime: &PeerStorageRuntime,
    preparing: PreparingOptiScalerJournalAggregate,
    json: String,
) -> PreparingOptiScalerJournalAggregate {
    let PreparingOptiScalerJournalAggregate {
        runtime_identity,
        reservation,
        begin,
        pending_identity,
        ..
    } = preparing;
    replace_pending_json(runtime, begin.operation_id(), &json);
    PreparingOptiScalerJournalAggregate {
        runtime_identity,
        reservation,
        begin,
        current_journal_json: json,
        pending_identity,
    }
}

fn retarget_prepared(
    runtime: &PeerStorageRuntime,
    prepared: PreparedOptiScalerJournalAggregate,
    json: String,
) -> PreparedOptiScalerJournalAggregate {
    let PreparedOptiScalerJournalAggregate {
        runtime_identity,
        reservation,
        begin,
        catalog_binding,
        pending_identity,
        ..
    } = prepared;
    replace_pending_json(runtime, begin.operation_id(), &json);
    PreparedOptiScalerJournalAggregate {
        runtime_identity,
        reservation,
        begin,
        catalog_binding,
        current_journal_json: json,
        pending_identity,
    }
}

fn retarget_committed(
    runtime: &PeerStorageRuntime,
    committed: super::model::CommittedOptiScalerJournalAggregate,
    json: String,
) -> super::model::CommittedOptiScalerJournalAggregate {
    let super::model::CommittedOptiScalerJournalAggregate {
        runtime_identity,
        aggregate,
        reservation,
        begin,
        pending_identity,
        ..
    } = committed;
    replace_pending_json(runtime, begin.operation_id(), &json);
    super::model::CommittedOptiScalerJournalAggregate {
        runtime_identity,
        aggregate,
        reservation,
        begin,
        current_journal_json: json,
        pending_identity,
    }
}

fn retarget_prepared_catalog_binding(
    prepared: PreparedOptiScalerJournalAggregate,
    catalog_binding: pending_file_mutations::PreparedOptiScalerCatalogBinding,
) -> PreparedOptiScalerJournalAggregate {
    let PreparedOptiScalerJournalAggregate {
        runtime_identity,
        reservation,
        begin,
        current_journal_json,
        pending_identity,
        ..
    } = prepared;
    PreparedOptiScalerJournalAggregate {
        runtime_identity,
        reservation,
        begin,
        catalog_binding,
        current_journal_json,
        pending_identity,
    }
}

fn clone_committed(
    committed: &super::model::CommittedOptiScalerJournalAggregate,
) -> super::model::CommittedOptiScalerJournalAggregate {
    super::model::CommittedOptiScalerJournalAggregate {
        runtime_identity: committed.runtime_identity.clone(),
        aggregate: committed.aggregate.clone(),
        reservation: committed.reservation.clone(),
        begin: committed.begin.clone(),
        current_journal_json: committed.current_journal_json.clone(),
        pending_identity: committed.pending_identity,
    }
}

fn committed_cleanup_cursor_json(json: &str) -> String {
    let mut journal: OptiScalerJournal = serde_json::from_str(json).expect("committed json");
    let workspace_id =
        u32::try_from(journal.private_workspaces().len() - 1).expect("workspace ordinal");
    let expected_identity = journal.private_workspaces()[workspace_id as usize]
        .identity()
        .expect("workspace identity")
        .to_owned();
    journal.set_cleanup(CleanupState::WorkspaceRemoveIntent {
        workspace_id,
        expected_identity,
    });
    serde_json::to_string(&journal).expect("cleanup cursor json")
}

fn pending_exists(runtime: &PeerStorageRuntime, operation_id: &str) -> bool {
    runtime
        .repositories()
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT 1 FROM pending_file_mutations WHERE id = ?1",
                    [operation_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
                .map(|value| value.is_some())
                .map_err(crate::error::storage_error)
        })
        .expect("pending existence")
}

fn recover_optiscaler(
    runtime: &PeerStorageRuntime,
    id: &GameId,
) -> RecoveringOptiScalerJournalAggregate {
    let candidates = runtime
        .recover_pending_file_mutation_candidates_for_game(id, |_| true)
        .expect("recovery candidate");
    let Some(PendingFileMutationRecoveryCandidate::OptiScaler(proof)) =
        candidates.into_iter().next()
    else {
        panic!("expected one OptiScaler recovery candidate");
    };
    *proof
}

fn mark_catalog_ready(runtime: &PeerStorageRuntime, id: &GameId) {
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE catalog_scan_authority
                     SET readiness = 'complete', completed_at = 1,
                         invalidation_reason = NULL, mutation_token = NULL
                     WHERE game_id = ?1",
                    [id.as_str()],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("ready catalog");
}

fn set_catalog_binding_fields(
    runtime: &PeerStorageRuntime,
    id: &GameId,
    epoch: u64,
    token: Option<&str>,
) {
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE catalog_scan_authority
                     SET readiness = 'invalidated', authority_epoch = ?1,
                         invalidation_reason = 'prepared_optiscaler_mutation',
                         mutation_token = ?2, completed_at = NULL
                     WHERE game_id = ?3",
                    rusqlite::params![
                        i64::try_from(epoch).expect("epoch fits i64"),
                        token,
                        id.as_str()
                    ],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("catalog binding fields");
}

fn receipt(identity: &str, digest: char) -> FileReceipt {
    FileReceipt::owned(
        identity,
        Sha256Hash::new(digest.to_string().repeat(64)).expect("digest"),
    )
    .expect("receipt")
}

fn reused_receipt(identity: &str, bytes: &[u8]) -> FileReceipt {
    FileReceipt::reused(
        identity,
        renderpilot_detection::sha256_bytes(bytes).expect("digest"),
    )
    .expect("reused receipt")
}

fn aggregate_fixture(
    label: &str,
) -> (
    PeerStorageRuntime,
    GameId,
    PathRef,
    OptiScalerInstallState,
    GameProxyTopology,
) {
    let storage = SqliteStorage::in_memory().expect("storage");
    let id = game_id(&format!("commit-{label}"));
    let root = path(&format!("C:/Games/renderpilot-journal-{label}"));
    let game = GameInstallation::new(
        GameIdentity::new(id.clone(), "Journal commit", Launcher::Steam).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        root.clone(),
    );
    storage.upsert_game(&game).expect("game");
    let topology_id = format!("optiscaler:{}", id.as_str());
    let state = renderpilot_domain::from_persisted(
        renderpilot_domain::OptiScalerInstallStateParts {
            game_id: id.clone(),
            release_id: "release".to_owned(),
            manifest_revision: "manifest".to_owned(),
            archive_sha256: None,
            source: None,
            target_exe_path: path(&format!("{}/Game.exe", root.as_str())),
            target_dir: root.clone(),
            modules: vec!["core".to_owned()],
            release_files: vec![OptiScalerFileReceipt {
                path: path(&format!("{}/OptiScaler.ini", root.as_str())),
                installed: receipt("config", 'a'),
                role: OptiScalerFileRole::Configuration,
                cleanup: OptiScalerFileCleanup::RemoveIfUnchanged,
                baseline: renderpilot_domain::OptiScalerReleaseFileBaseline::Absent,
            }],
            runtime_bindings: Vec::new(),
            directory_receipts: Vec::new(),
            proxy_topology_id: Some(topology_id.clone()),
            config_schema: 1,
            config_base_release: "release".to_owned(),
            adoption_state: OptiScalerAdoptionState::Managed,
            prerequisite_binding: renderpilot_domain::OptiScalerPrerequisiteBinding::None,
            created_at: None,
            updated_at: None,
        },
        OptiScalerConfigurationBaseline::absent(),
    )
    .expect("state");
    let root_slot = path(&format!("{}/dxgi.dll", root.as_str()));
    let topology = GameProxyTopology {
        id: topology_id,
        game_id: id.clone(),
        root_slot: root_slot.clone(),
        outer: ProxyLink {
            implementation: ProxyImplementation::OptiScaler,
            path: root_slot,
            receipt: receipt("outer", 'b'),
        },
        downstream: None,
        downstream_origin: None,
        root_prestate: ProxyRootPrestate::Absent,
    };
    (PeerStorageRuntime::new(storage), id, root, state, topology)
}

/// An already-adopted aggregate, used only where metadata adoption itself is
/// part of the fixture. Normal journal-install cases start from absent state.
fn exact_adoption_aggregate_fixture(
    label: &str,
) -> (
    PeerStorageRuntime,
    GameId,
    PathRef,
    OptiScalerInstallState,
    GameProxyTopology,
) {
    let (runtime, id, root, state, mut topology) = aggregate_fixture(label);
    let original_config_bytes = b"adopted OptiScaler configuration";
    let configuration = reused_receipt("adopted-config", original_config_bytes);
    let mut parts = renderpilot_domain::OptiScalerInstallStateParts::from(&state);
    parts.release_files[0].installed = configuration.clone();
    parts.release_files[0].cleanup = OptiScalerFileCleanup::PreserveUnchanged;
    parts.adoption_state = OptiScalerAdoptionState::AdoptedExact;
    let state = renderpilot_domain::from_new_adoption(
        parts,
        OptiScalerConfigurationBaseline::present(configuration, original_config_bytes.to_vec())
            .expect("configuration baseline"),
    )
    .expect("exact adoption state");
    topology.outer.receipt = FileReceipt::reused(
        topology.outer.receipt.identity(),
        topology.outer.receipt.digest().clone(),
    )
    .expect("reused topology outer");
    (runtime, id, root, state, topology)
}

#[derive(Clone, Copy)]
enum JournalAction {
    Write,
    Delete,
    Verify,
}

fn operation_record(
    ordinal: u32,
    workspace_id: Option<u32>,
    effect: OperationEffect,
) -> OperationRecord {
    OperationRecord::new(
        ordinal,
        Vec::new(),
        workspace_id,
        PrivateArtifactSlots::new(
            DurableObservation::Absent,
            DurableObservation::Absent,
            DurableObservation::Absent,
        )
        .expect("participant slots"),
        effect,
    )
    .expect("operation")
}

fn journal_from_operations(
    root: &PathRef,
    operation_id: &str,
    capability: NamespaceCapability,
    operations: Vec<OperationRecord>,
) -> OptiScalerJournal {
    let workspaces = operations
        .iter()
        .filter_map(|operation| {
            operation.workspace_id().map(|workspace_id| {
                PrivateWorkspaceBinding::new(
                    workspace_id,
                    0,
                    format!(
                        "{}/.renderpilot-optiscaler-workspace-{operation_id}-{workspace_id}-{}",
                        root.as_str(),
                        capability.as_str()
                    ),
                    None,
                    capability.clone(),
                )
                .expect("workspace")
            })
        })
        .collect();
    OptiScalerJournal::new(
        vec![root.as_str().to_owned()],
        ControlNamespaceBinding::new(
            format!(
                "{}/control-{operation_id}-{}",
                root.as_str(),
                capability.as_str()
            ),
            None,
            capability,
        )
        .expect("control namespace"),
        workspaces,
        operations,
    )
    .expect("journal")
}

fn bind_materialized_journal(journal: &mut OptiScalerJournal) {
    journal
        .control_namespace_mut()
        .set_identity("control-identity")
        .expect("control identity");
    for workspace in journal.private_workspaces_mut() {
        let workspace_id = workspace.workspace_id();
        workspace
            .set_identity(format!("workspace-{workspace_id}"))
            .expect("workspace identity");
    }
    journal.set_materialization(MaterializationState::Ready);
    journal.set_cleanup(CleanupState::Inactive);
}

fn file_observation(receipt: &FileReceipt) -> DurableObservation {
    DurableObservation::File {
        identity: receipt.identity().to_owned(),
        digest: receipt.digest().clone(),
    }
}

fn transition_journals(
    root: &PathRef,
    operation_id: &str,
    entries: &[(
        &PathRef,
        Option<&FileReceipt>,
        Option<&FileReceipt>,
        JournalAction,
    )],
) -> (String, String) {
    let capability = NamespaceCapability::new("a".repeat(64)).expect("capability");
    let mut operations = Vec::new();
    let mut next_workspace_id = 0_u32;
    for (ordinal, (target, before, _, action)) in entries.iter().enumerate() {
        let preimage = match before {
            Some(receipt) => Preimage::Initial {
                observation: file_observation(receipt),
                receipt: Some((*receipt).clone()),
                owned_basis: receipt
                    .authorizes_destructive_cleanup()
                    .then(|| (*receipt).clone()),
            },
            None => Preimage::Initial {
                observation: DurableObservation::Absent,
                receipt: None,
                owned_basis: None,
            },
        };
        let endpoint = OperationEndpoint::new(
            Endpoint::Single,
            target.as_str(),
            preimage,
            ExpectedAfter::Pending,
        )
        .expect("endpoint");
        let effect = match action {
            JournalAction::Write => OperationEffect::Write(
                WriteEffect::new(endpoint, WriteState::Planned).expect("write"),
            ),
            JournalAction::Delete => OperationEffect::Delete(
                DeleteEffect::new(endpoint, DeleteState::Planned).expect("delete"),
            ),
            JournalAction::Verify => OperationEffect::Verify(
                VerifyEffect::new(endpoint, VerifyState::Planned).expect("verify"),
            ),
        };
        let workspace_id = effect.requires_private_workspace().then(|| {
            let workspace_id = next_workspace_id;
            next_workspace_id += 1;
            workspace_id
        });
        let op_ordinal = u32::try_from(ordinal).expect("ordinal fits u32");
        operations.push(operation_record(op_ordinal, workspace_id, effect));
    }
    let mut initial = journal_from_operations(root, operation_id, capability, operations);
    let initial_json = serde_json::to_string(&initial).expect("initial json");

    for (operation, (_, before, after, action)) in
        initial.operations_mut().iter_mut().zip(entries.iter())
    {
        let after = after.map(file_observation);
        match (operation.effect_mut(), action, before, after) {
            (OperationEffect::Write(effect), JournalAction::Write, _, Some(after)) => {
                effect
                    .endpoint_mut()
                    .set_expected_after(ExpectedAfter::Known(after.clone()));
                *effect.state_mut() = WriteState::Applied {
                    live: after,
                    custody: DurableObservation::Absent,
                };
            }
            (OperationEffect::Delete(effect), JournalAction::Delete, Some(before), None) => {
                effect
                    .endpoint_mut()
                    .set_expected_after(ExpectedAfter::Known(DurableObservation::Absent));
                let custody = file_observation(before);
                *effect.state_mut() = DeleteState::Applied {
                    custody: custody.clone(),
                };
                *operation.slots_mut().custody_mut() = custody;
            }
            (OperationEffect::Verify(effect), JournalAction::Verify, _, Some(after)) => {
                effect
                    .endpoint_mut()
                    .set_expected_after(ExpectedAfter::Known(after.clone()));
                *effect.state_mut() = VerifyState::Applied { observed: after };
            }
            _ => panic!("journal entry action and image do not match"),
        }
    }
    bind_materialized_journal(&mut initial);
    let final_json = serde_json::to_string(&initial).expect("final json");
    (initial_json, final_json)
}

fn peer_replace_transition_journals(
    root: &PathRef,
    operation_id: &str,
    source: &PathRef,
    source_receipt: &FileReceipt,
    destination: &PathRef,
    destination_receipt: &FileReceipt,
    verifies: &[(&PathRef, &FileReceipt)],
) -> (String, String) {
    let capability = NamespaceCapability::new("a".repeat(64)).expect("capability");
    let source_endpoint = OperationEndpoint::new(
        Endpoint::Source,
        source.as_str(),
        Preimage::Initial {
            observation: file_observation(source_receipt),
            receipt: Some(source_receipt.clone()),
            owned_basis: source_receipt
                .authorizes_destructive_cleanup()
                .then(|| source_receipt.clone()),
        },
        ExpectedAfter::Pending,
    )
    .expect("source endpoint");
    let destination_endpoint = OperationEndpoint::new(
        Endpoint::Destination,
        destination.as_str(),
        Preimage::Initial {
            observation: DurableObservation::Absent,
            receipt: None,
            owned_basis: None,
        },
        ExpectedAfter::Pending,
    )
    .expect("destination endpoint");
    let relocation = operation_record(
        0,
        None,
        OperationEffect::Relocate(Box::new(
            RelocateEffect::new(
                source_endpoint,
                destination_endpoint,
                RelocateState::Planned,
            )
            .expect("relocation effect"),
        )),
    );

    let mut operations = vec![relocation];
    for (ordinal, (target, receipt)) in verifies.iter().enumerate() {
        let ordinal = u32::try_from(ordinal + 1).expect("verify ordinal");
        let endpoint = OperationEndpoint::new(
            Endpoint::Single,
            target.as_str(),
            Preimage::Initial {
                observation: file_observation(receipt),
                receipt: Some((*receipt).clone()),
                owned_basis: receipt
                    .authorizes_destructive_cleanup()
                    .then(|| (*receipt).clone()),
            },
            ExpectedAfter::Pending,
        )
        .expect("verify endpoint");
        operations.push(operation_record(
            ordinal,
            None,
            OperationEffect::Verify(
                VerifyEffect::new(endpoint, VerifyState::Planned).expect("verify effect"),
            ),
        ));
    }

    let mut journal = journal_from_operations(root, operation_id, capability, operations);
    let initial_json = serde_json::to_string(&journal).expect("initial replacement journal");

    if let OperationEffect::Relocate(effect) = journal.operations_mut()[0].effect_mut() {
        effect
            .source_mut()
            .set_expected_after(ExpectedAfter::Known(DurableObservation::Absent));
        effect
            .destination_mut()
            .set_expected_after(ExpectedAfter::Known(file_observation(destination_receipt)));
        *effect.state_mut() = RelocateState::Applied {
            source_after: DurableObservation::Absent,
            destination_after: file_observation(destination_receipt),
        };
    } else {
        panic!("replacement journal first operation is not relocation");
    }
    for (operation, (target, receipt)) in journal.operations_mut()[1..]
        .iter_mut()
        .zip(verifies.iter())
    {
        if let OperationEffect::Verify(effect) = operation.effect_mut() {
            effect
                .endpoint_mut()
                .set_expected_after(ExpectedAfter::Known(file_observation(receipt)));
            *effect.state_mut() = VerifyState::Applied {
                observed: file_observation(receipt),
            };
        } else {
            panic!("replacement journal verification operation is not verify");
        }
        assert_eq!(operation.effect().endpoints()[0].path(), target.as_str());
    }
    bind_materialized_journal(&mut journal);
    let final_json = serde_json::to_string(&journal).expect("final replacement journal");
    (initial_json, final_json)
}

fn prepared_install(
    label: &str,
    operation_id: &str,
) -> (
    PeerStorageRuntime,
    GameId,
    PathRef,
    OptiScalerInstallState,
    GameProxyTopology,
    PreparedOptiScalerJournalAggregate,
) {
    let (runtime, id, root, state, topology) = aggregate_fixture(label);
    let config = &state.release_files[0];
    let entries = [
        (
            &config.path,
            None,
            Some(&config.installed),
            JournalAction::Write,
        ),
        (
            &topology.outer.path,
            None,
            Some(&topology.outer.receipt),
            JournalAction::Write,
        ),
    ];
    let (initial, final_json) = transition_journals(&root, operation_id, &entries);
    let preparing = begin_for(
        &runtime,
        &id,
        operation_id,
        renderpilot_domain::mutation_features::OPTISCALER_INSTALL,
        Some(&topology.id),
        &initial,
    );
    let prepared = runtime
        .finish_optiscaler_journal_aggregate(preparing, final_json)
        .expect("finish");
    (runtime, id, root, state, topology, prepared)
}

fn install_mutation<'a>(
    state: &'a OptiScalerInstallState,
    topology: &'a GameProxyTopology,
    operation_id: &'a str,
) -> OptiScalerAggregateMutation<'a> {
    OptiScalerAggregateMutation::Filesystem {
        before_state: None,
        after_state: Some(state),
        before_topology: None,
        after_topology: Some(topology),
        peer: OptiScalerPeerMutation::Keep,
        mutation_id: operation_id,
    }
}

#[test]
fn begin_persists_exact_journal_binding_and_null_program() {
    let (runtime, id, initial) = seed("begin");
    let preparing = begin(&runtime, &id, "journal-begin", &initial);
    let (state, kind, revision) = pending_tuple(&runtime, "journal-begin");
    assert_eq!(state, "preparing");
    assert_eq!(kind, "optiscaler_journal");
    assert_eq!(revision, Some(0));
    let reservation = reservation(&runtime, &id, "journal-begin").expect("reservation");
    assert_eq!(reservation.kind(), PeerAggregateKind::OptiScalerJournal);
    assert_eq!(
        reservation.binding(),
        PeerAggregateKind::OptiScalerJournal.binding()
    );
    assert_eq!(
        reservation.state(),
        PeerAggregateReservationState::Preparing
    );
    assert_eq!(reservation.expected_revision().as_i64(), 0);
    assert_eq!(preparing.begin().initial_journal_json(), initial);
}

#[test]
fn begin_allows_an_unscoped_subject_and_persists_sql_null() {
    let (runtime, id, initial) = seed("none-subject");
    let begin = OptiScalerJournalAggregateBegin::new(
        "journal-none-subject",
        id,
        "optiscaler_install",
        None,
        initial,
    )
    .expect("begin input");
    let preparing = runtime
        .begin_optiscaler_journal_aggregate(begin)
        .expect("begin");
    assert_eq!(preparing.begin().subject_id(), None);
    let subject: Option<String> = runtime
        .repositories()
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT subject_id FROM pending_file_mutations WHERE id = ?1",
                    ["journal-none-subject"],
                    |row| row.get(0),
                )
                .map_err(crate::error::storage_error)
        })
        .expect("subject");
    assert_eq!(subject, None);
    drop(preparing);
}

#[test]
fn preparing_journal_cas_is_move_only_and_preserves_reservation_and_generation() {
    let (runtime, id, initial) = seed("preparing-cas");
    let preparing = begin(&runtime, &id, "journal-preparing-cas", &initial);
    let before = pending_identity(&runtime, "journal-preparing-cas");
    let control_intent =
        journal_with_materialization(&initial, MaterializationState::ControlCreateIntent, None);
    let preparing = runtime
        .cas_preparing_optiscaler_journal_aggregate(preparing, control_intent.clone())
        .expect("first preparing CAS");
    let after_first = pending_identity(&runtime, "journal-preparing-cas");
    assert_eq!(after_first.0, before.0);
    assert_eq!(after_first.1, before.1);
    assert!(after_first.2 >= before.2);
    assert_eq!(after_first.3, control_intent);

    let workspaces = journal_with_materialization(
        &control_intent,
        MaterializationState::Workspaces {
            next_workspace_id: 0,
        },
        Some("control-cas"),
    );
    let preparing = runtime
        .cas_preparing_optiscaler_journal_aggregate(preparing, workspaces.clone())
        .expect("second preparing CAS");
    let after_second = pending_identity(&runtime, "journal-preparing-cas");
    assert_eq!(after_second.0, before.0);
    assert_eq!(after_second.1, before.1);
    assert!(after_second.2 >= after_first.2);
    assert_eq!(after_second.3, workspaces);
    assert_eq!(
        pending_tuple(&runtime, "journal-preparing-cas").0,
        "preparing"
    );
    assert_eq!(
        reservation(&runtime, &id, "journal-preparing-cas")
            .expect("reservation")
            .expected_revision()
            .as_i64(),
        0
    );
    drop(preparing);
}

#[test]
fn finish_uses_the_current_preparing_cas_json() {
    let (runtime, id, initial) = seed("finish-after-cas");
    let preparing = begin(&runtime, &id, "journal-finish-after-cas", &initial);
    let control_intent =
        journal_with_materialization(&initial, MaterializationState::ControlCreateIntent, None);
    let preparing = runtime
        .cas_preparing_optiscaler_journal_aggregate(preparing, control_intent)
        .expect("preparing CAS");
    let final_json = prepared_journal();
    let prepared = runtime
        .finish_optiscaler_journal_aggregate(preparing, final_json.clone())
        .expect("finish after preparing CAS");
    assert_eq!(prepared.final_journal_json(), final_json);
    assert_eq!(
        pending_tuple(&runtime, "journal-finish-after-cas").0,
        "prepared"
    );
}

#[test]
fn finish_publishes_prepared_json_without_program() {
    let (runtime, id, initial) = seed("finish");
    let preparing = begin(&runtime, &id, "journal-finish", &initial);
    let final_json = prepared_journal();
    let prepared = runtime
        .finish_optiscaler_journal_aggregate(preparing, final_json.clone())
        .expect("finish");
    assert_eq!(prepared.final_journal_json(), final_json);
    assert_eq!(pending_tuple(&runtime, "journal-finish").0, "prepared");
    assert_eq!(
        pending_tuple(&runtime, "journal-finish").1,
        "optiscaler_journal"
    );
    assert_eq!(
        reservation(&runtime, &id, "journal-finish")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Prepared
    );
}

#[test]
fn prepared_journal_cas_accepts_one_reverse_action_and_returns_new_proof() {
    let (runtime, id, _root, state, topology, prepared) =
        prepared_install("prepared-cas", "journal-prepared-cas");
    let current = prepared.final_journal_json().to_owned();
    let next = prepared_reverse_json(&current);
    let prepared = runtime
        .cas_prepared_optiscaler_journal_aggregate(prepared, next.clone())
        .expect("prepared reverse CAS");
    assert_eq!(prepared.final_journal_json(), next);
    assert_eq!(pending_identity(&runtime, "journal-prepared-cas").3, next);
    assert_eq!(
        pending_tuple(&runtime, "journal-prepared-cas").0,
        "prepared"
    );
    assert_eq!(
        reservation(&runtime, &id, "journal-prepared-cas")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Prepared
    );
    let before = AggregateBefore::new(id, None, None, None).expect("before");
    assert!(
        runtime
            .commit_optiscaler_journal_aggregate(
                prepared,
                OptiScalerJournalAggregateCommit::new(
                    before,
                    install_mutation(&state, &topology, "journal-prepared-cas"),
                ),
            )
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-prepared-cas").0,
        "prepared"
    );
}

#[test]
fn journal_cas_rejects_an_invalid_transition_without_partial_state_change() {
    let (runtime, id, initial) = seed("cas-invalid-transition");
    let preparing = begin(&runtime, &id, "journal-cas-invalid-transition", &initial);
    let before = pending_identity(&runtime, "journal-cas-invalid-transition");
    let result = runtime.cas_preparing_optiscaler_journal_aggregate(preparing, prepared_journal());
    assert!(result.is_err(), "CAS must reject a skipped lifecycle edge");
    assert_eq!(
        pending_identity(&runtime, "journal-cas-invalid-transition"),
        before
    );
    let generation: i64 = runtime
        .repositories()
        .with_connection(|connection| {
            connection
                .query_row(
                    "SELECT peer_aggregate_revision FROM games WHERE id = ?1",
                    [id.as_str()],
                    |row| row.get(0),
                )
                .map_err(crate::error::storage_error)
        })
        .expect("generation");
    assert_eq!(generation, 0);
}

#[test]
fn journal_cas_rejects_runtime_row_and_reservation_drift() {
    let (runtime, id, initial) = seed("cas-runtime");
    let preparing = begin(&runtime, &id, "journal-cas-runtime", &initial);
    let other_runtime = PeerStorageRuntime::new(SqliteStorage::in_memory().expect("storage"));
    assert!(
        other_runtime
            .cas_preparing_optiscaler_journal_aggregate(preparing, initial)
            .is_err(),
        "a proof must be bound to its minting runtime"
    );

    let (runtime, id, initial) = seed("cas-row-drift");
    let preparing = begin(&runtime, &id, "journal-cas-row-drift", &initial);
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE pending_file_mutations
                     SET manifest_json = '{}', updated_at = updated_at + 1
                     WHERE id = ?1",
                    ["journal-cas-row-drift"],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("row drift");
    assert!(
        runtime
            .cas_preparing_optiscaler_journal_aggregate(preparing, initial)
            .is_err(),
        "row JSON and timestamp drift must invalidate the proof"
    );

    let (runtime, id, initial) = seed("cas-generation-drift");
    let preparing = begin(&runtime, &id, "journal-cas-generation-drift", &initial);
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE games SET peer_aggregate_revision = 1 WHERE id = ?1",
                    [id.as_str()],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("generation drift");
    assert!(
        runtime
            .cas_preparing_optiscaler_journal_aggregate(preparing, initial)
            .is_err(),
        "game generation drift must invalidate the proof"
    );

    let (runtime, id, initial) = seed("cas-reservation-drift");
    let preparing = begin(&runtime, &id, "journal-cas-reservation-drift", &initial);
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE peer_aggregate_reservations
                     SET state = 'prepared'
                     WHERE game_id = ?1 AND operation_id = ?2",
                    [id.as_str(), "journal-cas-reservation-drift"],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("reservation drift");
    assert!(
        runtime
            .cas_preparing_optiscaler_journal_aggregate(preparing, initial)
            .is_err(),
        "reservation revision drift must invalidate the proof"
    );
}

#[test]
fn finish_invalidates_ready_catalog_with_the_exact_operation_token() {
    let (runtime, id, initial) = seed("ready-catalog");
    mark_catalog_ready(&runtime, &id);
    let preparing = begin(&runtime, &id, "journal-ready-catalog", &initial);
    let prepared = runtime
        .finish_optiscaler_journal_aggregate(preparing, prepared_journal())
        .expect("finish");
    assert!(matches!(
        prepared.catalog_binding,
        pending_file_mutations::PreparedOptiScalerCatalogBinding::CatalogInvalidated {
            authority_epoch: 1,
            ref mutation_token,
        } if mutation_token == "journal-ready-catalog"
    ));
    let readiness = runtime
        .repositories()
        .catalog_readiness(&id)
        .expect("readiness");
    assert!(matches!(
        readiness,
        crate::repositories::CatalogReadiness::Invalidated {
            reason,
            mutation_token: Some(token),
            ..
        } if reason == "prepared_optiscaler_mutation" && token == "journal-ready-catalog"
    ));
}

#[test]
fn absent_catalog_binding_is_a_valid_noop() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let id = game_id("absent-catalog");
    let binding = storage
        .with_transaction(|transaction| {
            pending_file_mutations::prepare_optiscaler_catalog_binding(
                transaction,
                &id,
                "journal-absent-catalog",
            )
        })
        .expect("absent catalog binding");
    assert!(matches!(
        binding,
        pending_file_mutations::PreparedOptiScalerCatalogBinding::CatalogAbsent
    ));
}

#[test]
fn catalog_binding_failure_rolls_back_prepared_transition() {
    let (runtime, id, initial) = seed("catalog-failure");
    let preparing = begin(&runtime, &id, "journal-catalog-failure", &initial);
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "DELETE FROM catalog_scan_authority WHERE game_id = ?1",
                    [id.as_str()],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("remove catalog authority");
    assert!(
        runtime
            .finish_optiscaler_journal_aggregate(preparing, prepared_journal())
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-catalog-failure").0,
        "preparing"
    );
    assert_eq!(
        reservation(&runtime, &id, "journal-catalog-failure")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Preparing
    );
}

#[test]
fn prepared_catalog_binding_is_revalidated_before_cas_commit_and_cleanup() {
    let (runtime, id, _root, _state, _topology, prepared) = prepared_install(
        "catalog-absent-to-present",
        "journal-catalog-absent-to-present",
    );
    let prepared = retarget_prepared_catalog_binding(
        prepared,
        pending_file_mutations::PreparedOptiScalerCatalogBinding::CatalogAbsent,
    );
    assert!(
        runtime
            .cas_prepared_optiscaler_journal_aggregate(prepared, prepared_journal(),)
            .is_err(),
        "an absent binding cannot accept a present catalog"
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-catalog-absent-to-present").0,
        "prepared"
    );
    assert!(reservation(&runtime, &id, "journal-catalog-absent-to-present").is_some());

    let (runtime, id, initial) = seed("catalog-cas-epoch-drift");
    mark_catalog_ready(&runtime, &id);
    let preparing = begin(&runtime, &id, "journal-catalog-cas-epoch-drift", &initial);
    let prepared = runtime
        .finish_optiscaler_journal_aggregate(preparing, prepared_journal())
        .expect("finish");
    let before_identity = pending_identity(&runtime, "journal-catalog-cas-epoch-drift");
    set_catalog_binding_fields(&runtime, &id, 2, Some("journal-catalog-cas-epoch-drift"));
    assert!(
        runtime
            .cas_prepared_optiscaler_journal_aggregate(prepared, prepared_journal(),)
            .is_err(),
        "a changed authority epoch must reject Prepared CAS"
    );
    assert_eq!(
        pending_identity(&runtime, "journal-catalog-cas-epoch-drift"),
        before_identity
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-catalog-cas-epoch-drift").0,
        "prepared"
    );
    assert_eq!(
        reservation(&runtime, &id, "journal-catalog-cas-epoch-drift")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Prepared
    );

    let (runtime, id, initial) = seed("catalog-present-to-absent");
    let preparing = begin(&runtime, &id, "journal-catalog-present-to-absent", &initial);
    let prepared = runtime
        .finish_optiscaler_journal_aggregate(preparing, prepared_journal())
        .expect("finish");
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "DELETE FROM catalog_scan_authority WHERE game_id = ?1",
                    [id.as_str()],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("remove catalog authority");
    assert!(
        runtime
            .cas_prepared_optiscaler_journal_aggregate(prepared, prepared_journal(),)
            .is_err(),
        "a present binding cannot accept an absent catalog"
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-catalog-present-to-absent").0,
        "prepared"
    );
    assert!(reservation(&runtime, &id, "journal-catalog-present-to-absent").is_some());

    let (runtime, id, _root, state, topology, prepared) = prepared_install(
        "catalog-commit-token-drift",
        "journal-catalog-commit-token-drift",
    );
    set_catalog_binding_fields(&runtime, &id, 1, Some("other-operation"));
    let before = AggregateBefore::new(id.clone(), None, None, None).expect("before");
    assert!(
        runtime
            .commit_optiscaler_journal_aggregate(
                prepared,
                OptiScalerJournalAggregateCommit::new(
                    before,
                    install_mutation(&state, &topology, "journal-catalog-commit-token-drift"),
                ),
            )
            .is_err(),
        "a changed mutation token must reject aggregate commit"
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-catalog-commit-token-drift").0,
        "prepared"
    );
    assert_eq!(
        reservation(&runtime, &id, "journal-catalog-commit-token-drift")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Prepared
    );
    assert!(
        runtime
            .repositories()
            .get_optiscaler_install_state(&id)
            .expect("state")
            .is_none(),
        "catalog binding rejection must precede participant publication"
    );

    let (runtime, id, _root, _state, _topology, prepared) = prepared_install(
        "catalog-cleanup-token-drift",
        "journal-catalog-cleanup-token-drift",
    );
    set_catalog_binding_fields(&runtime, &id, 1, Some("other-operation"));
    assert!(
        runtime
            .delete_prepared_optiscaler_journal_aggregate_after_rollback(prepared)
            .is_err(),
        "a changed mutation token must reject Prepared rollback cleanup"
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-catalog-cleanup-token-drift").0,
        "prepared"
    );
    assert_eq!(
        reservation(&runtime, &id, "journal-catalog-cleanup-token-drift")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Prepared
    );
}

#[test]
fn aggregate_commit_installs_filesystem_participants_and_advances_every_fence() {
    let (runtime, id, _root, state, topology, prepared) =
        prepared_install("install", "journal-install");
    let before = AggregateBefore::new(id.clone(), None, None, None).expect("before");
    let mutation = install_mutation(&state, &topology, "journal-install");
    let committed = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(before, mutation),
        )
        .expect("aggregate install commit");

    assert_eq!(committed.aggregate().operation_id(), "journal-install");
    assert_eq!(committed.aggregate().generation().as_i64(), 1);
    assert_eq!(committed.aggregate().after().state(), Some(&state));
    assert_eq!(committed.aggregate().after().topology(), Some(&topology));
    assert_eq!(
        runtime
            .repositories()
            .get_optiscaler_install_state(&id)
            .expect("state")
            .as_ref()
            .map(|state| state.release_id.as_str()),
        Some("release")
    );
    assert_eq!(
        runtime
            .repositories()
            .get_proxy_topology(&id)
            .expect("topology")
            .as_ref(),
        Some(&topology)
    );
    assert_eq!(pending_tuple(&runtime, "journal-install").0, "committed");
    assert_eq!(
        reservation(&runtime, &id, "journal-install")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Committed
    );
}

#[test]
fn committed_journal_cas_accepts_the_namespace_cleanup_cursor() {
    let (runtime, id, _root, state, topology, prepared) =
        prepared_install("committed-cas", "journal-committed-cas");
    let before = AggregateBefore::new(id.clone(), None, None, None).expect("before");
    let committed = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(
                before,
                install_mutation(&state, &topology, "journal-committed-cas"),
            ),
        )
        .expect("aggregate commit");
    let current = committed.current_journal_json.clone();
    let next = committed_cleanup_cursor_json(&current);
    let committed = runtime
        .cas_committed_optiscaler_journal_aggregate(committed, next.clone())
        .expect("committed cleanup CAS");
    assert_eq!(committed.aggregate().generation().as_i64(), 1);
    assert_eq!(pending_identity(&runtime, "journal-committed-cas").3, next);
    assert_eq!(
        pending_tuple(&runtime, "journal-committed-cas").0,
        "committed"
    );
    assert_eq!(
        reservation(&runtime, &id, "journal-committed-cas")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Committed
    );
}

#[test]
fn terminal_rollback_deletes_preparing_and_prepared_pairs_atomically() {
    let (runtime, id, initial) = seed("rollback-preparing-terminal");
    let preparing = begin(
        &runtime,
        &id,
        "journal-rollback-preparing-terminal",
        &initial,
    );
    let terminal = rollback_terminal_json(&prepared_journal());
    let preparing = retarget_preparing(&runtime, preparing, terminal);
    runtime
        .delete_preparing_optiscaler_journal_aggregate_after_rollback(preparing)
        .expect("preparing terminal rollback cleanup");
    assert!(!pending_exists(
        &runtime,
        "journal-rollback-preparing-terminal"
    ));
    assert!(reservation(&runtime, &id, "journal-rollback-preparing-terminal").is_none());

    let (runtime, id, initial) = seed("rollback-prepared-terminal");
    let preparing = begin(
        &runtime,
        &id,
        "journal-rollback-prepared-terminal",
        &initial,
    );
    let prepared = runtime
        .finish_optiscaler_journal_aggregate(preparing, prepared_journal())
        .expect("finish");
    let terminal = rollback_terminal_json(prepared.final_journal_json());
    let prepared = retarget_prepared(&runtime, prepared, terminal);
    runtime
        .delete_prepared_optiscaler_journal_aggregate_after_rollback(prepared)
        .expect("prepared terminal rollback cleanup");
    assert!(!pending_exists(
        &runtime,
        "journal-rollback-prepared-terminal"
    ));
    assert!(reservation(&runtime, &id, "journal-rollback-prepared-terminal").is_none());
}

#[test]
fn committed_terminal_cleanup_deletes_only_durable_evidence() {
    let (runtime, id, _root, state, topology, prepared) =
        prepared_install("committed-terminal", "journal-committed-terminal");
    let before = AggregateBefore::new(id.clone(), None, None, None).expect("before");
    let committed = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(
                before,
                install_mutation(&state, &topology, "journal-committed-terminal"),
            ),
        )
        .expect("aggregate commit");
    let terminal = committed_terminal_json(&committed.current_journal_json);
    let committed = retarget_committed(&runtime, committed, terminal);
    let duplicate = clone_committed(&committed);
    runtime
        .delete_committed_optiscaler_journal_aggregate(committed)
        .expect("committed terminal cleanup");
    assert!(
        runtime
            .delete_committed_optiscaler_journal_aggregate(duplicate)
            .is_err(),
        "a terminal proof cannot be reused after its pair has been deleted"
    );

    assert!(!pending_exists(&runtime, "journal-committed-terminal"));
    assert!(reservation(&runtime, &id, "journal-committed-terminal").is_none());
    let persisted_state = runtime
        .repositories()
        .get_optiscaler_install_state(&id)
        .expect("state")
        .expect("persisted state");
    let mut expected_state = state;
    expected_state.created_at = persisted_state.created_at;
    expected_state.updated_at = persisted_state.updated_at;
    assert_eq!(
        persisted_state, expected_state,
        "committed cleanup must not restore or remove live participants"
    );
    assert_eq!(
        runtime
            .repositories()
            .get_proxy_topology(&id)
            .expect("topology")
            .as_ref(),
        Some(&topology),
        "committed cleanup must not mutate topology"
    );
}

#[test]
fn committed_terminal_cleanup_accepts_cleared_historical_custody() {
    let (runtime, id, _root, state, topology, prepared) = prepared_install(
        "committed-cleared-custody",
        "journal-committed-cleared-custody",
    );
    let before = AggregateBefore::new(id.clone(), None, None, None).expect("before");
    let committed = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(
                before,
                install_mutation(&state, &topology, "journal-committed-cleared-custody"),
            ),
        )
        .expect("aggregate commit");

    let mut terminal: OptiScalerJournal =
        serde_json::from_str(&committed.current_journal_json).expect("committed journal");
    let historical_custody = DurableObservation::File {
        identity: "historical-custody".to_owned(),
        digest: Sha256Hash::new("c".repeat(64)).expect("historical digest"),
    };
    if let OperationEffect::Write(effect) = terminal.operations_mut()[0].effect_mut()
        && let WriteState::Applied {
            custody: applied_custody,
            ..
        } = effect.state_mut()
    {
        *applied_custody = historical_custody;
    }
    *terminal.operations_mut()[0].slots_mut().custody_mut() = DurableObservation::Absent;
    terminal.set_cleanup(CleanupState::Complete);
    let terminal_json = serde_json::to_string(&terminal).expect("terminal json");
    let committed = retarget_committed(&runtime, committed, terminal_json);

    runtime
        .delete_committed_optiscaler_journal_aggregate(committed)
        .expect("committed cleanup may delete historical custody");
    assert!(!pending_exists(
        &runtime,
        "journal-committed-cleared-custody"
    ));
    assert!(reservation(&runtime, &id, "journal-committed-cleared-custody").is_none());
}

#[test]
fn terminal_cleanup_rejects_nonterminal_json_and_runtime_mismatch() {
    let (runtime, id, initial) = seed("terminal-nonterminal");
    let preparing = begin(&runtime, &id, "journal-terminal-nonterminal", &initial);
    assert!(
        runtime
            .delete_preparing_optiscaler_journal_aggregate_after_rollback(preparing)
            .is_err(),
        "rollback cleanup must require the exact terminal journal state"
    );
    assert!(pending_exists(&runtime, "journal-terminal-nonterminal"));
    assert!(reservation(&runtime, &id, "journal-terminal-nonterminal").is_some());

    let (runtime, id, initial) = seed("terminal-runtime-mismatch");
    let preparing = begin(&runtime, &id, "journal-terminal-runtime-mismatch", &initial);
    let foreign_runtime = PeerStorageRuntime::new(SqliteStorage::in_memory().expect("storage"));
    assert!(
        foreign_runtime
            .delete_preparing_optiscaler_journal_aggregate_after_rollback(preparing)
            .is_err(),
        "terminal proof must remain bound to its minting runtime"
    );
    assert!(pending_exists(
        &runtime,
        "journal-terminal-runtime-mismatch"
    ));
    assert!(reservation(&runtime, &id, "journal-terminal-runtime-mismatch").is_some());

    let (runtime, id, _root, state, topology, prepared) = prepared_install(
        "terminal-committed-nonterminal",
        "journal-committed-nonterminal",
    );
    let before = AggregateBefore::new(id.clone(), None, None, None).expect("before");
    let committed = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(
                before,
                install_mutation(&state, &topology, "journal-committed-nonterminal"),
            ),
        )
        .expect("aggregate commit");
    assert!(
        runtime
            .delete_committed_optiscaler_journal_aggregate(committed)
            .is_err(),
        "committed cleanup must reject a nonterminal journal"
    );
    assert!(pending_exists(&runtime, "journal-committed-nonterminal"));
    assert!(reservation(&runtime, &id, "journal-committed-nonterminal").is_some());
}

#[test]
fn terminal_cleanup_rejects_row_reservation_and_generation_drift_atomically() {
    let (runtime, id, initial) = seed("terminal-row-drift");
    let preparing = begin(&runtime, &id, "journal-terminal-row-drift", &initial);
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE pending_file_mutations
                     SET manifest_json = '{}', updated_at = updated_at + 1
                     WHERE id = ?1",
                    ["journal-terminal-row-drift"],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("row drift");
    assert!(
        runtime
            .delete_preparing_optiscaler_journal_aggregate_after_rollback(preparing)
            .is_err()
    );
    assert!(pending_exists(&runtime, "journal-terminal-row-drift"));
    assert!(reservation(&runtime, &id, "journal-terminal-row-drift").is_some());

    let (runtime, id, initial) = seed("terminal-reservation-drift");
    let preparing = begin(
        &runtime,
        &id,
        "journal-terminal-reservation-drift",
        &initial,
    );
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE peer_aggregate_reservations
                     SET state = 'prepared'
                     WHERE game_id = ?1 AND operation_id = ?2",
                    [id.as_str(), "journal-terminal-reservation-drift"],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("reservation drift");
    assert!(
        runtime
            .delete_preparing_optiscaler_journal_aggregate_after_rollback(preparing)
            .is_err()
    );
    assert!(pending_exists(
        &runtime,
        "journal-terminal-reservation-drift"
    ));
    assert!(reservation(&runtime, &id, "journal-terminal-reservation-drift").is_some());

    let (runtime, id, initial) = seed("terminal-generation-drift");
    let preparing = begin(&runtime, &id, "journal-terminal-generation-drift", &initial);
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE games SET peer_aggregate_revision = 1 WHERE id = ?1",
                    [id.as_str()],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("generation drift");
    assert!(
        runtime
            .delete_preparing_optiscaler_journal_aggregate_after_rollback(preparing)
            .is_err()
    );
    assert!(pending_exists(
        &runtime,
        "journal-terminal-generation-drift"
    ));
    assert!(reservation(&runtime, &id, "journal-terminal-generation-drift").is_some());
}

#[test]
fn aggregate_commit_rejects_metadata_adoption_and_wrong_operation_before_publishing() {
    let (runtime, id, _root, state, topology, prepared) =
        prepared_install("metadata-rejected", "journal-metadata-rejected");
    let before = AggregateBefore::new(id, None, None, None).expect("before");
    let error = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(
                before,
                OptiScalerAggregateMutation::AdoptExactMetadata {
                    state: &state,
                    topology: &topology,
                },
            ),
        )
        .expect_err("metadata adoption must not use a prepared journal");
    assert!(error.to_string().contains("metadata adoption"));
    assert_eq!(
        pending_tuple(&runtime, "journal-metadata-rejected").0,
        "prepared"
    );

    let (runtime, id, _root, state, topology, prepared) =
        prepared_install("wrong-operation", "journal-right-operation");
    let before = AggregateBefore::new(id, None, None, None).expect("before");
    let error = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(
                before,
                install_mutation(&state, &topology, "journal-wrong-operation"),
            ),
        )
        .expect_err("wrong operation id must fail before the transaction");
    assert!(error.to_string().contains("mutation id"));
    assert_eq!(
        pending_tuple(&runtime, "journal-right-operation").0,
        "prepared"
    );
}

#[test]
fn aggregate_commit_rejects_a_runtime_or_before_image_from_another_owner() {
    let (runtime, id, _root, state, topology, prepared) =
        prepared_install("runtime-drift", "journal-runtime-drift");
    let foreign_runtime = PeerStorageRuntime::new(SqliteStorage::in_memory().expect("storage"));
    let before = AggregateBefore::new(id, None, None, None).expect("before");
    assert!(
        foreign_runtime
            .commit_optiscaler_journal_aggregate(
                prepared,
                OptiScalerJournalAggregateCommit::new(
                    before,
                    install_mutation(&state, &topology, "journal-runtime-drift"),
                ),
            )
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-runtime-drift").0,
        "prepared"
    );

    let (runtime, id, _root, state, topology, prepared) =
        prepared_install("before-image-drift", "journal-before-image-drift");
    let before = AggregateBefore::new(id, None, None, None).expect("before");
    let mutation = OptiScalerAggregateMutation::Filesystem {
        before_state: Some(&state),
        after_state: Some(&state),
        before_topology: Some(&topology),
        after_topology: Some(&topology),
        peer: OptiScalerPeerMutation::Keep,
        mutation_id: "journal-before-image-drift",
    };
    assert!(
        runtime
            .commit_optiscaler_journal_aggregate(
                prepared,
                OptiScalerJournalAggregateCommit::new(before, mutation),
            )
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-before-image-drift").0,
        "prepared"
    );
}

#[test]
fn aggregate_commit_rolls_back_when_the_prepared_row_drifts() {
    let (runtime, id, _root, state, topology, prepared) =
        prepared_install("row-drift-commit", "journal-row-drift-commit");
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE pending_file_mutations SET manifest_json = ?1 WHERE id = ?2",
                    [
                        &serde_json::json!({"drifted": true}).to_string(),
                        "journal-row-drift-commit",
                    ],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("drift row");
    let before = AggregateBefore::new(id.clone(), None, None, None).expect("before");
    let error = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(
                before,
                OptiScalerAggregateMutation::Filesystem {
                    before_state: None,
                    after_state: Some(&state),
                    before_topology: None,
                    after_topology: Some(&topology),
                    peer: OptiScalerPeerMutation::Keep,
                    mutation_id: "journal-row-drift-commit",
                },
            ),
        )
        .expect_err("drifted row must fail closed");
    assert!(matches!(
        error.kind(),
        renderpilot_application::AppErrorKind::StorageFailed
    ));
    assert_eq!(
        pending_tuple(&runtime, "journal-row-drift-commit").0,
        "prepared"
    );
    assert_eq!(
        reservation(&runtime, &id, "journal-row-drift-commit")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Prepared
    );
    assert!(
        runtime
            .repositories()
            .get_optiscaler_install_state(&id)
            .expect("state")
            .is_none()
    );
}

#[test]
fn aggregate_commit_rejects_reservation_and_generation_drift_atomically() {
    let (runtime, id, _root, state, topology, prepared) = prepared_install(
        "reservation-drift-commit",
        "journal-reservation-drift-commit",
    );
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE peer_aggregate_reservations
                     SET state = 'committed'
                     WHERE game_id = ?1 AND operation_id = ?2",
                    [id.as_str(), "journal-reservation-drift-commit"],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("drift reservation");
    let before = AggregateBefore::new(id, None, None, None).expect("before");
    assert!(
        runtime
            .commit_optiscaler_journal_aggregate(
                prepared,
                OptiScalerJournalAggregateCommit::new(
                    before,
                    install_mutation(&state, &topology, "journal-reservation-drift-commit"),
                ),
            )
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-reservation-drift-commit").0,
        "prepared"
    );

    let (runtime, id, _root, state, topology, prepared) =
        prepared_install("generation-drift-commit", "journal-generation-drift-commit");
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE games SET peer_aggregate_revision = 1 WHERE id = ?1",
                    [id.as_str()],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("drift generation");
    let before = AggregateBefore::new(id, None, None, None).expect("before");
    assert!(
        runtime
            .commit_optiscaler_journal_aggregate(
                prepared,
                OptiScalerJournalAggregateCommit::new(
                    before,
                    install_mutation(&state, &topology, "journal-generation-drift-commit"),
                ),
            )
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-generation-drift-commit").0,
        "prepared"
    );
}

#[test]
fn aggregate_commit_updates_a_filesystem_receipt_with_keep_peer() {
    let (runtime, id, root, state, topology) = exact_adoption_aggregate_fixture("update");
    runtime
        .repositories()
        .commit_game_mutation(crate::repositories::GameMutationCommit {
            game_id: &id,
            component_set: None,
            baseline_mutations: &[],
            addon: crate::repositories::InstalledAddonMutation::OptiScaler(
                OptiScalerAggregateMutation::AdoptExactMetadata {
                    state: &state,
                    topology: &topology,
                },
            ),
            mutation_id: None,
        })
        .expect("seed aggregate");
    let before_state = runtime
        .repositories()
        .get_optiscaler_install_state(&id)
        .expect("state")
        .expect("persisted state");
    let before_topology = runtime
        .repositories()
        .get_proxy_topology(&id)
        .expect("topology")
        .expect("persisted topology");
    let mut after_parts = renderpilot_domain::OptiScalerInstallStateParts::from(&before_state);
    after_parts.release_files[0].installed = FileReceipt::owned(
        before_state.release_files[0].installed.identity(),
        Sha256Hash::new("c".repeat(64)).expect("updated config digest"),
    )
    .expect("updated configuration receipt");
    after_parts.release_files[0].cleanup =
        OptiScalerFileCleanup::PreserveCurrentThenRestoreBaseline;
    after_parts.adoption_state = OptiScalerAdoptionState::Managed;
    let after_state = OptiScalerInstallState::from_existing_with_configuration_acquisition(
        &before_state,
        after_parts,
        before_state.configuration_baseline().clone(),
    )
    .expect("managed configuration acquisition");
    let config = &before_state.release_files[0];
    let after_config = &after_state.release_files[0];
    let entries = [
        (
            &config.path,
            Some(&config.installed),
            Some(&after_config.installed),
            JournalAction::Write,
        ),
        (
            &before_topology.outer.path,
            Some(&before_topology.outer.receipt),
            Some(&before_topology.outer.receipt),
            JournalAction::Verify,
        ),
    ];
    let (initial, final_json) = transition_journals(&root, "journal-update", &entries);
    let preparing = begin_for(
        &runtime,
        &id,
        "journal-update",
        renderpilot_domain::mutation_features::OPTISCALER_UPDATE,
        Some(&before_topology.id),
        &initial,
    );
    let prepared = runtime
        .finish_optiscaler_journal_aggregate(preparing, final_json)
        .expect("finish");
    let before = AggregateBefore::new(
        id.clone(),
        Some(before_state.clone()),
        Some(before_topology.clone()),
        None,
    )
    .expect("before");
    let mutation = OptiScalerAggregateMutation::Filesystem {
        before_state: Some(&before_state),
        after_state: Some(&after_state),
        before_topology: Some(&before_topology),
        after_topology: Some(&before_topology),
        peer: OptiScalerPeerMutation::Keep,
        mutation_id: "journal-update",
    };
    let committed = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(before, mutation),
        )
        .expect("aggregate update commit");
    assert_eq!(committed.aggregate().generation().as_i64(), 1);
    assert_eq!(committed.aggregate().after().state(), Some(&after_state));
    assert_eq!(pending_tuple(&runtime, "journal-update").0, "committed");
    assert_eq!(
        reservation(&runtime, &id, "journal-update")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Committed
    );
}

#[test]
fn aggregate_commit_replaces_a_relocated_peer_with_an_exact_post_receipt() {
    let (runtime, id, root, state, topology) = exact_adoption_aggregate_fixture("peer-replace");
    let source = path(&format!("{}/ReShade-old.dll", root.as_str()));
    let destination = path(&format!("{}/ReShade-new.dll", root.as_str()));
    let addon_file = path(&format!("{}/renodx.addon64", root.as_str()));
    let peer_receipt = receipt("reshade-peer", 'd');
    let peer_before = InstalledAddon::new(id.clone(), AddonKind::RenoDx, addon_file.clone())
        .try_with_managed_files(vec![ManagedAddonFile::owned(
            source.clone(),
            ManagedFileBaseline::Absent,
            peer_receipt.digest().clone(),
        )])
        .expect("peer before");
    let peer_after_template = InstalledAddon::new(id.clone(), AddonKind::RenoDx, addon_file)
        .try_with_managed_files(vec![ManagedAddonFile::owned(
            destination.clone(),
            ManagedFileBaseline::Absent,
            peer_receipt.digest().clone(),
        )])
        .expect("peer after template");

    let mut before_topology = topology;
    before_topology.downstream = Some(ProxyLink {
        implementation: ProxyImplementation::ReShade,
        path: source.clone(),
        receipt: peer_receipt.clone(),
    });
    before_topology.downstream_origin = Some(before_topology.root_slot.clone());
    before_topology.root_prestate = ProxyRootPrestate::Absent;
    let mut after_topology = before_topology.clone();
    after_topology.downstream = Some(ProxyLink {
        implementation: ProxyImplementation::ReShade,
        path: destination.clone(),
        receipt: peer_receipt.clone(),
    });
    after_topology.downstream_origin = Some(after_topology.root_slot.clone());
    before_topology.validate().expect("valid before topology");
    after_topology.validate().expect("valid after topology");

    runtime
        .repositories()
        .commit_game_mutation(crate::repositories::GameMutationCommit {
            game_id: &id,
            component_set: None,
            baseline_mutations: &[],
            addon: crate::repositories::InstalledAddonMutation::Upsert(&peer_before),
            mutation_id: None,
        })
        .expect("seed peer");
    runtime
        .repositories()
        .commit_game_mutation(crate::repositories::GameMutationCommit {
            game_id: &id,
            component_set: None,
            baseline_mutations: &[],
            addon: crate::repositories::InstalledAddonMutation::OptiScaler(
                OptiScalerAggregateMutation::AdoptExactMetadata {
                    state: &state,
                    topology: &before_topology,
                },
            ),
            mutation_id: None,
        })
        .expect("seed OptiScaler aggregate");

    let persisted_state = runtime
        .repositories()
        .get_optiscaler_install_state(&id)
        .expect("state")
        .expect("persisted state");
    let persisted_topology = runtime
        .repositories()
        .get_proxy_topology(&id)
        .expect("topology")
        .expect("persisted topology");
    let persisted_peer = runtime
        .repositories()
        .get_installed_addon(&id)
        .expect("peer")
        .expect("persisted peer");
    let verifies = [
        (
            &persisted_state.release_files[0].path,
            &persisted_state.release_files[0].installed,
        ),
        (
            &persisted_topology.outer.path,
            &persisted_topology.outer.receipt,
        ),
    ];
    let (initial, final_json) = peer_replace_transition_journals(
        &root,
        "journal-peer-replace",
        &source,
        &peer_receipt,
        &destination,
        &peer_receipt,
        &verifies,
    );
    let preparing = begin_for(
        &runtime,
        &id,
        "journal-peer-replace",
        renderpilot_domain::mutation_features::OPTISCALER_UPDATE,
        Some(&persisted_topology.id),
        &initial,
    );
    let prepared = runtime
        .finish_optiscaler_journal_aggregate(preparing, final_json)
        .expect("finish peer replacement");
    let before = AggregateBefore::new(
        id.clone(),
        Some(persisted_state.clone()),
        Some(persisted_topology.clone()),
        Some(persisted_peer.clone()),
    )
    .expect("before aggregate");
    let peer_after = peer_after_template
        .with_timestamps(persisted_peer.installed_at(), persisted_peer.updated_at());
    let mutation = OptiScalerAggregateMutation::Filesystem {
        before_state: Some(&persisted_state),
        after_state: Some(&persisted_state),
        before_topology: Some(&persisted_topology),
        after_topology: Some(&after_topology),
        peer: OptiScalerPeerMutation::Replace {
            before: &persisted_peer,
            after: &peer_after,
        },
        mutation_id: "journal-peer-replace",
    };
    let committed = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(before, mutation),
        )
        .expect("aggregate peer replacement commit");

    assert_eq!(committed.aggregate().generation().as_i64(), 1);
    let persisted_peer_after = runtime
        .repositories()
        .get_installed_addon(&id)
        .expect("post peer")
        .expect("post peer record");
    assert_eq!(
        persisted_peer_after.with_timestamps(None, None),
        peer_after.clone().with_timestamps(None, None),
        "all peer receipt fields except storage-owned timestamps must remain exact"
    );
    assert_eq!(
        runtime
            .repositories()
            .get_proxy_topology(&id)
            .expect("post topology")
            .expect("post topology record"),
        after_topology
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-peer-replace").0,
        "committed"
    );
    assert_eq!(
        reservation(&runtime, &id, "journal-peer-replace")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Committed
    );
}

#[test]
fn aggregate_commit_uninstalls_filesystem_participants_and_uses_auxiliary_route() {
    let (runtime, id, root, state, topology) = exact_adoption_aggregate_fixture("uninstall");
    runtime
        .repositories()
        .commit_game_mutation(crate::repositories::GameMutationCommit {
            game_id: &id,
            component_set: None,
            baseline_mutations: &[],
            addon: crate::repositories::InstalledAddonMutation::OptiScaler(
                OptiScalerAggregateMutation::AdoptExactMetadata {
                    state: &state,
                    topology: &topology,
                },
            ),
            mutation_id: None,
        })
        .expect("seed aggregate");
    let persisted_state = runtime
        .repositories()
        .get_optiscaler_install_state(&id)
        .expect("state")
        .expect("persisted state");
    let persisted_topology = runtime
        .repositories()
        .get_proxy_topology(&id)
        .expect("topology")
        .expect("persisted topology");
    let config = &persisted_state.release_files[0];
    let entries = [
        (
            &config.path,
            Some(&config.installed),
            Some(&config.installed),
            JournalAction::Verify,
        ),
        (
            &persisted_topology.outer.path,
            Some(&persisted_topology.outer.receipt),
            None,
            JournalAction::Delete,
        ),
    ];
    let (initial, final_json) = transition_journals(&root, "journal-uninstall", &entries);
    let preparing = begin_for(
        &runtime,
        &id,
        "journal-uninstall",
        renderpilot_domain::mutation_features::OPTISCALER_UNINSTALL,
        Some(&persisted_topology.id),
        &initial,
    );
    let prepared = runtime
        .finish_optiscaler_journal_aggregate(preparing, final_json)
        .expect("finish");
    let before = AggregateBefore::new(
        id.clone(),
        Some(persisted_state.clone()),
        Some(persisted_topology.clone()),
        None,
    )
    .expect("before");
    let mutation = OptiScalerAggregateMutation::FilesystemWithAuxiliary {
        before_state: Some(&persisted_state),
        after_state: None,
        before_topology: Some(&persisted_topology),
        after_topology: None,
        peer: OptiScalerPeerMutation::Keep,
        auxiliary_preservations: &[],
        retained_claims: &[],
        mutation_id: "journal-uninstall",
    };
    let committed = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(before, mutation),
        )
        .expect("aggregate uninstall commit");

    assert_eq!(committed.aggregate().generation().as_i64(), 1);
    assert!(committed.aggregate().after().state().is_none());
    assert!(committed.aggregate().after().topology().is_none());
    assert!(
        runtime
            .repositories()
            .get_optiscaler_install_state(&id)
            .expect("state")
            .is_none()
    );
    assert!(
        runtime
            .repositories()
            .get_proxy_topology(&id)
            .expect("topology")
            .is_none()
    );
    assert_eq!(pending_tuple(&runtime, "journal-uninstall").0, "committed");
    assert_eq!(
        reservation(&runtime, &id, "journal-uninstall")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Committed
    );
}

#[test]
fn invalid_input_and_duplicate_begin_are_rejected() {
    let (runtime, id, initial) = seed("invalid");
    assert!(
        OptiScalerJournalAggregateBegin::new(
            " ",
            id.clone(),
            "optiscaler_install",
            Some("subject".to_owned()),
            &initial
        )
        .is_err()
    );
    assert!(
        OptiScalerJournalAggregateBegin::new(
            "bad\0id",
            id.clone(),
            "optiscaler_install",
            Some("subject".to_owned()),
            &initial
        )
        .is_err()
    );
    assert!(
        OptiScalerJournalAggregateBegin::new(
            "wrong-feature",
            id.clone(),
            "renodx_install",
            Some("subject".to_owned()),
            &initial,
        )
        .is_err()
    );
    begin(&runtime, &id, "journal-duplicate", &initial);
    let duplicate = OptiScalerJournalAggregateBegin::new(
        "journal-duplicate",
        id,
        "optiscaler_install",
        Some("optiscaler".to_owned()),
        initial,
    )
    .expect("duplicate input");
    assert!(
        runtime
            .begin_optiscaler_journal_aggregate(duplicate)
            .is_err()
    );
}

#[test]
fn finish_rejects_invalid_final_journal_and_preserves_preparing_pair() {
    let (runtime, id, initial) = seed("bad-finish");
    let preparing = begin(&runtime, &id, "journal-bad-finish", &initial);
    assert!(
        runtime
            .finish_optiscaler_journal_aggregate(preparing, r#"{"phase":"bad"}"#)
            .is_err()
    );
    assert_eq!(pending_tuple(&runtime, "journal-bad-finish").0, "preparing");
    assert_eq!(
        reservation(&runtime, &id, "journal-bad-finish")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Preparing
    );
}

#[test]
fn row_and_revision_drift_fail_closed_without_partial_transition() {
    let (runtime, id, initial) = seed("drift");
    let preparing = begin(&runtime, &id, "journal-drift", &initial);
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE pending_file_mutations SET manifest_json = ?1 WHERE id = ?2",
                    [
                        &serde_json::json!({"changed": true}).to_string(),
                        "journal-drift",
                    ],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("tamper row");
    assert!(
        runtime
            .finish_optiscaler_journal_aggregate(preparing, prepared_journal())
            .is_err()
    );
    assert_eq!(pending_tuple(&runtime, "journal-drift").0, "preparing");
    assert_eq!(
        reservation(&runtime, &id, "journal-drift")
            .expect("reservation")
            .state(),
        PeerAggregateReservationState::Preparing
    );

    let (runtime, id, initial) = seed("revision-drift");
    let preparing = begin(&runtime, &id, "journal-revision-drift", &initial);
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE games SET peer_aggregate_revision = 1 WHERE id = ?1",
                    [id.as_str()],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("tamper revision");
    assert!(
        runtime
            .finish_optiscaler_journal_aggregate(preparing, prepared_journal())
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-revision-drift").0,
        "preparing"
    );

    let (runtime, id, initial) = seed("reservation-drift");
    let preparing = begin(&runtime, &id, "journal-reservation-drift", &initial);
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE peer_aggregate_reservations
                     SET state = 'prepared' WHERE game_id = ?1 AND operation_id = ?2",
                    [id.as_str(), "journal-reservation-drift"],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("tamper reservation");
    assert!(
        runtime
            .finish_optiscaler_journal_aggregate(preparing, prepared_journal())
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-reservation-drift").0,
        "preparing"
    );
}

#[test]
fn recovery_acquires_preparing_and_prepared_journals_with_public_identity_only() {
    let (runtime, id, initial) = seed("recovery-preparing");
    let preparing = begin(&runtime, &id, "journal-recovery-preparing", &initial);
    let candidates = runtime
        .recover_pending_file_mutation_candidates_for_game(&id, |_| true)
        .expect("preparing recovery candidates");
    assert_eq!(candidates.len(), 1);
    let PendingFileMutationRecoveryCandidate::OptiScaler(proof) =
        candidates.into_iter().next().expect("candidate")
    else {
        panic!("expected OptiScaler candidate");
    };
    assert_eq!(proof.operation_id(), "journal-recovery-preparing");
    assert_eq!(proof.game_id(), &id);
    assert_eq!(
        proof.feature(),
        renderpilot_domain::mutation_features::OPTISCALER_INSTALL
    );
    assert_eq!(proof.subject_id(), Some("optiscaler"));
    assert_eq!(proof.journal(), &journal());
    drop(preparing);

    let (runtime, id, initial) = seed("recovery-prepared");
    let preparing = begin(&runtime, &id, "journal-recovery-prepared", &initial);
    runtime
        .finish_optiscaler_journal_aggregate(preparing, prepared_journal())
        .expect("prepared journal");
    let candidates = runtime
        .recover_pending_file_mutation_candidates_for_game(&id, |_| true)
        .expect("prepared recovery candidates");
    assert!(matches!(
        candidates.as_slice(),
        [PendingFileMutationRecoveryCandidate::OptiScaler(proof)]
            if proof.operation_id() == "journal-recovery-prepared"
                && proof.journal().materialization() == &MaterializationState::Ready
    ));
}

#[test]
fn recovery_rejects_noncanonical_control_path_before_cas() {
    assert_recovery_rejects_noncanonical_binding_path(
        "recovery-noncanonical-control",
        "journal-recovery-noncanonical-control",
        PersistedBindingPath::Control,
    );
}

#[test]
fn recovery_rejects_noncanonical_private_path_before_cas() {
    assert_recovery_rejects_noncanonical_binding_path(
        "recovery-noncanonical-private",
        "journal-recovery-noncanonical-private",
        PersistedBindingPath::Private,
    );
}

#[test]
fn recovery_selection_is_pure_and_deterministic_for_unbound_rows() {
    let storage = SqliteStorage::in_memory().expect("storage");
    let id = game_id("recovery-ordinary");
    storage
        .upsert_game(&GameInstallation::new(
            GameIdentity::new(id.clone(), "Recovery ordinary", Launcher::Steam).expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            path("C:/game"),
        ))
        .expect("game");
    for operation_id in ["ordinary-b", "ordinary-a"] {
        storage
            .begin_file_mutation_preparation(
                &pending_file_mutations::BeginFileMutationPreparation {
                    id: operation_id.to_owned(),
                    game_id: id.clone(),
                    feature: "ordinary_recovery".to_owned(),
                    subject_id: None,
                    initial_manifest_json: "{}".to_owned(),
                },
            )
            .expect("ordinary pending row");
    }
    storage
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE pending_file_mutations
                     SET created_at = CASE id
                         WHEN 'ordinary-b' THEN 10
                         WHEN 'ordinary-a' THEN 20
                     END,
                         updated_at = CASE id
                             WHEN 'ordinary-b' THEN 10
                             WHEN 'ordinary-a' THEN 20
                         END
                     WHERE id IN ('ordinary-b', 'ordinary-a')",
                    [],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("deterministic ordinary row timestamps");
    let runtime = PeerStorageRuntime::new(storage);
    let candidates = runtime
        .recover_pending_file_mutation_candidates_for_game(&id, |row| row.id != "ordinary-b")
        .expect("selected ordinary row");
    assert!(matches!(
        candidates.as_slice(),
        [PendingFileMutationRecoveryCandidate::NotAggregate(row)]
            if row.id == "ordinary-a"
    ));
    let candidates = runtime
        .recover_pending_file_mutation_candidates_for_game(&id, |_| true)
        .expect("all ordinary rows");
    assert!(matches!(
        candidates.as_slice(),
        [
            PendingFileMutationRecoveryCandidate::NotAggregate(first),
            PendingFileMutationRecoveryCandidate::NotAggregate(second),
        ] if first.id == "ordinary-b" && second.id == "ordinary-a"
    ));
}

#[test]
fn recovery_never_routes_an_optiscaler_feature_without_its_marker() {
    let (runtime, id, initial) = seed("recovery-missing-marker");
    runtime
        .repositories()
        .begin_file_mutation_preparation(&pending_file_mutations::BeginFileMutationPreparation {
            id: "journal-recovery-missing-marker".to_owned(),
            game_id: id.clone(),
            feature: renderpilot_domain::mutation_features::OPTISCALER_INSTALL.to_owned(),
            subject_id: Some("optiscaler".to_owned()),
            initial_manifest_json: initial,
        })
        .expect("ordinary row with OptiScaler feature");
    assert!(
        runtime
            .recover_pending_file_mutation_candidates_for_game(&id, |_| true)
            .is_err()
    );
    assert!(pending_exists(&runtime, "journal-recovery-missing-marker"));
}

#[test]
fn recovery_accepts_a_committed_nonterminal_journal_without_cleanup_authority() {
    let (runtime, id, _root, state, topology, prepared) =
        prepared_install("recovery-committed", "journal-recovery-committed");
    let before = AggregateBefore::new(id.clone(), None, None, None).expect("before");
    runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(
                before,
                install_mutation(&state, &topology, "journal-recovery-committed"),
            ),
        )
        .expect("aggregate commit");
    let candidates = runtime
        .recover_pending_file_mutation_candidates_for_game(&id, |_| true)
        .expect("committed recovery candidate");
    assert!(matches!(
        candidates.as_slice(),
        [PendingFileMutationRecoveryCandidate::OptiScaler(proof)]
            if proof.operation_id() == "journal-recovery-committed"
                && proof.journal().cleanup() == &CleanupState::Inactive
    ));
}

#[test]
fn recovery_rejects_prepared_catalog_binding_drift_without_changing_evidence() {
    let (runtime, id, _root, _state, _topology, prepared) =
        prepared_install("recovery-catalog-drift", "journal-recovery-catalog-drift");
    let before_row = pending_tuple(&runtime, "journal-recovery-catalog-drift");
    let before_reservation = reservation(&runtime, &id, "journal-recovery-catalog-drift")
        .expect("reservation before recovery");
    set_catalog_binding_fields(&runtime, &id, 99, Some("another-operation"));
    drop(prepared);
    assert!(
        runtime
            .recover_pending_file_mutation_candidates_for_game(&id, |_| true)
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-recovery-catalog-drift"),
        before_row
    );
    assert_eq!(
        reservation(&runtime, &id, "journal-recovery-catalog-drift"),
        Some(before_reservation)
    );
}

#[test]
fn recovery_rejects_generation_drift_without_changing_rows() {
    let (runtime, id, initial) = seed("recovery-generation-drift");
    begin(&runtime, &id, "journal-recovery-generation-drift", &initial);
    let before_row = pending_tuple(&runtime, "journal-recovery-generation-drift");
    let before_reservation = reservation(&runtime, &id, "journal-recovery-generation-drift")
        .expect("reservation before recovery");
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE games SET peer_aggregate_revision = 1 WHERE id = ?1",
                    [id.as_str()],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("generation drift fixture");
    assert!(
        runtime
            .recover_pending_file_mutation_candidates_for_game(&id, |_| true)
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-recovery-generation-drift"),
        before_row
    );
    assert_eq!(
        reservation(&runtime, &id, "journal-recovery-generation-drift"),
        Some(before_reservation)
    );
}

#[test]
fn recovery_cas_preserves_each_pending_state_and_returns_exact_json() {
    let (runtime, id, initial) = seed("recovery-cas-preparing");
    begin(&runtime, &id, "journal-recovery-cas-preparing", &initial);
    let proof = recover_optiscaler(&runtime, &id);
    let next = journal_with_materialization(
        proof.current_journal_json(),
        MaterializationState::ControlCreateIntent,
        None,
    );
    let proof = runtime
        .cas_recovering_optiscaler_journal_aggregate(proof, next.clone())
        .expect("Preparing recovery CAS");
    assert_eq!(
        proof.state(),
        crate::repositories::PendingFileMutationState::Preparing
    );
    assert_eq!(proof.current_journal_json(), next);
    assert_eq!(
        pending_identity(&runtime, "journal-recovery-cas-preparing").3,
        next
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-recovery-cas-preparing").0,
        "preparing"
    );

    let (runtime, id, _root, _state, _topology, prepared) =
        prepared_install("recovery-cas-prepared", "journal-recovery-cas-prepared");
    let current = prepared.final_journal_json().to_owned();
    drop(prepared);
    let proof = recover_optiscaler(&runtime, &id);
    let next = prepared_reverse_json(&current);
    let proof = runtime
        .cas_recovering_optiscaler_journal_aggregate(proof, next.clone())
        .expect("Prepared recovery CAS");
    assert_eq!(
        proof.state(),
        crate::repositories::PendingFileMutationState::Prepared
    );
    assert_eq!(proof.current_journal_json(), next);
    assert_eq!(
        pending_tuple(&runtime, "journal-recovery-cas-prepared").0,
        "prepared"
    );

    let (runtime, id, _root, state, topology, prepared) =
        prepared_install("recovery-cas-committed", "journal-recovery-cas-committed");
    let before = AggregateBefore::new(id.clone(), None, None, None).expect("before");
    let committed = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(
                before,
                install_mutation(&state, &topology, "journal-recovery-cas-committed"),
            ),
        )
        .expect("aggregate commit");
    let current = committed.current_journal_json.clone();
    drop(committed);
    let proof = recover_optiscaler(&runtime, &id);
    let next = committed_cleanup_cursor_json(&current);
    let proof = runtime
        .cas_recovering_optiscaler_journal_aggregate(proof, next.clone())
        .expect("Committed recovery CAS");
    assert_eq!(
        proof.state(),
        crate::repositories::PendingFileMutationState::Committed
    );
    assert_eq!(proof.current_journal_json(), next);
    assert_eq!(
        pending_tuple(&runtime, "journal-recovery-cas-committed").0,
        "committed"
    );
    assert_eq!(game_revision(&runtime, &id), 1);
}

#[test]
fn recovery_cas_rejects_cross_state_journal_progress() {
    let (runtime, id, initial) = seed("recovery-cross-preparing");
    begin(&runtime, &id, "journal-recovery-cross-preparing", &initial);
    let proof = recover_optiscaler(&runtime, &id);
    assert!(
        runtime
            .cas_recovering_optiscaler_journal_aggregate(proof, prepared_journal())
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-recovery-cross-preparing").0,
        "preparing"
    );

    let (runtime, id, _root, _state, _topology, prepared) =
        prepared_install("recovery-cross-prepared", "journal-recovery-cross-prepared");
    drop(prepared);
    let proof = recover_optiscaler(&runtime, &id);
    assert!(
        runtime
            .cas_recovering_optiscaler_journal_aggregate(
                proof,
                serde_json::to_string(&journal()).unwrap()
            )
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-recovery-cross-prepared").0,
        "prepared"
    );

    let (runtime, id, _root, state, topology, prepared) = prepared_install(
        "recovery-cross-committed",
        "journal-recovery-cross-committed",
    );
    let before = AggregateBefore::new(id.clone(), None, None, None).expect("before");
    let committed = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(
                before,
                install_mutation(&state, &topology, "journal-recovery-cross-committed"),
            ),
        )
        .expect("aggregate commit");
    drop(committed);
    let proof = recover_optiscaler(&runtime, &id);
    assert!(
        runtime
            .cas_recovering_optiscaler_journal_aggregate(
                proof,
                serde_json::to_string(&journal()).unwrap()
            )
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-recovery-cross-committed").0,
        "committed"
    );
}

#[test]
fn recovery_cas_rejects_stale_row_reservation_generation_and_catalog_fences() {
    let (runtime, id, initial) = seed("recovery-stale-row");
    begin(&runtime, &id, "journal-recovery-stale-row", &initial);
    let proof = recover_optiscaler(&runtime, &id);
    replace_pending_json(&runtime, "journal-recovery-stale-row", &prepared_journal());
    assert!(
        runtime
            .cas_recovering_optiscaler_journal_aggregate(proof, initial)
            .is_err()
    );
    assert_eq!(
        pending_tuple(&runtime, "journal-recovery-stale-row").0,
        "preparing"
    );

    let (runtime, id, initial) = seed("recovery-stale-reservation");
    begin(
        &runtime,
        &id,
        "journal-recovery-stale-reservation",
        &initial,
    );
    let proof = recover_optiscaler(&runtime, &id);
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE peer_aggregate_reservations SET state = 'prepared'
                     WHERE operation_id = ?1",
                    ["journal-recovery-stale-reservation"],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("reservation drift");
    assert!(
        runtime
            .cas_recovering_optiscaler_journal_aggregate(proof, initial)
            .is_err()
    );

    let (runtime, id, initial) = seed("recovery-stale-generation");
    begin(&runtime, &id, "journal-recovery-stale-generation", &initial);
    let proof = recover_optiscaler(&runtime, &id);
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE games SET peer_aggregate_revision = 1 WHERE id = ?1",
                    [id.as_str()],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("generation drift");
    assert!(
        runtime
            .cas_recovering_optiscaler_journal_aggregate(proof, initial)
            .is_err()
    );

    let (runtime, id, _root, _state, _topology, prepared) =
        prepared_install("recovery-stale-catalog", "journal-recovery-stale-catalog");
    drop(prepared);
    let proof = recover_optiscaler(&runtime, &id);
    set_catalog_binding_fields(&runtime, &id, 1, Some("another-operation"));
    let current = proof.current_journal_json().to_owned();
    assert!(
        runtime
            .cas_recovering_optiscaler_journal_aggregate(proof, current)
            .is_err()
    );
}

#[test]
fn recovery_terminal_cleanup_deletes_pairs_atomically_and_never_restores_committed_state() {
    let (runtime, id, initial) = seed("recovery-terminal-preparing");
    begin(
        &runtime,
        &id,
        "journal-recovery-terminal-preparing",
        &initial,
    );
    replace_pending_json(
        &runtime,
        "journal-recovery-terminal-preparing",
        &rollback_terminal_json(&prepared_journal()),
    );
    let proof = recover_optiscaler(&runtime, &id);
    runtime
        .delete_preparing_recovering_optiscaler_journal_aggregate_after_rollback(proof)
        .expect("Preparing terminal cleanup");
    assert!(!pending_exists(
        &runtime,
        "journal-recovery-terminal-preparing"
    ));
    assert!(reservation(&runtime, &id, "journal-recovery-terminal-preparing").is_none());

    let (runtime, id, _root, _state, _topology, prepared) = prepared_install(
        "recovery-terminal-prepared",
        "journal-recovery-terminal-prepared",
    );
    let terminal = rollback_terminal_json(prepared.final_journal_json());
    replace_pending_json(&runtime, "journal-recovery-terminal-prepared", &terminal);
    drop(prepared);
    let proof = recover_optiscaler(&runtime, &id);
    runtime
        .delete_prepared_recovering_optiscaler_journal_aggregate_after_rollback(proof)
        .expect("Prepared terminal cleanup");
    assert!(!pending_exists(
        &runtime,
        "journal-recovery-terminal-prepared"
    ));
    assert!(reservation(&runtime, &id, "journal-recovery-terminal-prepared").is_none());

    let (runtime, id, _root, state, topology, prepared) = prepared_install(
        "recovery-terminal-committed",
        "journal-recovery-terminal-committed",
    );
    let before = AggregateBefore::new(id.clone(), None, None, None).expect("before");
    let committed = runtime
        .commit_optiscaler_journal_aggregate(
            prepared,
            OptiScalerJournalAggregateCommit::new(
                before,
                install_mutation(&state, &topology, "journal-recovery-terminal-committed"),
            ),
        )
        .expect("aggregate commit");
    let terminal = committed_terminal_json(&committed.current_journal_json);
    replace_pending_json(&runtime, "journal-recovery-terminal-committed", &terminal);
    let proof = {
        drop(committed);
        recover_optiscaler(&runtime, &id)
    };
    runtime
        .delete_committed_recovering_optiscaler_journal_aggregate(proof)
        .expect("Committed terminal cleanup");
    assert!(!pending_exists(
        &runtime,
        "journal-recovery-terminal-committed"
    ));
    assert!(reservation(&runtime, &id, "journal-recovery-terminal-committed").is_none());
    let persisted_state = runtime
        .repositories()
        .get_optiscaler_install_state(&id)
        .unwrap()
        .expect("persisted state");
    let mut expected_state = state;
    expected_state.created_at = persisted_state.created_at;
    expected_state.updated_at = persisted_state.updated_at;
    assert_eq!(persisted_state, expected_state);
    assert_eq!(
        runtime.repositories().get_proxy_topology(&id).unwrap(),
        Some(topology)
    );
}

#[test]
fn recovery_terminal_cleanup_rejects_stale_reservation_without_partial_delete() {
    let (runtime, id, initial) = seed("recovery-terminal-stale-reservation");
    begin(
        &runtime,
        &id,
        "journal-recovery-terminal-stale-reservation",
        &initial,
    );
    replace_pending_json(
        &runtime,
        "journal-recovery-terminal-stale-reservation",
        &rollback_terminal_json(&prepared_journal()),
    );
    let proof = recover_optiscaler(&runtime, &id);
    runtime
        .repositories()
        .with_connection_mut(|connection| {
            connection
                .execute(
                    "UPDATE peer_aggregate_reservations SET state = 'prepared'
                     WHERE operation_id = ?1",
                    ["journal-recovery-terminal-stale-reservation"],
                )
                .map(|_| ())
                .map_err(crate::error::storage_error)
        })
        .expect("reservation drift");
    assert!(
        runtime
            .delete_preparing_recovering_optiscaler_journal_aggregate_after_rollback(proof)
            .is_err()
    );
    assert!(pending_exists(
        &runtime,
        "journal-recovery-terminal-stale-reservation"
    ));
    assert!(reservation(&runtime, &id, "journal-recovery-terminal-stale-reservation").is_some());
}
