use super::*;
use renderpilot_domain::{EngineConfigJournal, EngineConfigTransition};
use std::cell::{Cell, RefCell};
use tempfile::tempdir;

struct MemoryJournal {
    token: RefCell<Option<String>>,
}

/// Test-only store that makes the journal mutation boundary observable.
/// The production service still receives only the exact raw token and a
/// null-safe CAS; the counter lets the idempotency tests prove that no
/// hidden Pending/finalization cycle occurred.
struct RecordingJournal {
    token: RefCell<Option<String>>,
    cas_count: Cell<usize>,
}

impl RecordingJournal {
    fn new(token: Option<String>) -> Self {
        Self {
            token: RefCell::new(token),
            cas_count: Cell::new(0),
        }
    }

    fn cas_count(&self) -> usize {
        self.cas_count.get()
    }
}

impl EngineConfigJournalStore for MemoryJournal {
    fn engine_config_journal_token(
        &self,
        _game_id: &GameId,
        _kind: AddonKind,
    ) -> AppResult<Option<String>> {
        Ok(self.token.borrow().clone())
    }

    fn compare_and_swap_engine_config_journal(
        &self,
        _game_id: &GameId,
        kind: AddonKind,
        expected_raw: Option<&str>,
        journal: Option<&EngineConfigJournal>,
    ) -> AppResult<bool> {
        let current = self.token.borrow().clone();
        if current.as_deref() != expected_raw {
            return Ok(false);
        }
        journal
            .map(|value| value.validate_for_kind(kind))
            .transpose()
            .map_err(|error| renderpilot_application::AppError::invalid_input(error.to_string()))?;
        *self.token.borrow_mut() = journal
            .filter(|value| !value.is_empty())
            .map(|value| serde_json::to_string(value).expect("journal"));
        Ok(true)
    }
}

impl EngineConfigJournalStore for RecordingJournal {
    fn engine_config_journal_token(
        &self,
        _game_id: &GameId,
        _kind: AddonKind,
    ) -> AppResult<Option<String>> {
        Ok(self.token.borrow().clone())
    }

    fn compare_and_swap_engine_config_journal(
        &self,
        _game_id: &GameId,
        kind: AddonKind,
        expected_raw: Option<&str>,
        journal: Option<&EngineConfigJournal>,
    ) -> AppResult<bool> {
        self.cas_count.set(self.cas_count.get() + 1);
        let current = self.token.borrow().clone();
        if current.as_deref() != expected_raw {
            return Ok(false);
        }
        journal
            .map(|value| value.validate_for_kind(kind))
            .transpose()
            .map_err(|error| renderpilot_application::AppError::invalid_input(error.to_string()))?;
        *self.token.borrow_mut() = journal
            .filter(|value| !value.is_empty())
            .map(|value| serde_json::to_string(value).expect("journal"));
        Ok(true)
    }
}

#[test]
fn apply_and_release_follow_pending_before_visible_mutation() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("Engine.ini");
    let recipe = super::super::EngineIniRecipe::new(
        "renodx.test.engine",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:engine-config").expect("game");

    assert_eq!(
        apply(
            &store,
            &game,
            AddonKind::RenoDx,
            &path,
            &recipes,
            "apply-test",
        )
        .expect("apply"),
        ApplyOutcome::Applied
    );
    assert!(path.is_file());
    assert!(store.token.borrow().is_some());

    assert_eq!(
        release(&store, &game, AddonKind::RenoDx, &path, "release-test",).expect("release"),
        ReleaseOutcome::Released
    );
    assert!(!path.exists());
    assert!(store.token.borrow().is_none());
}

#[test]
fn canonical_engine_journal_can_be_released_without_an_installed_addon_row() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("Engine.ini");
    let recipe = super::super::EngineIniRecipe::new(
        "canonical.owner",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:canonical-engine-owner").expect("game");

    apply(
        &store,
        &game,
        AddonKind::RenoDx,
        &path,
        &recipes,
        "canonical-owner-apply",
    )
    .expect("apply");

    assert_eq!(
        release_record_by_kind(&store, &game, AddonKind::RenoDx, "canonical-owner-release",)
            .expect("release canonical owner"),
        ReleaseOutcome::Released
    );
    assert!(!path.exists(), "the persisted target was released");
    assert!(
        store.token.borrow().is_none(),
        "the canonical owner was cleared"
    );
}

