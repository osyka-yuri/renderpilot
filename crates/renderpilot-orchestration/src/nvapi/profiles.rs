//! Explicit lifecycle operations for one RenderPilot-owned NVIDIA profile.

use std::path::Path;

use renderpilot_nvapi::{
    ApplicationIdentity, DrsSession, Nvapi, NvapiError, Profile, ProfileIdentity, SettingIdentity,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{Context, ServiceError};

/// Stable status for the profile card shown on every Windows game details page.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NvapiProfileState {
    /// No executable is currently selected or detected.
    NoExecutable,
    /// NVAPI could not be loaded or initialized.
    NvapiUnavailable,
    /// Full-path executable lookup returned the documented ambiguous result.
    Ambiguous,
    /// A non-absence NVAPI failure prevented classification.
    Error,
    /// The selected full path is proven absent from DRS.
    Missing,
    /// The path resolves to a predefined NVIDIA profile.
    Predefined,
    /// The path resolves to a user profile not owned by RenderPilot.
    External,
    /// This game owns the exact profile and full-path binding.
    Owned,
    /// A different catalog game owns the resolved RenderPilot profile.
    OwnedByAnotherGame,
    /// The receipt and currently observed DRS state no longer match.
    Conflict,
    /// A durable mutation affecting this profile or path remains unresolved.
    Pending,
}

/// Complete profile card response. Paths are normalized full paths.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NvapiProfileStatusDto {
    /// Current shared executable selection, or `None` when the game has no executable.
    pub selected_executable: Option<String>,
    /// The durable RenderPilot profile binding, if this game owns one.
    pub binding_path: Option<String>,
    /// Exact NVIDIA DRS profile name, when lookup succeeded.
    pub profile_name: Option<String>,
    /// Current classification.
    pub state: NvapiProfileState,
    /// Whether the resolved DRS profile is predefined.
    pub is_predefined: Option<bool>,
    /// Whether this game owns the profile receipt.
    pub owned_by_this_game: bool,
    /// Whether an explicit create action is safe and available.
    pub can_create: bool,
    /// Whether an explicit delete action is safe and available.
    pub can_delete: bool,
    /// Pending operation kind, when this path/profile is fenced.
    pub pending_operation: Option<String>,
    /// Catalog game whose pending operation currently fences this profile.
    /// `None` means the operation is global or there is no pending operation.
    pub pending_operation_game_id: Option<String>,
    /// Safe user-facing diagnosis for error/conflict states.
    pub detail: Option<String>,
}

impl NvapiProfileStatusDto {
    fn new(path: Option<String>, binding_path: Option<String>) -> Self {
        Self {
            selected_executable: path,
            binding_path,
            profile_name: None,
            state: NvapiProfileState::Error,
            is_predefined: None,
            owned_by_this_game: false,
            can_create: false,
            can_delete: false,
            pending_operation: None,
            pending_operation_game_id: None,
            detail: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProfileTransition {
    Before,
    After,
    Conflict,
    Pending,
}

/// A missing exact-state result means a DRS lookup, identity read, or enum read
/// failed, so the durable intent must remain pending.
fn classify_profile_transition(
    before_exact: Option<bool>,
    after_exact: Option<bool>,
) -> ProfileTransition {
    match (before_exact, after_exact) {
        (None, _) | (_, None) | (Some(true), Some(true)) => ProfileTransition::Pending,
        (Some(true), Some(false)) => ProfileTransition::Before,
        (Some(false), Some(true)) => ProfileTransition::After,
        (Some(false), Some(false)) => ProfileTransition::Conflict,
    }
}

mod create;
mod delete;
mod move_profile;
mod status;
#[cfg(test)]
mod tests;

pub(super) use create::create_owned_profile;
pub(super) use delete::delete_owned_profile;
pub(super) use move_profile::move_owned_profile;
pub(super) use status::profile_status;
fn open_drs_session() -> Result<DrsSession<'static>, NvapiError> {
    let nvapi = Nvapi::get().ok_or(NvapiError::DriverUnavailable)?;
    nvapi.initialize()?;
    nvapi.create_session()
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "Result::map_err passes the owned NvapiError to this shared adapter"
)]
fn map_drs(error: NvapiError) -> ServiceError {
    ServiceError::command_failed(format!("NVIDIA DRS operation failed: {error}"))
}

fn validate_game_executable(
    context: &Context,
    game_id: &str,
    requested_path: &str,
) -> Result<(std::path::PathBuf, String), ServiceError> {
    let game = super::resolve::load_game_with_context(context, game_id)?;
    let install = Path::new(game.install_path().as_str());
    let canonical_install = crate::paths::canonicalize_existing(install).map_err(|error| {
        ServiceError::command_failed(format!("could not resolve game installation path: {error}"))
    })?;
    let canonical =
        crate::paths::canonicalize_existing(Path::new(requested_path)).map_err(|error| {
            ServiceError::command_failed(format!(
                "could not resolve executable path {requested_path}: {error}"
            ))
        })?;
    if !crate::game_executable::is_existing_executable_file(&canonical) {
        return Err(ServiceError::command_failed(
            "NVIDIA profile executables must be existing .exe files",
        ));
    }
    if !canonical.starts_with(&canonical_install) {
        return Err(ServiceError::command_failed(
            "NVIDIA profile executables must remain inside the game's installation directory",
        ));
    }
    let canonical = crate::paths::strip_windows_verbatim_prefix(&canonical).map_err(|error| {
        ServiceError::command_failed(format!(
            "could not convert executable path to a supported NVIDIA DRS path: {error}"
        ))
    })?;
    let basename = canonical
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ServiceError::command_failed("executable path has no file name"))?
        .to_owned();
    Ok((canonical, basename))
}

