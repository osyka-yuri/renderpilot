use std::path::{Path, PathBuf};

use super::manifest::{ManifestKind, classify_manifest};
use super::pe::read_pe_architecture;
use super::registry::LayerRegistry;
use super::types::{
    LayerRegistryEntry, RegistryHive, VulkanLayerArchitecture, VulkanLayerDiagnostic,
    VulkanLayerFacts, VulkanLayerReport, VulkanLayerState, VulkanLoaderVisibility,
};
use super::util::same_path;
use super::{LAYER_DLL_NAME, LAYER_JSON_NAME};

/// Detects the ReShade Vulkan layer state.
#[cfg(test)]
#[must_use]
pub(crate) fn detect(
    registry: &(impl LayerRegistry + ?Sized),
    layer_dir: &Path,
) -> VulkanLayerState {
    detect_report(registry, layer_dir).state
}

/// Builds a detailed detector report for the shared ReShade Vulkan layer.
#[must_use]
pub fn detect_report(
    registry: &(impl LayerRegistry + ?Sized),
    layer_dir: &Path,
) -> VulkanLayerReport {
    let mut report = detect_report_inner(registry, layer_dir);
    // Post-processing: surface a `RegistryScopeNotWritable` caveat when the
    // state suggests the user might want to install or re-register the layer
    // (Absent, InstalledDisabled, or RegistryMissing) but the process cannot
    // write to HKLM (e.g. not elevated). This is a caveat, not a state change.
    let needs_hklm_write = matches!(
        report.state,
        VulkanLayerState::Absent | VulkanLayerState::InstalledDisabled
    ) || report
        .diagnostics
        .contains(&VulkanLayerDiagnostic::RegistryMissing);
    if needs_hklm_write
        && !registry.can_write_scope()
        && !report
            .diagnostics
            .contains(&VulkanLayerDiagnostic::RegistryScopeNotWritable)
    {
        report
            .diagnostics
            .push(VulkanLayerDiagnostic::RegistryScopeNotWritable);
    }
    report
}

