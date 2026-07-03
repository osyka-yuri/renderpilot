//! Installs RenoDX from upstream or from a user-selected add-on file.

use std::path::Path;
use std::time::SystemTime;

use renderpilot_application::InstalledAddonRepository;
use renderpilot_domain::{Architecture, GameId, InstalledAddon, InstalledAddonHostKind, PathRef};

use crate::addons::renodx::anticheat::{RiskAssessment, assess_risk};
use crate::addons::renodx::arch_from_addon_file;
use crate::addons::renodx::errors;
use crate::addons::renodx::facts::{analyze_game, install_target_dir};
use crate::addons::renodx::fetch::{LocalAddonSource, prepare_install, prepare_install_from_file};
use crate::addons::renodx::game_context::{analyze_and_resolve, executable_override, require_game};
use crate::addons::renodx::host_policy;
use crate::addons::renodx::install::{install as install_files, uninstall as uninstall_files};
use crate::addons::renodx::matcher::{
    RenoDxResolution, ResolvedInstall, generic_file_install_plan, resolve_external_install,
};
use crate::addons::renodx::operation_lock;
use crate::addons::renodx::policy::HostKind;
use crate::addons::renodx::progress::emit_finalizing;
use crate::addons::renodx::types::{RenoDxManifest, ReshadeChannel};
use crate::addons::renodx::use_cases::commands::shared_vulkan_layer;
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

/// Upper bound on a user-selected add-on file, so a stray pick cannot exhaust
/// memory. A RenoDX add-on DLL is a few MB; this is a generous ceiling.
const MAX_ADDON_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Shared parameters for a RenoDX install operation.
pub struct InstallRequest<'a> {
    /// Backend context (game repository, addon repository, settings).
    pub context: &'a Context,
    /// The resolved RenoDX manifest (catalogue + ReShade host config).
    pub manifest: &'a RenoDxManifest,
    /// The game to install RenoDX for.
    pub game_id: &'a GameId,
    /// The ReShade host channel to install (stable or nightly).
    pub requested_channel: ReshadeChannel,
    /// Must be `true` to proceed when the anti-cheat risk assessment requires confirmation.
    pub confirm_anticheat: bool,
    /// Whether this caller permits installing the shared Vulkan layer when needed.
    pub allow_shared_vulkan_layer_install: bool,
    /// Optional download progress observer.
    pub progress: Option<&'a ProgressObserver<'a>>,
}

