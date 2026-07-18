use std::path::{Path, PathBuf};

use renderpilot_domain::{
    AddonKind, InstalledAddon, InstalledAddonHostKind, TrackedSource, TrackedSourceRole,
};

use crate::ServiceError;
use crate::addons::engine;
use crate::addons::luma::tracking;
use crate::addons::record;
use crate::addons::reshade::types::ReshadeChannel;
use crate::addons::reshade::update::host_binary_source;

use super::super::dgvoodoo::DgVoodooInstall;
use super::PreparedInstall;

pub(crate) struct RecordInstallResult<'a> {
    pub(crate) tracks_host: bool,
    pub(crate) adopted_host_path: Option<&'a Path>,
    pub(crate) adopted_existing: &'a [PathBuf],
    pub(crate) receipt: &'a engine::InstallReceipt,
    pub(crate) managed_file: Option<renderpilot_domain::ManagedAddonFile>,
}

/// Assembles the [`InstalledAddon`] from the engine receipt and the upstream
/// sources to track: the release asset (always), a normal ReShade host entry
/// when this install wrote one, or an advisory nightly entry when it adopted a
/// proved-empty host already on disk.
pub(crate) fn build_record(
    prepared: &PreparedInstall,
    game_dir: &Path,
    addon_dir: &Path,
    install: RecordInstallResult<'_>,
) -> Result<InstalledAddon, ServiceError> {
    let RecordInstallResult {
        tracks_host,
        adopted_host_path,
        adopted_existing,
        receipt,
        managed_file,
    } = install;
    let addon_path = addon_dir.join(&prepared.main_addon_rel);

    let mut sources = vec![
        TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            prepared.asset_source_url.clone(),
            prepared.source_etag.clone(),
            prepared.zip_digest.clone(),
        )
        .with_last_modified(prepared.source_last_modified.clone()),
    ];
    if tracks_host {
        sources.push(host_binary_source(
            prepared.reshade_source_url.clone(),
            prepared.reshade_source_etag.clone(),
            prepared.reshade_digest.clone(),
            prepared.reshade_last_modified.clone(),
            Some(ReshadeChannel::Nightly),
        ));
    } else if let Some(host_path) = adopted_host_path {
        sources.push(tracking::advisory_nightly_host_source(
            host_path,
            prepared.reshade_source_url.clone(),
        )?);
    }
    if let Some(DgVoodooInstall::Managed(dgvoodoo)) = &prepared.dgvoodoo {
        sources.push(dgvoodoo.tracked_source());
    }

    let reused_config = prepared
        .reused_dgvoodoo_config_file()
        .map(|name| game_dir.join(name));
    let managed_path = managed_file
        .as_ref()
        .map(|managed| PathBuf::from(managed.path().as_str()));
    let ignored_created: Vec<&Path> = reused_config
        .as_deref()
        .into_iter()
        .chain(managed_path.as_deref())
        .collect();

    let mut record = record::build_ignoring_created(
        prepared.game_id.clone(),
        AddonKind::Luma,
        &addon_path,
        receipt,
        sources,
        &ignored_created,
    )?
    .with_host_kind(InstalledAddonHostKind::Proxy);
    record = record::adopt_existing_paths(record, adopted_existing)?;
    if let Some(label) = &prepared.build_label {
        record = record.with_addon_version(label.clone());
    }
    // A freshly written host and a proved-empty adopted host are both covered
    // by Luma's nightly-only contract. A reused foreign host remains unlabelled
    // rather than guessing its channel.
    if tracks_host || adopted_host_path.is_some() {
        record = record.with_reshade_channel(ReshadeChannel::Nightly.as_str());
    }
    // Host slot path for this install. When we wrote or adopted the host it
    // must appear in `created_files` so uninstall removes it — never leave a
    // ReShade proxy behind that then blocks RenoDX (InactiveSlot / conflict).
    let host_slot = adopted_host_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| game_dir.join(&prepared.proxy_dll_name));
    if tracks_host || adopted_host_path.is_some() {
        record = record::adopt_existing_paths(record, std::slice::from_ref(&host_slot))?;
    }
    if let Some(managed_file) = managed_file {
        record = record
            .try_with_managed_files(vec![managed_file])
            .map_err(|error| crate::addons::luma::errors::failed(error.to_string()))?;
    }
    Ok(record)
}
