//! Orchestrates payload + optional dgVoodoo + ReShade host into a PreparedInstall.

use renderpilot_domain::GameId;

use crate::ServiceError;
use crate::addons::progress::sequential_stage_observer;
use crate::addons::reshade::fetch::fetch_reshade_from_source;
use crate::addons::reshade::source::require_reshade_source;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeSourceCatalog};
use crate::net::ProgressObserver;

use super::super::dgvoodoo;
use super::super::install::PreparedInstall;
use super::super::matcher::ResolvedLumaInstall;
use super::super::source;
use super::download::fetch_luma_payload;

/// Fetches everything needed to install `resolved` into the game folder: always
/// the release asset, and -- only when the shared host policy decided the
/// install must write one -- the **nightly** ReShade host (Luma has no stable
/// channel to pick from).
pub(crate) async fn prepare_install(
    resolved: &ResolvedLumaInstall,
    reshade_sources: &ReshadeSourceCatalog,
    game_id: GameId,
    writes_host: bool,
    dgvoodoo_preparation: Option<dgvoodoo::DgVoodooPreparation<'_>>,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<PreparedInstall, ServiceError> {
    let downloads_dgvoodoo = matches!(
        dgvoodoo_preparation,
        Some(dgvoodoo::DgVoodooPreparation::Managed(_))
    );
    let stage_count = 1 + u64::from(downloads_dgvoodoo) + u64::from(writes_host);

    let payload_progress_fn = sequential_stage_observer(progress, 0, stage_count);
    let payload_progress = payload_progress_fn
        .as_ref()
        .map(|observer| observer as &ProgressObserver<'_>);
    let payload = fetch_luma_payload(
        &resolved.asset,
        &resolved.addon_file,
        resolved.arch,
        payload_progress,
    )
    .await?;

    let dgvoodoo = match dgvoodoo_preparation {
        Some(dgvoodoo::DgVoodooPreparation::Managed(requirement)) => {
            let dgvoodoo_progress_fn = sequential_stage_observer(progress, 1, stage_count);
            let dgvoodoo_progress = dgvoodoo_progress_fn
                .as_ref()
                .map(|observer| observer as &ProgressObserver<'_>);
            let prepared = dgvoodoo::fetch(requirement, dgvoodoo_progress).await?;
            Some(dgvoodoo::DgVoodooInstall::Managed(prepared))
        }
        Some(dgvoodoo::DgVoodooPreparation::Reused(reused)) => {
            Some(dgvoodoo::DgVoodooInstall::Reused(reused))
        }
        Some(dgvoodoo::DgVoodooPreparation::Adopted(adopted)) => {
            Some(dgvoodoo::DgVoodooInstall::Adopted(adopted))
        }
        None => None,
    };

    let host_source =
        require_reshade_source(reshade_sources, ReshadeChannel::Nightly, resolved.arch)?;
    let host = if writes_host {
        let host_stage = 1 + u64::from(downloads_dgvoodoo);
        let host_progress_fn = sequential_stage_observer(progress, host_stage, stage_count);
        let host_progress = host_progress_fn
            .as_ref()
            .map(|observer| observer as &ProgressObserver<'_>);
        let download =
            fetch_reshade_from_source(&host_source, resolved.arch, host_progress).await?;
        FetchedHost {
            bytes: download.bytes,
            source_url: host_source.url,
            etag: download.etag,
            last_modified: download.last_modified,
            digest: download.digest,
        }
    } else {
        FetchedHost::not_downloaded(host_source.url)
    };

    Ok(PreparedInstall {
        game_id,
        proxy_dll_name: resolved.proxy_dll_name.clone(),
        payload: payload.files,
        main_addon_rel: payload.main_addon_rel,
        asset_source_url: source::asset_url(&resolved.asset),
        zip_digest: payload.zip_digest,
        source_etag: payload.etag,
        source_last_modified: payload.last_modified,
        build_label: payload.build_number.map(source::build_label),
        reshade_dll_bytes: host.bytes,
        reshade_source_url: host.source_url,
        reshade_source_etag: host.etag,
        reshade_last_modified: host.last_modified,
        reshade_digest: host.digest,
        dgvoodoo,
    })
}

/// The ReShade host DLL bytes plus the upstream identity to track it for
/// updates. When a compatible empty host is adopted, only its known nightly
/// URL is retained; its digest is read from the exact on-disk DLL during record
/// construction rather than fabricated from a download that did not happen.
struct FetchedHost {
    bytes: Vec<u8>,
    source_url: String,
    etag: Option<String>,
    last_modified: Option<String>,
    digest: String,
}

impl FetchedHost {
    fn not_downloaded(source_url: String) -> Self {
        Self {
            bytes: Vec::new(),
            source_url,
            etag: None,
            last_modified: None,
            digest: String::new(),
        }
    }
}
