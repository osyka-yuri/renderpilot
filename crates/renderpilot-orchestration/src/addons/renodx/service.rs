//! Top-level RenoDX orchestration: the end-to-end install, uninstall, status, and
//! availability flows that wire the pure pieces together.
//!
//! `availability` and `status` are read-only previews for the UI; `install` drives
//! `analyze → resolve → assess risk → confirm-gate → fetch → install → persist`,
//! and `uninstall` replays the persisted record and clears it. The manifest is
//! supplied by the caller (fetched/cached elsewhere).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use renderpilot_application::{GameRepository, InstalledAddonRepository};
use renderpilot_domain::{
    AddonKind, Architecture, GameId, GameInstallation, InstalledAddon, RenoDxInstallState,
    TrackedSource, TrackedSourceRole,
};
use serde::Serialize;

use crate::addons::engine::{self, FileOp, InstallPlan};
use crate::net::ProgressObserver;
use crate::{Context, ServiceError};

use super::anticheat::{RiskAssessment, assess_risk};
use super::arch_from_addon_file;
use super::channel;
use super::dlss_fix::resolve_dlss_fix;
use super::errors;
use super::facts::{GameAnalysis, analyze_game, install_target_dir};
use super::fetch::{self, LocalAddonSource, prepare_install, prepare_install_from_file};
use super::host_policy;
use super::install::{
    dlss_fix_file_name, dlss_fix_file_path, install as install_files, uninstall as uninstall_files,
};
use super::matcher::{
    IncompatibilityReason, MatchConfidence, MatchFacts, RenoDxResolution, ResolvedInstall,
    file_installable, generic_file_install_plan, generic_risk, matched_slug, resolve,
    resolve_external_install,
};
use super::operation_lock;
use super::progress::emit_finalizing;
use super::reshade::{self, RenoDxAddonState, ReshadeHost, ReshadeHostAction};
use super::reshade_ini;
use super::tracking;
use super::types::{DlssFixIniTweaks, RenoDxManifest, ReshadeChannel, ReshadeIniTweaks};

/// Upper bound on a user-selected add-on file, so a stray pick cannot exhaust
/// memory. A RenoDX add-on DLL is a few MB; this is a generous ceiling.
const MAX_ADDON_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Read-only preview of whether RenoDX can be installed for a game.
#[derive(Debug, Clone, Serialize)]
pub struct AvailabilityReport {
    /// Current install state for the game.
    pub state: RenoDxInstallState,
    /// ReShade host detected in the proxy slot this resolution would use, when
    /// that can be determined.
    pub reshade_host: ReshadeHost,
    /// Strict host policy verdict for the detected host.
    pub reshade_host_action: ReshadeHostAction,
    /// Whether more than one ReShade host was found in the target directory.
    pub reshade_conflict: bool,
    /// ReShade channel recorded for a managed host, when known.
    pub reshade_channel: Option<ReshadeChannel>,
    /// Whether the manifest can provide a stable ReShade host source.
    pub reshade_stable_supported: bool,
    /// Ownership/health of the detected or recorded ReShade host.
    pub reshade_ownership: ReshadeHostOwnership,
    /// Read-only state of the RenoDX add-on file/config, when the expected file
    /// name is known.
    pub renodx_addon: Option<RenoDxAddonState>,
    /// Whether and how RenoDX can be installed.
    pub outcome: AvailabilityOutcome,
    /// The manual "install ReShade host + your own add-on file" escape hatch,
    /// present for a DirectX game that has no automatic or curated-external path.
    pub manual_install: Option<ManualFileInstall>,
}

/// Ownership/health of the ReShade host relevant to this RenoDX install.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReshadeHostOwnership {
    /// RenderPilot has a managed Host source for this install.
    Managed {
        /// Whether the recorded host source matches the file currently on disk.
        health: ManagedReshadeHealth,
    },
    /// A foreign host is present and can load add-ons.
    UnmanagedCompatible,
    /// A foreign or conflicting host is present but not safe to reuse.
    UnmanagedConflicting,
    /// No ReShade host is present and no managed Host source exists.
    Missing,
}

/// Health of a ReShade host that RenderPilot owns in the install record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedReshadeHealth {
    /// The file exists and matches the recorded installed DLL digest.
    Healthy,
    /// The record owns a host but the file is missing or unreadable.
    Missing,
    /// The managed record or current directory scan is ambiguous.
    Conflicting,
}

