//! Resolves executable facts and applies the shared Microsoft runtime policy.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_application::{
    AppError, AppResult, D3d12ExecutableProfile, D3d12ExecutableSnapshot, SwapTargetProfile,
    ensure_replacement_compatible,
};
use renderpilot_domain::{
    ComponentRollbackBaseline, D3d12ExecutableIdentity, GameInstallation, GraphicsComponent,
    LibraryArtifact, PathRef, Sha256Hash,
};

use crate::Context;

pub(super) const D3D12_SDK_VERSION_EXPORT: &str = "D3D12SDKVersion";

/// Fresh profile plus byte identities needed for planning and stale-state checks.
#[derive(Debug, Clone)]
pub(super) struct TargetProfileAssessment {
    pub(super) profile: SwapTargetProfile,
    pub(super) d3d12: Option<D3d12ExecutableState>,
}

/// Read-only state of the executable and its immutable sidecar.
#[derive(Debug, Clone)]
pub(super) struct D3d12ExecutableState {
    pub(super) executable_path: PathBuf,
    pub(super) backup_path: PathBuf,
    pub(super) original_sha256: Sha256Hash,
    pub(super) current_sha256: Sha256Hash,
    pub(super) original_sdk_version: u32,
    pub(super) current_sdk_version: u32,
    pub(super) backup_exists: bool,
    pub(super) repair_required: bool,
}

/// Lightweight facts used only to render candidate actions and current status.
///
/// This path performs bounded PE reads and cheap metadata checks. It never
/// produces confirmation hashes; every mutation plan re-runs [`target_profile`].
#[derive(Debug, Clone)]
pub(super) struct D3d12ExecutablePresentationState {
    pub(super) executable_path: PathBuf,
    pub(super) backup_path: PathBuf,
    pub(super) original_sdk_version: u32,
    pub(super) current_sdk_version: u32,
    pub(super) repair_required: bool,
    pub(super) selection_locked: bool,
}

/// Lightweight target profile for catalog read models.
#[derive(Debug, Clone)]
pub(super) struct PresentationTargetProfileAssessment {
    pub(super) profile: SwapTargetProfile,
    pub(super) d3d12: Option<D3d12ExecutablePresentationState>,
}

struct ResolvedExecutableTarget {
    recorded: Option<ComponentRollbackBaseline>,
    aggregate_unavailable: bool,
    executable_path: Option<PathBuf>,
    architecture: Option<renderpilot_domain::Architecture>,
}

/// Resolves the selected executable once and, for D3D12, derives original/current facts.
pub(super) fn target_profile(
    context: &Context,
    game: &GameInstallation,
    d3d12_component: Option<&GraphicsComponent>,
) -> AppResult<TargetProfileAssessment> {
    let resolved_target = resolve_executable_target(context, game, d3d12_component)?;
    let ResolvedExecutableTarget {
        recorded,
        aggregate_unavailable,
        executable_path,
        architecture,
    } = resolved_target;

    let Some(component) = d3d12_component else {
        return Ok(TargetProfileAssessment {
            profile: SwapTargetProfile::new(architecture, None),
            d3d12: None,
        });
    };
    let Some(executable_path) = executable_path else {
        return Ok(TargetProfileAssessment {
            profile: SwapTargetProfile::new(architecture, None),
            d3d12: None,
        });
    };

    let recorded_executable = recorded
        .as_ref()
        .and_then(ComponentRollbackBaseline::d3d12_executable);
    let mut state = assess_d3d12_executable(&executable_path, recorded.as_ref())?;
    state.repair_required |= aggregate_unavailable;
    state.repair_required |= recorded_executable.is_none()
        && state.backup_exists
        && !is_unique_valid_executable_pair(
            &executable_path,
            game.executable_candidates()
                .iter()
                .map(|path| Path::new(path.as_str())),
        );
    state.repair_required |= recorded_executable.is_none()
        && state.backup_exists
        && state.current_sha256 != state.original_sha256
        && !has_complete_d3d12_dll_sidecars(component, recorded.as_ref());
    let executable_ref = path_ref(&executable_path, "executable")?;
    let backup_ref = path_ref(&state.backup_path, "backup")?;
    let managed = D3d12ExecutableSnapshot::new(
        executable_ref,
        backup_ref,
        D3d12ExecutableIdentity::new(state.original_sdk_version, state.original_sha256.clone()),
        D3d12ExecutableIdentity::new(state.current_sdk_version, state.current_sha256.clone()),
        state.backup_exists,
        state.repair_required,
    );
    let profile = SwapTargetProfile::new(architecture, Some(state.current_sdk_version))
        .with_d3d12_executable_snapshot(managed);

    Ok(TargetProfileAssessment {
        profile,
        d3d12: Some(state),
    })
}

