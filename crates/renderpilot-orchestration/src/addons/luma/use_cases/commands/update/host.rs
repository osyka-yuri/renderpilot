//! ReShade host preparation and apply steps. Network work and validation happen
//! before the update transaction opens its sentinel; `apply` is disk-only.
//!
//! Durable crash-recovery is **not** owned here: the outer Luma update command
//! (`super::mod`) opens a [`crate::file_mutation::DurableFileTransaction`], includes
//! any host write path in the mutation snapshot, and finishes with
//! `commit_or_rollback`. This module only performs the local host/DLL write via
//! `apply_replacements_with_outcome` (which rolls back that write on failure) and
//! persists catalog state under the outer transaction's `mutation_id`.
//!
//! [`crate::addons::luma::use_cases::update_target`] resolves the current update
//! target and assesses host compatibility (shared with
//! `use_cases::queries::updates`); this module applies the write during the
//! update command and persists the host-only pass.

use std::path::PathBuf;

use renderpilot_domain::{InstalledAddon, TrackedSource, TrackedSourceRole};

use super::record_rebuild::path_refs_contain;
use crate::addons::file_update::{
    OriginalFile, Replacement, ReplacementFailure, apply_replacements_with_outcome,
};
use crate::addons::luma::errors;
use crate::addons::luma::tracking;
use crate::addons::luma::types::LumaManifest;
use crate::addons::luma::use_cases::update_target::{
    self, ResolvedUpdateTarget, host_needs_write, host_rewrite_allowed,
};
use crate::addons::reshade::fetch::fetch_reshade_from_source;
use crate::addons::reshade::fetch::sha256_hex;
use crate::addons::reshade::source::require_reshade_source;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

/// The result of assessing (and, if needed, applying) the ReShade host update
/// for one pass. `host_path` is present only for a host artifact RenderPilot
/// already tracks, so rebuilding the record cannot acquire removal rights over
/// a reused user runtime.
pub(super) struct HostUpdateOutcome {
    pub(super) source: Option<TrackedSource>,
    pub(super) host_path: Option<PathBuf>,
    pub(super) originals: Vec<OriginalFile>,
}

/// Host update prepared without changing the game folder.
pub(super) struct PreparedHostUpdate {
    source: Option<TrackedSource>,
    host_path: Option<PathBuf>,
    replacement: Option<Replacement>,
}

impl PreparedHostUpdate {
    /// Nothing needs writing: carry over whatever the record already tracked.
    pub(super) fn unchanged(record: &InstalledAddon) -> Self {
        Self {
            source: record
                .tracked_sources()
                .iter()
                .find(|source| source.role() == TrackedSourceRole::HostBinary)
                .cloned(),
            host_path: crate::addons::tracking::owned_proxy_host_path(record),
            replacement: None,
        }
    }

    /// Whether this prepared plan will write a host DLL on apply.
    #[must_use]
    pub(super) fn writes(&self) -> bool {
        self.replacement.is_some()
    }

    /// Absolute path the replacement will write, when any.
    #[must_use]
    pub(super) fn write_path(&self) -> Option<&std::path::Path> {
        self.replacement
            .as_ref()
            .map(|replacement| replacement.path.as_path())
    }

    /// Applies the already-downloaded replacement, if any.
    pub(super) fn apply(self) -> Result<HostUpdateOutcome, ReplacementFailure> {
        let originals = match self.replacement {
            Some(replacement) => apply_replacements_with_outcome(vec![replacement])?,
            None => Vec::new(),
        };
        Ok(HostUpdateOutcome {
            source: self.source,
            host_path: self.host_path,
            originals,
        })
    }
}