/// The installability verdict for a game.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AvailabilityOutcome {
    /// A compatible game matched; the add-on can be installed.
    Installable {
        /// Confidence shown to the user (verified / experimental / untested).
        confidence: MatchConfidence,
        /// Ban/stability risk and whether explicit confirmation is required.
        risk: RiskAssessment,
        /// i18n note/requirement keys (a generic install carries its engine label here).
        notes_keys: Vec<String>,
    },
    /// The add-on is distributed off-GitHub; link the user out, and — when the
    /// game is compatible — offer to install a file the user downloaded.
    External {
        /// Where to send the user (Discord/Nexus).
        url: String,
        /// i18n label key for the link.
        label_key: String,
        /// Present when the game is compatible, enabling "install from file".
        file_install: Option<ExternalFileInstall>,
    },
    /// The game already has native HDR; RenoDX is not offered.
    NativeHdr,
    /// A game matched but cannot be installed for it.
    Incompatible {
        /// Why it cannot be installed.
        reason: IncompatibilityReason,
    },
    /// The game is blacklisted / known-broken.
    Blacklisted {
        /// i18n reason key, when the manifest gives one.
        reason: Option<String>,
    },
    /// No RenoDX profile matched the game.
    Unsupported,
}

/// The manual file-install escape hatch for a DirectX game with no automatic or
/// curated-external path: install the ReShade host and add a user-downloaded add-on.
#[derive(Debug, Clone, Serialize)]
pub struct ManualFileInstall {
    /// Ban/stability risk and whether explicit confirmation is required (assessed).
    pub risk: RiskAssessment,
    /// The catalogue add-on stem (`renodx-<slug>`) when a title matched, for a soft
    /// filename check in the UI; `None` for an unrecognized game.
    pub expected_addon_name: Option<String>,
    /// The game's architecture (`"x64"` / `"x86"`) for an immediate add-on-arch
    /// check in the UI; `None` when detection was inconclusive.
    pub game_arch: Option<String>,
}

/// The file-install offer for a compatible external game, shown alongside the link.
#[derive(Debug, Clone, Serialize)]
pub struct ExternalFileInstall {
    /// Confidence shown to the user (verified / experimental / untested).
    pub confidence: MatchConfidence,
    /// Ban/stability risk and whether explicit confirmation is required.
    pub risk: RiskAssessment,
    /// i18n note/requirement keys.
    pub notes_keys: Vec<String>,
}

/// Returns the current RenoDX install state for a game from the persisted record.
pub fn status(context: &Context, game_id: &GameId) -> Result<RenoDxInstallState, ServiceError> {
    Ok(context
        .storage()
        .get_installed_addon(game_id)?
        .map(|record| tracking::install_state_from_record(&record))
        .unwrap_or(RenoDxInstallState::NotInstalled))
}

fn reshade_ownership(
    record: Option<&InstalledAddon>,
    host: &ReshadeHost,
    action: ReshadeHostAction,
    conflict: bool,
) -> ReshadeHostOwnership {
    let Some(record) = record else {
        return unmanaged_ownership(host, action, conflict);
    };
    let host_source = match channel::single_host_source(record) {
        Ok(source) => source,
        Err(channel::ChannelReadIssue::DuplicateHostSources) => {
            return ReshadeHostOwnership::Managed {
                health: ManagedReshadeHealth::Conflicting,
            };
        }
    };
    if host_source.is_none() {
        return unmanaged_ownership(host, action, conflict);
    }
    if conflict || action == ReshadeHostAction::Conflict {
        return ReshadeHostOwnership::Managed {
            health: ManagedReshadeHealth::Conflicting,
        };
    }
    // Cheap existence check only: never read+hash the multi-MB host DLL here —
    // availability runs on every card open and after every mutation, and the
    // host folder scan already read the DLL for its PE facts.
    let health = match tracking::managed_host_path(record) {
        Some(path) if path.is_file() => ManagedReshadeHealth::Healthy,
        _ => ManagedReshadeHealth::Missing,
    };
    ReshadeHostOwnership::Managed { health }
}