/// Resolves a read-model profile without hashing or loading complete EXE files.
pub(super) fn presentation_target_profile(
    context: &Context,
    game: &GameInstallation,
    d3d12_component: Option<&GraphicsComponent>,
) -> AppResult<PresentationTargetProfileAssessment> {
    let ResolvedExecutableTarget {
        recorded,
        aggregate_unavailable,
        executable_path,
        architecture,
    } = resolve_executable_target(context, game, d3d12_component)?;

    let Some(component) = d3d12_component else {
        return Ok(PresentationTargetProfileAssessment {
            profile: SwapTargetProfile::new(architecture, None),
            d3d12: None,
        });
    };
    let Some(executable_path) = executable_path else {
        return Ok(PresentationTargetProfileAssessment {
            profile: SwapTargetProfile::new(architecture, None),
            d3d12: None,
        });
    };

    let backup_path = crate::fs::backup_path(&executable_path)
        .map_err(|error| AppError::invalid_input(error.to_string()))?;
    let recorded_executable = recorded
        .as_ref()
        .and_then(ComponentRollbackBaseline::d3d12_executable);
    let current_sdk =
        renderpilot_detection::read_pe_exported_u32(&executable_path, D3D12_SDK_VERSION_EXPORT);
    let original_sdk = if backup_path.is_file() {
        renderpilot_detection::read_pe_exported_u32(&backup_path, D3D12_SDK_VERSION_EXPORT)
    } else {
        None
    };
    let original_sdk_version = recorded_executable
        .map(|baseline| baseline.original().sdk_version())
        .or(original_sdk)
        .or(current_sdk)
        .unwrap_or_default();
    let current_sdk_version = current_sdk
        .or_else(|| recorded_executable.map(|baseline| baseline.expected_active().sdk_version()))
        .unwrap_or(original_sdk_version);
    let backup_claim_exists = backup_path.exists();
    let backup_exists = backup_path.is_file();
    let file_sizes_match = fs::metadata(&executable_path)
        .and_then(|live| fs::metadata(&backup_path).map(|backup| live.len() == backup.len()))
        .unwrap_or(!backup_claim_exists);

    let mut repair_required = aggregate_unavailable || current_sdk.is_none();
    repair_required |=
        backup_claim_exists && (!backup_exists || original_sdk.is_none() || !file_sizes_match);
    if let Some(baseline) = recorded_executable {
        repair_required |= !crate::paths::same_path(
            Path::new(baseline.executable_path().as_str()),
            &executable_path,
        );
        repair_required |= !backup_exists;
        repair_required |= original_sdk != Some(baseline.original().sdk_version());
        repair_required |= current_sdk != Some(baseline.expected_active().sdk_version());
    } else if backup_exists {
        repair_required |= !is_unique_valid_executable_pair_for_presentation(
            &executable_path,
            game.executable_candidates()
                .iter()
                .map(|path| Path::new(path.as_str())),
        );
        repair_required |= current_sdk != original_sdk
            && !has_complete_d3d12_dll_sidecars(component, recorded.as_ref());
    }

    let presentation = D3d12ExecutablePresentationState {
        executable_path: executable_path.clone(),
        backup_path: backup_path.clone(),
        original_sdk_version,
        current_sdk_version,
        repair_required,
        selection_locked: recorded_executable.is_some(),
    };
    let managed = D3d12ExecutableProfile::new(
        path_ref(&executable_path, "executable")?,
        path_ref(&backup_path, "backup")?,
        original_sdk_version,
        current_sdk_version,
        backup_exists,
        repair_required,
    );
    let profile = SwapTargetProfile::new(architecture, Some(current_sdk_version))
        .with_d3d12_executable_profile(managed);

    Ok(PresentationTargetProfileAssessment {
        profile,
        d3d12: Some(presentation),
    })
}