/// Installs RenoDX into `game`, fetching the add-on + ReShade from upstream and
/// persisting the record needed to reverse it.
///
/// `confirm_anticheat` must be `true` to proceed when the risk assessment requires
/// it. `allow_shared_vulkan_layer_install` must be `true` for a Vulkan game when
/// no ReShade Vulkan layer is present yet. The ReShade host (when one must be
/// installed) uses the requested channel, with old manifests falling back from
/// stable to nightly.
///
/// Returns the `managed_app_record` (the per-game `InstalledAddon`).
pub async fn install(request: InstallRequest<'_>) -> Result<InstalledAddon, ServiceError> {
    let InstallRequest {
        context,
        manifest,
        game_id,
        requested_channel,
        confirm_anticheat,
        allow_shared_vulkan_layer_install,
        progress,
    } = request;
    let _guard = operation_lock::lock(game_id).await;
    let game = require_game(context, game_id)?;
    let scan_dir = Path::new(game.install_path().as_str());
    let override_path = executable_override(context, game_id);
    let (analysis, resolution) = analyze_and_resolve(&game, manifest, override_path.as_deref());
    let target_dir = install_target_dir(&analysis)?;

    let plan: ResolvedInstall = match resolution {
        RenoDxResolution::Installable(plan) => *plan,
        RenoDxResolution::External { .. } => {
            return Err(errors::invalid(
                "RenoDX for this game is distributed externally; install it manually".to_owned(),
            ));
        }
        RenoDxResolution::NativeHdr => {
            return Err(errors::invalid(
                "this game has native HDR; RenoDX is not needed".to_owned(),
            ));
        }
        RenoDxResolution::Incompatible { reason } => {
            return Err(errors::invalid(format!(
                "RenoDX is not compatible with this game: {reason:?}"
            )));
        }
        RenoDxResolution::Unsupported { .. } => {
            return Err(errors::invalid(
                "RenoDX is not supported for this game".to_owned(),
            ));
        }
        RenoDxResolution::NoMatch => {
            return Err(errors::invalid(
                "RenoDX has no profile for this game".to_owned(),
            ));
        }
    };

    let risk = assess_risk(&plan.risk, scan_dir);
    enforce_gate(&risk, confirm_anticheat)?;
    let channel = manifest
        .reshade
        .effective_install_channel(requested_channel);
    let registered_exe_path = analysis
        .primary_executable
        .as_ref()
        .map(|e| Path::new(e.as_str()));
    shared_vulkan_layer::ensure_for_install(
        context,
        &plan,
        &manifest.reshade,
        channel,
        allow_shared_vulkan_layer_install,
        registered_exe_path,
        progress,
    )
    .await?;

    // The DirectX host policy scans the game folder for ReShade proxy DLLs and
    // refuses install on slot conflicts. This is irrelevant for Vulkan — the
    // host is a shared system-wide layer, not a per-game proxy. Running the
    // proxy assessment with an empty `proxy_dll_name` can flag leftover
    // ReShade DLLs in the game folder as `InactiveSlot` conflicts and block
    // the Vulkan install. Skip it for Vulkan.
    let writes_host = if matches!(plan.host_kind, HostKind::Vulkan) {
        false
    } else {
        let host = host_policy::assess(&target_dir, &plan.proxy_dll_name);
        host.ensure_not_conflicting(&plan.proxy_dll_name)?;
        host.writes_host()
    };

    let prepared = prepare_install(
        &plan,
        &manifest.reshade,
        game_id.clone(),
        channel,
        writes_host,
        progress,
    )
    .await?;
    emit_finalizing(progress);
    let record = annotate_install_record(
        install_files(&target_dir, &prepared)?,
        plan.host_kind,
        channel,
        registered_exe_path,
    )?;
    crate::fs::stamp_mtime_best_effort(
        Path::new(record.addon_file().as_str()),
        prepared.source_last_modified.as_deref(),
        None,
    );
    persist_or_revert(context, record)
}