fn generated_profile_name(title: &str, game_id: &str) -> String {
    let digest = Sha256::digest(game_id.as_bytes());
    let short = hex::encode(&digest[..6]);
    let title = title
        .chars()
        .filter(|character| !character.is_control())
        .take(96)
        .collect::<String>();
    format!("{title} [{short}]")
}

fn composition_json(
    mut applications: Vec<ApplicationIdentity>,
    mut settings: Vec<SettingIdentity>,
) -> Result<String, ServiceError> {
    applications.sort_by_cached_key(|application| application.app_name.to_lowercase());
    settings.sort_by_key(|setting| setting.id);
    serde_json::to_string(&serde_json::json!({"applications": applications, "settings": settings}))
        .map_err(|error| {
            ServiceError::command_failed(format!(
                "could not encode NVIDIA profile composition: {error}"
            ))
        })
}

fn verify_owned_composition(
    owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
    profile: &Profile<'_>,
    game_id: &str,
) -> Result<(ProfileIdentity, String), ServiceError> {
    let observed = read_profile_receipts(profile)?;
    if !owned_profile_receipt_matches(
        owner,
        &observed.identity,
        &observed.witness,
        &observed.witness_json,
        &observed.composition,
    ) {
        return Err(ServiceError::command_failed(format!(
            "RenderPilot-owned NVIDIA profile changed since its last confirmation for game {game_id}; deletion is blocked"
        )));
    }
    Ok((observed.identity, observed.composition))
}

struct ProfileReceipts {
    identity: ProfileIdentity,
    witness: ApplicationIdentity,
    witness_json: String,
    applications: Vec<ApplicationIdentity>,
    settings: Vec<SettingIdentity>,
    composition: String,
}

fn read_profile_receipts(profile: &Profile<'_>) -> Result<ProfileReceipts, ServiceError> {
    let identity = profile.identity().map_err(map_drs)?;
    let witness = profile.matched_application().ok_or_else(|| {
        ServiceError::command_failed("full-path lookup returned no application witness")
    })?;
    let witness_json = serde_json::to_string(&witness)
        .map_err(|error| ServiceError::command_failed(error.to_string()))?;
    let applications = profile.applications().map_err(map_drs)?;
    let settings = profile.settings().map_err(map_drs)?;
    let composition = composition_json(applications.clone(), settings.clone())?;
    Ok(ProfileReceipts {
        identity,
        witness,
        witness_json,
        applications,
        settings,
        composition,
    })
}

fn owned_profile_receipt_matches(
    owner: &renderpilot_storage_sqlite::NvapiOwnedProfileRow,
    identity: &ProfileIdentity,
    witness: &ApplicationIdentity,
    witness_json: &str,
    composition: &str,
) -> bool {
    owner.state == "active"
        && !identity.is_predefined
        && identity.name == owner.profile_name
        && renderpilot_domain::normalized_path_key(&witness.app_name)
            == renderpilot_domain::normalized_path_key(&owner.binding_path)
        && witness_json == owner.application_witness_json
        && composition == owner.composition_json
}

fn selected_executable_allows_owned_delete(
    selected_path: Option<&str>,
    binding_path: &str,
) -> Result<bool, ServiceError> {
    if selected_path.is_none_or(|path| {
        renderpilot_domain::normalized_path_key(path)
            == renderpilot_domain::normalized_path_key(binding_path)
    }) {
        return Ok(true);
    }
    match std::fs::metadata(binding_path) {
        Ok(metadata) => Ok(!metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(ServiceError::command_failed(format!(
            "could not verify whether the owned executable binding still exists: {error}"
        ))),
    }
}

fn matching_pending(
    context: &Context,
    target_id: Option<&str>,
    path: &str,
) -> Result<Option<renderpilot_storage_sqlite::NvapiPendingOperationRow>, ServiceError> {
    let mut path_key = None;
    Ok(context
        .storage()
        .list_pending_nvapi_operations()?
        .into_iter()
        .find(|operation| {
            if operation
                .target_id
                .as_deref()
                .is_some_and(|target| Some(target) == target_id)
            {
                return true;
            }
            let path_key =
                path_key.get_or_insert_with(|| renderpilot_domain::normalized_path_key(path));
            operation_references_path(&operation.before_json, path_key)
                || operation_references_path(&operation.after_json, path_key)
        }))
}

fn pending_operation_detail(pending_game_id: Option<&str>, current_game_id: &str) -> String {
    match pending_game_id {
        Some(owner) if owner != current_game_id => format!(
            "An unresolved NVIDIA operation for game {owner} blocks this shared profile. Open that game's details page to retry automatic recovery."
        ),
        Some(_) => "An NVIDIA operation for this game remains unresolved. Its before and expected states are preserved; reopening the game's NVIDIA profile page retries automatic recovery.".to_owned(),
        None => "A global NVIDIA operation remains unresolved and blocks this shared profile. Open NVIDIA settings to retry automatic recovery.".to_owned(),
    }
}

fn has_pending_for_path(
    context: &Context,
    target_id: Option<&str>,
    path: &str,
) -> Result<bool, ServiceError> {
    Ok(matching_pending(context, target_id, path)?.is_some())
}

fn operation_references_path(snapshot: &str, path_key: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(snapshot) else {
        return false;
    };
    ["binding_path", "executable_path", "old_path", "new_path"]
        .into_iter()
        .filter_map(|key| value.get(key).and_then(serde_json::Value::as_str))
        .any(|path| renderpilot_domain::normalized_path_key(path) == path_key)
}