fn resolve_executable_target(
    context: &Context,
    game: &GameInstallation,
    d3d12_component: Option<&GraphicsComponent>,
) -> AppResult<ResolvedExecutableTarget> {
    let (recorded, aggregate_unavailable) = match d3d12_component
        .map(|component| {
            crate::coordinated_files::load_component_backup_availability(
                context.storage(),
                component,
            )
        })
        .transpose()?
    {
        Some(crate::coordinated_files::ComponentBackupAvailability::Available(baseline)) => {
            (Some(baseline), false)
        }
        Some(crate::coordinated_files::ComponentBackupAvailability::Unavailable(baseline)) => {
            (Some(baseline), true)
        }
        Some(crate::coordinated_files::ComponentBackupAvailability::NotRecorded) | None => {
            (None, false)
        }
    };
    let recorded_executable = recorded
        .as_ref()
        .and_then(ComponentRollbackBaseline::d3d12_executable);

    let override_path = crate::addons::game_context::executable_override(context, game.id());
    let pinned_path = recorded_executable
        .map(|baseline| PathBuf::from(baseline.executable_path().as_str()))
        .or_else(|| override_path.filter(|path| path.is_file()));
    let resolved = crate::game_executable::resolve_primary_executable(
        Path::new(game.install_path().as_str()),
        pinned_path.as_deref(),
        true,
    );
    let executable_path = pinned_path
        .or_else(|| {
            resolved
                .as_ref()
                .map(|executable| PathBuf::from(executable.path.as_str()))
        })
        .or_else(|| unique_known_executable_candidate(game.executable_candidates()));
    let architecture = executable_path
        .as_deref()
        .and_then(|path| renderpilot_detection::analyze_executable(path).architecture());
    Ok(ResolvedExecutableTarget {
        recorded,
        aggregate_unavailable,
        executable_path,
        architecture,
    })
}

/// Fail-closed fallback for persisted scans when native executable discovery is
/// unavailable (for example while validating the Windows catalog on Linux).
///
/// A single readable PE with a known architecture is unambiguous. Multiple
/// candidates remain unresolved rather than guessing which executable owns the
/// runtime.
fn unique_known_executable_candidate(candidates: &[PathRef]) -> Option<PathBuf> {
    let mut known = candidates.iter().filter_map(|candidate| {
        let path = PathBuf::from(candidate.as_str());
        renderpilot_detection::analyze_executable(&path)
            .architecture()
            .is_some()
            .then_some(path)
    });
    let selected = known.next()?;
    known.next().is_none().then_some(selected)
}

fn path_ref(path: &Path, kind: &str) -> AppResult<PathRef> {
    PathRef::new(path.to_string_lossy().into_owned())
        .map_err(|error| AppError::invalid_input(format!("invalid {kind} path: {error}")))
}

fn is_unique_valid_executable_pair<'a>(
    selected: &Path,
    candidates: impl IntoIterator<Item = &'a Path>,
) -> bool {
    let mut paths = std::iter::once(selected.to_path_buf())
        .chain(candidates.into_iter().map(Path::to_path_buf))
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(crate::paths::normalized_key(path)));
    let valid = paths
        .into_iter()
        .filter(|path| {
            assess_d3d12_executable(path, None)
                .is_ok_and(|state| state.backup_exists && !state.repair_required)
        })
        .collect::<Vec<_>>();
    valid.len() == 1 && crate::paths::same_path(&valid[0], selected)
}