/// Installs RenoDX from a user-downloaded add-on file — the manual path for any
/// DirectX game, whether or not the catalogue knows it.
///
/// Same engine and reversibility as [`install`]; the add-on bytes come from
/// `file_path` (validated as a PE) instead of an upstream download, and the record
/// tracks no upstream source. A curated *External* title is no longer a special
/// case: it just yields a richer plan, while any DirectX game falls back to a
/// generic "ReShade host + your add-on" plan. The renderer must be able to load a
/// proxy DLL (a confirmed Vulkan/OpenGL game is refused), and the add-on's
/// architecture must match the game's.
pub async fn install_from_file(
    request: InstallRequest<'_>,
    file_path: &str,
) -> Result<InstalledAddon, ServiceError> {
    let InstallRequest {
        context,
        manifest,
        game_id,
        requested_channel,
        confirm_anticheat,
        allow_shared_vulkan_layer_install,
        progress,
    } = request;
    let _guard = operation_lock::lock(game_id).await;
    let game = require_game(context, game_id)?;
    let scan_dir = Path::new(game.install_path().as_str());
    let analysis = analyze_game(&game, executable_override(context, game_id).as_deref());
    let target_dir = install_target_dir(&analysis)?;

    // The architecture the user's add-on targets (`.addon64` → X64). A non-add-on
    // file is rejected outright.
    let file_arch = arch_from_addon_file(file_path).ok_or_else(|| {
        errors::invalid("the selected file is not a RenoDX add-on (.addon64 / .addon32)".to_owned())
    })?;
    // Hard guard: a known game architecture must match the add-on's, or ReShade
    // would load a wrong-bitness add-on it cannot use.
    if let Some(game_arch) = analysis.facts.graphics.architecture()
        && game_arch != file_arch
    {
        return Err(errors::invalid(format!(
            "this add-on is {} but the game is {} — download the matching add-on",
            arch_label(file_arch),
            arch_label(game_arch),
        )));
    }

    // A curated External title's plan, else a generic plan for any DirectX game.
    let plan = resolve_external_install(manifest, &analysis.facts)
        .or_else(|| generic_file_install_plan(&analysis.facts, file_arch))
        .ok_or_else(|| {
            errors::invalid(
                "RenoDX cannot be installed for this game: its renderer is not Direct3D".to_owned(),
            )
        })?;

    // Authoritative invariant: the add-on and the ReShade host it sits beside must be
    // the same bitness. A generic plan satisfies this by construction; a curated title
    // enforces *its* architecture even when detection was inconclusive (which the
    // friendly game-vs-add-on check above could not catch).
    ensure_addon_arch(file_arch, plan.arch)?;

    let risk = assess_risk(&plan.risk, scan_dir);
    enforce_gate(&risk, confirm_anticheat)?;
    let channel = manifest
        .reshade
        .effective_install_channel(requested_channel);
    let registered_exe_path = analysis
        .primary_executable
        .as_ref()
        .map(|e| Path::new(e.as_str()));
    shared_vulkan_layer::ensure_for_install(
        context,
        &plan,
        &manifest.reshade,
        channel,
        allow_shared_vulkan_layer_install,
        registered_exe_path,
        progress,
    )
    .await?;

    // Same Vulkan guard as in `install()` — see comment there.
    let writes_host = if matches!(plan.host_kind, HostKind::Vulkan) {
        false
    } else {
        let host = host_policy::assess(&target_dir, &plan.proxy_dll_name);
        host.ensure_not_conflicting(&plan.proxy_dll_name)?;
        host.writes_host()
    };

    let (addon_bytes, source_mtime) = read_addon_file(file_path)?;
    let prepared = prepare_install_from_file(
        &plan,
        &manifest.reshade,
        game_id.clone(),
        LocalAddonSource {
            bytes: addon_bytes,
            last_modified: source_mtime.map(crate::fs::format_http_date),
        },
        channel,
        writes_host,
        progress,
    )
    .await?;
    emit_finalizing(progress);
    let record = annotate_install_record(
        install_files(&target_dir, &prepared)?,
        plan.host_kind,
        channel,
        registered_exe_path,
    )?;
    crate::fs::stamp_mtime_best_effort(Path::new(record.addon_file().as_str()), None, source_mtime);
    persist_or_revert(context, record)
}

fn annotate_install_record(
    record: InstalledAddon,
    host_kind: HostKind,
    channel: ReshadeChannel,
    registered_exe_path: Option<&Path>,
) -> Result<InstalledAddon, ServiceError> {
    let mut record = record
        .with_host_kind(match host_kind {
            HostKind::Proxy => InstalledAddonHostKind::Proxy,
            HostKind::Vulkan => InstalledAddonHostKind::SharedVulkanLayer,
        })
        .with_reshade_channel(channel.as_str());

    if matches!(host_kind, HostKind::Vulkan) {
        let exe_path = registered_exe_path.ok_or_else(|| {
            errors::invalid(
                "cannot record Vulkan install metadata without a registered executable".to_owned(),
            )
        })?;
        record = record.with_registered_exe_path(path_ref(exe_path)?);
    }

    Ok(record)
}

fn path_ref(path: &Path) -> Result<PathRef, ServiceError> {
    PathRef::new(path.to_string_lossy().into_owned())
        .map_err(|error| errors::failed(format!("invalid install metadata path: {error}")))
}

/// Reads a user-selected add-on file, rejecting non-files and anything larger than
/// a sane add-on so a stray pick cannot exhaust memory. PE validation happens in
/// the fetch layer alongside the download path.
fn read_addon_file(file_path: &str) -> Result<(Vec<u8>, Option<SystemTime>), ServiceError> {
    let path = Path::new(file_path);
    let metadata =
        std::fs::metadata(path).map_err(|error| errors::io("read add-on file", path, &error))?;
    if !metadata.is_file() {
        return Err(errors::invalid(
            "the selected add-on path is not a file".to_owned(),
        ));
    }
    if metadata.len() > MAX_ADDON_FILE_BYTES {
        return Err(errors::invalid(format!(
            "add-on file is too large (maximum {} MB)",
            MAX_ADDON_FILE_BYTES / (1024 * 1024)
        )));
    }
    let source_mtime = metadata.modified().ok();
    let bytes =
        std::fs::read(path).map_err(|error| errors::io("read add-on file", path, &error))?;
    Ok((bytes, source_mtime))
}