fn unmanaged_ownership(
    host: &ReshadeHost,
    action: ReshadeHostAction,
    conflict: bool,
) -> ReshadeHostOwnership {
    let Some(present) = host.as_present() else {
        return ReshadeHostOwnership::Missing;
    };
    if !conflict
        && action == ReshadeHostAction::UpToDate
        && present.addon_support == reshade::ReshadeAddonSupport::Full
    {
        ReshadeHostOwnership::UnmanagedCompatible
    } else {
        ReshadeHostOwnership::UnmanagedConflicting
    }
}

/// Previews whether RenoDX can be installed for the game, without changing disk.
pub fn availability(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
) -> Result<AvailabilityReport, ServiceError> {
    let record = context.storage().get_installed_addon(game_id)?;
    let state = record
        .as_ref()
        .map(tracking::install_state_from_record)
        .unwrap_or(RenoDxInstallState::NotInstalled);
    let game = require_game(context, game_id)?;
    let scan_dir = Path::new(game.install_path().as_str());

    let override_path = executable_override(context, game_id);
    let (analysis, resolution) = analyze_and_resolve(&game, manifest, override_path.as_deref());
    let host_report = reshade_report(&analysis, &resolution, record.as_ref());
    let manual_install = manual_file_install(manifest, &analysis.facts, &resolution, scan_dir);
    let outcome = match resolution {
        RenoDxResolution::Installable(plan) => AvailabilityOutcome::Installable {
            confidence: plan.confidence,
            risk: assess_risk(&plan.risk, scan_dir),
            notes_keys: plan.notes_keys,
        },
        RenoDxResolution::External {
            url,
            label_key,
            file_install,
        } => AvailabilityOutcome::External {
            url,
            label_key,
            file_install: file_install.map(|fi| ExternalFileInstall {
                confidence: fi.confidence,
                risk: assess_risk(&fi.risk, scan_dir),
                notes_keys: fi.notes_keys,
            }),
        },
        RenoDxResolution::NativeHdr => AvailabilityOutcome::NativeHdr,
        RenoDxResolution::Incompatible { reason } => AvailabilityOutcome::Incompatible { reason },
        RenoDxResolution::Unsupported { reason } => AvailabilityOutcome::Blacklisted { reason },
        RenoDxResolution::NoMatch => AvailabilityOutcome::Unsupported,
    };

    Ok(AvailabilityReport {
        state,
        reshade_host: host_report.host,
        reshade_host_action: host_report.action,
        reshade_conflict: host_report.conflict,
        reshade_channel: host_report.channel,
        reshade_stable_supported: manifest.reshade.supports_channel(ReshadeChannel::Stable),
        reshade_ownership: host_report.ownership,
        renodx_addon: host_report.addon,
        outcome,
        manual_install,
    })
}

struct ReshadeReport {
    host: ReshadeHost,
    action: ReshadeHostAction,
    conflict: bool,
    channel: Option<ReshadeChannel>,
    ownership: ReshadeHostOwnership,
    addon: Option<RenoDxAddonState>,
}

fn reshade_report(
    analysis: &GameAnalysis,
    resolution: &RenoDxResolution,
    record: Option<&InstalledAddon>,
) -> ReshadeReport {
    let Some(target_dir) = install_target_dir(analysis).ok() else {
        return missing_host_report(record);
    };
    let Some(active_proxy) = active_proxy_slot(resolution) else {
        return missing_host_report(record);
    };
    let assessment = host_policy::assess(&target_dir, active_proxy);
    let paths = reshade::resolve_paths(
        &target_dir,
        assessment.host.as_present().map(|present| present.path),
    );
    let addon = expected_addon_file_name(resolution)
        .map(|file_name| reshade::renodx_addon_state(&paths, &file_name));
    let ownership = reshade_ownership(
        record,
        &assessment.host,
        assessment.action,
        assessment.conflict,
    );

    ReshadeReport {
        host: assessment.host,
        action: assessment.action,
        conflict: assessment.conflict,
        channel: recorded_channel(record),
        ownership,
        addon,
    }
}

/// The host report for an install whose install target directory or active proxy
/// slot cannot be resolved: no host, a conflict verdict, and only the channel
/// carried through from the install record.
fn missing_host_report(record: Option<&InstalledAddon>) -> ReshadeReport {
    ReshadeReport {
        host: ReshadeHost::Absent,
        action: ReshadeHostAction::Conflict,
        conflict: false,
        channel: recorded_channel(record),
        ownership: ReshadeHostOwnership::Missing,
        addon: None,
    }
}