fn is_unique_valid_executable_pair_for_presentation<'a>(
    selected: &Path,
    candidates: impl IntoIterator<Item = &'a Path>,
) -> bool {
    let mut paths = std::iter::once(selected.to_path_buf())
        .chain(candidates.into_iter().map(Path::to_path_buf))
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(crate::paths::normalized_key(path)));
    let valid = paths
        .into_iter()
        .filter(|path| {
            let Ok(backup) = crate::fs::backup_path(path) else {
                return false;
            };
            path.is_file()
                && backup.is_file()
                && renderpilot_detection::read_pe_exported_u32(path, D3D12_SDK_VERSION_EXPORT)
                    .is_some()
                && renderpilot_detection::read_pe_exported_u32(&backup, D3D12_SDK_VERSION_EXPORT)
                    .is_some()
        })
        .collect::<Vec<_>>();
    valid.len() == 1 && crate::paths::same_path(&valid[0], selected)
}

fn has_complete_d3d12_dll_sidecars(
    component: &GraphicsComponent,
    recorded: Option<&ComponentRollbackBaseline>,
) -> bool {
    let files = recorded.map_or(component.files(), ComponentRollbackBaseline::files);
    !files.is_empty()
        && files.iter().all(|file| {
            crate::fs::backup_path(Path::new(file.path().as_str()))
                .is_ok_and(|path| crate::fs::is_readable_non_empty_file(&path))
        })
}

/// Enforces transition compatibility against one already-fresh assessment.
pub(super) fn ensure_transition_compatible(
    component: &GraphicsComponent,
    artifact: &LibraryArtifact,
    assessment: &TargetProfileAssessment,
) -> AppResult<()> {
    ensure_replacement_compatible(component, artifact, &assessment.profile).map_err(|error| {
        AppError::invalid_input(format!("runtime artifact is incompatible: {error}"))
    })
}

pub(super) fn assess_d3d12_executable(
    executable_path: &Path,
    recorded: Option<&ComponentRollbackBaseline>,
) -> AppResult<D3d12ExecutableState> {
    let backup_path = crate::fs::backup_path(executable_path)
        .map_err(|error| AppError::invalid_input(error.to_string()))?;
    let recorded_executable = recorded.and_then(ComponentRollbackBaseline::d3d12_executable);
    let (live_bytes, live_read_failed) = match fs::read(executable_path) {
        Ok(bytes) => (bytes, false),
        Err(_) if recorded_executable.is_some() => (Vec::new(), true),
        Err(error) => {
            return Err(AppError::invalid_input(format!(
                "cannot read D3D12 executable {}: {error}",
                executable_path.display()
            )));
        }
    };
    let live_export =
        renderpilot_detection::pe_exported_u32_from_bytes(&live_bytes, D3D12_SDK_VERSION_EXPORT);
    let current_sha256 = if live_read_failed {
        recorded_executable
            .expect("tracked unreadable executable has a baseline")
            .expected_active()
            .sha256()
            .clone()
    } else {
        renderpilot_detection::sha256_bytes(&live_bytes)?
    };

    let backup_claim_exists = backup_path.exists();
    let backup_exists = backup_path.is_file();
    let (original_bytes, original_sha256, original_sdk_version, mut repair_required) =
        if backup_claim_exists {
            let (bytes, backup_read_failed) = match fs::read(&backup_path) {
                Ok(bytes) => (bytes, false),
                Err(_) if recorded_executable.is_some() => (Vec::new(), true),
                Err(_) => (Vec::new(), true),
            };
            if backup_read_failed || bytes.is_empty() {
                recorded_executable.map_or_else(
                    || {
                        (
                            Vec::new(),
                            current_sha256.clone(),
                            live_export.as_ref().map_or(0, |value| value.value),
                            true,
                        )
                    },
                    |baseline| {
                        (
                            bytes,
                            baseline.original().sha256().clone(),
                            baseline.original().sdk_version(),
                            true,
                        )
                    },
                )
            } else {
                let sha256 = renderpilot_detection::sha256_bytes(&bytes)?;
                let export = renderpilot_detection::pe_exported_u32_from_bytes(
                    &bytes,
                    D3D12_SDK_VERSION_EXPORT,
                );
                let sdk = export
                    .as_ref()
                    .map(|value| value.value)
                    .or_else(|| recorded_executable.map(|value| value.original().sdk_version()))
                    .unwrap_or_default();
                (bytes, sha256, sdk, export.is_none())
            }
        } else if let Some(baseline) = recorded_executable {
            (
                Vec::new(),
                baseline.original().sha256().clone(),
                baseline.original().sdk_version(),
                true,
            )
        } else {
            let sdk = live_export
                .as_ref()
                .map(|value| value.value)
                .unwrap_or_default();
            (
                live_bytes.clone(),
                current_sha256.clone(),
                sdk,
                live_export.is_none(),
            )
        };
    let current_sdk_version = live_export
        .as_ref()
        .map(|value| value.value)
        .or_else(|| recorded_executable.map(|value| value.expected_active().sdk_version()))
        .unwrap_or(original_sdk_version);
    repair_required |= live_read_failed;

    if backup_claim_exists {
        repair_required |= !differs_only_at_sdk_export(&original_bytes, &live_bytes);
    }
    if let Some(baseline) = recorded_executable {
        repair_required |= baseline.executable_path().as_str()
            != PathRef::new(executable_path.to_string_lossy().into_owned())
                .map_err(|error| AppError::invalid_input(error.to_string()))?
                .as_str();
        repair_required |= baseline.original().sha256() != &original_sha256;
        repair_required |= baseline.original().sdk_version() != original_sdk_version;
        repair_required |= baseline.expected_active().sha256() != &current_sha256;
        repair_required |= baseline.expected_active().sdk_version() != current_sdk_version;
        repair_required |= !backup_exists;
    }

    Ok(D3d12ExecutableState {
        executable_path: executable_path.to_path_buf(),
        backup_path,
        original_sha256,
        current_sha256,
        original_sdk_version,
        current_sdk_version,
        backup_exists,
        repair_required,
    })
}