/// Human-readable bitness label for an add-on/game architecture-mismatch message.
fn arch_label(arch: Architecture) -> &'static str {
    match arch {
        Architecture::X64 => "64-bit",
        Architecture::X86 => "32-bit",
    }
}

/// Enforces the add-on ↔ host bitness invariant: a picked add-on must match the
/// architecture of the resolved install plan (the ReShade host it installs beside),
/// so a 32-bit add-on can never be paired with a 64-bit host or vice-versa.
fn ensure_addon_arch(file_arch: Architecture, plan_arch: Architecture) -> Result<(), ServiceError> {
    if file_arch != plan_arch {
        return Err(errors::invalid(format!(
            "this add-on is {} but RenoDX for this game needs the {} build — download the matching add-on",
            arch_label(file_arch),
            arch_label(plan_arch),
        )));
    }
    Ok(())
}

/// Anti-cheat / risk gate shared by every install path: a manifest block always
/// refuses; a warning requires explicit confirmation.
fn enforce_gate(risk: &RiskAssessment, confirm_anticheat: bool) -> Result<(), ServiceError> {
    match gate(risk, confirm_anticheat) {
        InstallGate::Proceed => Ok(()),
        InstallGate::Blocked => Err(errors::invalid(
            "RenoDX is blocked for this game and will not be installed".to_owned(),
        )),
        InstallGate::NeedsConfirmation => Err(errors::invalid(
            "RenoDX install requires explicit confirmation of the anti-cheat ban risk".to_owned(),
        )),
    }
}

/// Persists the install record, reverting the filesystem if persistence fails so an
/// install never survives without a record to reverse it. A double-fault (revert
/// also fails) is logged, never silent.
fn persist_or_revert(
    context: &Context,
    record: InstalledAddon,
) -> Result<InstalledAddon, ServiceError> {
    if let Err(error) = context.storage().upsert_installed_addon(&record) {
        if let Err(revert_error) = uninstall_files(&record) {
            log::warn!(
                "RenoDX install: record persistence failed and the filesystem revert also failed: {revert_error}"
            );
        }
        return Err(error.into());
    }
    Ok(record)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallGate {
    Proceed,
    Blocked,
    NeedsConfirmation,
}

/// A manifest block always refuses; a warning requires explicit confirmation.
fn gate(risk: &RiskAssessment, confirm_anticheat: bool) -> InstallGate {
    if risk.is_blocked() {
        InstallGate::Blocked
    } else if risk.requires_confirmation() && !confirm_anticheat {
        InstallGate::NeedsConfirmation
    } else {
        InstallGate::Proceed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::types::{
        AnticheatEngine, AssessmentConfidence, OnlineKind, RiskSeverity,
    };

    fn risk(severity: RiskSeverity) -> RiskAssessment {
        RiskAssessment {
            severity,
            anticheat_engine: AnticheatEngine::None,
            online: OnlineKind::Singleplayer,
            message_key: "k".to_owned(),
            confidence: AssessmentConfidence::Medium,
            reference_url: None,
            detected_locally: false,
        }
    }

    #[test]
    fn safe_risk_proceeds_without_confirmation() {
        assert_eq!(gate(&risk(RiskSeverity::Info), false), InstallGate::Proceed);
    }

    #[test]
    fn warn_risk_needs_confirmation_then_proceeds() {
        assert_eq!(
            gate(&risk(RiskSeverity::Warn), false),
            InstallGate::NeedsConfirmation
        );
        assert_eq!(gate(&risk(RiskSeverity::Warn), true), InstallGate::Proceed);
    }

    #[test]
    fn blocked_risk_is_refused_even_with_confirmation() {
        assert_eq!(gate(&risk(RiskSeverity::Block), true), InstallGate::Blocked);
    }

    #[test]
    fn addon_arch_invariant_rejects_a_bitness_mismatch() {
        assert!(ensure_addon_arch(Architecture::X64, Architecture::X64).is_ok());
        let error = ensure_addon_arch(Architecture::X86, Architecture::X64)
            .expect_err("a 32-bit add-on for a 64-bit host must be rejected");
        assert!(matches!(error, ServiceError::InvalidInput(_)));
    }
}