fn detect_report_inner(
    registry: &(impl LayerRegistry + ?Sized),
    layer_dir: &Path,
) -> VulkanLayerReport {
    let mut candidates = Vec::new();
    let mut broken: Vec<(VulkanLayerDiagnostic, VulkanLayerFacts)> = Vec::new();
    // Track which manifest paths have an HKLM registration. If the winning
    // candidate is only registered under HKCU, the layer may not be visible
    // to elevated games — a caveat the UI must surface.
    let mut hklm_manifests: Vec<PathBuf> = Vec::new();
    for entry in logical_registry_entries(registry.registered_layers()) {
        if entry.hive == RegistryHive::Hklm {
            hklm_manifests.push(entry.manifest_path.clone());
        }
        match classify_manifest(&entry.manifest_path, layer_dir, entry.active) {
            ManifestKind::Candidate(candidate) => candidates.push(candidate),
            ManifestKind::Broken { diagnostic, facts } => broken.push((diagnostic, facts)),
            ManifestKind::Other => {}
        }
    }

    if candidates.len() > 1 {
        let mut facts = candidates
            .first()
            .map(|candidate| candidate.facts.clone())
            .unwrap_or_default();
        facts.loader_visibility = VulkanLoaderVisibility::Ambiguous;
        return VulkanLayerReport {
            state: VulkanLayerState::Conflict,
            facts,
            diagnostics: vec![
                VulkanLayerDiagnostic::DuplicateLayerManifest,
                VulkanLayerDiagnostic::AmbiguousLoaderVisibility,
            ],
        };
    }

    if let Some(candidate) = candidates.into_iter().next() {
        if candidate.facts.architecture == VulkanLayerArchitecture::X86 {
            return VulkanLayerReport {
                state: VulkanLayerState::Unsupported,
                facts: candidate.facts,
                diagnostics: vec![VulkanLayerDiagnostic::UnsupportedArchitecture],
            };
        }
        if candidate.facts.architecture == VulkanLayerArchitecture::Unknown {
            return VulkanLayerReport {
                state: VulkanLayerState::Conflict,
                facts: candidate.facts,
                diagnostics: vec![VulkanLayerDiagnostic::BackendValidationFailed],
            };
        }
        // Check HKCU-only visibility: if the candidate's manifest path is not
        // registered under HKLM, the layer may not be visible to elevated
        // games. Surface this as a loader-visibility caveat + diagnostic.
        let is_hklm_registered = candidate
            .facts
            .manifest_path
            .as_ref()
            .is_some_and(|p| hklm_manifests.iter().any(|h| same_path(h, p)));
        let mut diagnostics = candidate.diagnostics;
        let mut facts = candidate.facts;
        if !is_hklm_registered {
            facts.loader_visibility = VulkanLoaderVisibility::HkcuNotVisibleWhenElevated;
            if !diagnostics.contains(&VulkanLayerDiagnostic::HkcuNotVisibleWhenElevated) {
                diagnostics.push(VulkanLayerDiagnostic::HkcuNotVisibleWhenElevated);
            }
        }
        return VulkanLayerReport {
            state: candidate.state,
            facts,
            diagnostics,
        };
    }

    // Only report a broken layer as Conflict if it's in the standard location
    // (`C:\ProgramData\ReShade\`). A broken ReShade-looking manifest in a
    // non-standard location (e.g. a leftover HKCU entry from a previous
    // install in `%LOCALAPPDATA%`) is ignored — it's not our layer and
    // shouldn't block a fresh install in the standard location.
    //
    // A broken standard-location entry is a broken state (Conflict), not
    // Absent: the registry key exists but the backing files are missing or
    // unreadable. The UI distinguishes these via diagnostics and offers
    // reinstall. A non-standard broken entry is ignored.
    let standard_manifest = layer_dir.join(LAYER_JSON_NAME);
    if let Some((diagnostic, facts)) = broken.into_iter().find(|(_, facts)| {
        facts
            .manifest_path
            .as_ref()
            .is_some_and(|p| same_path(p, &standard_manifest))
    }) {
        let state = match diagnostic {
            VulkanLayerDiagnostic::UnsupportedArchitecture => VulkanLayerState::Unsupported,
            _ => VulkanLayerState::Conflict,
        };
        return VulkanLayerReport {
            state,
            facts,
            diagnostics: vec![diagnostic],
        };
    }

    // Nothing was found in the registry. Check whether the layer files exist
    // in the standard location but aren't registered — a broken state where
    // the Vulkan loader can't find the layer even though the files are on
    // disk (e.g. the registry key was deleted manually, or a previous
    // uninstall removed the key but not the directory).
    let dll_path = layer_dir.join(LAYER_DLL_NAME);
    let manifest_path = layer_dir.join(LAYER_JSON_NAME);
    if dll_path.is_file() && manifest_path.is_file() {
        let architecture = match read_pe_architecture(&dll_path) {
            Ok(arch) => arch,
            Err(_) => VulkanLayerArchitecture::Unknown,
        };
        let state = if architecture == VulkanLayerArchitecture::X86 {
            VulkanLayerState::Unsupported
        } else {
            VulkanLayerState::Conflict
        };
        let diagnostic = if architecture == VulkanLayerArchitecture::X86 {
            VulkanLayerDiagnostic::UnsupportedArchitecture
        } else if architecture == VulkanLayerArchitecture::Unknown {
            VulkanLayerDiagnostic::BackendValidationFailed
        } else {
            // Files exist and are valid but the registry entry is missing —
            // the loader cannot discover this layer.
            VulkanLayerDiagnostic::RegistryMissing
        };
        return VulkanLayerReport {
            state,
            facts: VulkanLayerFacts {
                manifest_path: Some(manifest_path),
                dll_path: Some(dll_path),
                architecture,
                ..VulkanLayerFacts::default()
            },
            diagnostics: vec![diagnostic],
        };
    }

    VulkanLayerReport {
        state: VulkanLayerState::Absent,
        facts: VulkanLayerFacts::default(),
        diagnostics: Vec::new(),
    }
}

/// Collapses the raw per-view registry reads (see the WOW64 view matrix in
/// `registry.rs`) into one entry per manifest path, keeping the reading with
/// the strongest loader visibility for each. The same manifest can appear
/// several times from different hive/view combinations that all resolve to
/// it; the loader itself only cares about the strongest registration.
fn logical_registry_entries(entries: Vec<LayerRegistryEntry>) -> Vec<LayerRegistryEntry> {
    let mut logical: Vec<LayerRegistryEntry> = Vec::new();
    for entry in entries {
        if let Some(existing) = logical
            .iter_mut()
            .find(|existing| same_path(&existing.manifest_path, &entry.manifest_path))
        {
            if entry_visibility_rank(&entry) > entry_visibility_rank(existing) {
                existing.active = entry.active;
                existing.hive = entry.hive;
            }
        } else {
            logical.push(entry);
        }
    }
    logical
}

/// Orders registrations by how visible they are to the Vulkan loader: an
/// active (enabled) HKLM entry is seen by every process including elevated
/// ones, so it outranks HKCU; a disabled entry of either hive is weaker than
/// any active one, since the loader skips it.
fn entry_visibility_rank(entry: &LayerRegistryEntry) -> u8 {
    match (entry.active, entry.hive) {
        (true, RegistryHive::Hklm) => 3,
        (true, RegistryHive::Hkcu) => 2,
        (false, RegistryHive::Hklm) => 1,
        (false, RegistryHive::Hkcu) => 0,
    }
}

// -----------------------------------------------------------------------------
// Install / Uninstall
// -----------------------------------------------------------------------------
