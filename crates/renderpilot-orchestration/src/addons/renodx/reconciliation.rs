use std::path::{Path, PathBuf};

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{
    AddonKind, Architecture, GameId, InstalledAddon, InstalledAddonHostKind, PathRef,
    SharedArtifactOrigin, TrackedSource, TrackedSourceRole,
};

use crate::addons::operation_lock;
use crate::addons::records;
use crate::{Context, ServiceError};

use super::{errors, install, source, vulkan};
use crate::addons::reshade::host_policy::{self, HostLifecycle};
use crate::addons::reshade::scan as reshade;
use crate::addons::reshade::source::reshade_source;
use crate::addons::reshade::types::{ReshadeChannel, ReshadeConfig};

/// An on-disk RenoDX install found while no per-game DB row exists.
#[derive(Debug, Clone)]
pub(crate) struct OrphanedInstall {
    pub(crate) game_id: GameId,
    pub(crate) game_dir: PathBuf,
    pub(crate) addon_file: PathBuf,
    /// Detected runtime file, when one is actually present. A DB-loss recovery
    /// may legitimately find only the add-on payload; absent runtime files are
    /// never invented or claimed.
    pub(crate) host_file: Option<PathBuf>,
    pub(crate) host_kind: InstalledAddonHostKind,
    pub(crate) registered_exe_path: Option<PathBuf>,
    pub(crate) reshade_config: ReshadeConfig,
    pub(crate) game_arch: Option<Architecture>,
    pub(crate) addon_url: Option<String>,
}

/// Adopts an on-disk RenoDX install into the per-game DB if the row is still
/// missing. The returned record is re-read from storage so DB timestamps are
/// present and the caller reports the same state future queries will see.
///
/// This runs on the availability read path, which is reachable from an async
/// Tauri command and must never block the runtime thread. Adoption is an
/// opportunistic, idempotent convenience — not required for correctness — so
/// it uses a non-blocking lock attempt: if the game is already locked by a
/// concurrent install/update/uninstall, this call returns `Ok(None)` instead
/// of waiting, and the next `availability()` call retries.
///
/// A record already present for a **different** addon kind is never
/// adopted over: this returns `Ok(None)` exactly as if there were nothing to
/// adopt, so a foreign-tool install is never silently overwritten. The
/// availability caller already gates this whole path behind its own
/// exclusivity check; this is defense in depth for any other caller.
pub(crate) fn reconcile_orphaned_install(
    context: &Context,
    candidate: &OrphanedInstall,
) -> Result<Option<InstalledAddon>, ServiceError> {
    let Some(_guard) = operation_lock::try_lock(&candidate.game_id) else {
        return Ok(None);
    };
    if records::foreign_record(context, &candidate.game_id, AddonKind::RenoDx)?.is_some() {
        return Ok(None);
    }
    if let Some(record) = records::record_of_kind(context, &candidate.game_id, AddonKind::RenoDx)? {
        return Ok(Some(record));
    }

    let record = build_adopted_record(candidate)?;
    context.storage().upsert_installed_addon(&record)?;

    if matches!(
        candidate.host_kind,
        InstalledAddonHostKind::SharedVulkanLayer
    ) && candidate.host_file.is_some()
    {
        record_shared_vulkan_layer_best_effort(context);
    }

    let record = records::record_of_kind(context, &candidate.game_id, AddonKind::RenoDx)?
        .ok_or_else(|| errors::failed("adopted RenoDX install was not persisted".to_owned()))?;
    Ok(Some(record))
}