pub(super) fn differs_only_at_sdk_export(original: &[u8], live: &[u8]) -> bool {
    if original.len() != live.len() {
        return false;
    }
    let Some(original_export) =
        renderpilot_detection::pe_exported_u32_from_bytes(original, D3D12_SDK_VERSION_EXPORT)
    else {
        return false;
    };
    let Some(live_export) =
        renderpilot_detection::pe_exported_u32_from_bytes(live, D3D12_SDK_VERSION_EXPORT)
    else {
        return false;
    };
    if original_export.file_offset != live_export.file_offset {
        return false;
    }
    let offset = original_export.file_offset;
    original[..offset] == live[..offset] && original[offset + 4..] == live[offset + 4..]
}

#[cfg(test)]
pub(super) fn synthetic_d3d12_executable(sdk_version: u32) -> Vec<u8> {
    const PE_OFFSET: usize = 0x80;
    const COFF_HEADER_LEN: usize = 20;
    const OPTIONAL_HEADER_SIZE: usize = 0xf0;
    const PE32_PLUS_DATA_DIRECTORY_OFFSET: usize = 0x70;
    const SECTION_HEADER_LEN: usize = 40;
    const EXPORT_DIRECTORY_LEN: usize = 40;
    const SECTION_RVA: u32 = 0x1000;

    let coff_offset = PE_OFFSET + 4;
    let optional_header_offset = coff_offset + COFF_HEADER_LEN;
    let section_table_offset = optional_header_offset + OPTIONAL_HEADER_SIZE;
    let headers_end = section_table_offset + SECTION_HEADER_LEN;
    let section_raw_pointer = (headers_end as u32).div_ceil(0x200) * 0x200;
    let name = b"D3D12SDKVersion\0";
    let functions_offset = EXPORT_DIRECTORY_LEN;
    let names_offset = functions_offset + 4;
    let ordinals_offset = names_offset + 4;
    let string_offset = ordinals_offset + 2;
    let value_offset = string_offset + name.len();
    let mut section = vec![0u8; value_offset + 4];

    section[20..24].copy_from_slice(&1u32.to_le_bytes());
    section[24..28].copy_from_slice(&1u32.to_le_bytes());
    section[28..32].copy_from_slice(&(SECTION_RVA + functions_offset as u32).to_le_bytes());
    section[32..36].copy_from_slice(&(SECTION_RVA + names_offset as u32).to_le_bytes());
    section[36..40].copy_from_slice(&(SECTION_RVA + ordinals_offset as u32).to_le_bytes());
    section[functions_offset..functions_offset + 4]
        .copy_from_slice(&(SECTION_RVA + value_offset as u32).to_le_bytes());
    section[names_offset..names_offset + 4]
        .copy_from_slice(&(SECTION_RVA + string_offset as u32).to_le_bytes());
    section[ordinals_offset..ordinals_offset + 2].copy_from_slice(&0u16.to_le_bytes());
    section[string_offset..string_offset + name.len()].copy_from_slice(name);
    section[value_offset..value_offset + 4].copy_from_slice(&sdk_version.to_le_bytes());

    let mut bytes = vec![0u8; section_raw_pointer as usize + section.len()];
    bytes[0..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&(PE_OFFSET as u32).to_le_bytes());
    bytes[PE_OFFSET..PE_OFFSET + 4].copy_from_slice(b"PE\0\0");
    bytes[coff_offset..coff_offset + 2].copy_from_slice(&0x8664u16.to_le_bytes());
    bytes[coff_offset + 2..coff_offset + 4].copy_from_slice(&1u16.to_le_bytes());
    bytes[coff_offset + 16..coff_offset + 18]
        .copy_from_slice(&(OPTIONAL_HEADER_SIZE as u16).to_le_bytes());
    bytes[optional_header_offset..optional_header_offset + 2]
        .copy_from_slice(&0x20bu16.to_le_bytes());
    let export_directory = optional_header_offset + PE32_PLUS_DATA_DIRECTORY_OFFSET;
    bytes[export_directory..export_directory + 4].copy_from_slice(&SECTION_RVA.to_le_bytes());
    bytes[export_directory + 4..export_directory + 8]
        .copy_from_slice(&(EXPORT_DIRECTORY_LEN as u32).to_le_bytes());
    bytes[section_table_offset..section_table_offset + 8].copy_from_slice(b".edata\0\0");
    bytes[section_table_offset + 8..section_table_offset + 12]
        .copy_from_slice(&(section.len() as u32).to_le_bytes());
    bytes[section_table_offset + 12..section_table_offset + 16]
        .copy_from_slice(&SECTION_RVA.to_le_bytes());
    bytes[section_table_offset + 16..section_table_offset + 20]
        .copy_from_slice(&(section.len() as u32).to_le_bytes());
    bytes[section_table_offset + 20..section_table_offset + 24]
        .copy_from_slice(&section_raw_pointer.to_le_bytes());
    bytes[section_raw_pointer as usize..].copy_from_slice(&section);
    bytes
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{
        ComponentRollbackBaseline, D3d12ExecutableBaseline, D3d12ExecutableIdentity, PathRef,
    };

    use super::{
        D3D12_SDK_VERSION_EXPORT, assess_d3d12_executable, differs_only_at_sdk_export,
        is_unique_valid_executable_pair, synthetic_d3d12_executable,
        unique_known_executable_candidate,
    };

    #[test]
    fn arbitrary_bytes_are_not_a_managed_executable_pair() {
        assert!(!differs_only_at_sdk_export(b"same", b"same"));
        assert!(!differs_only_at_sdk_export(b"short", b"longer"));
    }

    #[test]
    fn assessment_uses_backup_as_original_and_detects_external_changes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let executable = dir.path().join("game.exe");
        let backup = dir.path().join("game.exe.bak");
        let original = synthetic_d3d12_executable(606);
        let mut patched = original.clone();
        renderpilot_detection::replace_pe_exported_u32_in_bytes(
            &mut patched,
            D3D12_SDK_VERSION_EXPORT,
            606,
            619,
        )
        .expect("patch fixture");
        std::fs::write(&executable, &patched).expect("live");
        std::fs::write(&backup, &original).expect("backup");

        let state = assess_d3d12_executable(&executable, None).expect("assessment");
        assert_eq!(state.original_sdk_version, 606);
        assert_eq!(state.current_sdk_version, 619);
        assert!(!state.repair_required);

        patched[2] ^= 1;
        std::fs::write(&executable, &patched).expect("externally changed live");
        let changed = assess_d3d12_executable(&executable, None).expect("changed assessment");
        assert!(changed.repair_required);
    }

    #[test]
    fn missing_tracked_executable_is_reported_as_repair_instead_of_overwritten() {
        let dir = tempfile::tempdir().expect("temp dir");
        let executable = dir.path().join("missing-game.exe");
        let original = synthetic_d3d12_executable(606);
        let original_hash = renderpilot_detection::sha256_bytes(&original).expect("original hash");
        let expected_active_hash = original_hash.clone();
        let baseline = ComponentRollbackBaseline::new(Vec::new()).with_d3d12_executable(
            D3d12ExecutableBaseline::new(
                PathRef::new(executable.to_string_lossy().into_owned()).expect("path"),
                D3d12ExecutableIdentity::new(606, original_hash),
                D3d12ExecutableIdentity::new(606, expected_active_hash),
            ),
        );

        let state =
            assess_d3d12_executable(&executable, Some(&baseline)).expect("repair assessment");
        assert!(state.repair_required);
        assert!(!executable.exists());
    }

    #[test]
    fn empty_untracked_backup_fails_closed_without_panicking() {
        let dir = tempfile::tempdir().expect("temp dir");
        let executable = dir.path().join("game.exe");
        std::fs::write(&executable, synthetic_d3d12_executable(606)).expect("live");
        std::fs::write(dir.path().join("game.exe.bak"), []).expect("empty backup");

        let state = assess_d3d12_executable(&executable, None).expect("repair assessment");
        assert!(state.repair_required);
    }

    #[test]
    fn executable_pair_adoption_requires_one_unambiguous_candidate() {
        let dir = tempfile::tempdir().expect("temp dir");
        let first = dir.path().join("first.exe");
        let second = dir.path().join("second.exe");
        for executable in [&first, &second] {
            std::fs::write(executable, synthetic_d3d12_executable(619)).expect("live");
            std::fs::write(
                crate::fs::backup_path(executable).expect("backup path"),
                synthetic_d3d12_executable(606),
            )
            .expect("backup");
        }

        assert!(is_unique_valid_executable_pair(&first, std::iter::empty()));
        assert!(!is_unique_valid_executable_pair(
            &first,
            [first.as_path(), second.as_path()]
        ));
    }

    #[test]
    fn persisted_executable_fallback_requires_one_readable_pe() {
        let dir = tempfile::tempdir().expect("temp dir");
        let first = dir.path().join("first.exe");
        let second = dir.path().join("second.exe");
        let invalid = dir.path().join("invalid.exe");
        std::fs::write(&first, synthetic_d3d12_executable(606)).expect("first executable");
        std::fs::write(&invalid, b"not a PE").expect("invalid executable");
        let candidate = |path: &std::path::Path| {
            PathRef::new(path.to_string_lossy().into_owned()).expect("candidate path")
        };

        assert_eq!(
            unique_known_executable_candidate(&[candidate(&invalid), candidate(&first)]),
            Some(first.clone()),
            "invalid persisted paths do not make the one valid PE ambiguous"
        );

        std::fs::write(&second, synthetic_d3d12_executable(619)).expect("second executable");
        assert_eq!(
            unique_known_executable_candidate(&[candidate(&first), candidate(&second)]),
            None,
            "multiple readable PEs must remain unresolved"
        );
    }
}
