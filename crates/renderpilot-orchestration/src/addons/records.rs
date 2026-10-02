//! Kind-aware access to the single `installed_addons` row a game can have.
//!
//! `InstalledAddonRepository::get_installed_addon` returns whichever record is on
//! file for a game with **no kind filter** — a caller that assumes any record it
//! gets back is "its own" tool's would misread a foreign-tool record (or a stale
//! test fixture) as its own install. Add-on-facing reads must use this module's
//! persisted or active selectors rather than calling the repository directly;
//! [`foreign_record`] is the explicit cross-kind ownership view.
//!
//! Persisted ownership and active installation are deliberately separate:
//! mutation/cleanup paths use [`record_of_kind`] so manually removed files do
//! not strand the metadata needed for repair or uninstall, while presentation,
//! availability, and same-tool reinstall decisions use
//! [`active_record_of_kind`]. Cross-tool ownership remains record-authoritative:
//! storage deliberately refuses to overwrite a different add-on kind.

use std::path::PathBuf;

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, ManagedFileMode, TrackedSource, TrackedSourceRole,
};

use crate::addons::errors;
use crate::{Context, ServiceError};

/// Live paths owned by an install record, expanded with `.bak` sidecars.
///
/// Collects `created_files`, `backed_up_files`, and `managed_files`, then expands
/// each with [`crate::fs::expand_with_sidecars`] so durable mutation scopes
/// snapshot both the live file and its sidecar.
pub(crate) fn record_live_and_sidecar_paths(record: &InstalledAddon) -> Vec<PathBuf> {
    let live = record
        .created_files()
        .iter()
        .chain(record.backed_up_files())
        .map(|path| PathBuf::from(path.as_str()))
        .chain(
            record
                .managed_files()
                .iter()
                .map(|managed| PathBuf::from(managed.path().as_str())),
        );
    crate::fs::expand_with_sidecars(live)
}

/// Live paths of managed bindings with [`ManagedFileMode::Owned`].
///
/// Shared selector for cascade planning when those bindings are about to
/// disappear (uninstall, or an update that no longer ships them).
pub(crate) fn owned_managed_paths(record: &InstalledAddon) -> Vec<PathBuf> {
    record
        .managed_files()
        .iter()
        .filter(|managed| managed.mode() == ManagedFileMode::Owned)
        .map(|managed| PathBuf::from(managed.path().as_str()))
        .collect()
}

/// The requested game is not present in the library. Shared "not found" error
/// constructor for every addon flow that requires an installed game.
pub(crate) fn game_not_found(game_id: &GameId) -> ServiceError {
    ServiceError::GameNotFound(game_id.as_str().to_owned())
}

/// The installed-addon record for `game_id`, if one exists **and** it belongs to
/// `kind`. A record of a different kind reads as `Ok(None)` — exactly as if
/// nothing were installed — so a caller scoped to one tool never mistakes another
/// tool's install for its own.
pub(crate) fn record_of_kind(
    context: &Context,
    game_id: &GameId,
    kind: AddonKind,
) -> Result<Option<InstalledAddon>, ServiceError> {
    Ok(context
        .storage()
        .get_installed_addon(game_id)?
        .filter(|record| record.kind() == kind))
}

/// Verifies that releasing the Engine.ini journal changed only the journal
/// projection of an authoritative installed record.
pub(crate) fn verify_engine_config_release(
    original: &InstalledAddon,
    refreshed: InstalledAddon,
    addon_name: &str,
) -> Result<InstalledAddon, ServiceError> {
    if !refreshed.is_engine_config_release_of(original) {
        return Err(ServiceError::command_failed(format!(
            "{addon_name} record changed while Engine.ini ownership was released"
        )));
    }
    Ok(refreshed)
}

/// The persisted record for `game_id` only while the owning tool considers the
/// installation active on disk.
pub(crate) fn active_record(
    context: &Context,
    game_id: &GameId,
) -> Result<Option<InstalledAddon>, ServiceError> {
    let Some(record) = context.storage().get_installed_addon(game_id)? else {
        return Ok(None);
    };
    if crate::addons::tool::record_is_active_for_game(context, &record)? {
        Ok(Some(record))
    } else {
        Ok(None)
    }
}

/// The requested kind's persisted record only while the owning tool still
/// considers the installation active on disk.
pub(crate) fn active_record_of_kind(
    context: &Context,
    game_id: &GameId,
    kind: AddonKind,
) -> Result<Option<InstalledAddon>, ServiceError> {
    Ok(active_record(context, game_id)?.filter(|record| record.kind() == kind))
}

/// Every persisted add-on record that still represents an active installation.
pub(crate) fn active_records(
    context: &Context,
) -> Result<impl Iterator<Item = InstalledAddon>, ServiceError> {
    let mut active = Vec::new();
    for record in context.storage().list_installed_addons()? {
        if crate::addons::tool::record_is_active_for_game(context, &record)? {
            active.push(record);
        }
    }
    Ok(active.into_iter())
}