/// Revalidates Luma's owned nightly host and, when its exact current DLL differs
/// from upstream or no longer meets the minimum version, replaces it in place.
///
/// Owned hosts that are **missing on disk** (`InstallNew`) are rewritten the
/// same way as empty under-min hosts (`RepairEmpty`). Adopted empty hosts are
/// only rewritten when the nightly digest differs.
///
/// A managed Luma record that never tracked a host path (payload-only orphan
/// adoption) may still gain a host when the proxy slot is empty
/// (`InstallNew` / `RepairEmpty`) and there is no conflict — so update can
/// self-heal without requiring uninstall+reinstall. Reused user runtimes and
/// content-bearing conflicts are never rewritten.
pub(super) async fn prepare_host_update_if_needed(
    manifest: &LumaManifest,
    reshade_sources: &ReshadeSourceCatalog,
    target: &ResolvedUpdateTarget,
    record: &InstalledAddon,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedHostUpdate, ServiceError> {
    let host_path = target.game_dir.join(&target.proxy_dll_name);
    let owns_host = tracking::owns_path(record, &host_path);
    let Ok(min_version) = manifest.min_reshade_version_parsed() else {
        return Ok(PreparedHostUpdate::unchanged(record));
    };
    let Some(assessment) = update_target::assess_host_for_update(target, &min_version) else {
        return Ok(PreparedHostUpdate::unchanged(record));
    };
    if !host_rewrite_allowed(owns_host, assessment.conflict, assessment.lifecycle) {
        return Ok(PreparedHostUpdate::unchanged(record));
    }

    let (entry, replacement) = prepare_host_replacement(reshade_sources, target, progress).await?;
    let current = if host_path.is_file() {
        match std::fs::read(&host_path) {
            Ok(bytes) => Some(sha256_hex(&bytes)),
            // Present but unreadable (sharing violation while the game is
            // running, permissions, etc.): surface the I/O error so Update/
            // Repair cannot report success while leaving a stale host in place.
            Err(error) => {
                return Err(errors::io(
                    "read host for update (close the game if it is running and retry)",
                    &host_path,
                    &error,
                ));
            }
        }
    } else {
        // Missing owned (or empty-slot untracked) host must be (re)installed.
        None
    };
    if !host_needs_write(assessment.writes_host(), current.as_deref(), entry.digest()) {
        return Ok(PreparedHostUpdate::unchanged(record));
    }
    let host_path = replacement.path.clone();
    Ok(PreparedHostUpdate {
        source: Some(entry),
        host_path: Some(host_path),
        replacement: Some(replacement),
    })
}

async fn prepare_host_replacement(
    reshade_sources: &ReshadeSourceCatalog,
    target: &ResolvedUpdateTarget,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<(TrackedSource, Replacement), ServiceError> {
    let host_source =
        require_reshade_source(reshade_sources, ReshadeChannel::Nightly, target.arch)?;
    let download = fetch_reshade_from_source(&host_source, target.arch, progress).await?;
    let entry = crate::addons::reshade::update::host_binary_source(
        host_source.url,
        download.etag,
        download.digest,
        download.last_modified,
        Some(ReshadeChannel::Nightly),
    );
    let replacement = Replacement {
        path: target.game_dir.join(&target.proxy_dll_name),
        bytes: download.bytes,
        mtime: None,
    };
    Ok((entry, replacement))
}

/// Persists a host-only pass: the refreshed sources, and (A.3) `host_path`
/// folded into `created_files` when this update wrote an already tracked host.
///
/// `addon_version` is the label prepared upstream (often newly bound from a
/// validated ZIP during advisory promotion). Callers must not silently clear a
/// prior version unless the prepare path intentionally resolved `None`.
///
/// `mutation_id` is always the outer durable game-file transaction id; host-only
/// persistence never invents or omits a mutation.
pub(super) fn persist_host_only_result(
    context: &Context,
    record: &InstalledAddon,
    sources: Vec<TrackedSource>,
    host_path: Option<PathBuf>,
    dgvoodoo_receipt: &crate::addons::engine::InstallReceipt,
    addon_version: Option<String>,
    mutation_id: &str,
) -> Result<(), ServiceError> {
    let mut created_files = record.created_files().to_vec();
    if let Some(host_path) = host_path
        && !path_refs_contain(&created_files, &host_path)
    {
        created_files.push(crate::addons::record::to_path_ref(&host_path)?);
    }
    for path in &dgvoodoo_receipt.created_files {
        if !path_refs_contain(&created_files, path) {
            created_files.push(crate::addons::record::to_path_ref(path)?);
        }
    }
    let mut backed_up_files = record.backed_up_files().to_vec();
    for path in &dgvoodoo_receipt.backed_up_files {
        if !path_refs_contain(&backed_up_files, path) {
            backed_up_files.push(crate::addons::record::to_path_ref(path)?);
        }
    }
    let refreshed = tracking::rebuild(
        record,
        crate::addons::tracking::RebuildParts {
            addon_file: record.addon_file().clone(),
            addon_version: crate::addons::tracking::AddonVersionUpdate::Set(addon_version),
            managed_files: crate::addons::tracking::ManagedFilesUpdate::Keep,
            created_files,
            backed_up_files,
            tracked_sources: sources,
            label: "Luma host-only update rebuild".to_owned(),
        },
    )?;
    context
        .storage()
        .commit_game_mutation(renderpilot_storage_sqlite::GameMutationCommit {
            game_id: record.game_id(),
            component_set: None,
            baseline_mutations: &[],
            addon: renderpilot_storage_sqlite::InstalledAddonMutation::Upsert(&refreshed),
            mutation_id: Some(mutation_id),
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{AddonKind, Architecture, GameId};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn host_update_outcome_unchanged_carries_over_existing_host_source() {
        let dir = tempdir().expect("tempdir");
        let addon = dir.path().join("Luma-Game.addon");
        let host = dir.path().join("dxgi.dll");
        let record = InstalledAddon::from_parts(
            GameId::new("steam:403640").expect("id"),
            AddonKind::Luma,
            renderpilot_domain::PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
            None,
            vec![
                renderpilot_domain::PathRef::new(addon.to_string_lossy().into_owned())
                    .expect("path"),
                renderpilot_domain::PathRef::new(host.to_string_lossy().into_owned())
                    .expect("path"),
            ],
            Vec::new(),
            vec![TrackedSource::new(
                TrackedSourceRole::HostBinary,
                "https://example/host.zip",
                None,
                "host-digest",
            )],
        )
        .expect("record");

        let outcome = PreparedHostUpdate::unchanged(&record)
            .apply()
            .expect("unchanged apply");

        assert_eq!(outcome.host_path.as_deref(), Some(host.as_path()));
        assert_eq!(
            outcome.source.expect("host source carried over").digest(),
            "host-digest"
        );
        assert!(outcome.originals.is_empty());
    }

    #[test]
    fn host_apply_reports_a_complete_rollback_when_its_prepared_write_fails() {
        let dir = tempdir().expect("tempdir");
        let invalid_host_slot = dir.path().join("dxgi.dll");
        std::fs::create_dir(&invalid_host_slot).expect("host directory");
        let prepared = PreparedHostUpdate {
            source: None,
            host_path: Some(invalid_host_slot.clone()),
            replacement: Some(Replacement {
                path: invalid_host_slot.clone(),
                bytes: b"host".to_vec(),
                mtime: None,
            }),
        };

        let failure = match prepared.apply() {
            Ok(_) => panic!("directory slot cannot be replaced"),
            Err(failure) => failure,
        };

        assert!(failure.rollback_complete);
        assert!(invalid_host_slot.is_dir());
    }

    #[tokio::test]
    async fn reused_user_host_is_never_adopted_by_an_update() {
        // Content-bearing foreign ReShade (user runtime + foreign addon) must
        // never be rewritten — even when the Luma record does not track the host.
        let dir = tempdir().expect("tempdir");
        let addon = dir.path().join("Luma-Game.addon");
        // Recognized custom build short-circuits assess_host_for_update to None
        // without network work (same offline pattern as other update tests).
        std::fs::write(dir.path().join("dxgi.dll"), b"gshade-proxy-stub").expect("proxy");
        std::fs::write(dir.path().join("GShade64.dll"), b"gshade-runtime").expect("gshade");
        let record = InstalledAddon::new(
            GameId::new("steam:403641").expect("id"),
            AddonKind::Luma,
            renderpilot_domain::PathRef::new(addon.to_string_lossy().into_owned()).expect("path"),
        );
        let target = ResolvedUpdateTarget {
            game_dir: dir.path().to_path_buf(),
            asset: "Luma-Game.zip".to_owned(),
            addon_file: "Luma-Game.addon".to_owned(),
            arch: Architecture::X64,
            proxy_dll_name: "dxgi.dll".to_owned(),
            external_requirement: None,
        };

        let outcome = prepare_host_update_if_needed(
            &crate::addons::luma::test_support::manifest(Vec::new()),
            &crate::addons::luma::test_support::reshade_sources(),
            &target,
            &record,
            None,
        )
        .await
        .expect("foreign/custom host is skipped before any network work")
        .apply()
        .expect("unchanged apply");

        assert!(outcome.source.is_none());
        assert!(outcome.host_path.is_none());
        assert!(outcome.originals.is_empty());
    }
}