/// The ReShade channel recorded on the install's managed host source, if any. A
/// record with duplicate host sources or an unreadable channel degrades to `None`.
fn recorded_channel(record: Option<&InstalledAddon>) -> Option<ReshadeChannel> {
    record.and_then(|record| channel::installed_channel(record).ok().flatten())
}

fn active_proxy_slot(resolution: &RenoDxResolution) -> Option<&str> {
    match resolution {
        RenoDxResolution::Installable(plan) => Some(plan.proxy_dll_name.as_str()),
        RenoDxResolution::External {
            file_install: Some(plan),
            ..
        } => Some(plan.proxy_dll_name.as_str()),
        _ => None,
    }
}

fn expected_addon_file_name(resolution: &RenoDxResolution) -> Option<String> {
    match resolution {
        RenoDxResolution::Installable(plan) => {
            Some(super::source::addon_file_name(&plan.slug, plan.arch))
        }
        _ => None,
    }
}

/// The manual file-install escape hatch for the availability preview: offered only
/// where there is no automatic or curated-external path (an unmatched game or a
/// DirectX incompatibility) and the renderer can actually load a proxy DLL. A
/// blacklisted or native-HDR game, or one with an automatic/external path, gets
/// `None` — the manual path would be redundant or deliberately withheld.
fn manual_file_install(
    manifest: &RenoDxManifest,
    facts: &MatchFacts,
    resolution: &RenoDxResolution,
    scan_dir: &Path,
) -> Option<ManualFileInstall> {
    let offered = matches!(
        resolution,
        RenoDxResolution::Incompatible { .. } | RenoDxResolution::NoMatch
    );
    if !offered || !file_installable(facts) {
        return None;
    }
    Some(ManualFileInstall {
        risk: assess_risk(&generic_risk(), scan_dir),
        expected_addon_name: matched_slug(manifest, facts)
            .map(|slug| super::source::addon_file_stem(&slug)),
        game_arch: facts.graphics.architecture().map(arch_str),
    })
}

/// Stable wire string for a game's architecture, for the UI's add-on-arch check.
fn arch_str(arch: Architecture) -> String {
    match arch {
        Architecture::X64 => "x64",
        Architecture::X86 => "x86",
    }
    .to_owned()
}

/// The user's pinned executable for a game, if set. This is the shared
/// game-level override (also honored by NVAPI); the resolver checks it exists. A
/// storage read error degrades to auto-detection rather than failing the preview.
fn executable_override(context: &Context, game_id: &GameId) -> Option<PathBuf> {
    crate::nvapi::resolve::stored_override_path(context, game_id.as_str())
        .ok()
        .flatten()
}

/// Inspects the game on disk and resolves it against the manifest in one step.
fn analyze_and_resolve(
    game: &GameInstallation,
    manifest: &RenoDxManifest,
    override_path: Option<&Path>,
) -> (GameAnalysis, RenoDxResolution) {
    let analysis = analyze_game(game, override_path);
    let resolution = resolve(manifest, &analysis.facts);
    (analysis, resolution)
}

