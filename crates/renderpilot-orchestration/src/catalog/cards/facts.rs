use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use renderpilot_application::{
    CandidateComparison, ComponentReplacementCandidates, D3d12ExecutableProfile, SwapTargetProfile,
    is_automatic_catalog_candidate,
};
use renderpilot_domain::{
    ComponentId, ComponentRollbackBaseline, GameInstallation, InstalledAddon, LibraryComponent,
    LibraryTechnology, PathRef, Swappability,
};

use crate::ServiceError;
use crate::catalog::components_for_candidate_matching_with_installed;

use super::CatalogCardRiskLevel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SnapshotFactsMode {
    Durable,
    ValidateLive,
}

pub(super) struct CardDynamicFacts<'components> {
    pub(super) rollback_available: bool,
    pub(super) matching_components: Cow<'components, [LibraryComponent]>,
    pub(super) target_profile: SwapTargetProfile,
}

pub(super) fn card_dynamic_facts<'components>(
    mode: SnapshotFactsMode,
    game: &GameInstallation,
    components: &'components [LibraryComponent],
    installed_addon: Option<&InstalledAddon>,
    component_backups: &mut HashMap<ComponentId, ComponentRollbackBaseline>,
    executable_override: Option<&Path>,
) -> Result<CardDynamicFacts<'components>, ServiceError> {
    match mode {
        SnapshotFactsMode::Durable => Ok(CardDynamicFacts {
            rollback_available: components
                .iter()
                .any(|component| component_backups.contains_key(component.id())),
            matching_components: Cow::Borrowed(components),
            target_profile: durable_card_target_profile(components, component_backups),
        }),
        SnapshotFactsMode::ValidateLive => {
            let mut rollback_available = false;
            let mut d3d12_backup_availability = None;
            for component in components {
                let availability = crate::coordinated_files::classify_component_backup(
                    component_backups.remove(component.id()),
                    component.files(),
                );
                rollback_available |= availability.is_available();
                if component.technology() == LibraryTechnology::D3D12Agility {
                    d3d12_backup_availability = Some(availability);
                }
            }
            let matching_components = components_for_candidate_matching_with_installed(
                game.id(),
                components,
                installed_addon,
            )?;
            let d3d12_component = components
                .iter()
                .find(|component| component.technology() == LibraryTechnology::D3D12Agility);
            let target_profile = if d3d12_component.is_some() {
                crate::catalog::runtime_compatibility::presentation_target_profile_from_facts(
                    game,
                    d3d12_component,
                    d3d12_backup_availability,
                    executable_override,
                )?
                .profile
            } else {
                durable_card_target_profile(components, component_backups)
            };
            Ok(CardDynamicFacts {
                rollback_available,
                matching_components: Cow::Owned(matching_components),
                target_profile,
            })
        }
    }
}

fn durable_card_target_profile(
    components: &[LibraryComponent],
    component_backups: &HashMap<ComponentId, ComponentRollbackBaseline>,
) -> SwapTargetProfile {
    let architecture = components
        .iter()
        .flat_map(LibraryComponent::files)
        .filter_map(|file| file.pe_compatibility())
        .map(|profile| profile.architecture())
        .next();
    let d3d12_component = components
        .iter()
        .find(|component| component.technology() == LibraryTechnology::D3D12Agility);
    let d3d12_baseline =
        d3d12_component.and_then(|component| component_backups.get(component.id()));
    let persisted_sdk_version = d3d12_baseline
        .and_then(ComponentRollbackBaseline::d3d12_executable)
        .map(|baseline| baseline.expected_active().sdk_version())
        .or_else(|| d3d12_component.and_then(component_d3d12_sdk_version));
    let profile = SwapTargetProfile::new(architecture, persisted_sdk_version);

    let Some(executable) = d3d12_baseline.and_then(ComponentRollbackBaseline::d3d12_executable)
    else {
        return profile;
    };
    let executable_path = Path::new(executable.executable_path().as_str());
    let Ok(backup_path) = crate::fs::backup_path(executable_path) else {
        return profile;
    };
    let Ok(backup_path) = PathRef::new(backup_path.to_string_lossy().into_owned()) else {
        return profile;
    };

    profile.with_d3d12_executable_profile(D3d12ExecutableProfile::new(
        executable.executable_path().clone(),
        backup_path,
        executable.original().sdk_version(),
        executable.expected_active().sdk_version(),
        true,
        false,
    ))
}

fn component_d3d12_sdk_version(component: &LibraryComponent) -> Option<u32> {
    component
        .files()
        .iter()
        .filter_map(|file| file.version())
        .find_map(|version| {
            version
                .segments()
                .get(1)
                .copied()
                .and_then(|segment| u32::try_from(segment).ok())
        })
}

pub(super) struct CardMetrics {
    pub(super) library_tags: Vec<String>,
    pub(super) component_count: usize,
    pub(super) update_count: usize,
    pub(super) risk_level: CatalogCardRiskLevel,
}

pub(super) fn card_metrics(
    components: &[LibraryComponent],
    candidate_groups: &[ComponentReplacementCandidates],
) -> CardMetrics {
    let visible_ids = components
        .iter()
        .filter(|component| component.technology() != LibraryTechnology::Unknown)
        .map(|component| component.id().as_str())
        .collect::<HashSet<_>>();
    let library_tags = components
        .iter()
        .filter(|component| component.technology() != LibraryTechnology::Unknown)
        .map(|component| component.technology().as_slug().to_owned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let component_count = visible_ids.len();
    let update_count = candidate_groups
        .iter()
        .filter(|group| visible_ids.contains(group.component_id().as_str()))
        .filter(|group| {
            group.candidates().iter().any(|candidate| {
                candidate.comparison() == CandidateComparison::NewerVersion
                    && is_automatic_catalog_candidate(candidate)
            })
        })
        .count();
    let risk_level = components
        .iter()
        .map(|component| match component.swappability() {
            Swappability::Unsafe | Swappability::IntegratedIntoEngine => CatalogCardRiskLevel::High,
            Swappability::BundleOnly | Swappability::ReadOnly => CatalogCardRiskLevel::Medium,
            Swappability::Swappable => CatalogCardRiskLevel::Low,
            _ => CatalogCardRiskLevel::Unknown,
        })
        .max()
        .unwrap_or(CatalogCardRiskLevel::Unknown);

    CardMetrics {
        library_tags,
        component_count,
        update_count,
        risk_level,
    }
}
