//! Shared fixtures for RenoDX unit tests.

use renderpilot_domain::Architecture;

use super::types::{
    MatchKind, MatchRule, RenoDxCategory, RenoDxCompatibility, RenoDxManifest, RenoDxTitle, Status,
};

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
) -> RenoDxTitle {
    RenoDxTitle {
        id: id.to_owned(),
        name: format!("Game {id}"),
        category: RenoDxCategory::default(),
        slug: slug.to_owned(),
        arch,
        status,
        match_rules,
        compatibility: RenoDxCompatibility::default(),
        proxy_dll_override: None,
        download_url: None,
    }
}

/// Builds a manifest over the given titles.
pub(crate) fn manifest(titles: Vec<RenoDxTitle>) -> RenoDxManifest {
    RenoDxManifest {
        schema_version: 1,
        generated_at: "2026-06-15T00:00:00Z".to_owned(),
        generics: Vec::new(),
        titles,
    }
}

// Shared fixtures re-exported so RenoDX tests keep addressing them here.
pub(crate) use crate::addons::test_support::{
    MACHINE_AMD64, PE32_PLUS_MAGIC, build_pe_with_exports, reshade_sources,
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
