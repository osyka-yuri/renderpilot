//! Kind-aware access to the single `installed_addons` row a game can have.
//!
//! `InstalledAddonRepository::get_installed_addon` returns whichever record is on
//! file for a game with **no kind filter** — a caller that assumes any record it
//! gets back is "its own" tool's would misread a foreign-tool record (or a stale
//! test fixture) as its own install. Every addon tool must read through
//! [`record_of_kind`] (or [`foreign_record`] when it specifically wants the
//! opposite) rather than calling the repository directly.

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
pub(crate) fn persist_record_or_revert(
    context: &Context,
    record: InstalledAddon,
    revert: impl FnOnce(&InstalledAddon) -> Result<(), ServiceError>,
) -> Result<InstalledAddon, ServiceError> {
    if let Err(error) = context.storage().upsert_installed_addon(&record) {
        if let Err(revert_error) = revert(&record) {
            log::warn!("addon install: persistence failed and revert failed: {revert_error}");
        }
        return Err(error.into());
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{PathRef, TrackedSource};
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
                        log::warn!(
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
