//! Kind-aware access to the single `installed_addons` row a game can have.
//!
//! `InstalledAddonRepository::get_installed_addon` returns whichever record is on
//! file for a game with **no kind filter** — a caller that assumes any record it
//! gets back is "its own" tool's would misread a foreign-tool record (or a stale
//! test fixture) as its own install. Every addon tool must read through
//! [`record_of_kind`] (or [`foreign_record`] when it specifically wants the
//! opposite) rather than calling the repository directly.

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{AddonKind, GameId, InstalledAddon, TrackedSource, TrackedSourceRole};

use crate::{Context, ServiceError};

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

/// A cosmetic fetch/log label for an add-on (the file name identifies the title).
pub(crate) fn addon_label(record: &InstalledAddon) -> &str {
    std::path::Path::new(record.addon_file().as_str())
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("add-on")
}

/// Persists the install record, calling the provided revert closure (which receives
/// a reference to the record) if persistence fails. This ensures an install never
/// survives without a reversible record. Double-faults are logged.
pub(crate) fn persist_record_or_revert(
    context: &Context,
    record: InstalledAddon,
    revert: impl FnOnce(&InstalledAddon) -> Result<(), ServiceError>,
) -> Result<InstalledAddon, ServiceError> {
    use renderpilot_application::InstalledAddonRepository;
    if let Err(error) = context.storage().upsert_installed_addon(&record) {
        if let Err(revert_error) = revert(&record) {
            log::warn!(
                "addon install: record persistence failed and the filesystem revert also failed: {revert_error}"
            );
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
    fn record_of_kind_returns_a_matching_record() {
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
