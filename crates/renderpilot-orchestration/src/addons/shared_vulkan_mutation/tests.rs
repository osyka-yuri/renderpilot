use std::cell::RefCell;
use std::path::PathBuf;

use renderpilot_platform_windows::vulkan_layer::{
    LayerRegistry, LayerRegistryEntry, RegistryValueState,
};
use renderpilot_storage_sqlite::{
    BeginSharedVulkanMutation, SharedVulkanMutationReservation, SharedVulkanMutationScope,
};

use super::io::{self, ParticipantState};
use super::manifest::{MANIFEST_VERSION, Manifest, RegistryParticipant, RegistryValue, Scope};
use super::plan::{FileIntent, MutationPlan, Request};

struct FakeRegistry {
    value: RefCell<RegistryValueState>,
}

impl LayerRegistry for FakeRegistry {
    fn registered_layers(&self) -> Vec<LayerRegistryEntry> {
        Vec::new()
    }

    fn observe_canonical_registration(
        &self,
        _manifest_path: &std::path::Path,
    ) -> std::io::Result<RegistryValueState> {
        Ok(self.value.borrow().clone())
    }

    fn restore_canonical_registration(
        &self,
        _manifest_path: &std::path::Path,
        state: &RegistryValueState,
    ) -> std::io::Result<()> {
        *self.value.borrow_mut() = state.clone();
        Ok(())
    }
}