/// Every active install record belonging to `kind`.
pub(crate) fn active_records_of_kind(
    context: &Context,
    kind: AddonKind,
) -> Result<impl Iterator<Item = InstalledAddon>, ServiceError> {
    Ok(active_records(context)?.filter(move |record| record.kind() == kind))
}

/// The installed-addon record for `game_id`, if one exists and belongs to a
/// **different** kind than `requesting`. Used by the mutual-exclusion policy
/// (`addons::exclusivity`) and by any flow that must never act as though a
/// foreign-tool record were its own (e.g. orphan-install reconciliation).
pub(crate) fn foreign_record(
    context: &Context,
    game_id: &GameId,
    requesting: AddonKind,
) -> Result<Option<InstalledAddon>, ServiceError> {
    Ok(context
        .storage()
        .get_installed_addon(game_id)?
        .filter(|record| record.kind() != requesting))
}

/// Resolves the tracked source with the given role, if the install recorded one.
pub(crate) fn source_with_role(
    record: &InstalledAddon,
    role: TrackedSourceRole,
) -> Option<&TrackedSource> {
    record
        .tracked_sources()
        .iter()
        .find(|source| source.role() == role)
}

/// Removes any existing source with `role`, then optionally inserts `replacement`.
///
/// When `replacement` is `Some`, its role must match `role` (debug-asserted).
/// Used by update apply paths that refresh HostBinary / DgVoodooWrapper / etc.
pub(crate) fn replace_source_with_role(
    sources: &mut Vec<TrackedSource>,
    role: TrackedSourceRole,
    replacement: Option<TrackedSource>,
) {
    sources.retain(|source| source.role() != role);
    if let Some(source) = replacement {
        debug_assert_eq!(source.role(), role);
        sources.push(source);
    }
}

/// A cosmetic fetch/log label for an add-on (the file name identifies the title).
pub(crate) fn addon_label(record: &InstalledAddon) -> &str {
    std::path::Path::new(record.addon_file().as_str())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("add-on")
}

