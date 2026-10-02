use std::{
    cell::Cell,
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use super::{
    EnginePendingClass, classify_engine_pending, engine_release_operation_id,
    vulkan_unregister_operation_id,
};
use renderpilot_application::{AppResult, GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, EngineConfigContribution, EngineConfigJournal, EngineConfigReceipt,
    EngineConfigTransition, GameIdentity, GameInstallation, GameRuntime, InstalledAddon, Launcher,
    PathRef, Platform,
};
use renderpilot_storage_sqlite::{AuthorityCas, EngineConfigJournalOwner, SqliteStorage};

use crate::{
    Context,
    addons::engine_config::{
        EngineIniEntry, EngineIniRecipe, EngineIniRecipeSet,
        service::{self, EngineConfigJournalStore},
    },
};

static NEXT_GAME: AtomicU64 = AtomicU64::new(0);

struct FailFinalEngineCas<'a> {
    storage: &'a SqliteStorage,
    calls: Cell<usize>,
}

impl EngineConfigJournalStore for FailFinalEngineCas<'_> {
    fn engine_config_journal_token(
        &self,
        game_id: &renderpilot_domain::GameId,
        kind: AddonKind,
    ) -> AppResult<Option<String>> {
        self.storage.engine_config_journal_token(game_id, kind)
    }

    fn compare_and_swap_engine_config_journal(
        &self,
        game_id: &renderpilot_domain::GameId,
        kind: AddonKind,
        expected_raw: Option<&str>,
        journal: Option<&EngineConfigJournal>,
    ) -> AppResult<bool> {
        let call = self.calls.get() + 1;
        self.calls.set(call);
        if call == 2 && journal.is_none_or(EngineConfigJournal::is_empty) {
            return Ok(false);
        }
        self.storage
            .compare_and_swap_engine_config_journal(game_id, kind, expected_raw, journal)
    }
}

fn receipt() -> EngineConfigReceipt {
    EngineConfigReceipt {
        schema_version: 1,
        path: "C:/Users/test/Saved/Config/Engine.ini".to_owned(),
        file_created: false,
        encoding: "utf8".to_owned(),
        before_digest: "a".repeat(64),
        after_digest: "b".repeat(64),
        recipe_fingerprint: "c".repeat(64),
        contributions: vec![EngineConfigContribution {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
            line: b"r.AllowHDR=1\n".to_vec(),
            left_anchor: "d".repeat(64),
            right_anchor: "e".repeat(64),
            introduced_prefix: Vec::new(),
            group: "SystemSettings".to_owned(),
            ordinal: 0,
        }],
        created_headers: Vec::new(),
        created_header_prefixes: Vec::new(),
        created_header_groups: Vec::new(),
        created_header_ordinals: Vec::new(),
    }
}

fn game() -> GameInstallation {
    let id =
        renderpilot_domain::GameId::new("manual:leftovers-engine-classifier").expect("game id");
    GameInstallation::new(
        GameIdentity::new(id, "Classifier fixture", Launcher::Manual).expect("identity"),
        Platform::Windows,
        GameRuntime::NativeWindows,
        PathRef::new("C:/Games/Classifier").expect("game root"),
    )
}

fn path_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().replace('\\', "/")).expect("path")
}

#[test]
fn operation_ids_are_deterministic_for_the_current_game_and_owner() {
    let game = game();
    let receipt = receipt();
    assert_eq!(
        engine_release_operation_id(&game, AddonKind::RenoDx, &receipt),
        engine_release_operation_id(&game, AddonKind::RenoDx, &receipt)
    );
    let id = vulkan_unregister_operation_id(game.id(), 7, "C:/Games/Classifier/game.exe");
    assert_eq!(
        id,
        vulkan_unregister_operation_id(game.id(), 7, "C:/Games/Classifier/game.exe")
    );
    assert_ne!(
        id,
        vulkan_unregister_operation_id(game.id(), 8, "C:/Games/Classifier/game.exe")
    );
}

#[test]
fn engine_pending_classifier_accepts_only_the_exact_receipt_release_transition() {
    let game = game();
    let receipt = receipt();
    let id = engine_release_operation_id(&game, AddonKind::RenoDx, &receipt);
    let journal = EngineConfigJournal {
        stable: Some(receipt.clone()),
        pending: Some(EngineConfigTransition {
            operation_id: id.clone(),
            stage_name: format!(".renderpilot-engine-{id}.stage"),
            prior: Some(receipt),
            after: None,
            before_digest: "f".repeat(64),
            after_digest: "1".repeat(64),
        }),
    };
    let owner = EngineConfigJournalOwner {
        kind: AddonKind::RenoDx,
        journal: journal.clone(),
        raw_token: "exact persisted token".to_owned(),
    };
    assert_eq!(
        classify_engine_pending(&game, &owner),
        EnginePendingClass::OwnRelease
    );

    let mut foreign = journal;
    foreign.pending.as_mut().expect("pending").operation_id = "other-operation".to_owned();
    let owner = EngineConfigJournalOwner {
        kind: AddonKind::RenoDx,
        journal: foreign,
        raw_token: "foreign persisted token".to_owned(),
    };
    assert_eq!(
        classify_engine_pending(&game, &owner),
        EnginePendingClass::Foreign
    );
}