#[test]
fn plan_stages_applies_verifies_and_restores_exact_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("mutation");
    std::fs::create_dir_all(&root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    let plan = MutationPlan::build(Request {
        transaction_root: root.clone(),
        mutation_id: "test-plan".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live.clone(),
            before: None,
            after: Some(b"new layer".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");

    io::verify_all_before(&root, &plan.manifest, &plan.roots, None).expect("before fence");
    io::materialize_stages(&plan).expect("stage");
    io::apply_files(&root, &plan.manifest, &plan.payloads, &plan.roots).expect("apply");
    io::verify_all_after(&root, &plan.manifest, &plan.roots, None).expect("after fence");
    assert_eq!(std::fs::read(&live).expect("live bytes"), b"new layer");

    io::restore_files(&root, &plan.manifest, &plan.payloads, &plan.roots).expect("restore");
    io::verify_all_before(&root, &plan.manifest, &plan.roots, None).expect("restored before fence");
    io::cleanup_artifacts(&root, &plan.manifest, &plan.roots, ParticipantState::Before)
        .expect("cleanup");
    assert!(!root.exists());
}

#[test]
fn deletion_moves_to_an_owned_tomb_and_restores_without_replacement() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("mutation");
    std::fs::create_dir_all(&root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    std::fs::write(&live, b"before").expect("seed");
    let plan = MutationPlan::build(Request {
        transaction_root: root.clone(),
        mutation_id: "test-delete".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live.clone(),
            before: Some(b"before".to_vec()),
            after: None,
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");

    io::materialize_stages(&plan).expect("stage");
    io::apply_files(&root, &plan.manifest, &plan.payloads, &plan.roots).expect("delete");
    assert!(!live.exists());
    assert!(plan.payloads[0].tomb_path.as_ref().expect("tomb").exists());

    io::restore_files(&root, &plan.manifest, &plan.payloads, &plan.roots).expect("restore");
    assert_eq!(std::fs::read(&live).expect("restored"), b"before");
    assert!(!plan.payloads[0].tomb_path.as_ref().expect("tomb").exists());
}

#[test]
fn recovery_classifies_before_after_mix_without_calling_it_third_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("mutation");
    std::fs::create_dir_all(&root).expect("mutation root");
    let before = temp.path().join("before.dll");
    let after = temp.path().join("after.dll");
    std::fs::write(&after, b"before").expect("seed after");
    let plan = MutationPlan::build(Request {
        transaction_root: root.clone(),
        mutation_id: "test-mix".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![
            FileIntent {
                live_path: before,
                before: None,
                after: Some(b"new-before".to_vec()),
            },
            FileIntent {
                live_path: after,
                before: Some(b"before".to_vec()),
                after: Some(b"new-after".to_vec()),
            },
        ],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");
    io::materialize_stages(&plan).expect("stage");
    io::apply_files(&root, &plan.manifest, &plan.payloads, &plan.roots).expect("apply");
    // Restore one participant to its before state and leave the second after.
    let first = &plan.manifest.files[0];
    let first_live = plan.roots.resolve(&first.live_path).expect("live path");
    std::fs::remove_file(&first_live).expect("restore first before state");
    let states = io::classify_all(&root, &plan.manifest, &plan.roots, None).expect("classify");
    assert_eq!(
        states,
        vec![ParticipantState::Before, ParticipantState::After]
    );
}

#[test]
fn pending_file_only_recovery_without_native_authority_is_observational() {
    let temp = tempfile::tempdir().expect("tempdir");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let mutation_id = "file-only-no-native-authority";
    let live = temp.path().join("ReShadeApps.ini");
    std::fs::write(&live, b"before").expect("seed live file");
    let roots = super::TrustedRoots::shared_only(temp.path()).expect("roots");
    let initial_manifest = Manifest::empty(Scope::SharedOnly, None, "test")
        .to_json()
        .expect("initial manifest");
    let reservation = context
        .storage()
        .try_begin_shared_vulkan_mutation(&BeginSharedVulkanMutation {
            id: mutation_id.to_owned(),
            scope: SharedVulkanMutationScope::SharedOnly,
            game_id: None,
            feature: "test".to_owned(),
            initial_manifest_json: initial_manifest,
            root_capabilities_json: roots.to_json().expect("root capabilities"),
        })
        .expect("reserve");
    assert!(matches!(
        reservation,
        SharedVulkanMutationReservation::Reserved(_)
    ));

    super::ensure_transaction_namespace(context.file_mutation_root())
        .expect("transaction namespace");
    let transaction_root = super::transaction_root(context.file_mutation_root(), mutation_id)
        .expect("transaction root");
    std::fs::create_dir(&transaction_root).expect("transaction root directory");
    let plan = MutationPlan::build(Request {
        transaction_root: transaction_root.clone(),
        mutation_id: mutation_id.to_owned(),
        roots,
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live.clone(),
            before: Some(b"before".to_vec()),
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");
    io::materialize_stages(&plan).expect("materialize stage");
    io::sync_prepared_artifacts(&transaction_root);
    context
        .storage()
        .finish_preparing_shared_vulkan_mutation(
            mutation_id,
            SharedVulkanMutationScope::SharedOnly,
            None,
            &plan.manifest.to_json().expect("prepared manifest"),
        )
        .expect("finish preparation");
    io::apply_files(
        &transaction_root,
        &plan.manifest,
        &plan.payloads,
        &plan.roots,
    )
    .expect("publish file postimage");
    let row_before = context
        .storage()
        .pending_shared_vulkan_mutation()
        .expect("read row")
        .expect("pending row");

    let error = super::recovery::recover_pending(&context, None)
        .expect_err("recovery without native authority must fail closed");

    assert!(error.to_string().contains("native registry authority"));
    assert_eq!(std::fs::read(&live).expect("live postimage"), b"after");
    assert!(transaction_root.exists());
    assert_eq!(
        context
            .storage()
            .pending_shared_vulkan_mutation()
            .expect("read unchanged row")
            .expect("pending row retained"),
        row_before
    );
}

#[test]
fn registry_forward_and_restore_use_the_requested_fence() {
    let temp = tempfile::tempdir().expect("tempdir");
    let registry = FakeRegistry {
        value: RefCell::new(RegistryValueState::Absent),
    };
    let manifest_path = temp.path().join("ReShade64.json");
    let roots = super::TrustedRoots::shared_only(temp.path()).expect("roots");
    let after = RegistryValue::Present {
        value_type: 4,
        raw_bytes: vec![0; 4],
    };
    let manifest = Manifest {
        version: MANIFEST_VERSION,
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        files: Vec::new(),
        registry: vec![RegistryParticipant {
            manifest_path: roots.authorize(&manifest_path).expect("manifest path"),
            before: RegistryValue::Absent,
            after,
        }],
        directories: Vec::new(),
    };

    io::restore_registry(&registry, &manifest, &roots, false).expect("apply registry after");
    assert_eq!(
        *registry.value.borrow(),
        RegistryValueState::Present {
            value_type: 4,
            raw_bytes: vec![0; 4],
        }
    );
    io::restore_registry(&registry, &manifest, &roots, true).expect("restore registry before");
    assert_eq!(*registry.value.borrow(), RegistryValueState::Absent);
}

#[test]
fn registry_publication_order_is_derived_from_the_durable_transition() {
    let temp = tempfile::tempdir().expect("tempdir");
    let roots = super::TrustedRoots::shared_only(temp.path()).expect("roots");
    let participant = |before, after| RegistryParticipant {
        manifest_path: roots
            .authorize(&temp.path().join("ReShade64.json"))
            .expect("manifest path"),
        before,
        after,
    };
    let manifest = |registry| Manifest {
        version: MANIFEST_VERSION,
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        files: Vec::new(),
        registry: vec![registry],
        directories: Vec::new(),
    };
    let active = RegistryValue::Present {
        value_type: 4,
        raw_bytes: vec![0; 4],
    };

    assert!(!io::deactivates_registry(&manifest(participant(
        RegistryValue::Absent,
        active.clone(),
    ))));
    assert!(io::deactivates_registry(&manifest(participant(
        active,
        RegistryValue::Absent,
    ))));
}

#[test]
fn registry_restore_refuses_to_overwrite_a_third_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let external = RegistryValueState::Present {
        value_type: 1,
        raw_bytes: b"external".to_vec(),
    };
    let registry = FakeRegistry {
        value: RefCell::new(external.clone()),
    };
    let roots = super::TrustedRoots::shared_only(temp.path()).expect("roots");
    let manifest = Manifest {
        version: MANIFEST_VERSION,
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        files: Vec::new(),
        registry: vec![RegistryParticipant {
            manifest_path: roots
                .authorize(&temp.path().join("ReShade64.json"))
                .expect("manifest path"),
            before: RegistryValue::Absent,
            after: RegistryValue::Present {
                value_type: 4,
                raw_bytes: vec![0; 4],
            },
        }],
        directories: Vec::new(),
    };

    assert!(io::restore_registry(&registry, &manifest, &roots, false).is_err());
    assert_eq!(*registry.value.borrow(), external);
}

#[test]
fn apply_rechecks_each_file_immediately_before_publication() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("mutation");
    std::fs::create_dir(&root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    std::fs::write(&live, b"before").expect("seed");
    let plan = MutationPlan::build(Request {
        transaction_root: root.clone(),
        mutation_id: "prepublish-drift".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live.clone(),
            before: Some(b"before".to_vec()),
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");
    io::materialize_stages(&plan).expect("stage");
    std::fs::write(&live, b"external").expect("drift");

    assert!(io::apply_files(&root, &plan.manifest, &plan.payloads, &plan.roots).is_err());
    assert_eq!(std::fs::read(live).expect("preserved"), b"external");
}

#[test]
fn forward_publication_requires_its_durable_before_snapshot() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("mutation");
    std::fs::create_dir(&root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    std::fs::write(&live, b"before").expect("seed");
    let plan = MutationPlan::build(Request {
        transaction_root: root.clone(),
        mutation_id: "missing-preimage".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live.clone(),
            before: Some(b"before".to_vec()),
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");
    io::materialize_stages(&plan).expect("stage");
    let super::manifest::FileBefore::Snapshot { snapshot_path, .. } =
        &plan.manifest.files[0].before
    else {
        panic!("snapshot")
    };
    std::fs::remove_file(root.join(snapshot_path)).expect("remove snapshot");

    assert!(io::apply_files(&root, &plan.manifest, &plan.payloads, &plan.roots).is_err());
    assert_eq!(std::fs::read(live).expect("preserved"), b"before");
}

#[test]
fn transaction_root_rejects_path_traversal_ids() {
    let root = PathBuf::from("mutation-root");
    assert!(super::transaction_root(&root, "../outside").is_err());
    let nested = PathBuf::from("nested").join("child");
    assert!(super::transaction_root(&root, &nested.to_string_lossy()).is_err());
    assert!(super::transaction_root(&root, "owner:stream").is_err());
    assert_eq!(
        super::transaction_root(&root, "01JVALID").expect("valid id"),
        PathBuf::from("mutation-root.shared-vulkan-v1").join("01JVALID")
    );
}

#[test]
fn transaction_namespace_is_outside_the_legacy_orphan_sweep_root() {
    let legacy = PathBuf::from("file-transactions").join("catalog.sqlite");
    let transaction = super::transaction_root(&legacy, "01JVALID").expect("transaction root");

    assert_eq!(
        transaction,
        PathBuf::from("file-transactions")
            .join("catalog.sqlite.shared-vulkan-v1")
            .join("01JVALID")
    );
    assert!(!transaction.starts_with(&legacy));
}

#[test]
fn preparing_cleanup_preserves_unknown_entries() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("mutation");
    let snapshots = root.join("snapshots");
    std::fs::create_dir_all(&snapshots).expect("snapshots");
    let owned = snapshots.join("file-0.bin");
    let foreign = snapshots.join("notes.txt");
    std::fs::write(&owned, b"snapshot").expect("owned snapshot");
    std::fs::write(&foreign, b"foreign").expect("foreign file");

    assert!(super::recovery::cleanup_preparing_artifacts(&root).is_err());
    assert_eq!(std::fs::read(owned).expect("owned retained"), b"snapshot");
    assert_eq!(
        std::fs::read(foreign).expect("foreign retained"),
        b"foreign"
    );
}

#[test]
fn declared_created_directory_is_created_and_removed_exactly() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("mutation");
    std::fs::create_dir_all(&root).expect("mutation root");
    let created_dir = temp.path().join("new-layer");
    let live = created_dir.join("ReShade64.dll");
    let plan = MutationPlan::build(Request {
        transaction_root: root.clone(),
        mutation_id: "test-dir".to_owned(),
        roots: super::TrustedRoots::shared_only(&created_dir).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live,
            before: None,
            after: Some(b"new layer".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: vec![created_dir.clone()],
    })
    .expect("plan");

    io::materialize_stages(&plan).expect("stage");
    io::apply_files(&root, &plan.manifest, &plan.payloads, &plan.roots).expect("apply");
    io::restore_files(&root, &plan.manifest, &plan.payloads, &plan.roots).expect("restore");
    io::cleanup_artifacts(&root, &plan.manifest, &plan.roots, ParticipantState::Before)
        .expect("cleanup");
    assert!(!created_dir.exists());
}

#[test]
fn recovery_classifies_external_drift_as_third_without_writing() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path().join("mutation");
    std::fs::create_dir_all(&root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    std::fs::write(&live, b"before").expect("seed");
    let plan = MutationPlan::build(Request {
        transaction_root: root.clone(),
        mutation_id: "test-drift".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live.clone(),
            before: Some(b"before".to_vec()),
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");
    std::fs::write(&live, b"external").expect("drift");

    assert_eq!(
        io::classify_all(&root, &plan.manifest, &plan.roots, None).expect("classify"),
        vec![ParticipantState::Third]
    );
    assert_eq!(std::fs::read(&live).expect("read"), b"external");
}

#[test]
fn plan_omits_exact_file_no_ops_without_creating_snapshots() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transaction_root = temp.path().join("mutation");
    std::fs::create_dir(&transaction_root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    std::fs::write(&live, b"same").expect("seed");

    let plan = MutationPlan::build(Request {
        transaction_root: transaction_root.clone(),
        mutation_id: "test-no-op".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live,
            before: Some(b"same".to_vec()),
            after: Some(b"same".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");

    assert!(plan.manifest.files.is_empty());
    assert!(plan.payloads.is_empty());
    assert!(!transaction_root.join("snapshots").exists());
}

#[test]
fn manifest_rejects_auxiliary_not_derived_from_transaction_id() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transaction_root = temp.path().join("mutation");
    std::fs::create_dir(&transaction_root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    let mut plan = MutationPlan::build(Request {
        transaction_root,
        mutation_id: "owned".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live,
            before: None,
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");
    let stage = plan.manifest.files[0].stage_path.as_ref().expect("stage");
    plan.manifest.files[0].stage_path = Some(
        super::CapabilityPath::from_parts(stage.root_id(), "foreign.stage").expect("capability"),
    );

    assert!(plan.manifest.validate_for_transaction("owned").is_err());
}

#[test]
fn occupied_stage_is_never_adopted_even_when_its_bytes_match() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transaction_root = temp.path().join("mutation");
    std::fs::create_dir(&transaction_root).expect("mutation root");
    let plan = MutationPlan::build(Request {
        transaction_root,
        mutation_id: "occupied".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: temp.path().join("ReShade64.dll"),
            before: None,
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");
    let stage = plan.payloads[0].stage_path.as_ref().expect("stage");
    std::fs::write(stage, b"after").expect("foreign stage");

    assert!(io::materialize_stages(&plan).is_err());
    assert_eq!(std::fs::read(stage).expect("preserved"), b"after");
}

#[test]
fn foreign_created_directory_child_is_third_state_and_is_preserved() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transaction_root = temp.path().join("mutation");
    std::fs::create_dir(&transaction_root).expect("mutation root");
    let created_dir = temp.path().join("layer");
    let plan = MutationPlan::build(Request {
        transaction_root: transaction_root.clone(),
        mutation_id: "foreign-child".to_owned(),
        roots: super::TrustedRoots::shared_only(&created_dir).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: created_dir.join("ReShade64.dll"),
            before: None,
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: vec![created_dir.clone()],
    })
    .expect("plan");
    io::materialize_stages(&plan).expect("stage");
    io::apply_files(
        &transaction_root,
        &plan.manifest,
        &plan.payloads,
        &plan.roots,
    )
    .expect("apply");
    let foreign = created_dir.join("foreign.txt");
    std::fs::write(&foreign, b"foreign").expect("foreign child");

    assert!(
        io::classify_all(&transaction_root, &plan.manifest, &plan.roots, None)
            .expect("classify")
            .contains(&ParticipantState::Third)
    );
    assert!(
        io::cleanup_artifacts(
            &transaction_root,
            &plan.manifest,
            &plan.roots,
            ParticipantState::Before,
        )
        .is_err()
    );
    assert_eq!(std::fs::read(foreign).expect("preserved"), b"foreign");
}

#[test]
fn cleanup_refuses_to_delete_a_drifted_snapshot() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transaction_root = temp.path().join("mutation");
    std::fs::create_dir(&transaction_root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    std::fs::write(&live, b"before").expect("seed");
    let plan = MutationPlan::build(Request {
        transaction_root: transaction_root.clone(),
        mutation_id: "snapshot-drift".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live,
            before: Some(b"before".to_vec()),
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");
    io::materialize_stages(&plan).expect("stage");
    io::apply_files(
        &transaction_root,
        &plan.manifest,
        &plan.payloads,
        &plan.roots,
    )
    .expect("apply");
    let super::manifest::FileBefore::Snapshot { snapshot_path, .. } =
        &plan.manifest.files[0].before
    else {
        panic!("snapshot");
    };
    let snapshot = transaction_root.join(snapshot_path);
    std::fs::write(&snapshot, b"foreign").expect("drift snapshot");

    assert!(
        io::cleanup_artifacts(
            &transaction_root,
            &plan.manifest,
            &plan.roots,
            ParticipantState::After,
        )
        .is_err()
    );
    assert_eq!(std::fs::read(snapshot).expect("preserved"), b"foreign");
}

#[test]
fn restored_before_state_can_finish_cleanup_after_snapshot_was_already_removed() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transaction_root = temp.path().join("mutation");
    std::fs::create_dir(&transaction_root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    std::fs::write(&live, b"before").expect("seed");
    let plan = MutationPlan::build(Request {
        transaction_root: transaction_root.clone(),
        mutation_id: "cleanup-retry".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live,
            before: Some(b"before".to_vec()),
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    })
    .expect("plan");
    let super::manifest::FileBefore::Snapshot { snapshot_path, .. } =
        &plan.manifest.files[0].before
    else {
        panic!("snapshot")
    };
    std::fs::remove_file(transaction_root.join(snapshot_path)).expect("partial cleanup");

    assert_eq!(
        io::classify_all(&transaction_root, &plan.manifest, &plan.roots, None)
            .expect("classify retry"),
        vec![ParticipantState::Before]
    );
    io::cleanup_artifacts(
        &transaction_root,
        &plan.manifest,
        &plan.roots,
        ParticipantState::Before,
    )
    .expect("finish cleanup");
    assert!(!transaction_root.exists());
}

#[test]
fn stale_file_preimage_is_rejected_before_snapshot_or_target_write() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transaction_root = temp.path().join("mutation");
    std::fs::create_dir(&transaction_root).expect("mutation root");
    let live = temp.path().join("ReShade64.dll");
    std::fs::write(&live, b"current").expect("seed");

    let result = MutationPlan::build(Request {
        transaction_root: transaction_root.clone(),
        mutation_id: "stale-file".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: vec![FileIntent {
            live_path: live.clone(),
            before: Some(b"stale".to_vec()),
            after: Some(b"after".to_vec()),
        }],
        registry: Vec::new(),
        registry_authority: None,
        created_dirs: Vec::new(),
    });

    assert!(result.is_err());
    assert_eq!(std::fs::read(live).expect("unchanged"), b"current");
    assert!(!transaction_root.join("snapshots").exists());
}

#[test]
fn stale_registry_preimage_is_rejected_without_mutation() {
    let temp = tempfile::tempdir().expect("tempdir");
    let transaction_root = temp.path().join("mutation");
    std::fs::create_dir(&transaction_root).expect("mutation root");
    let registry = FakeRegistry {
        value: RefCell::new(RegistryValueState::Present {
            value_type: 4,
            raw_bytes: vec![0; 4],
        }),
    };

    let result = MutationPlan::build(Request {
        transaction_root,
        mutation_id: "stale-registry".to_owned(),
        roots: super::TrustedRoots::shared_only(temp.path()).expect("roots"),
        scope: Scope::SharedOnly,
        game_id: None,
        feature: "test".to_owned(),
        intents: Vec::new(),
        registry: vec![super::RegistryIntent {
            manifest_path: temp.path().join("ReShade64.json"),
            before: RegistryValue::Absent,
            after: RegistryValue::Present {
                value_type: 4,
                raw_bytes: vec![0; 4],
            },
        }],
        registry_authority: Some(&registry),
        created_dirs: Vec::new(),
    });

    assert!(result.is_err());
    assert_eq!(
        *registry.value.borrow(),
        RegistryValueState::Present {
            value_type: 4,
            raw_bytes: vec![0; 4],
        }
    );
}