#[test]
fn kind_scoped_release_does_not_consume_another_kind_engine_owner() {
    let temp = tempdir().expect("temp");
    let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
    let path = temp.path().join("Engine.ini");
    let recipe = super::super::EngineIniRecipe::new(
        "kind.scoped.owner",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let game = GameId::new("manual:kind-scoped-engine-owner").expect("game");
    apply(
        context.storage(),
        &game,
        AddonKind::RenoDx,
        &path,
        &recipes,
        "kind-scoped-owner-apply",
    )
    .expect("apply RenoDX owner");
    let owner_token = context
        .storage()
        .engine_config_journal_token(&game, AddonKind::RenoDx)
        .expect("RenoDX token")
        .expect("RenoDX owner");
    let bytes_before = std::fs::read(&path).expect("Engine.ini");

    assert_eq!(
        release_record_by_kind(
            context.storage(),
            &game,
            AddonKind::Luma,
            "wrong-kind-release",
        )
        .expect("unrelated kind is a no-op"),
        ReleaseOutcome::NotConfigured
    );

    assert_eq!(
        context
            .storage()
            .engine_config_journal_token(&game, AddonKind::RenoDx)
            .expect("RenoDX token remains")
            .as_deref(),
        Some(owner_token.as_str())
    );
    assert_eq!(
        std::fs::read(&path).expect("unchanged Engine.ini"),
        bytes_before
    );
}

#[test]
fn recovery_promotes_after_and_cancels_before_without_editing_target() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("Engine.ini");
    let recipe = super::super::EngineIniRecipe::new(
        "renodx.recovery",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:engine-recovery").expect("game");
    apply(
        &store,
        &game,
        AddonKind::RenoDx,
        &path,
        &recipes,
        "recovery-apply",
    )
    .expect("initial apply");
    let stable: EngineConfigJournal =
        serde_json::from_str(store.token.borrow().as_deref().expect("stable"))
            .expect("stable journal");
    let stable_receipt = stable.stable.expect("receipt");
    let pending = EngineConfigJournal {
        stable: Some(stable_receipt.clone()),
        pending: Some(EngineConfigTransition {
            operation_id: "recovery-after".to_owned(),
            stage_name: ".renderpilot-engine-recovery-after.stage".to_owned(),
            prior: Some(stable_receipt.clone()),
            after: Some(stable_receipt.clone()),
            before_digest: stable_receipt.before_digest.clone(),
            after_digest: stable_receipt.after_digest.clone(),
        }),
    };
    *store.token.borrow_mut() = Some(serde_json::to_string(&pending).expect("pending"));
    let after_bytes = std::fs::read(&path).expect("after bytes");
    assert_eq!(
        recover_pending(&store, &game, AddonKind::RenoDx).expect("promote"),
        RecoveryOutcome::PromotedAfter
    );
    assert_eq!(std::fs::read(&path).expect("unchanged bytes"), after_bytes);

    let pending = EngineConfigJournal {
        stable: Some(stable_receipt.clone()),
        pending: Some(EngineConfigTransition {
            operation_id: "recovery-before".to_owned(),
            stage_name: ".renderpilot-engine-recovery-before.stage".to_owned(),
            before_digest: stable_receipt.before_digest.clone(),
            after_digest: stable_receipt.after_digest.clone(),
            prior: Some(stable_receipt.clone()),
            after: Some(stable_receipt),
        }),
    };
    std::fs::remove_file(&path).expect("restore before");
    *store.token.borrow_mut() = Some(serde_json::to_string(&pending).expect("pending"));
    assert_eq!(
        recover_pending(&store, &game, AddonKind::RenoDx).expect("cancel"),
        RecoveryOutcome::CancelledBefore
    );
    assert!(!path.exists());
}

#[test]
fn recovery_neither_image_preserves_pending_for_manual_repair() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("Engine.ini");
    let recipe = super::super::EngineIniRecipe::new(
        "renodx.recovery.neither",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:engine-recovery-neither").expect("game");
    apply(
        &store,
        &game,
        AddonKind::RenoDx,
        &path,
        &recipes,
        "neither-apply",
    )
    .expect("initial apply");
    let stable: EngineConfigJournal =
        serde_json::from_str(store.token.borrow().as_deref().expect("stable"))
            .expect("stable journal");
    let receipt = stable.stable.expect("receipt");
    let pending = EngineConfigJournal {
        stable: Some(receipt.clone()),
        pending: Some(EngineConfigTransition {
            operation_id: "recovery-neither".to_owned(),
            stage_name: ".renderpilot-engine-recovery-neither.stage".to_owned(),
            before_digest: receipt.before_digest.clone(),
            after_digest: receipt.after_digest.clone(),
            prior: Some(receipt.clone()),
            after: Some(receipt),
        }),
    };
    std::fs::write(&path, b"foreign").expect("foreign bytes");
    let before_token = serde_json::to_string(&pending).expect("pending");
    *store.token.borrow_mut() = Some(before_token.clone());
    assert!(matches!(
        recover_pending(&store, &game, AddonKind::RenoDx),
        Err(EngineConfigServiceError::RecoveryRequired)
    ));
    assert_eq!(store.token.borrow().as_deref(), Some(before_token.as_str()));
}

#[test]
fn recovery_promotes_release_after_none_for_rewritten_file() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("Engine.ini");
    std::fs::write(&path, b"[SystemSettings]\n").expect("before");
    let recipe = super::super::EngineIniRecipe::new(
        "release.after-none.rewritten",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:release-after-none-rewritten").expect("game");
    apply(
        &store,
        &game,
        AddonKind::RenoDx,
        &path,
        &recipes,
        "release-none-1",
    )
    .expect("apply");
    let stable: EngineConfigJournal =
        serde_json::from_str(store.token.borrow().as_deref().expect("stable"))
            .expect("stable journal");
    let receipt = stable.stable.expect("receipt");
    let current = std::fs::read(&path).expect("current");
    let released = super::super::release_engine_ini_report(
        &current,
        &receipt_to_runtime(&receipt).expect("runtime receipt"),
    )
    .expect("release report");
    assert!(released.complete);
    std::fs::write(&path, &released.bytes).expect("released bytes");
    let before_digest = receipt.after_digest.clone();
    let pending = EngineConfigJournal {
        stable: Some(receipt.clone()),
        pending: Some(EngineConfigTransition {
            operation_id: "release-after-none-rewritten".to_owned(),
            stage_name: ".renderpilot-engine-release-after-none-rewritten.stage".to_owned(),
            prior: Some(receipt),
            after: None,
            before_digest,
            after_digest: super::publication::digest_bytes(&released.bytes),
        }),
    };
    std::fs::write(
        path.parent()
            .expect("parent")
            .join(".renderpilot-engine-release-after-none-rewritten.stage"),
        &released.bytes,
    )
    .expect("crashed stage");
    *store.token.borrow_mut() = Some(serde_json::to_string(&pending).expect("pending"));

    assert_eq!(
        recover_pending(&store, &game, AddonKind::RenoDx).expect("promote release"),
        RecoveryOutcome::PromotedAfter
    );
    assert!(
        store.token.borrow().is_none(),
        "release after=None becomes SQL NULL"
    );
    assert_eq!(
        std::fs::read(&path).expect("released bytes"),
        released.bytes
    );
    assert!(
        !path
            .parent()
            .expect("parent")
            .join(".renderpilot-engine-release-after-none-rewritten.stage")
            .exists()
    );
}

#[test]
fn recovery_promotes_release_after_none_for_created_file_deletion() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("Engine.ini");
    let recipe = super::super::EngineIniRecipe::new(
        "release.after-none.created",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:release-after-none-created").expect("game");
    apply(
        &store,
        &game,
        AddonKind::Luma,
        &path,
        &recipes,
        "release-none-2",
    )
    .expect("apply");
    let stable: EngineConfigJournal =
        serde_json::from_str(store.token.borrow().as_deref().expect("stable"))
            .expect("stable journal");
    let receipt = stable.stable.expect("receipt");
    std::fs::remove_file(&path).expect("delete released file");
    let before_digest = receipt.after_digest.clone();
    let pending = EngineConfigJournal {
        stable: Some(receipt.clone()),
        pending: Some(EngineConfigTransition {
            operation_id: "release-after-none-created".to_owned(),
            stage_name: ".renderpilot-engine-release-after-none-created.stage".to_owned(),
            prior: Some(receipt),
            after: None,
            before_digest,
            after_digest: super::publication::digest_bytes(&[]),
        }),
    };
    std::fs::write(
        path.parent()
            .expect("parent")
            .join(".renderpilot-engine-release-after-none-created.stage"),
        b"",
    )
    .expect("crashed deletion stage");
    *store.token.borrow_mut() = Some(serde_json::to_string(&pending).expect("pending"));

    assert_eq!(
        recover_pending(&store, &game, AddonKind::Luma).expect("promote deletion"),
        RecoveryOutcome::PromotedAfter
    );
    assert!(store.token.borrow().is_none());
    assert!(
        !path
            .parent()
            .expect("parent")
            .join(".renderpilot-engine-release-after-none-created.stage")
            .exists()
    );
}

#[test]
fn recovery_cancels_release_pending_when_before_image_remains() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("Engine.ini");
    std::fs::write(&path, b"[SystemSettings]\n").expect("before");
    let recipe = super::super::EngineIniRecipe::new(
        "release.before-image",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:release-before-image").expect("game");
    apply(
        &store,
        &game,
        AddonKind::RenoDx,
        &path,
        &recipes,
        "release-before",
    )
    .expect("apply");
    let stable: EngineConfigJournal =
        serde_json::from_str(store.token.borrow().as_deref().expect("stable"))
            .expect("stable journal");
    let receipt = stable.stable.expect("receipt");
    std::fs::write(&path, b"[SystemSettings]\n").expect("restore before");
    let pending = EngineConfigJournal {
        stable: Some(receipt.clone()),
        pending: Some(EngineConfigTransition {
            operation_id: "release-before-image".to_owned(),
            stage_name: ".renderpilot-engine-release-before-image.stage".to_owned(),
            prior: Some(receipt.clone()),
            after: None,
            before_digest: receipt.before_digest.clone(),
            after_digest: super::publication::digest_bytes(b"released"),
        }),
    };
    *store.token.borrow_mut() = Some(serde_json::to_string(&pending).expect("pending"));
    assert_eq!(
        recover_pending(&store, &game, AddonKind::RenoDx).expect("cancel"),
        RecoveryOutcome::CancelledBefore
    );
    let recovered: EngineConfigJournal =
        serde_json::from_str(store.token.borrow().as_deref().expect("stable restored"))
            .expect("journal");
    assert_eq!(recovered.stable.expect("stable").path, receipt.path);
}

#[test]
fn release_disclaims_changed_duplicate_and_foreign_owned_content() {
    let cases = [
        ("changed", b"[SystemSettings]\nr.AllowHDR=2\n".as_slice()),
        (
            "duplicate",
            b"[SystemSettings]\nr.AllowHDR=1\nr.AllowHDR=1\n".as_slice(),
        ),
        (
            "foreign-comment",
            b"[SystemSettings]\nr.AllowHDR=1\n; user setting\n".as_slice(),
        ),
    ];
    for (label, edited) in cases {
        let temp = tempdir().expect("temp");
        let path = temp.path().join("Engine.ini");
        let recipe = super::super::EngineIniRecipe::new(
            format!("release.disclaimer.{label}"),
            1,
            vec![super::super::EngineIniEntry {
                section: "SystemSettings".to_owned(),
                key: "r.AllowHDR".to_owned(),
                value: "1".to_owned(),
            }],
        )
        .expect("recipe");
        let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
        let store = MemoryJournal {
            token: RefCell::new(None),
        };
        let game = GameId::new(format!("manual:release-disclaimer-{label}")).expect("game");
        apply(
            &store,
            &game,
            AddonKind::RenoDx,
            &path,
            &recipes,
            "release-disclaimer",
        )
        .expect("apply");
        std::fs::write(&path, edited).expect("foreign edit");
        let original = edited.to_vec();
        assert_eq!(
            release(
                &store,
                &game,
                AddonKind::RenoDx,
                &path,
                "release-disclaimer"
            )
            .expect("disclaimer release"),
            ReleaseOutcome::Released
        );
        assert!(store.token.borrow().is_none(), "{label} clears ownership");
        assert_eq!(std::fs::read(&path).expect("file remains"), original);
    }
}

#[test]
fn release_missing_file_disclaims_ownership_and_unblocks_uninstall() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("Engine.ini");
    let recipe = super::super::EngineIniRecipe::new(
        "release.missing",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:release-missing").expect("game");
    apply(
        &store,
        &game,
        AddonKind::RenoDx,
        &path,
        &recipes,
        "release-missing",
    )
    .expect("apply");
    std::fs::remove_file(&path).expect("external delete");
    assert_eq!(
        release(&store, &game, AddonKind::RenoDx, &path, "release-missing")
            .expect("disclaim missing"),
        ReleaseOutcome::Released
    );
    assert!(store.token.borrow().is_none());
}

#[test]
fn apply_reconciles_changed_removed_and_new_entries_and_uninstall_is_clean() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("Engine.ini");
    let initial = super::super::EngineIniRecipe::new(
        "revision.initial",
        1,
        vec![
            super::super::EngineIniEntry {
                section: "SystemSettings".to_owned(),
                key: "r.Keep".to_owned(),
                value: "1".to_owned(),
            },
            super::super::EngineIniEntry {
                section: "SystemSettings".to_owned(),
                key: "r.Remove".to_owned(),
                value: "1".to_owned(),
            },
        ],
    )
    .expect("initial");
    let initial_set = super::super::EngineIniRecipeSet::from_recipes([&initial]).expect("set");
    let revised = super::super::EngineIniRecipe::new(
        "revision.revised",
        2,
        vec![
            super::super::EngineIniEntry {
                section: "SystemSettings".to_owned(),
                key: "r.Keep".to_owned(),
                value: "1".to_owned(),
            },
            super::super::EngineIniEntry {
                section: "SystemSettings".to_owned(),
                key: "r.Changed".to_owned(),
                value: "2".to_owned(),
            },
        ],
    )
    .expect("revised");
    let revised_set = super::super::EngineIniRecipeSet::from_recipes([&revised]).expect("set");
    let store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:revision").expect("game");
    apply(
        &store,
        &game,
        AddonKind::Luma,
        &path,
        &initial_set,
        "revision-initial",
    )
    .expect("initial apply");
    let mut foreign = std::fs::read(&path).expect("initial bytes");
    foreign.extend_from_slice(b"; user comment\n");
    std::fs::write(&path, foreign).expect("foreign comment");
    apply(
        &store,
        &game,
        AddonKind::Luma,
        &path,
        &revised_set,
        "revision-update",
    )
    .expect("revision apply");
    let bytes = std::fs::read_to_string(&path).expect("revised file");
    assert!(bytes.contains("; user comment"));
    assert!(bytes.contains("r.Keep=1"));
    assert!(bytes.contains("r.Changed=2"));
    assert!(!bytes.contains("r.Remove=1"));
    assert_eq!(
        release(&store, &game, AddonKind::Luma, &path, "revision-release").expect("release"),
        ReleaseOutcome::Released
    );
    let released = std::fs::read_to_string(&path).expect("foreign file remains");
    assert!(released.contains("; user comment"));
    assert!(!released.contains("r.Keep=1"));
    assert!(!released.contains("r.Changed=2"));
    assert!(!released.contains("r.Remove=1"));
    assert!(store.token.borrow().is_none());
}