fn build_adopted_record(candidate: &OrphanedInstall) -> Result<InstalledAddon, ServiceError> {
    let mut record = InstalledAddon::new(
        candidate.game_id.clone(),
        AddonKind::RenoDx,
        path_ref("add-on", &candidate.addon_file)?,
    )
    .with_host_kind(candidate.host_kind);

    let adopts_proxy_runtime = matches!(candidate.host_kind, InstalledAddonHostKind::Proxy)
        && candidate
            .host_file
            .as_deref()
            .is_some_and(|host_file| may_adopt_proxy_runtime(candidate, host_file));
    record = attach_advisory_provenance(record, candidate, adopts_proxy_runtime);

    match candidate.host_kind {
        InstalledAddonHostKind::Proxy => {
            if let Some(host_file) = candidate.host_file.as_deref()
                && adopts_proxy_runtime
            {
                record = with_created_path(record, host_file)?;
                let paths = reshade::resolve_paths(&candidate.game_dir, Some(host_file));
                if let Some(ini_path) = paths.ini_path.filter(|path| path.is_file()) {
                    record = with_created_path(record, &ini_path)?;
                }
            }
        }
        InstalledAddonHostKind::SharedVulkanLayer => {
            let exe_path = candidate.registered_exe_path.as_deref().ok_or_else(|| {
                errors::invalid(
                    "cannot adopt a Vulkan RenoDX install without a resolved executable".to_owned(),
                )
            })?;
            record = record.with_registered_exe_path(path_ref("registered executable", exe_path)?);
        }
    }

    Ok(record)
}

/// Attaches best-effort advisory provenance to a freshly adopted record: the
/// guessed ReShade channel (from the host file's PE identity strings) for
/// every host kind, a tracked `HostBinary` source for Proxy installs only, a
/// tracked `AddonPayload` source for the main add-on, and, for every host
/// kind, a tracked `DlssFix` source when a DLSS-Fix companion file is
/// physically present alongside the main add-on. A Vulkan layer's real host
/// provenance lives in the shared-artifact table (see
/// [`record_shared_vulkan_layer_best_effort`]), so only the channel guess is
/// useful for its host binary here; DLSS-Fix is unaffected by host kind since
/// it is always a per-game file next to the main add-on. Every step degrades
/// gracefully: a file that cannot be inspected or hashed, or a channel/URL
/// that cannot be resolved, just leaves the record without that piece of
/// provenance. A recognized custom build (see
/// [`reshade::is_known_custom_build`], e.g. GShade) gets neither a channel
/// guess nor a `HostBinary` source at all — RenoDX has no business guessing at
/// a build it doesn't own the update path for.
fn attach_advisory_provenance(
    mut record: InstalledAddon,
    candidate: &OrphanedInstall,
    owns_proxy_runtime: bool,
) -> InstalledAddon {
    let may_describe_host =
        candidate.host_kind == InstalledAddonHostKind::SharedVulkanLayer || owns_proxy_runtime;
    if may_describe_host {
        match candidate
            .host_file
            .as_deref()
            .and_then(renderpilot_detection::inspect_pe)
        {
            Some(pe) if reshade::is_known_custom_build(&candidate.game_dir, Some(&pe.identity)) => {
            }
            Some(pe) => {
                let channel = reshade::guess_advisory_channel(&pe.identity);
                record = record.with_reshade_channel(channel.as_str());

                if candidate.host_kind == InstalledAddonHostKind::Proxy
                    && let Some(source) = build_advisory_host_source(candidate, channel)
                {
                    record = record.with_tracked_source(source);
                }
            }
            None => {
                if let Some(host_file) = candidate.host_file.as_deref() {
                    log::debug!(
                        "Failed to inspect PE for adopted file: {}",
                        host_file.display()
                    );
                }
            }
        }
    }

    if let Some(source) = build_advisory_addon_source(candidate) {
        record = record.with_tracked_source(source);
    }

    if let Some(source) = build_advisory_dlss_fix_source(candidate) {
        record = record.with_tracked_source(source);
    }

    record
}

