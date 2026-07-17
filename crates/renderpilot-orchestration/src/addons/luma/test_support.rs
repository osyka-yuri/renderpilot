//! Shared fixtures for Luma unit tests.

use renderpilot_domain::{Architecture, GraphicsApi};

use super::types::{
    ExternalConfigEntry, ExternalConfigSection, LumaCategory, LumaExternalRequirement,
    LumaManifest, LumaProfile, LumaTitle, ManagedArchiveSource, ManagedInstallMapEntry, Status,
};
use crate::addons::matching::{MatchKind, MatchRule};

pub(crate) use crate::addons::test_support::reshade_sources;

pub(crate) const DEFAULT_MIN_RESHADE_VERSION: &str = "6.7.0";

/// Builds a single match rule.
pub(crate) fn rule(kind: MatchKind, value: &str, tier: u32) -> MatchRule {
    MatchRule {
        kind,
        value: value.to_owned(),
        tier,
    }
}

/// Builds a standard title with the given match rules.
pub(crate) fn title(
    id: &str,
    asset: &str,
    arch: Architecture,
    status: Status,
    match_rules: Vec<MatchRule>,
) -> LumaTitle {
    LumaTitle {
        id: id.to_owned(),
        name: format!("Game {id}"),
        asset: asset.to_owned(),
        addon_file: format!("{}.addon", asset.trim_end_matches(".zip")),
        arch,
        status,
        category: LumaCategory::default(),
        match_rules,
        features: None,
        guidance: Vec::new(),
        launch_args: Vec::new(),
        external_requirement: None,
        profile: LumaProfile::Game,
    }
}

/// Builds a manifest over the given titles with the default Luma host policy.
pub(crate) fn manifest(titles: Vec<LumaTitle>) -> LumaManifest {
    LumaManifest {
        schema_version: 1,
        generated_at: "2026-07-04T00:00:00Z".to_owned(),
        min_reshade_version: DEFAULT_MIN_RESHADE_VERSION.to_owned(),
        titles,
    }
}

/// Realistic curated dgVoodoo2 requirement used by matcher / validate / availability tests.
#[must_use]
pub(crate) fn sample_dgvoodoo_requirement() -> LumaExternalRequirement {
    LumaExternalRequirement::Dgvoodoo2 {
        version: "2.87.3".to_owned(),
        accepted_detected_apis: vec![GraphicsApi::D3D9],
        reshade_proxy_dll: "dxgi.dll".to_owned(),
        source: ManagedArchiveSource {
            url: "https://github.com/dege-diosg/dgVoodoo2/releases/download/v2.87.3/dgVoodoo2_87_3.zip"
                .to_owned(),
            sha256: "6fb954bed55bf70e948c5045a663a9df31ea206faf105e327bafe46c318f867f"
                .to_owned(),
            size: 9_082_391,
        },
        install_map: vec![ManagedInstallMapEntry {
            source: "MS/x86/D3D9.dll".to_owned(),
            dest: "D3D9.dll".to_owned(),
            sha256: "c13e3c0969d2c70a1a63cf96b83c7cd3bc47f925f28ec92c07d5b72d6df4c240"
                .to_owned(),
            size: 485_888,
        }],
        config_file: "dgVoodoo.conf".to_owned(),
        config: vec![
            ExternalConfigSection {
                section: "General".to_owned(),
                entries: vec![ExternalConfigEntry {
                    key: "OutputAPI".to_owned(),
                    value: "d3d11_fl11_0".to_owned(),
                }],
            },
            ExternalConfigSection {
                section: "DirectX".to_owned(),
                entries: vec![ExternalConfigEntry {
                    key: "VideoCard".to_owned(),
                    value: "geforce_9800_gt".to_owned(),
                }],
            },
        ],
    }
}

// Synthetic PE images and zip archives are shared with the ReShade/RenoDX fetch
// tests; re-exported from [`crate::addons::test_support`] so Luma fixtures keep
// addressing them here.
pub(crate) use crate::addons::test_support::{
    MACHINE_AMD64, MACHINE_I386, PE32_MAGIC, PE32_PLUS_MAGIC, build_nvidia_dlss_pe,
    build_pe_with_exports, zip_with_entries,
};