#[test]
fn ordinary_engine_release_after_user_edit_recovers_from_final_cas_failure() {
    let temp = tempfile::tempdir().expect("fixture directory");
    let game_root = temp.path().join("game");
    fs::create_dir_all(&game_root).expect("game root");
    let identity = GameIdentity::new(
        renderpilot_domain::GameId::new(format!(
            "manual:leftovers-engine-retry-{}-{}",
            std::process::id(),
            NEXT_GAME.fetch_add(1, Ordering::Relaxed)
        ))
        .expect("game id"),
        "Engine retry fixture",
        Launcher::Manual,
    )
    .expect("game identity");
    let game = GameInstallation::new(
        identity,
        Platform::Windows,
        GameRuntime::NativeWindows,
        path_ref(&game_root),
    );
    let storage = SqliteStorage::open(temp.path().join("catalog.sqlite")).expect("storage");
    storage.upsert_game(&game).expect("game registration");
    let addon_path = game_root.join("addon.addon64");
    storage
        .upsert_installed_addon(&InstalledAddon::new(
            game.id().clone(),
            AddonKind::RenoDx,
            path_ref(&addon_path),
        ))
        .expect("add-on metadata");
    let config_dir = temp.path().join("Saved/Config/Windows");
    fs::create_dir_all(&config_dir).expect("Engine.ini directory");
    let engine_ini = config_dir.join("Engine.ini");
    fs::write(
        &engine_ini,
        b"[SystemSettings]\r\nr.UserSetting=2\r\n; original comment\r\n",
    )
    .expect("initial Engine.ini");
    let recipe = EngineIniRecipe::new(
        "leftovers.retry.test",
        1,
        vec![EngineIniEntry {
            section: "SystemSettings".to_owned(),
            key: "r.AllowHDR".to_owned(),
            value: "1".to_owned(),
        }],
    )
    .expect("recipe");
    let recipes = EngineIniRecipeSet::from_recipes([&recipe]).expect("recipe set");
    service::apply(
        &storage,
        game.id(),
        AddonKind::RenoDx,
        &engine_ini,
        &recipes,
        "leftovers-retry-apply",
    )
    .expect("ordinary Engine.ini install");
    let mut edited = fs::read(&engine_ini).expect("current Engine.ini");
    edited.extend_from_slice(b"; user edit after install\r\nr.UserAfterInstall=7\r\n");
    fs::write(&engine_ini, &edited).expect("unrelated user edit");
    let installed_owner = storage
        .engine_config_journal_owner(game.id())
        .expect("installed Engine owner")
        .expect("stable Engine receipt");
    let installed_receipt = installed_owner
        .journal
        .stable
        .as_ref()
        .expect("stable receipt before release");
    let operation_id = engine_release_operation_id(&game, AddonKind::RenoDx, installed_receipt);
    let readiness = storage
        .catalog_readiness(game.id())
        .expect("catalog readiness");
    storage
        .mark_installation_absent(&game, AuthorityCas::new(readiness.authority_epoch()))
        .expect("mark game absent");
    let context = Context::from_storage(storage);
    let wrapper = FailFinalEngineCas {
        storage: context.storage(),
        calls: Cell::new(0),
    };
    assert!(
        service::release_record_by_kind(&wrapper, game.id(), AddonKind::RenoDx, &operation_id,)
            .is_err()
    );
    assert_eq!(wrapper.calls.get(), 2);
    let published = String::from_utf8(fs::read(&engine_ini).expect("published release bytes"))
        .expect("UTF-8 published Engine.ini");
    assert!(!published.contains("r.AllowHDR=1"));
    assert!(published.contains("r.UserAfterInstall=7"));

    let owners = context
        .storage()
        .read_local_cleanup_owners(game.id())
        .expect("pending release owners");
    let owner = owners.engine_journal().expect("pending Engine owner");
    let prior = owner
        .journal
        .pending
        .as_ref()
        .and_then(|pending| pending.prior.as_ref())
        .expect("pending prior receipt");
    let pending = owner.journal.pending.as_ref().expect("pending release");
    assert_ne!(pending.before_digest, prior.after_digest);
    assert_ne!(pending.after_digest, prior.before_digest);
    assert_eq!(
        classify_engine_pending(&game, owner),
        EnginePendingClass::OwnRelease
    );
    let listed = super::super::list_retired_game_leftovers(&context).expect("List pending release");
    assert_eq!(listed.proposals.len(), 1);
    assert!(listed.proposals[0].items.iter().any(|item| {
        item.category == super::super::dto::LeftoverCategory::EngineConfig
            && item.disposition == super::super::dto::LeftoverDisposition::Retryable
    }));
    let cleaned = super::super::clean_retired_game_leftovers(
        &context,
        game.id(),
        &listed.proposals[0].intent,
    )
    .expect("Clean pending release");
    assert_eq!(cleaned.status, super::super::dto::CleanupStatus::Complete);
    assert!(cleaned.remaining_proposal.is_none());
    let text = String::from_utf8(fs::read(&engine_ini).expect("released Engine.ini"))
        .expect("UTF-8 Engine.ini");
    assert!(text.contains("r.UserSetting=2"));
    assert!(text.contains("; original comment"));
    assert!(text.contains("; user edit after install"));
    assert!(text.contains("r.UserAfterInstall=7"));
    assert!(!text.contains("r.AllowHDR=1"));
    assert!(
        context
            .storage()
            .engine_config_journal_owner(game.id())
            .expect("released owner")
            .is_none()
    );
}
