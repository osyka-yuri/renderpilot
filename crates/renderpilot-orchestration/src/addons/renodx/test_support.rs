//! Shared fixtures for RenoDX unit tests.

use renderpilot_domain::Architecture;

use super::types::{
    Category, Channel, Compatibility, MatchKind, MatchRule, RenoDxManifest, Status, Title,
    manifest_defaults,
};
use crate::addons::reshade::types::{ReshadeConfig, ReshadeNightly, ReshadeStable};

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
    slug: &str,
    arch: Architecture,
    status: Status,
    match_rules: Vec<MatchRule>,
) -> Title {
    Title {
        id: id.to_owned(),
        name: format!("Game {id}"),
        category: Category::default(),
        slug: slug.to_owned(),
        arch,
        status,
        channel: Channel::default(),
        min_app_version: "1.0.0".to_owned(),
        match_rules,
        compatibility: Compatibility::default(),
        proxy_dll_override: None,
        notes_keys: Vec::new(),
        download_url: None,
    }
}

/// Builds a manifest over the given titles with a default ReShade config.
pub(crate) fn manifest(titles: Vec<Title>) -> RenoDxManifest {
    RenoDxManifest {
        schema_version: 3,
        generated_at: "2026-06-15T00:00:00Z".to_owned(),
        reshade: ReshadeConfig {
            stable: Some(ReshadeStable {
                url: "https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe".to_owned(),
            }),
            nightly: ReshadeNightly {
                url64: "https://nightly.link/crosire/reshade/workflows/build/main/ReShade%20(64-bit).zip".to_owned(),
                url32: "https://nightly.link/crosire/reshade/workflows/build/main/ReShade%20(32-bit).zip".to_owned(),
            },
        },
        generics: Vec::new(),
        defaults: manifest_defaults(),
        titles,
    }
}

// Synthetic PE images are shared with ReShade fetch tests; re-exported from
// [`crate::addons::test_support`] so RenoDX fixtures keep addressing them here.
pub(crate) use crate::addons::test_support::{
    MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports,
};

#[cfg(windows)]
use renderpilot_application::{AppResult, SharedArtifactRepository};
#[cfg(windows)]
use renderpilot_domain::{SharedArtifactKind, SharedArtifactRecord};
#[cfg(windows)]
use std::sync::{Arc, Mutex};

#[cfg(windows)]
#[derive(Default, Clone)]
pub(crate) struct InMemorySharedArtifactRepository {
    pub artifacts: Arc<Mutex<Vec<SharedArtifactRecord>>>,
}

#[cfg(windows)]
impl SharedArtifactRepository for InMemorySharedArtifactRepository {
    fn upsert_shared_artifact(&self, record: &SharedArtifactRecord) -> AppResult<()> {
        let mut artifacts = self.artifacts.lock().unwrap();
        artifacts.retain(|r| r.kind() != record.kind());
        artifacts.push(record.clone());
        Ok(())
    }

    fn get_shared_artifact(
        &self,
        kind: SharedArtifactKind,
    ) -> AppResult<Option<SharedArtifactRecord>> {
        let artifacts = self.artifacts.lock().unwrap();
        Ok(artifacts.iter().find(|r| r.kind() == kind).cloned())
    }

    fn delete_shared_artifact(&self, kind: SharedArtifactKind) -> AppResult<()> {
        let mut artifacts = self.artifacts.lock().unwrap();
        artifacts.retain(|r| r.kind() != kind);
        Ok(())
    }
}