/// Ensures no record exists for the given kind. Uses a tool-provided error message
/// so the message can be specific ("Luma is already...") while the check logic is shared.
pub(crate) fn ensure_no_record(
    context: &Context,
    game_id: &GameId,
    kind: AddonKind,
    message: impl Into<String>,
) -> Result<(), ServiceError> {
    if record_of_kind(context, game_id, kind)?.is_some() {
        return Err(errors::invalid(message.into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_application::GameRepository;
    use renderpilot_domain::{
        EngineConfigJournal, EngineConfigReceipt, FileReceipt, GameIdentity, GameInstallation,
        GameProxyTopology, GameRuntime, InstalledAddonHostKind, Launcher, PathRef, Platform,
        ProxyImplementation, ProxyLink, ProxyRootPrestate, TrackedSource, TrackedSourceRole,
    };
    use renderpilot_storage_sqlite::AuthorityCas;
    use tempfile::tempdir;

    use super::*;

    /// Test-only sentinel finish helper. Production installs use
    /// `durable::run_install_mutation` + `commit_game_mutation`.
    fn persist_record_or_revert(
        context: &Context,
        record: InstalledAddon,
        commit: crate::addons::engine::PendingInstallCommit,
        revert: impl FnOnce(&InstalledAddon) -> Result<(), ServiceError>,
    ) -> Result<InstalledAddon, ServiceError> {
        match context.storage().upsert_installed_addon(&record) {
            Ok(()) => {
                commit.finish_committed();
                Ok(record)
            }
            Err(error) => {
                match revert(&record) {
                    Ok(()) => {
                        commit.finish_rolled_back();
                    }
                    Err(revert_error) => {
                        tracing::warn!(
                            "addon install: record persistence failed and filesystem revert also \
                             failed (leaving torn sentinel `{}`): {revert_error}",
                            commit.path().display()
                        );
                        drop(commit);
                    }
                }
                Err(error.into())
            }
        }
    }

    #[test]
    fn record_of_kind_is_none_when_nothing_is_installed() {
        let db_dir = tempdir().expect("tempdir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1").expect("game id");

        assert!(
            record_of_kind(&context, &game_id, AddonKind::RenoDx)
                .expect("query")
                .is_none()
        );
        assert!(
            foreign_record(&context, &game_id, AddonKind::RenoDx)
                .expect("query")
                .is_none()
        );
    }

    #[test]
    fn record_of_kind_returns_a_matching_record_and_hides_others() {
        let db_dir = tempdir().expect("tempdir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1").expect("game id");
        let record = addon_record();
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed record");

        assert_eq!(
            record_of_kind(&context, &game_id, AddonKind::RenoDx)
                .expect("query")
                .as_ref()
                .map(InstalledAddon::kind),
            Some(AddonKind::RenoDx)
        );
        // A second kind now exists: the RenoDX record must read as "nothing
        // installed" for Luma...
        assert!(
            record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("query")
                .is_none()
        );
        // ...while Luma's foreign-record view finds it, and RenoDX's own foreign
        // view stays empty.
        assert_eq!(
            foreign_record(&context, &game_id, AddonKind::Luma)
                .expect("query")
                .as_ref()
                .map(InstalledAddon::kind),
            Some(AddonKind::RenoDx)
        );
        assert!(
            foreign_record(&context, &game_id, AddonKind::RenoDx)
                .expect("query")
                .is_none()
        );
    }

    #[test]
    fn active_views_follow_the_renodx_payload_without_discarding_ownership_metadata() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("steam:1").expect("game id");
        seed_proxy_game(&context, &game_id, game_dir.path());
        write_compatible_proxy_host(game_dir.path());
        let addon = game_dir.path().join("renodx-test.addon64");
        let record = InstalledAddon::new(
            game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(addon.to_string_lossy()).expect("path"),
        );
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed record");

        assert!(
            record_of_kind(&context, &game_id, AddonKind::RenoDx)
                .expect("stored query")
                .is_some(),
            "cleanup metadata must survive manual payload removal"
        );
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::RenoDx)
                .expect("active query")
                .is_none()
        );
        std::fs::create_dir(&addon).expect("create payload-shaped directory");
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::RenoDx)
                .expect("active query")
                .is_none(),
            "a directory at the payload path is not an active installation"
        );
        std::fs::remove_dir(&addon).expect("remove payload-shaped directory");
        std::fs::write(&addon, b"").expect("write empty payload");
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::RenoDx)
                .expect("active query")
                .is_none(),
            "an empty DLL is not an active installation"
        );

        std::fs::write(&addon, b"addon").expect("write payload");
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::RenoDx)
                .expect("active query")
                .is_some()
        );
    }

    #[test]
    fn proxy_records_follow_current_host_and_addon_path_while_internal_and_split_paths_work() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let external_dir = tempdir().expect("external addon dir");
        let changed_dir = tempdir().expect("changed addon dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:proxy-binding").expect("game id");
        seed_proxy_game(&context, &game_id, game_dir.path());
        write_compatible_proxy_host(game_dir.path());

        let external_payload = external_dir.path().join("Luma-Game.addon64");
        std::fs::write(&external_payload, b"luma payload").expect("external payload");
        write_addon_path(game_dir.path(), external_dir.path());
        let external_record = proxy_record(AddonKind::Luma, &game_id, &external_payload);
        context
            .storage()
            .upsert_installed_addon(&external_record)
            .expect("seed external Luma record");
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("matching external binding")
                .is_some(),
            "a current compatible host and matching split AddonPath keep the record active"
        );

        write_addon_path(game_dir.path(), changed_dir.path());
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("changed AddonPath")
                .is_none(),
            "the persisted payload path alone cannot override the current AddonPath"
        );

        write_addon_path(game_dir.path(), external_dir.path());
        std::fs::write(game_dir.path().join("dxgi.dll"), b"not a PE host").expect("invalid host");
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("invalid current host")
                .is_none(),
            "an invalid current host cannot make an external receipt active"
        );

        write_compatible_proxy_host(game_dir.path());
        let internal_payload = game_dir.path().join("Luma-Game.addon64");
        std::fs::write(&internal_payload, b"internal luma payload").expect("internal payload");
        std::fs::remove_file(game_dir.path().join("ReShade.ini")).expect("remove config");
        let internal_record = proxy_record(AddonKind::Luma, &game_id, &internal_payload);
        context
            .storage()
            .upsert_installed_addon(&internal_record)
            .expect("seed internal Luma record");
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("internal binding")
                .is_some(),
            "an internal payload remains active when ReShade uses its default AddonPath"
        );

        let renodx_game_dir = tempdir().expect("RenoDX game dir");
        let renodx_external_dir = tempdir().expect("RenoDX external addon dir");
        let renodx_game_id = GameId::new("manual:renodx-proxy-binding").expect("RenoDX game id");
        seed_proxy_game(&context, &renodx_game_id, renodx_game_dir.path());
        write_compatible_proxy_host(renodx_game_dir.path());
        write_addon_path(renodx_game_dir.path(), renodx_external_dir.path());
        let renodx_payload = renodx_external_dir.path().join("renodx-game.addon64");
        std::fs::write(&renodx_payload, b"renodx payload").expect("RenoDX payload");
        let renodx_record = InstalledAddon::new(
            renodx_game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(renodx_payload.to_string_lossy()).expect("RenoDX payload path"),
        );
        context
            .storage()
            .upsert_installed_addon(&renodx_record)
            .expect("seed external RenoDX record");
        assert!(
            active_record_of_kind(&context, &renodx_game_id, AddonKind::RenoDx)
                .expect("RenoDX split binding")
                .is_some(),
            "RenoDX proxy records use the same current host and AddonPath binding"
        );
        std::fs::remove_file(renodx_game_dir.path().join("dxgi.dll")).expect("remove proxy host");
        assert!(
            active_record_of_kind(&context, &renodx_game_id, AddonKind::RenoDx)
                .expect("legacy RenoDX without host kind")
                .is_none(),
            "a RenoDX record without host-kind metadata still requires current proxy evidence"
        );
        write_compatible_proxy_host(renodx_game_dir.path());
        assert!(
            active_record_of_kind(&context, &renodx_game_id, AddonKind::RenoDx)
                .expect("legacy RenoDX with restored host")
                .is_some()
        );
        assert!(
            context
                .storage()
                .find_active_game(&renodx_game_id)
                .expect("game lookup")
                .is_some()
        );
    }

    #[test]
    fn current_binding_accepts_unique_legacy_override_and_validated_optiscaler_chain() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let external_dir = tempdir().expect("external add-on dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:current-loading-chain").expect("game id");
        seed_proxy_game(&context, &game_id, game_dir.path());
        let payload = external_dir.path().join("Luma-Game.addon64");
        std::fs::write(&payload, b"Luma payload").expect("payload");
        write_addon_path(game_dir.path(), external_dir.path());

        // A supported current override remains usable for a legacy record
        // with no HostBinary receipt.
        write_compatible_proxy_host_at(game_dir.path(), "d3d11.dll");
        let legacy = proxy_record(AddonKind::Luma, &game_id, &payload);
        context
            .storage()
            .upsert_installed_addon(&legacy)
            .expect("seed legacy external receipt");
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("unique override binding")
                .is_some(),
            "a unique current compatible override must remain active"
        );
        let owned_d3d11 = legacy.clone().with_created_file(
            PathRef::new(game_dir.path().join("d3d11.dll").to_string_lossy())
                .expect("owned d3d11 host path"),
        );
        assert!(
            crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &owned_d3d11,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                None,
            ),
            "an exact created-file receipt recognizes a supported d3d11 ReShade host"
        );

        // A persisted host path is authoritative evidence of which host the
        // record owns. A missing recorded host must not fall through to the
        // otherwise-valid override found in the same root.
        let stale_receipt = legacy.clone().with_created_file(
            PathRef::new(game_dir.path().join("dxgi.dll").to_string_lossy())
                .expect("stale host path"),
        );
        context
            .storage()
            .upsert_installed_addon(&stale_receipt)
            .expect("replace with stale host receipt");
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("stale recorded host")
                .is_none(),
            "an invalid recorded host cannot silently switch to another slot"
        );
        assert!(
            record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("raw owner after stale host")
                .is_some(),
            "inactive reads preserve the exact cleanup receipt"
        );

        // RenoDX titles can persist supported overrides outside ReShade's
        // built-in proxy-slot set. A unique unowned dinput8 override remains
        // discoverable, and an exact recorded path takes precedence once the
        // install owns that host.
        context
            .storage()
            .upsert_installed_addon(&legacy)
            .expect("restore legacy receipt");
        std::fs::remove_file(game_dir.path().join("d3d11.dll")).expect("remove d3d11 host");
        write_compatible_proxy_host_at(game_dir.path(), "dinput8.dll");
        assert!(
            crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &legacy,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                None,
            ),
            "a unique reused dinput8 override remains active without a host ownership path"
        );
        let owned_dinput8 = legacy.clone().with_created_file(
            PathRef::new(game_dir.path().join("dinput8.dll").to_string_lossy())
                .expect("owned dinput8 host path"),
        );
        assert!(
            crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &owned_dinput8,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                None,
            ),
            "an exact created-file receipt recognizes a supported dinput8 ReShade host"
        );
        std::fs::write(game_dir.path().join("dinput8.dll"), b"not a ReShade PE")
            .expect("replace recorded host with non-ReShade file");
        write_compatible_proxy_host(game_dir.path());
        assert!(
            !crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &owned_dinput8,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                None,
            ),
            "a readable non-ReShade recorded host cannot fall through to a compatible dxgi host"
        );
        std::fs::remove_file(game_dir.path().join("dxgi.dll")).expect("remove fallback host");
        write_compatible_proxy_host_at(game_dir.path(), "dinput8.dll");
        std::fs::remove_file(game_dir.path().join("dinput8.dll"))
            .expect("remove recorded dinput8 host");
        write_compatible_proxy_host(game_dir.path());
        assert!(
            !crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &owned_dinput8,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                None,
            ),
            "a missing recorded nonstandard host cannot fall through to a compatible dxgi host"
        );
        std::fs::remove_file(game_dir.path().join("dxgi.dll")).expect("remove fallback host");
        write_compatible_proxy_host_at(game_dir.path(), "dinput8.dll");
        write_compatible_proxy_host_at(game_dir.path(), "d3d11.dll");
        assert!(
            crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &owned_dinput8,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                None,
            ),
            "an owned dinput8 host stays exact when another compatible host appears"
        );
        assert!(
            !crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &legacy,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                None,
            ),
            "two compatible unowned hosts remain ambiguous"
        );
        // The OptiScaler chain is a distinct, exact current loading topology:
        // game-loaded OptiScaler outer -> current ReShade64 downstream.
        let topology_game = GameId::new("manual:current-optiscaler-chain").expect("game id");
        let topology_payload = external_dir.path().join("RenoDX-Game.addon64");
        std::fs::write(&topology_payload, b"RenoDX payload").expect("topology payload");
        let outer = game_dir.path().join("d3d11.dll");
        let downstream = game_dir.path().join("ReShade64.dll");
        std::fs::write(
            &outer,
            crate::addons::test_support::build_pe_with_exports(
                crate::addons::test_support::MACHINE_AMD64,
                crate::addons::test_support::PE32_PLUS_MAGIC,
                &[],
            ),
        )
        .expect("write OptiScaler outer");
        write_compatible_proxy_host_at(game_dir.path(), "ReShade64.dll");
        write_addon_path(game_dir.path(), external_dir.path());

        let root_slot = PathRef::new(outer.to_string_lossy()).expect("root slot");
        let downstream_path = PathRef::new(downstream.to_string_lossy()).expect("downstream");
        let topology = GameProxyTopology {
            id: "opti:current-chain".to_owned(),
            game_id: topology_game.clone(),
            root_slot: root_slot.clone(),
            outer: ProxyLink {
                implementation: ProxyImplementation::OptiScaler,
                path: root_slot.clone(),
                receipt: exact_file_receipt(game_dir.path(), &outer),
            },
            downstream: Some(ProxyLink {
                implementation: ProxyImplementation::ReShade,
                path: downstream_path,
                receipt: exact_file_receipt(game_dir.path(), &downstream),
            }),
            downstream_origin: Some(root_slot),
            root_prestate: ProxyRootPrestate::Absent,
        };
        let topology_record = proxy_record(AddonKind::RenoDx, &topology_game, &topology_payload);
        assert!(
            crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &topology_record,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                Some(&topology),
            ),
            "a receipt-validated OptiScaler outer and exact compatible downstream remain active"
        );
        let topology_luma_record = proxy_record(AddonKind::Luma, &topology_game, &topology_payload);
        assert!(
            crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &topology_luma_record,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                Some(&topology),
            ),
            "a current validated OptiScaler chain also binds Luma"
        );

        let mut wrong_game = topology.clone();
        wrong_game.game_id = GameId::new("manual:other-current-chain").expect("other game id");
        assert!(
            !crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &topology_record,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                Some(&wrong_game),
            ),
            "a topology for another game cannot bind this record"
        );

        let other_root = tempdir().expect("other root");
        let other_outer = other_root.path().join("d3d11.dll");
        let mut wrong_root = topology.clone();
        let other_root_slot = PathRef::new(other_outer.to_string_lossy()).expect("other root slot");
        wrong_root.root_slot = other_root_slot.clone();
        wrong_root.outer.path = other_root_slot.clone();
        wrong_root.downstream_origin = Some(other_root_slot);
        assert!(
            wrong_root.validate().is_ok(),
            "wrong-root fixture remains a structurally valid topology"
        );
        assert!(
            !crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &topology_record,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                Some(&wrong_root),
            ),
            "a topology whose root is outside the current runtime is inactive"
        );

        let mut wrong_receipt = topology.clone();
        let wrong_downstream_receipt = wrong_receipt.outer.receipt.clone();
        wrong_receipt
            .downstream
            .as_mut()
            .expect("downstream")
            .receipt = wrong_downstream_receipt;
        assert!(
            wrong_receipt.validate().is_ok(),
            "wrong-receipt fixture remains structurally valid"
        );
        assert!(
            !crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &topology_record,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                Some(&wrong_receipt),
            ),
            "a downstream snapshot must match its own typed receipt"
        );

        std::fs::remove_file(&downstream).expect("remove downstream host");
        assert!(
            !crate::addons::tool::proxy_binding_matches_current_loading_chain(
                &topology_record,
                game_dir.path(),
                Some(renderpilot_domain::Architecture::X64),
                Some(&topology),
            ),
            "a missing exact downstream cannot be replaced by a folder-scanned proxy"
        );
    }

    #[test]
    fn owned_reshade_host_is_not_ambiguous_with_lumas_dgvoodoo_wrapper() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let external_dir = tempdir().expect("external add-on dir");
        let changed_dir = tempdir().expect("changed add-on dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:luma-dgvoodoo-host-binding").expect("game id");
        seed_proxy_game(&context, &game_id, game_dir.path());
        let payload = external_dir.path().join("Luma-Game.addon64");
        std::fs::write(&payload, b"Luma payload").expect("payload");
        write_addon_path(game_dir.path(), external_dir.path());

        let host = game_dir.path().join("D3D11.dll");
        write_compatible_proxy_host_at(game_dir.path(), "D3D11.dll");
        let wrapper = game_dir.path().join("D3D9.dll");
        std::fs::write(&wrapper, b"dgvoodoo wrapper bytes").expect("dgVoodoo wrapper");
        let record = proxy_record(AddonKind::Luma, &game_id, &payload)
            .with_created_file(PathRef::new(host.to_string_lossy()).expect("host path"))
            .with_created_file(PathRef::new(wrapper.to_string_lossy()).expect("wrapper path"))
            .with_tracked_source(TrackedSource::new(
                TrackedSourceRole::HostBinary,
                "https://example.test/ReShade.zip",
                None,
                "reshade-host-digest",
            ))
            .with_tracked_source(TrackedSource::new(
                TrackedSourceRole::DgVoodooWrapper,
                "https://example.test/dgVoodoo2.zip",
                None,
                "dgvoodoo-archive-digest",
            ));
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed Luma receipt with owned wrapper");

        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("host plus wrapper")
                .is_some(),
            "the owned D3D9 support wrapper is not a second ReShade host"
        );

        write_addon_path(game_dir.path(), changed_dir.path());
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("wrong AddonPath")
                .is_none(),
            "a current host whose AddonPath moved away from the payload is inactive"
        );

        write_addon_path(game_dir.path(), external_dir.path());
        let moved_runtime = tempdir().expect("moved runtime root");
        seed_proxy_game(&context, &game_id, moved_runtime.path());
        write_addon_path(moved_runtime.path(), external_dir.path());
        write_compatible_proxy_host_at(moved_runtime.path(), "dinput8.dll");
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("moved runtime root")
                .is_none(),
            "moving the game root cannot make stale recorded DLLs fall through to a foreign host"
        );
        seed_proxy_game(&context, &game_id, game_dir.path());

        std::fs::remove_file(&host).expect("remove recorded host");
        write_compatible_proxy_host(game_dir.path());
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("removed recorded host")
                .is_none(),
            "a removed recorded ReShade host cannot fall through to a compatible dxgi host"
        );
        assert!(
            record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("raw retained owner")
                .is_some(),
            "read-side inactivity preserves cleanup ownership"
        );
    }

    #[test]
    fn proxy_receipts_stay_raw_but_are_inactive_without_host_payload_or_active_game() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let external_dir = tempdir().expect("external addon dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:proxy-absence").expect("game id");
        let game = seed_proxy_game(&context, &game_id, game_dir.path());
        let payload = external_dir.path().join("Luma-Game.addon64");
        std::fs::write(&payload, b"luma payload").expect("payload");
        write_addon_path(game_dir.path(), external_dir.path());
        let record = proxy_record(AddonKind::Luma, &game_id, &payload);
        context
            .storage()
            .upsert_installed_addon(&record)
            .expect("seed raw Luma receipt");

        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("fresh root without host")
                .is_none(),
            "an external payload receipt is inactive when the current root has no host"
        );
        assert!(
            record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("raw receipt query")
                .is_some(),
            "active status must not discard the persisted owner receipt"
        );

        write_compatible_proxy_host(game_dir.path());
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("restored host")
                .is_some()
        );

        let readiness = context
            .storage()
            .catalog_readiness(&game_id)
            .expect("catalog readiness");
        context
            .storage()
            .mark_installation_absent(&game, AuthorityCas::new(readiness.authority_epoch()))
            .expect("mark installation absent");
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("absent active query")
                .is_none()
        );
        assert!(
            active_records_of_kind(&context, AddonKind::Luma)
                .expect("active records")
                .all(|active| active.game_id() != &game_id),
            "absent current registrations stay out of active listings"
        );
        assert!(
            record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("raw receipt survives absence")
                .is_some()
        );
        assert!(
            foreign_record(&context, &game_id, AddonKind::RenoDx)
                .expect("raw foreign owner survives absence")
                .is_some(),
            "inactive status cannot bypass raw cross-tool ownership"
        );

        let readiness = context
            .storage()
            .catalog_readiness(&game_id)
            .expect("absence readiness");
        context
            .storage()
            .save_complete_scan_write_unit(renderpilot_storage_sqlite::CompleteScanWriteUnit {
                game: &game,
                components: &[],
                artifacts: &[],
                observations: &[],
                authority: AuthorityCas::new(readiness.authority_epoch()),
                prune_empty_operations: false,
            })
            .expect("complete scan reactivates game");
        assert!(
            record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("external receipt after collection")
                .is_some(),
            "complete-scan absence collection retains the external owner receipt"
        );
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("reactivated current binding")
                .is_some()
        );

        std::fs::remove_file(&payload).expect("remove current payload");
        assert!(
            active_record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("missing Luma payload")
                .is_none(),
            "Luma is inactive when its current payload is absent"
        );
        assert!(
            record_of_kind(&context, &game_id, AddonKind::Luma)
                .expect("raw Luma receipt after payload removal")
                .is_some()
        );
    }

    #[test]
    fn shared_vulkan_status_keeps_payload_semantics_but_requires_active_registration() {
        let db_dir = tempdir().expect("db dir");
        let game_dir = tempdir().expect("game dir");
        let payload_dir = tempdir().expect("payload dir");
        let context = Context::open_at(db_dir.path().join("catalog.sqlite")).expect("context");
        let active_game_id = GameId::new("manual:active-shared").expect("active game id");
        let active_game = seed_proxy_game(&context, &active_game_id, game_dir.path());
        let active_payload = payload_dir.path().join("active-shared-vulkan.addon64");
        std::fs::write(&active_payload, b"shared payload").expect("active payload");
        let active_record = InstalledAddon::new(
            active_game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(active_payload.to_string_lossy()).expect("shared payload path"),
        )
        .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);
        context
            .storage()
            .upsert_installed_addon(&active_record)
            .expect("seed active Shared Vulkan owner");
        assert!(
            active_record_of_kind(&context, &active_game_id, AddonKind::RenoDx)
                .expect("active Shared Vulkan selector")
                .is_some(),
            "Shared Vulkan uses its current nonempty payload without Proxy-host or AddonPath checks"
        );

        let readiness = context
            .storage()
            .catalog_readiness(&active_game_id)
            .expect("catalog readiness");
        context
            .storage()
            .mark_installation_absent(&active_game, AuthorityCas::new(readiness.authority_epoch()))
            .expect("mark Shared Vulkan registration absent");
        assert!(
            active_record_of_kind(&context, &active_game_id, AddonKind::RenoDx)
                .expect("absent Shared Vulkan selector")
                .is_none()
        );
        assert!(
            record_of_kind(&context, &active_game_id, AddonKind::RenoDx)
                .expect("raw Shared Vulkan owner")
                .is_some(),
            "inactive presentation must retain raw cleanup ownership"
        );

        let unregistered_game_id = GameId::new("manual:unregistered-shared").expect("game id");
        let unregistered_payload = payload_dir
            .path()
            .join("unregistered-shared-vulkan.addon64");
        std::fs::write(&unregistered_payload, b"shared payload").expect("unregistered payload");
        let unregistered_record = InstalledAddon::new(
            unregistered_game_id.clone(),
            AddonKind::RenoDx,
            PathRef::new(unregistered_payload.to_string_lossy()).expect("shared payload path"),
        )
        .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);
        context
            .storage()
            .upsert_installed_addon(&unregistered_record)
            .expect("seed unregistered Shared Vulkan owner");

        assert!(
            active_record_of_kind(&context, &unregistered_game_id, AddonKind::RenoDx)
                .expect("unregistered active selector")
                .is_none(),
            "a surviving payload cannot make a record active without a game registration"
        );
        assert!(
            record_of_kind(&context, &unregistered_game_id, AddonKind::RenoDx)
                .expect("unregistered raw owner")
                .is_some()
        );
    }

    fn seed_proxy_game(
        context: &Context,
        game_id: &GameId,
        root: &std::path::Path,
    ) -> GameInstallation {
        let exe = root.join("Game.exe");
        std::fs::write(
            &exe,
            crate::addons::test_support::build_pe_with_exports(
                crate::addons::test_support::MACHINE_AMD64,
                crate::addons::test_support::PE32_PLUS_MAGIC,
                &[],
            ),
        )
        .expect("write game executable");
        let game = GameInstallation::new(
            GameIdentity::new(game_id.clone(), "Proxy binding test game", Launcher::Manual)
                .expect("game identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(root.to_string_lossy()).expect("game root"),
        )
        .with_executable_candidate(PathRef::new(exe.to_string_lossy()).expect("game exe"));
        context.storage().upsert_game(&game).expect("seed game");
        game
    }

    fn proxy_record(
        kind: AddonKind,
        game_id: &GameId,
        payload: &std::path::Path,
    ) -> InstalledAddon {
        InstalledAddon::new(
            game_id.clone(),
            kind,
            PathRef::new(payload.to_string_lossy()).expect("payload path"),
        )
        .with_host_kind(InstalledAddonHostKind::Proxy)
    }

    fn write_compatible_proxy_host(root: &std::path::Path) {
        write_compatible_proxy_host_at(root, "dxgi.dll");
    }

    fn write_compatible_proxy_host_at(root: &std::path::Path, name: &str) {
        std::fs::write(
            root.join(name),
            crate::addons::test_support::build_pe_with_exports(
                crate::addons::test_support::MACHINE_AMD64,
                crate::addons::test_support::PE32_PLUS_MAGIC,
                &[
                    "ReShadeVersion",
                    "ReShadeRegisterAddon",
                    "ReShadeUnregisterAddon",
                    "ReShadeRegisterEvent",
                ],
            ),
        )
        .expect("write compatible proxy host");
    }

    fn exact_file_receipt(root: &std::path::Path, path: &std::path::Path) -> FileReceipt {
        let root = PathRef::new(root.to_string_lossy()).expect("root path");
        let path = PathRef::new(path.to_string_lossy()).expect("file path");
        let snapshot = crate::peer_mutation_executor::observe_peer_path_snapshot(&path, &root)
            .expect("no-follow fixture snapshot");
        let file = snapshot.file().expect("fixture file");
        FileReceipt::reused(file.identity().to_owned(), file.digest().clone())
            .expect("fixture receipt")
    }

    fn write_addon_path(root: &std::path::Path, addon_dir: &std::path::Path) {
        let addon_dir = addon_dir.to_string_lossy().replace('\\', "/");
        std::fs::write(
            root.join("ReShade.ini"),
            format!("[ADDON]\nAddonPath={addon_dir}\n"),
        )
        .expect("write ReShade AddonPath");
    }

    #[test]
    fn engine_config_release_verification_allows_only_journal_change() {
        let receipt = EngineConfigReceipt {
            schema_version: 1,
            path: r"C:\games\x\Engine.ini".to_owned(),
            file_created: false,
            encoding: "utf8".to_owned(),
            before_digest: "0".repeat(64),
            after_digest: "1".repeat(64),
            recipe_fingerprint: "2".repeat(64),
            contributions: Vec::new(),
            created_headers: Vec::new(),
            created_header_prefixes: Vec::new(),
            created_header_groups: Vec::new(),
            created_header_ordinals: Vec::new(),
        };
        let journal = EngineConfigJournal {
            stable: Some(receipt),
            pending: None,
        };
        let original = addon_record()
            .with_engine_config_journal(Some(journal))
            .expect("journal");
        let released = original
            .clone()
            .with_engine_config_journal(None)
            .expect("release");
        assert!(verify_engine_config_release(&original, released, "RenoDX").is_ok());

        let changed = original.clone().with_addon_version("changed");
        let changed = changed.with_engine_config_journal(None).expect("release");
        assert!(verify_engine_config_release(&original, changed, "RenoDX").is_err());
    }

    fn addon_record() -> InstalledAddon {
        InstalledAddon::new(
            GameId::new("steam:1").expect("game id"),
            AddonKind::RenoDx,
            PathRef::new(r"C:\games\x\addon.dll").expect("path"),
        )
    }

    fn context_with_broken_install_table(
        db_path: &std::path::Path,
    ) -> (Context, tempfile::TempDir) {
        let sentinel_dir = tempdir().expect("sentinel dir");
        let context = Context::open_at(db_path).expect("context");
        rusqlite::Connection::open(db_path)
            .expect("second connection")
            .execute_batch("DROP TABLE installed_addons")
            .expect("drop installed_addons");
        (context, sentinel_dir)
    }

    #[test]
    fn persistence_failure_clears_sentinel_after_complete_revert() {
        let db_dir = tempdir().expect("db dir");
        let db_path = db_dir.path().join("catalog.sqlite");
        let (context, sentinel_dir) = context_with_broken_install_table(&db_path);
        let commit = crate::addons::engine::PendingInstallCommit::begin(
            sentinel_dir.path(),
            AddonKind::RenoDx,
        )
        .expect("commit");

        let error = persist_record_or_revert(&context, addon_record(), commit, |_| Ok(()))
            .expect_err("persistence must fail");

        assert!(matches!(error, ServiceError::StorageFailed(_)));
        assert!(!crate::addons::engine::is_install_torn(
            sentinel_dir.path(),
            AddonKind::RenoDx
        ));
    }

    #[test]
    fn persistence_and_revert_failure_retain_sentinel() {
        let db_dir = tempdir().expect("db dir");
        let db_path = db_dir.path().join("catalog.sqlite");
        let (context, sentinel_dir) = context_with_broken_install_table(&db_path);
        let commit = crate::addons::engine::PendingInstallCommit::begin(
            sentinel_dir.path(),
            AddonKind::RenoDx,
        )
        .expect("commit");

        persist_record_or_revert(&context, addon_record(), commit, |_| {
            Err(ServiceError::command_failed("revert failed"))
        })
        .expect_err("persistence must fail");

        assert!(crate::addons::engine::is_install_torn(
            sentinel_dir.path(),
            AddonKind::RenoDx
        ));
    }

    #[test]
    fn source_with_role_finds_the_matching_tracked_source() {
        let record = addon_record().with_tracked_sources(vec![
            TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                "https://example.test/a".to_owned(),
                None,
                "digest-a".to_owned(),
            ),
            TrackedSource::new(
                TrackedSourceRole::HostBinary,
                "https://example.test/h".to_owned(),
                None,
                "digest-h".to_owned(),
            ),
        ]);

        let found = source_with_role(&record, TrackedSourceRole::HostBinary)
            .expect("host source is present");
        assert_eq!(found.digest(), "digest-h");
        assert!(source_with_role(&record, TrackedSourceRole::DlssFix).is_none());
    }

    #[test]
    fn addon_label_uses_the_file_name() {
        let record = addon_record();
        assert_eq!(addon_label(&record), "addon.dll");
    }
}