/// Best-effort advisory `HostBinary` source for an adopted Proxy install: the
/// manifest URL for the guessed channel/architecture, and the on-disk file's
/// digest. `None` if the manifest has no URL for that channel/architecture or
/// the file cannot be hashed.
fn build_advisory_host_source(
    candidate: &OrphanedInstall,
    channel: ReshadeChannel,
) -> Option<TrackedSource> {
    let host_file = candidate.host_file.as_deref()?;
    let arch = candidate.game_arch.unwrap_or(Architecture::X64);
    let url = reshade_source(&candidate.reshade_config, channel, arch)?.url;

    let digest = match renderpilot_detection::sha256_file(host_file) {
        Ok(digest) => digest.to_string(),
        Err(error) => {
            log::debug!(
                "Failed to hash adopted file {}: {error}",
                host_file.display()
            );
            return None;
        }
    };

    Some(
        TrackedSource::new(TrackedSourceRole::HostBinary, url, None, digest)
            .with_channel(channel.as_str())
            .with_advisory(),
    )
}

/// Best-effort advisory `AddonPayload` source for an adopted install's add-on
/// file. `None` if the resolved add-on URL is unknown/empty or the file
/// cannot be hashed.
fn build_advisory_addon_source(candidate: &OrphanedInstall) -> Option<TrackedSource> {
    let url = candidate
        .addon_url
        .as_deref()
        .filter(|url| !url.is_empty())?;

    let digest = match renderpilot_detection::sha256_file(&candidate.addon_file) {
        Ok(digest) => digest.to_string(),
        Err(error) => {
            log::debug!(
                "Failed to hash adopted add-on {}: {error}",
                candidate.addon_file.display()
            );
            return None;
        }
    };

    Some(
        TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            url.to_owned(),
            None,
            digest,
        )
        .with_advisory(),
    )
}

/// Best-effort advisory `DlssFix` source for an adopted install's DLSS-Fix
/// companion add-on, when one is physically present alongside the main add-on.
/// Host-kind-agnostic — unlike the Vulkan-shared `HostBinary` case, DLSS-Fix is
/// always a per-game file next to the main add-on regardless of host kind.
/// `None` if no DLSS-Fix file exists at the expected co-located path or it
/// cannot be hashed.
fn build_advisory_dlss_fix_source(candidate: &OrphanedInstall) -> Option<TrackedSource> {
    let addon_dir = candidate.addon_file.parent()?;
    let arch = candidate.game_arch.unwrap_or(Architecture::X64);
    let dlss_fix_path = addon_dir.join(install::dlss_fix_file_name(arch));
    if !dlss_fix_path.is_file() {
        return None;
    }

    let digest = match renderpilot_detection::sha256_file(&dlss_fix_path) {
        Ok(digest) => digest.to_string(),
        Err(error) => {
            log::debug!(
                "Failed to hash adopted DLSS-Fix file {}: {error}",
                dlss_fix_path.display()
            );
            return None;
        }
    };

    Some(
        TrackedSource::new(
            TrackedSourceRole::DlssFix,
            source::dlss_fix_url(arch),
            None,
            digest,
        )
        .with_advisory(),
    )
}

fn may_adopt_proxy_runtime(candidate: &OrphanedInstall, host_file: &Path) -> bool {
    let Some(proxy_dll_name) = host_file.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let mut allowed = vec![
        candidate
            .addon_file
            .file_name()
            .and_then(|name| name.to_str()),
    ];
    let dlss_fix = install::dlss_fix_file_name(candidate.game_arch.unwrap_or(Architecture::X64));
    if candidate
        .addon_file
        .parent()
        .is_some_and(|dir| dir.join(&dlss_fix).is_file())
    {
        allowed.push(Some(dlss_fix.as_str()));
    }
    let allowed: Vec<&str> = allowed.into_iter().flatten().collect();
    let assessment = host_policy::assess_for_tool_with_allowed_addons(
        &candidate.game_dir,
        proxy_dll_name,
        "RenoDX",
        None,
        &allowed,
    );
    assessment.lifecycle == HostLifecycle::AdoptEmpty
        && reshade::same_path(&assessment.target_path, host_file)
}

fn with_created_path(
    mut record: InstalledAddon,
    path: &Path,
) -> Result<InstalledAddon, ServiceError> {
    let path = path_ref("created file", path)?;
    if !record.created_files().contains(&path) {
        record = record.with_created_file(path);
    }
    Ok(record)
}