#[test]
fn relocation_releases_old_target_before_fresh_new_target_apply() {
    let temp = tempdir().expect("temp");
    let old_path = temp.path().join("old/Engine.ini");
    let new_path = temp.path().join("new/Engine.ini");
    std::fs::create_dir_all(old_path.parent().expect("old parent")).expect("old dir");
    std::fs::create_dir_all(new_path.parent().expect("new parent")).expect("new dir");
    let recipe = super::super::EngineIniRecipe::new(
        "renodx.relocation",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:engine-relocation").expect("game");
    apply(
        &store,
        &game,
        AddonKind::Luma,
        &old_path,
        &recipes,
        "relocation-initial",
    )
    .expect("old apply");
    apply(
        &store,
        &game,
        AddonKind::Luma,
        &new_path,
        &recipes,
        "relocation-new",
    )
    .expect("relocation");
    assert!(!old_path.exists());
    assert!(new_path.is_file());
    let journal: EngineConfigJournal =
        serde_json::from_str(store.token.borrow().as_deref().expect("journal")).expect("journal");
    assert_eq!(
        journal.stable.expect("stable").path,
        new_path.to_string_lossy()
    );
}

#[test]
fn repeated_identical_recipe_is_a_true_noop() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("Engine.ini");
    let recipe = super::super::EngineIniRecipe::new(
        "idempotency.same",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let initial_store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:engine-idempotency").expect("game");
    apply(
        &initial_store,
        &game,
        AddonKind::RenoDx,
        &path,
        &recipes,
        "idempotency-initial",
    )
    .expect("initial apply");
    let stable_raw = initial_store.token.borrow().clone().expect("stable token");
    let before = std::fs::read(&path).expect("before bytes");
    let store = RecordingJournal::new(Some(stable_raw));

    assert_eq!(
        apply(
            &store,
            &game,
            AddonKind::RenoDx,
            &path,
            &recipes,
            "idempotency-repeat",
        )
        .expect("repeat apply"),
        ApplyOutcome::AlreadyConfigured
    );
    assert_eq!(store.cas_count(), 0, "identical apply must not CAS");
    assert_eq!(std::fs::read(&path).expect("after bytes"), before);
    assert!(
        !path
            .parent()
            .expect("parent")
            .join(".renderpilot-engine-idempotency-repeat.stage")
            .exists()
    );
}

#[test]
fn foreign_comment_rebases_receipt_without_publishing() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("Engine.ini");
    let recipe = super::super::EngineIniRecipe::new(
        "idempotency.rebase",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let initial_store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:engine-rebase").expect("game");
    apply(
        &initial_store,
        &game,
        AddonKind::RenoDx,
        &path,
        &recipes,
        "rebase-initial",
    )
    .expect("initial apply");
    let prior_raw = initial_store.token.borrow().clone().expect("stable token");
    let prior: EngineConfigJournal = serde_json::from_str(&prior_raw).expect("prior journal");
    let mut foreign = std::fs::read(&path).expect("before bytes");
    foreign.extend_from_slice(b"; user comment\n");
    std::fs::write(&path, &foreign).expect("foreign edit");
    let modified_before = std::fs::metadata(&path)
        .expect("metadata before")
        .modified()
        .expect("mtime before");
    let store = RecordingJournal::new(Some(prior_raw));

    assert_eq!(
        apply(
            &store,
            &game,
            AddonKind::RenoDx,
            &path,
            &recipes,
            "rebase-metadata",
        )
        .expect("metadata rebase"),
        ApplyOutcome::MetadataReconciled
    );
    assert_eq!(store.cas_count(), 1, "metadata rebase uses one direct CAS");
    assert_eq!(std::fs::read(&path).expect("after bytes"), foreign);
    assert_eq!(
        std::fs::metadata(&path)
            .expect("metadata after")
            .modified()
            .expect("mtime after"),
        modified_before,
        "journal-only rebase must not touch the file"
    );
    let next: EngineConfigJournal =
        serde_json::from_str(store.token.borrow().as_deref().expect("next journal"))
            .expect("next journal json");
    assert!(next.pending.is_none());
    assert_ne!(
        next.stable.expect("next stable").after_digest,
        prior.stable.expect("prior stable").after_digest,
        "foreign bytes update receipt metadata"
    );
}

#[test]
fn harmless_foreign_comment_remains_configured_in_availability() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("Engine.ini");
    let recipe = super::super::EngineIniRecipe::new(
        "availability.foreign-comment",
        1,
        vec![super::super::EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = super::super::EngineIniRecipeSet::from_recipes([&recipe]).expect("set");
    let store = MemoryJournal {
        token: RefCell::new(None),
    };
    let game = GameId::new("manual:engine-availability").expect("game");
    apply(
        &store,
        &game,
        AddonKind::RenoDx,
        &path,
        &recipes,
        "availability-initial",
    )
    .expect("initial apply");
    let journal: EngineConfigJournal =
        serde_json::from_str(store.token.borrow().as_deref().expect("stable")).expect("journal");
    let mut foreign = std::fs::read(&path).expect("before bytes");
    foreign.extend_from_slice(b"; harmless user comment\n");
    std::fs::write(&path, foreign).expect("foreign edit");

    let report = inspect_availability(
        &EngineIniResolution::Ready(path),
        Some(&recipes),
        false,
        Some(&journal),
    );
    assert_eq!(report.status, EngineConfigStatus::Configured);
    assert!(!report.can_apply);
}
