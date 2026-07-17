//! Derived add-on capabilities used by the game catalog.
//!
//! The catalog needs a cheap, side-effect-free answer to "which add-ons have a
//! usable profile for this game?". Full availability queries are deliberately
//! not reused here: RenoDX availability may adopt an orphaned install and both
//! tools perform host/filesystem checks that do not belong in card rendering.
//! Instead, a scan refresh resolves the pure manifest matchers once and stores
//! the resulting profile snapshot in [`crate::Context`].
//!
//! `CapabilityProbe` type-erases the per-tool manifest + pure matcher behind
//! one `Fn(&MatchFacts) -> bool`, built by each tool's
//! `AddonTool::load_capability_probe`. Adding a tool means implementing that
//! one method — this module never lists tools by name.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use renderpilot_domain::{AddonKind, GameId};

use crate::addons::game_analysis::analyze_game;
use crate::addons::game_context::executable_override;
use crate::addons::matching::MatchFacts;
use crate::addons::tool;
use crate::{Context, ServiceError};

/// Boxed future returned by `AddonTool::load_capability_probe`: fetches
/// (or reuses the cached) manifest and wraps it in a `CapabilityProbe`.
pub(crate) type CapabilityProbeFuture =
    Pin<Box<dyn Future<Output = Result<CapabilityProbe, ServiceError>> + Send + 'static>>;

/// Type-erased, loaded capability probe for one tool: answers "does this
/// tool's manifest expose a usable profile for these game facts?" without the
/// caller needing to know the tool's manifest or resolution types.
pub(crate) struct CapabilityProbe {
    kind: AddonKind,
    is_available: Box<dyn Fn(&MatchFacts) -> bool + Send + Sync>,
}

impl CapabilityProbe {
    /// Wraps a tool's pure `manifest + facts -> available` policy behind the
    /// type-erased signature this module operates on. Tools build this from
    /// their own manifest/matcher/resolution types, which never leak here.
    #[must_use]
    pub(crate) fn new(
        kind: AddonKind,
        is_available: impl Fn(&MatchFacts) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            is_available: Box::new(is_available),
        }
    }

    fn kind(&self) -> AddonKind {
        self.kind
    }

    fn is_profile_available(&self, facts: &MatchFacts) -> bool {
        (self.is_available)(facts)
    }
}

impl std::fmt::Debug for CapabilityProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilityProbe")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// In-memory, profile-derived capability set keyed by game.
///
/// Installed add-ons are intentionally not stored here. Catalog aggregation
/// reads those from `installed_addons` so install/uninstall changes are visible
/// immediately without synchronizing derived state.
#[derive(Debug, Clone, Default)]
pub(crate) struct ProfileCapabilitySnapshot {
    by_game: HashMap<GameId, HashSet<AddonKind>>,
}

impl ProfileCapabilitySnapshot {
    pub(crate) fn capabilities_for(&self, game_id: &GameId) -> Vec<AddonKind> {
        let Some(capabilities) = self.by_game.get(game_id) else {
            return Vec::new();
        };

        AddonKind::ALL
            .iter()
            .copied()
            .filter(|kind| capabilities.contains(kind))
            .collect()
    }

    fn clear_kind(&mut self, kind: AddonKind) {
        self.by_game.retain(|_, capabilities| {
            capabilities.remove(&kind);
            !capabilities.is_empty()
        });
    }

    fn insert(&mut self, game_id: GameId, kind: AddonKind) {
        self.by_game.entry(game_id).or_default().insert(kind);
    }
}

/// Rebuilds profile-derived catalog capabilities for every known game.
///
/// Each supplied probe replaces only its own capability kind. A missing kind
/// preserves the previous in-process values, so a transient CDN/cache failure
/// never clears a previously valid snapshot.
fn refresh_profile_capabilities(
    context: &Context,
    probes: &[CapabilityProbe],
) -> Result<(), ServiceError> {
    if probes.is_empty() {
        return Ok(());
    }

    let games = context.storage().list_games()?;
    let mut next = context.profile_capability_snapshot();

    for probe in probes {
        next.clear_kind(probe.kind());
    }

    for game in games {
        let override_path = executable_override(context, game.id());
        let analysis = analyze_game(&game, override_path.as_deref());
        let capabilities = profile_capabilities_for_facts(probes, &analysis.facts);

        for kind in capabilities {
            next.insert(game.id().clone(), kind);
        }
    }

    context.replace_profile_capability_snapshot(next);
    Ok(())
}

fn profile_capabilities_for_facts(
    probes: &[CapabilityProbe],
    facts: &MatchFacts,
) -> Vec<AddonKind> {
    probes
        .iter()
        .filter(|probe| probe.is_profile_available(facts))
        .map(CapabilityProbe::kind)
        .collect()
}

/// Owned capability probes loaded for a catalog refresh. Callers move this
/// into a blocking task and call [`LoadedCapabilityProbes::refresh`] so
/// network I/O stays async while filesystem analysis stays off the async
/// runtime.
#[derive(Debug, Default)]
pub struct LoadedCapabilityProbes {
    items: Vec<CapabilityProbe>,
}

impl LoadedCapabilityProbes {
    /// Whether at least one tool probe was loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Rebuilds the in-process profile capability snapshot from the loaded
    /// probes (blocking; call from a worker thread).
    pub fn refresh(self, context: &Context) -> Result<(), ServiceError> {
        refresh_profile_capabilities(context, &self.items)
    }
}

/// Concurrently loads cached (or freshly fetched) tool capability probes for a
/// catalog refresh. Per-kind failures are logged and skipped so one offline
/// CDN does not block another tool's snapshot update.
///
/// Every tool registered in `tool::TOOLS` is loaded automatically —
/// implementing `AddonTool::load_capability_probe` is the only step a new
/// tool needs here.
pub async fn load_capability_probes() -> LoadedCapabilityProbes {
    let loads = tool::TOOLS.iter().map(|t| {
        let kind = t.kind();
        let probe = t.load_capability_probe();
        async move { (kind, probe.await) }
    });

    let items = futures_util::future::join_all(loads)
        .await
        .into_iter()
        .filter_map(|(kind, result)| match result {
            Ok(probe) => Some(probe),
            Err(error) => {
                log::warn!("failed to load {kind:?} manifest for catalog capabilities: {error}");
                None
            }
        })
        .collect();

    LoadedCapabilityProbes { items }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{Architecture, ExeGraphicsInfo, GraphicsApi, Launcher};

    use super::*;
    use crate::addons::matching::MatchKind;
    use crate::addons::renodx;

    fn borderlands_facts() -> MatchFacts {
        MatchFacts {
            launcher: Launcher::Steam,
            external_id: Some("49520".to_owned()),
            exe_file_name: Some("Borderlands2.exe".to_owned()),
            exe_sha256: None,
            engine: None,
            graphics: ExeGraphicsInfo::new(vec![GraphicsApi::D3D9], Some(Architecture::X86)),
        }
    }

    #[test]
    fn missing_manifest_preserves_that_kind_in_snapshot() {
        let game_id = GameId::new("game:test").expect("game id");
        let mut snapshot = ProfileCapabilitySnapshot::default();
        snapshot.insert(game_id.clone(), AddonKind::RenoDx);
        snapshot.insert(game_id.clone(), AddonKind::Luma);

        snapshot.clear_kind(AddonKind::Luma);

        assert_eq!(snapshot.capabilities_for(&game_id), vec![AddonKind::RenoDx]);
    }
}