fn path_ref(label: &str, path: &Path) -> Result<PathRef, ServiceError> {
    PathRef::new(path.to_string_lossy().into_owned())
        .map_err(|error| errors::failed(format!("invalid adopted {label} path: {error}")))
}

/// Persists an advisory record of the adopted shared Vulkan layer. Best-effort
/// by design: this is a read-path convenience (see [`reconcile_orphaned_install`]),
/// so a failure here — including losing the race for the shared-layer lock —
/// only means the next `availability()` call re-attempts the same adoption;
/// it never blocks or fails the read the caller actually asked for.
fn record_shared_vulkan_layer_best_effort(context: &Context) {
    let Some(_guard) = operation_lock::try_shared_vulkan_lock() else {
        log::warn!("skipped persisting adopted Vulkan layer record: layer lock is busy");
        return;
    };
    if let Err(error) = vulkan::record_detected_layer(
        context.storage(),
        SharedArtifactOrigin::AdoptedOfficial,
        None,
    ) {
        log::warn!("failed to persist adopted Vulkan layer record: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::test_support::{self, MACHINE_AMD64, PE32_PLUS_MAGIC};
    use renderpilot_domain::InstalledAddonHostKind;
    use tempfile::tempdir;

    fn context() -> (tempfile::TempDir, Context) {
        let dir = tempdir().expect("tempdir");
        let context = Context::open_at(dir.path().join("catalog.sqlite")).expect("context");
        (dir, context)
    }

    fn write_file(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create parent");
        }
        std::fs::write(path, bytes).expect("write file");
    }

    fn full_reshade_host() -> Vec<u8> {
        test_support::build_pe_with_exports(
            MACHINE_AMD64,
            PE32_PLUS_MAGIC,
            &[
                "ReShadeVersion",
                "ReShadeRegisterAddon",
                "ReShadeUnregisterAddon",
                "ReShadeRegisterEvent",
                "ReShadeUnregisterEvent",
            ],
        )
    }

    /// A distinct game ID per test — the per-game operation lock is a global
    /// `static`, so tests sharing one ID would contend for the same lock when
    /// `cargo test` runs them in parallel.
    fn game_id(appid: &str) -> GameId {
        GameId::new(format!("steam:{appid}")).expect("game id")
    }

    fn proxy_candidate(game_dir: &Path, appid: &str) -> OrphanedInstall {
        OrphanedInstall {
            game_id: game_id(appid),
            game_dir: game_dir.to_path_buf(),
            addon_file: game_dir.join("renodx-cp2077.addon64"),
            host_file: Some(game_dir.join("dxgi.dll")),
            host_kind: InstalledAddonHostKind::Proxy,
            registered_exe_path: None,
            reshade_config: test_support::manifest(Vec::new()).reshade,
            game_arch: None,
            addon_url: None,
        }
    }

    fn created_names(record: &InstalledAddon) -> Vec<String> {
        record
            .created_files()
            .iter()
            .filter_map(PathRef::file_name)
            .map(str::to_owned)
            .collect()
    }

    fn backed_names(record: &InstalledAddon) -> Vec<String> {
        record
            .backed_up_files()
            .iter()
            .filter_map(PathRef::file_name)
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn proxy_adoption_rereads_timestamps_and_claims_minimal_renderpilot_files() {
        let (_db_dir, context) = context();
        let game_dir = tempdir().expect("game dir");
        write_file(&game_dir.path().join("renodx-cp2077.addon64"), b"addon");
        write_file(&game_dir.path().join("dxgi.dll"), &full_reshade_host());
        write_file(&game_dir.path().join("dxgi.dll.bak"), b"original");
        write_file(
            &game_dir.path().join("ReShade.ini"),
            b"[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
        );
        write_file(&game_dir.path().join("ReShade.ini.bak"), b"original ini");

        let record =
            reconcile_orphaned_install(&context, &proxy_candidate(game_dir.path(), "1938090"))
                .expect("adopt")
                .expect("adopted record");

        assert_eq!(record.host_kind(), Some(InstalledAddonHostKind::Proxy));
        assert_eq!(record.addon_version(), None);
        assert!(record.installed_at().is_some());
        assert!(record.updated_at().is_some());
        assert_eq!(
            created_names(&record),
            vec!["renodx-cp2077.addon64", "dxgi.dll", "ReShade.ini"]
        );
        assert!(backed_names(&record).is_empty());
    }

    #[tokio::test]
    async fn reconcile_orphaned_install_does_not_block_the_async_runtime() {
        // Regression test: `reconcile_orphaned_install` used to lock via
        // `operation_lock::blocking_lock`, which panics when called from
        // within an async execution context. Its only real caller,
        // `availability()`, is reached exclusively through an async Tauri
        // command — so this must run inside a tokio task, not a plain
        // `#[test]`, to actually exercise that failure mode.
        let (_db_dir, context) = context();
        let game_dir = tempdir().expect("game dir");
        write_file(&game_dir.path().join("renodx-cp2077.addon64"), b"addon");
        write_file(&game_dir.path().join("dxgi.dll"), b"reshade");

        let record =
            reconcile_orphaned_install(&context, &proxy_candidate(game_dir.path(), "2050650"))
                .expect("adopting from an async context must not panic")
                .expect("adopted record");

        assert_eq!(record.host_kind(), Some(InstalledAddonHostKind::Proxy));
    }

    #[test]
    fn proxy_adoption_keeps_user_effect_hosts_read_only() {
        let (_db_dir, context) = context();
        let game_dir = tempdir().expect("game dir");
        write_file(&game_dir.path().join("renodx-cp2077.addon64"), b"addon");
        write_file(&game_dir.path().join("dxgi.dll"), b"reshade");
        write_file(
            &game_dir.path().join("ReShade.ini"),
            b"[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\n",
        );
        write_file(
            &game_dir
                .path()
                .join("reshade-shaders")
                .join("Shaders")
                .join("User.fx"),
            b"technique User {}",
        );

        let record =
            reconcile_orphaned_install(&context, &proxy_candidate(game_dir.path(), "1145360"))
                .expect("adopt")
                .expect("adopted record");

        assert_eq!(record.host_kind(), Some(InstalledAddonHostKind::Proxy));
        assert_eq!(created_names(&record), vec!["renodx-cp2077.addon64"]);
        assert!(record.backed_up_files().is_empty());
    }

    #[test]
    fn proxy_adoption_keeps_foreign_addon_hosts_read_only() {
        let (_db_dir, context) = context();
        let game_dir = tempdir().expect("game dir");
        write_file(&game_dir.path().join("renodx-cp2077.addon64"), b"addon");
        write_file(&game_dir.path().join("dxgi.dll"), &full_reshade_host());
        write_file(&game_dir.path().join("foreign.addon64"), b"foreign");

        let record =
            reconcile_orphaned_install(&context, &proxy_candidate(game_dir.path(), "1145361"))
                .expect("adopt")
                .expect("adopted record");

        assert_eq!(created_names(&record), vec!["renodx-cp2077.addon64"]);
        assert!(host_source(&record).is_none());
    }

    #[test]
    fn proxy_adoption_without_a_detected_host_keeps_only_the_addon_payload() {
        let (_db_dir, context) = context();
        let game_dir = tempdir().expect("game dir");
        let addon = game_dir.path().join("renodx-cp2077.addon64");
        write_file(&addon, b"addon");

        let record = reconcile_orphaned_install(
            &context,
            &OrphanedInstall {
                game_id: game_id("1145362"),
                game_dir: game_dir.path().to_path_buf(),
                addon_file: addon,
                host_file: None,
                host_kind: InstalledAddonHostKind::Proxy,
                registered_exe_path: None,
                reshade_config: test_support::manifest(Vec::new()).reshade,
                game_arch: None,
                addon_url: None,
            },
        )
        .expect("adopt")
        .expect("adopted record");

        assert_eq!(created_names(&record), vec!["renodx-cp2077.addon64"]);
        assert!(record.tracked_sources().is_empty());
    }

    #[test]
    fn vulkan_adoption_records_registered_exe_without_claiming_shared_layer() {
        let (_db_dir, context) = context();
        let game_dir = tempdir().expect("game dir");
        let layer_dir = tempdir().expect("layer dir");
        let exe = game_dir.path().join("Game.exe");
        write_file(&game_dir.path().join("renodx-cp2077.addon64"), b"addon");
        write_file(&exe, b"exe");
        write_file(&layer_dir.path().join("ReShade64.dll"), b"reshade");

        let record = reconcile_orphaned_install(
            &context,
            &OrphanedInstall {
                game_id: game_id("1817070"),
                game_dir: game_dir.path().to_path_buf(),
                addon_file: game_dir.path().join("renodx-cp2077.addon64"),
                host_file: Some(layer_dir.path().join("ReShade64.dll")),
                host_kind: InstalledAddonHostKind::SharedVulkanLayer,
                registered_exe_path: Some(exe.clone()),
                reshade_config: test_support::manifest(Vec::new()).reshade,
                game_arch: None,
                addon_url: None,
            },
        )
        .expect("adopt")
        .expect("adopted record");

        assert_eq!(
            record.host_kind(),
            Some(InstalledAddonHostKind::SharedVulkanLayer)
        );
        assert_eq!(
            record
                .registered_exe_path()
                .map(PathRef::as_str)
                .map(str::to_owned),
            Some(exe.to_string_lossy().replace('\\', "/"))
        );
        assert_eq!(created_names(&record), vec!["renodx-cp2077.addon64"]);
        assert!(record.backed_up_files().is_empty());
    }

    fn base_record(host_kind: InstalledAddonHostKind) -> InstalledAddon {
        InstalledAddon::new(
            game_id("1091500"),
            AddonKind::RenoDx,
            PathRef::new("C:/Games/Test/renodx-test.addon64").expect("addon path"),
        )
        .with_host_kind(host_kind)
    }

    fn host_source(record: &InstalledAddon) -> Option<&TrackedSource> {
        record
            .tracked_sources()
            .iter()
            .find(|s| s.role() == TrackedSourceRole::HostBinary)
    }

    #[test]
    fn attach_advisory_provenance_records_channel_and_host_source_for_proxy_pe_host() {
        let dir = tempdir().expect("tempdir");
        let host_file = dir.path().join("dxgi.dll");
        write_file(
            &host_file,
            &test_support::build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
        );
        let candidate = OrphanedInstall {
            game_id: game_id("1091500"),
            game_dir: dir.path().to_path_buf(),
            addon_file: dir.path().join("renodx-test.addon64"),
            host_file: Some(host_file),
            host_kind: InstalledAddonHostKind::Proxy,
            registered_exe_path: None,
            reshade_config: test_support::manifest(Vec::new()).reshade,
            game_arch: None,
            addon_url: None,
        };

        let record = attach_advisory_provenance(
            base_record(InstalledAddonHostKind::Proxy),
            &candidate,
            true,
        );

        assert_eq!(record.reshade_channel(), Some("stable"));
        let host = host_source(&record).expect("advisory host source recorded");
        assert!(host.is_advisory());
        assert_eq!(host.channel(), Some("stable"));
    }

    #[test]
    fn attach_advisory_provenance_skips_channel_and_host_source_for_a_recognized_custom_build() {
        let dir = tempdir().expect("tempdir");
        let host_file = dir.path().join("dxgi.dll");
        write_file(
            &host_file,
            &test_support::build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
        );
        // GShade's real runtime sitting next to the proxy stub is the reliable
        // signal — adoption must never guess a channel or track this as ours.
        write_file(&dir.path().join("GShade64.dll"), b"gshade-runtime");
        let candidate = OrphanedInstall {
            game_id: game_id("1091500"),
            game_dir: dir.path().to_path_buf(),
            addon_file: dir.path().join("renodx-test.addon64"),
            host_file: Some(host_file),
            host_kind: InstalledAddonHostKind::Proxy,
            registered_exe_path: None,
            reshade_config: test_support::manifest(Vec::new()).reshade,
            game_arch: None,
            addon_url: None,
        };

        let record = attach_advisory_provenance(
            base_record(InstalledAddonHostKind::Proxy),
            &candidate,
            true,
        );

        assert_eq!(record.reshade_channel(), None);
        assert!(host_source(&record).is_none());
    }

    #[test]
    fn attach_advisory_provenance_skips_provenance_when_pe_inspection_fails() {
        let dir = tempdir().expect("tempdir");
        // Never written to disk, so `inspect_pe` cannot read it.
        let host_file = dir.path().join("dxgi.dll");
        let candidate = OrphanedInstall {
            game_id: game_id("1091500"),
            game_dir: dir.path().to_path_buf(),
            addon_file: dir.path().join("renodx-test.addon64"),
            host_file: Some(host_file),
            host_kind: InstalledAddonHostKind::Proxy,
            registered_exe_path: None,
            reshade_config: test_support::manifest(Vec::new()).reshade,
            game_arch: None,
            addon_url: None,
        };

        let record = attach_advisory_provenance(
            base_record(InstalledAddonHostKind::Proxy),
            &candidate,
            true,
        );

        assert_eq!(record.reshade_channel(), None);
        assert!(host_source(&record).is_none());
    }

    #[test]
    fn attach_advisory_provenance_for_vulkan_host_records_channel_but_no_host_source() {
        let dir = tempdir().expect("tempdir");
        let host_file = dir.path().join("ReShade64.dll");
        write_file(
            &host_file,
            &test_support::build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
        );
        let candidate = OrphanedInstall {
            game_id: game_id("1091500"),
            game_dir: dir.path().to_path_buf(),
            addon_file: dir.path().join("renodx-test.addon64"),
            host_file: Some(host_file),
            host_kind: InstalledAddonHostKind::SharedVulkanLayer,
            registered_exe_path: None,
            reshade_config: test_support::manifest(Vec::new()).reshade,
            game_arch: None,
            addon_url: None,
        };

        let record = attach_advisory_provenance(
            base_record(InstalledAddonHostKind::SharedVulkanLayer),
            &candidate,
            false,
        );

        assert_eq!(record.reshade_channel(), Some("stable"));
        assert!(host_source(&record).is_none());
    }

    #[test]
    fn attach_advisory_provenance_records_advisory_addon_source_when_addon_url_present() {
        let dir = tempdir().expect("tempdir");
        let host_file = dir.path().join("dxgi.dll");
        let addon_file = dir.path().join("renodx-test.addon64");
        write_file(&host_file, b"reshade");
        write_file(&addon_file, b"addon-bytes");
        let candidate = OrphanedInstall {
            game_id: game_id("1091500"),
            game_dir: dir.path().to_path_buf(),
            addon_file,
            host_file: Some(host_file),
            host_kind: InstalledAddonHostKind::Proxy,
            registered_exe_path: None,
            reshade_config: test_support::manifest(Vec::new()).reshade,
            game_arch: None,
            addon_url: Some("https://example.com/renodx-test.addon64".to_owned()),
        };

        let record = attach_advisory_provenance(
            base_record(InstalledAddonHostKind::Proxy),
            &candidate,
            true,
        );

        let addon = record
            .tracked_sources()
            .iter()
            .find(|s| s.role() == TrackedSourceRole::AddonPayload)
            .expect("advisory addon source recorded");
        assert!(addon.is_advisory());
        assert_eq!(addon.url(), "https://example.com/renodx-test.addon64");
    }

    fn dlss_fix_source(record: &InstalledAddon) -> Option<&TrackedSource> {
        record
            .tracked_sources()
            .iter()
            .find(|s| s.role() == TrackedSourceRole::DlssFix)
    }

    #[test]
    fn attach_advisory_provenance_records_dlss_fix_source_when_companion_present_proxy() {
        let dir = tempdir().expect("tempdir");
        let host_file = dir.path().join("dxgi.dll");
        let addon_file = dir.path().join("renodx-test.addon64");
        write_file(&host_file, b"reshade");
        write_file(&addon_file, b"addon-bytes");
        write_file(&dir.path().join("renodx-dlssfix.addon64"), b"dlssfix-bytes");
        let candidate = OrphanedInstall {
            game_id: game_id("1091500"),
            game_dir: dir.path().to_path_buf(),
            addon_file,
            host_file: Some(host_file),
            host_kind: InstalledAddonHostKind::Proxy,
            registered_exe_path: None,
            reshade_config: test_support::manifest(Vec::new()).reshade,
            game_arch: Some(Architecture::X64),
            addon_url: Some("https://example.com/renodx-test.addon64".to_owned()),
        };

        let record = attach_advisory_provenance(
            base_record(InstalledAddonHostKind::Proxy),
            &candidate,
            true,
        );

        let dlss_fix = dlss_fix_source(&record).expect("advisory dlss-fix source recorded");
        assert!(dlss_fix.is_advisory());
        assert_eq!(
            dlss_fix.url(),
            "https://clshortfuse.github.io/renodx/renodx-dlssfix.addon64"
        );
        // Digest must come from the DLSS-Fix file, not the main addon.
        assert_ne!(
            dlss_fix.digest(),
            record
                .tracked_sources()
                .iter()
                .find(|s| s.role() == TrackedSourceRole::AddonPayload)
                .expect("addon source recorded")
                .digest()
        );
    }

    #[test]
    fn attach_advisory_provenance_records_dlss_fix_source_when_companion_present_vulkan() {
        let dir = tempdir().expect("tempdir");
        let host_file = dir.path().join("ReShade64.dll");
        let addon_file = dir.path().join("renodx-test.addon64");
        write_file(
            &host_file,
            &test_support::build_pe_with_exports(MACHINE_AMD64, PE32_PLUS_MAGIC, &[]),
        );
        write_file(&addon_file, b"addon-bytes");
        write_file(&dir.path().join("renodx-dlssfix.addon64"), b"dlssfix-bytes");
        let candidate = OrphanedInstall {
            game_id: game_id("1091500"),
            game_dir: dir.path().to_path_buf(),
            addon_file,
            host_file: Some(host_file),
            host_kind: InstalledAddonHostKind::SharedVulkanLayer,
            registered_exe_path: None,
            reshade_config: test_support::manifest(Vec::new()).reshade,
            game_arch: Some(Architecture::X64),
            addon_url: Some("https://example.com/renodx-test.addon64".to_owned()),
        };

        let record = attach_advisory_provenance(
            base_record(InstalledAddonHostKind::SharedVulkanLayer),
            &candidate,
            false,
        );

        // Proves DLSS-Fix attribution is NOT gated by host kind, unlike HostBinary (Proxy-only).
        let dlss_fix = dlss_fix_source(&record).expect("advisory dlss-fix source recorded");
        assert!(dlss_fix.is_advisory());
    }

    #[test]
    fn attach_advisory_provenance_skips_dlss_fix_source_when_companion_absent() {
        let dir = tempdir().expect("tempdir");
        let host_file = dir.path().join("dxgi.dll");
        let addon_file = dir.path().join("renodx-test.addon64");
        write_file(&host_file, b"reshade");
        write_file(&addon_file, b"addon-bytes");
        // No renodx-dlssfix.addon64 written.
        let candidate = OrphanedInstall {
            game_id: game_id("1091500"),
            game_dir: dir.path().to_path_buf(),
            addon_file,
            host_file: Some(host_file),
            host_kind: InstalledAddonHostKind::Proxy,
            registered_exe_path: None,
            reshade_config: test_support::manifest(Vec::new()).reshade,
            game_arch: Some(Architecture::X64),
            addon_url: Some("https://example.com/renodx-test.addon64".to_owned()),
        };

        let record = attach_advisory_provenance(
            base_record(InstalledAddonHostKind::Proxy),
            &candidate,
            true,
        );

        assert!(dlss_fix_source(&record).is_none());
    }
}