/// Installs RenoDX into `game`, fetching the add-on + ReShade from upstream and
/// persisting the record needed to reverse it.
///
/// `confirm_anticheat` must be `true` to proceed when the risk assessment requires
/// it. The ReShade host (when one must be installed) uses the requested channel,
/// with old manifests falling back from stable to nightly.
pub async fn install(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    requested_channel: ReshadeChannel,
    confirm_anticheat: bool,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<InstalledAddon, ServiceError> {
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

    let host = host_policy::assess(&target_dir, &plan.proxy_dll_name);
    host.ensure_not_conflicting(&plan.proxy_dll_name)?;

    let channel = manifest
        .reshade
        .effective_install_channel(requested_channel);
    let prepared = prepare_install(
        &plan,
        &manifest.reshade,
        game_id.clone(),
        channel,
        host.writes_host(),
        progress,
    )
    .await?;
    emit_finalizing(progress);
    let record = install_files(&target_dir, &prepared)?;
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
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    file_path: &str,
    requested_channel: ReshadeChannel,
    confirm_anticheat: bool,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<InstalledAddon, ServiceError> {
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
    if let Some(game_arch) = analysis.facts.graphics.architecture() {
        if game_arch != file_arch {
            return Err(errors::invalid(format!(
                "this add-on is {} but the game is {} — download the matching add-on",
                arch_label(file_arch),
                arch_label(game_arch),
            )));
        }
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

    let host = host_policy::assess(&target_dir, &plan.proxy_dll_name);
    host.ensure_not_conflicting(&plan.proxy_dll_name)?;

    let (addon_bytes, source_mtime) = read_addon_file(file_path)?;
    let channel = manifest
        .reshade
        .effective_install_channel(requested_channel);
    let prepared = prepare_install_from_file(
        &plan,
        &manifest.reshade,
        game_id.clone(),
        LocalAddonSource {
            bytes: addon_bytes,
            last_modified: source_mtime.map(crate::fs::format_http_date),
        },
        channel,
        host.writes_host(),
        progress,
    )
    .await?;
    emit_finalizing(progress);
    let record = install_files(&target_dir, &prepared)?;
    crate::fs::stamp_mtime_best_effort(Path::new(record.addon_file().as_str()), None, source_mtime);
    persist_or_revert(context, record)
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

/// Loads a game's installation by id, or fails with a clear "not found" error.
fn require_game(context: &Context, game_id: &GameId) -> Result<GameInstallation, ServiceError> {
    context
        .storage()
        .find_game(game_id)?
        .ok_or_else(|| errors::game_not_found(game_id))
}

/// Uninstalls RenoDX from a game, restoring the folder and clearing the record.
///
/// Order matters: the filesystem is reverted *before* the record is deleted, so a
/// failed restore keeps the record and uninstall stays retryable.
pub fn uninstall(context: &Context, game_id: &GameId) -> Result<(), ServiceError> {
    let _guard = operation_lock::blocking_lock(game_id);
    let record = context
        .storage()
        .get_installed_addon(game_id)?
        .ok_or_else(|| errors::invalid("RenoDX is not installed for this game".to_owned()))?;

    uninstall_files(&record)?;
    context.storage().delete_installed_addon(game_id)?;
    Ok(())
}

/// Switches a managed ReShade host between stable and nightly for this game.
pub async fn switch_reshade_channel(
    context: &Context,
    manifest: &RenoDxManifest,
    game_id: &GameId,
    target_channel: ReshadeChannel,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<RenoDxInstallState, ServiceError> {
    let _guard = operation_lock::lock(game_id).await;
    let record = context
        .storage()
        .get_installed_addon(game_id)?
        .ok_or_else(|| errors::invalid("RenoDX is not installed for this game".to_owned()))?;
    let host_source = channel::single_host_source(&record)
        .map_err(|_| errors::duplicate_host_sources())?
        .ok_or_else(|| {
            errors::invalid("RenoDX does not manage the ReShade host for this game".to_owned())
        })?;

    if !manifest.reshade.supports_channel(target_channel) {
        return Err(errors::invalid(format!(
            "ReShade channel `{}` is not available",
            target_channel.as_str()
        )));
    }

    let current = channel::installed_channel(&record)
        .map_err(|_| errors::duplicate_host_sources())?
        .or_else(|| channel::infer_legacy_channel_from_url(host_source.url()));

    if current == Some(target_channel) {
        let healed_source = channel::with_host_channel(host_source, target_channel);
        let healed = tracking::replace_host_source(&record, &healed_source)?;
        context.storage().upsert_installed_addon(&healed)?;
        return Ok(tracking::install_state_from_record(&healed));
    }

    let game = require_game(context, game_id)?;
    let analysis = analyze_game(&game, executable_override(context, game_id).as_deref());
    let resolution = resolve(manifest, &analysis.facts);
    let (arch, proxy_dll_name) = match resolution {
        RenoDxResolution::Installable(plan) => (plan.arch, plan.proxy_dll_name.clone()),
        RenoDxResolution::External {
            file_install: Some(plan),
            ..
        } => (plan.arch, plan.proxy_dll_name.clone()),
        _ => {
            return Err(errors::invalid(
                "cannot resolve the ReShade proxy slot for this game".to_owned(),
            ));
        }
    };
    let source = super::source::reshade_source(&manifest.reshade, target_channel, arch)
        .ok_or_else(|| {
            errors::invalid(format!(
                "ReShade channel `{}` is not available",
                target_channel.as_str()
            ))
        })?;
    let download = fetch::fetch_reshade_from_source(&source, arch, progress).await?;

    let host_path = tracking::managed_host_path(&record).unwrap_or_else(|| {
        install_target_dir(&analysis)
            .unwrap_or_default()
            .join(proxy_dll_name)
    });
    if !host_path.is_file() {
        return Err(errors::invalid(
            "managed ReShade host is missing; repair it before switching channel".to_owned(),
        ));
    }
    let original = crate::fs::read_file(&host_path)?;
    emit_finalizing(progress);
    engine::replace_file(&host_path, &download.bytes)?;

    let new_source = TrackedSource::new(
        TrackedSourceRole::Host,
        source.url,
        download.etag,
        download.digest,
    )
    .with_last_modified(download.last_modified)
    .with_channel(target_channel.as_str());
    let updated = tracking::replace_host_source(&record, &new_source)?;
    if let Err(error) = context.storage().upsert_installed_addon(&updated) {
        if let Err(revert_error) = engine::replace_file(&host_path, &original) {
            log::warn!(
                "RenoDX channel switch: record persistence failed and DLL rollback failed: {revert_error}"
            );
        }
        return Err(error.into());
    }
    Ok(tracking::install_state_from_record(&updated))
}

// ---------------------------------------------------------------------------
// DLSS-Fix companion add-on: install / uninstall / status
// ---------------------------------------------------------------------------

/// Installs the DLSS-Fix companion add-on for a game that already has RenoDX.
///
/// Downloads `renodx-dlssfix.addon64`, places it in the game folder, and merges
/// `ReShade.ini` to add `LoadFromDllMain` under `[ADDON]` and a `[RENODX-DLSSFIX]`
/// section with the resolved DLL paths. The install runs through the engine like
/// any other plan, but uses [`FileOp::UpdateText`] (not [`FileOp::MergeText`]) for
/// the ini so the main install's `.bak` is preserved — a companion update must
/// never clobber the rollback backup of the primary install.
pub async fn install_dlss_fix(
    context: &Context,
    game_id: &GameId,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<RenoDxInstallState, ServiceError> {
    let _guard = operation_lock::lock(game_id).await;
    let record = context
        .storage()
        .get_installed_addon(game_id)?
        .ok_or_else(|| errors::invalid("RenoDX is not installed for this game".to_owned()))?;

    if record.has_dlss_fix() {
        return Err(errors::invalid(
            "DLSS-Fix is already installed for this game".to_owned(),
        ));
    }

    let request = resolve_dlss_fix(context.storage(), game_id)?.ok_or_else(|| {
        errors::invalid(
            "this game does not have NVIDIA Frame Generation + DLSS + Streamline; \
             DLSS-Fix is not available"
                .to_owned(),
        )
    })?;

    let game_dir = Path::new(record.addon_file().as_str())
        .parent()
        .ok_or_else(|| errors::invalid("installed add-on has no parent directory".to_owned()))?;

    let arch = arch_from_addon_file(record.addon_file().as_str()).ok_or_else(|| {
        errors::invalid("cannot determine architecture from add-on file name".to_owned())
    })?;
    let file_name = dlss_fix_file_name(arch);

    let download = super::fetch::fetch_dlss_fix(arch, progress).await?;
    let download_last_modified = download.last_modified.clone();

    let ini_tweaks = ReshadeIniTweaks {
        disabled_addons: Vec::new(),
        addon_path: None,
        dlss_fix: Some(DlssFixIniTweaks {
            addon_file_name: file_name.clone(),
            dlss_path: request.dlss_path,
            streamline_path: request.streamline_path,
        }),
    };
    let strategy = reshade_ini::ini_merge_strategy(&ini_tweaks);

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::Create {
                name: file_name.clone(),
                bytes: download.bytes,
            },
            FileOp::UpdateText {
                name: reshade::RESHADE_INI_FILE_NAME.to_owned(),
                default: String::new(),
                strategy,
            },
        ],
    };

    emit_finalizing(progress);
    let receipt = engine::install(game_dir, &plan)?;
    crate::fs::stamp_mtime_best_effort(
        &game_dir.join(&file_name),
        download_last_modified.as_deref(),
        None,
    );

    let source = TrackedSource::new(
        TrackedSourceRole::DlssFix,
        super::source::dlss_fix_url(arch),
        download.etag,
        download.digest,
    )
    .with_last_modified(download.last_modified);
    let updated =
        tracking::rebuild_after_receipt(&record, &receipt, None, Some(source), "DLSS-Fix rebuild")?;
    context.storage().upsert_installed_addon(&updated)?;

    Ok(tracking::install_state_from_record(&updated))
}

/// Removes the DLSS-Fix companion add-on, leaving the main RenoDX install intact.
///
/// Deletes the `renodx-dlssfix.addon*` file and merges `ReShade.ini` to remove
/// `LoadFromDllMain` from `[ADDON]` and the entire `[RENODX-DLSSFIX]` section.
pub fn uninstall_dlss_fix(
    context: &Context,
    game_id: &GameId,
) -> Result<RenoDxInstallState, ServiceError> {
    let _guard = operation_lock::blocking_lock(game_id);
    let record = context
        .storage()
        .get_installed_addon(game_id)?
        .ok_or_else(|| errors::invalid("RenoDX is not installed for this game".to_owned()))?;

    if !record.has_dlss_fix() {
        return Err(errors::invalid(
            "DLSS-Fix is not installed for this game".to_owned(),
        ));
    }

    let dll_path = dlss_fix_file_path(&record)
        .ok_or_else(|| errors::invalid("DLSS-Fix file not found in install record".to_owned()))?;
    let game_dir = dll_path
        .parent()
        .ok_or_else(|| errors::invalid("dlss-fix path has no parent".to_owned()))?;

    let strategy = reshade_ini::ini_remove_dlss_fix_strategy();
    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::Remove {
                name: dll_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            },
            FileOp::UpdateText {
                name: reshade::RESHADE_INI_FILE_NAME.to_owned(),
                default: String::new(),
                strategy,
            },
        ],
    };

    let receipt = engine::install(game_dir, &plan)?;

    let updated = tracking::rebuild_after_receipt(
        &record,
        &receipt,
        Some((&dll_path, TrackedSourceRole::DlssFix)),
        None,
        "DLSS-Fix rebuild",
    )?;
    context.storage().upsert_installed_addon(&updated)?;

    Ok(tracking::install_state_from_record(&updated))
}

/// Returns whether a DLSS-Fix can be installed for this game (RenoDX installed +
/// FG + DLSS + Streamline detected). A read-only preview for the UI.
pub fn dlss_fix_availability(context: &Context, game_id: &GameId) -> Result<bool, ServiceError> {
    let Some(record) = context.storage().get_installed_addon(game_id)? else {
        return Ok(false);
    };
    if record.has_dlss_fix() {
        return Ok(false);
    }
    Ok(resolve_dlss_fix(context.storage(), game_id)?.is_some())
}

/// Whether an install may proceed given its risk and the user's confirmation.
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
            source: None,
            detected_locally: false,
        }
    }

    #[test]
    fn safe_risk_proceeds_without_confirmation() {
        assert_eq!(gate(&risk(RiskSeverity::Info), false), InstallGate::Proceed);
    }

    #[test]
    fn addon_arch_invariant_rejects_a_bitness_mismatch() {
        // The add-on must match the resolved host's bitness; a curated x64 title
        // with a 32-bit add-on is the case detection alone could miss.
        assert!(ensure_addon_arch(Architecture::X64, Architecture::X64).is_ok());
        let error = ensure_addon_arch(Architecture::X86, Architecture::X64)
            .expect_err("a 32-bit add-on for a 64-bit host must be rejected");
        assert!(matches!(error, ServiceError::InvalidInput(_)));
    }

    #[test]
    fn expected_addon_file_name_uses_resolved_canonical_slug() {
        let plan = ResolvedInstall {
            slug: "unityengine".to_owned(),
            addon_url: "https://example.com/renodx-unityengine.addon64".to_owned(),
            arch: Architecture::X64,
            proxy_dll_name: "dxgi.dll".to_owned(),
            risk: generic_risk(),
            confidence: MatchConfidence::Verified,
            notes_keys: Vec::new(),
        };
        let resolution = RenoDxResolution::Installable(Box::new(plan));

        assert_eq!(
            expected_addon_file_name(&resolution).as_deref(),
            Some("renodx-unityengine.addon64")
        );
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
}
