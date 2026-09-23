//! Derived add-on capabilities used by the game catalog.
//!
//! The catalog needs a cheap, side-effect-free answer to "which add-ons have a
//! usable profile for this game?". Full availability queries are deliberately
//! not reused here: RenoDX availability may adopt an orphaned install and both
//! tools perform host/filesystem checks that do not belong in card rendering.
//! Instead, a scan refresh resolves each tool's pure capability policy once
//! and stores the resulting profile snapshot durably in SQLite.
//!
//! `CapabilityProbe` type-erases each tool's pure matcher behind one
//! `Fn(&MatchFacts, &[LibraryComponent]) -> bool`, built by each tool's
//! `AddonTool::load_capability_probe`. Adding a tool means implementing that
//! one method — this module never lists tools by name.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use renderpilot_application::ComponentRepository;
use renderpilot_domain::{AddonKind, GameId, LibraryComponent};

use crate::addons::game_analysis::analyze_game;
use crate::addons::matching::MatchFacts;
use crate::addons::tool;
use crate::{Context, ServiceError};

/// Boxed future returned by `AddonTool::load_capability_probe`.
pub(crate) type CapabilityProbeFuture =
    Pin<Box<dyn Future<Output = Result<CapabilityProbe, ServiceError>> + Send + 'static>>;

/// Type-erased, loaded capability probe for one tool.
///
/// Components are passed as a per-game slice so a probe can use detected
/// native-library evidence without reaching into storage or performing I/O.
type CapabilityAvailability = dyn Fn(&MatchFacts, &[LibraryComponent]) -> bool + Send + Sync;

pub(crate) struct CapabilityProbe {
    kind: AddonKind,
    source_revision: String,
    is_available: Box<CapabilityAvailability>,
}

impl CapabilityProbe {
    /// Wraps a tool's pure `facts + components -> available` policy behind the
    /// type-erased signature this module operates on. Tool-specific matcher
    /// types never leak into the capability refresh pipeline.
    #[must_use]
    pub(crate) fn new(
        kind: AddonKind,
        source_revision: impl Into<String>,
        is_available: impl Fn(&MatchFacts, &[LibraryComponent]) -> bool + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            source_revision: source_revision.into(),
            is_available: Box::new(is_available),
        }
    }

    fn kind(&self) -> AddonKind {
        self.kind
    }

    fn source_revision(&self) -> &str {
        &self.source_revision
    }

    fn is_profile_available(&self, facts: &MatchFacts, components: &[LibraryComponent]) -> bool {
        (self.is_available)(facts, components)
    }
}

impl std::fmt::Debug for CapabilityProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CapabilityProbe")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// Profile-derived capability set keyed by game, loaded from durable facts.
///
/// Installed add-ons are intentionally not stored here. Catalog aggregation
/// reads those from `installed_addons` so install/uninstall changes are visible
/// immediately without synchronizing derived state.
#[derive(Debug, Clone, Default)]
pub(crate) struct DurableProfileCapabilities {
    by_game: HashMap<GameId, HashSet<AddonKind>>,
}

impl DurableProfileCapabilities {
    pub(crate) fn load(context: &Context) -> Result<Self, ServiceError> {
        let mut snapshot = Self::default();
        for (game_id, kind) in context.storage().list_profile_addon_capabilities()? {
            snapshot.insert(game_id, kind);
        }
        Ok(snapshot)
    }

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

    pub(crate) fn load_for_game(
        context: &Context,
        game_id: &GameId,
    ) -> Result<Vec<AddonKind>, ServiceError> {
        context
            .storage()
            .list_profile_addon_capabilities_for_game(game_id)
            .map_err(Into::into)
    }

    fn insert(&mut self, game_id: GameId, kind: AddonKind) {
        self.by_game.entry(game_id).or_default().insert(kind);
    }
}

/// Rebuilds profile-derived catalog capabilities for every known game.
///
/// Each supplied probe replaces only its own capability kind. A missing kind
/// preserves the previous durable values, so a transient CDN/cache failure
/// never clears a previously valid snapshot.
fn refresh_profile_capabilities(
    context: &Context,
    probes: &[CapabilityProbe],
) -> Result<bool, ServiceError> {
    if probes.is_empty() {
        return Ok(false);
    }

    let games = context.storage().list_games()?;
    let executable_overrides = context
        .storage()
        .list_nvapi_executable_overrides()?
        .into_iter()
        .map(|row| (row.game_id, std::path::PathBuf::from(row.selected_path)))
        .collect::<HashMap<_, _>>();
    let components_by_game = components_by_game(context.storage().list_all_components()?);
    let mut matches = HashMap::<AddonKind, Vec<GameId>>::new();
    for game in games {
        let override_path = executable_overrides
            .get(game.id().as_str())
            .map(std::path::PathBuf::as_path);
        let analysis = analyze_game(&game, override_path);
        let components = components_by_game
            .get(game.id())
            .map_or(&[][..], Vec::as_slice);
        let capabilities = profile_capabilities_for_facts(probes, &analysis.facts, components);

        for kind in capabilities {
            matches.entry(kind).or_default().push(game.id().clone());
        }
    }
    let mut changed = false;
    for probe in probes {
        changed |= context.storage().replace_profile_addon_capabilities(
            probe.kind(),
            probe.source_revision(),
            matches.get(&probe.kind()).map_or(&[], Vec::as_slice),
        )?;
    }
    Ok(changed)
}

fn refresh_profile_capabilities_for_game(
    context: &Context,
    probes: &[CapabilityProbe],
    game_id: &GameId,
) -> Result<bool, ServiceError> {
    if probes.is_empty() {
        return Ok(false);
    }

    let game = crate::addons::game_context::require_game(context, game_id)?;
    let override_path = context
        .storage()
        .get_nvapi_executable_override(game_id.as_str())?
        .map(|row| std::path::PathBuf::from(row.selected_path));
    let analysis = analyze_game(&game, override_path.as_deref());
    let components = context.storage().list_components_for_game(game_id)?;
    let capabilities = probes
        .iter()
        .map(|probe| {
            (
                probe.kind(),
                probe.source_revision().to_owned(),
                probe.is_profile_available(&analysis.facts, &components),
            )
        })
        .collect::<Vec<_>>();
    context
        .storage()
        .replace_game_profile_addon_capabilities(game_id, &capabilities)
        .map_err(Into::into)
}

fn profile_capabilities_for_facts(
    probes: &[CapabilityProbe],
    facts: &MatchFacts,
    components: &[LibraryComponent],
) -> Vec<AddonKind> {
    probes
        .iter()
        .filter(|probe| probe.is_profile_available(facts, components))
        .map(CapabilityProbe::kind)
        .collect()
}

fn components_by_game(components: Vec<LibraryComponent>) -> HashMap<GameId, Vec<LibraryComponent>> {
    let mut by_game = HashMap::new();
    for component in components {
        by_game
            .entry(component.game_id().clone())
            .or_insert_with(Vec::new)
            .push(component);
    }
    by_game
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

    /// Rebuilds durable profile capability rows from the loaded probes
    /// (blocking; call from a worker thread).
    pub fn refresh(self, context: &Context) -> Result<bool, ServiceError> {
        refresh_profile_capabilities(context, &self.items)
    }

    /// Refreshes only one game's rows for the successfully loaded kinds.
    pub fn refresh_game(self, context: &Context, game_id: &GameId) -> Result<bool, ServiceError> {
        refresh_profile_capabilities_for_game(context, &self.items, game_id)
    }
}

/// Concurrently loads tool capability probes for a
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
                tracing::warn!("failed to load {kind:?} capability probe: {error}");
                None
            }
        })
        .collect();

    LoadedCapabilityProbes { items }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{
        Architecture, ComponentId, ComponentKind, ExeGraphicsInfo, GraphicsApi, Launcher,
        LibraryComponent, LibraryTechnology, Swappability,
    };

    use super::*;
    use crate::addons::luma;
    use crate::addons::matching::MatchKind;
    use crate::addons::renodx;

    fn borderlands_facts() -> MatchFacts {
        MatchFacts {
            launcher: Launcher::Steam,
            external_id: Some("49520".to_owned()),
            exe_file_name: Some("Borderlands2.exe".to_owned()),
            engine: None,
            unreal_version: None,
            graphics: ExeGraphicsInfo::new(vec![GraphicsApi::D3D9], Some(Architecture::X86)),
            unreal_detection: None,
            target_platform: None,
        }
    }

    #[test]
    fn borderlands_style_facts_can_expose_both_profiles() {
        let renodx_manifest = renodx::test_support::manifest(vec![renodx::test_support::title(
            "borderlands-2",
            "borderlands2",
            Architecture::X86,
            renodx::types::Status::Working,
            vec![renodx::test_support::rule(
                renodx::types::MatchKind::SteamAppid,
                "49520",
                100,
            )],
        )]);
        let mut luma_title = luma::test_support::title(
            "borderlands-2",
            "Luma-Borderlands_2.zip",
            Architecture::X86,
            luma::types::Status::Working,
            vec![luma::test_support::rule(
                MatchKind::SteamAppid,
                "49520",
                100,
            )],
        );
        luma_title.external_requirement = Some(luma::types::LumaExternalRequirement::Dgvoodoo2 {
            version: "2.87.3".to_owned(),
            accepted_detected_apis: vec![GraphicsApi::D3D9],
            reshade_proxy_dll: "dxgi.dll".to_owned(),
            source: luma::types::ManagedArchiveSource {
                url: "https://example.com/dgVoodoo.zip".to_owned(),
                sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .to_owned(),
                size: 1,
            },
            install_map: Vec::new(),
            config_file: "dgVoodoo.conf".to_owned(),
            config: Vec::new(),
        });
        let luma_manifest = luma::test_support::manifest(vec![luma_title]);

        let probes = [
            renodx::tool::capability_probe(renodx_manifest),
            luma::tool::capability_probe(luma_manifest),
        ];
        assert_eq!(
            profile_capabilities_for_facts(&probes, &borderlands_facts(), &[]),
            vec![AddonKind::RenoDx, AddonKind::Luma]
        );
    }

    #[test]
    fn capability_snapshot_orders_kinds_stably() {
        let game_id = GameId::new("game:test").expect("game id");
        let mut snapshot = DurableProfileCapabilities::default();
        snapshot.insert(game_id.clone(), AddonKind::RenoDx);
        snapshot.insert(game_id.clone(), AddonKind::Luma);

        assert_eq!(
            snapshot.capabilities_for(&game_id),
            vec![AddonKind::RenoDx, AddonKind::Luma]
        );
    }

    #[test]
    fn components_are_grouped_by_game_without_cross_game_leakage() {
        let first = GameId::new("game:first").expect("game id");
        let second = GameId::new("game:second").expect("game id");
        let components = vec![
            LibraryComponent::new(
                ComponentId::new("component:first").expect("component id"),
                first.clone(),
                ComponentKind::NativeLibrary,
                LibraryTechnology::AmdFsr,
                Swappability::ReadOnly,
            ),
            LibraryComponent::new(
                ComponentId::new("component:second").expect("component id"),
                second.clone(),
                ComponentKind::NativeLibrary,
                LibraryTechnology::IntelXeSs,
                Swappability::ReadOnly,
            ),
        ];

        let grouped = components_by_game(components);
        assert_eq!(grouped.get(&first).expect("first components").len(), 1);
        assert_eq!(grouped.get(&second).expect("second components").len(), 1);
        assert_eq!(grouped[&first][0].technology(), LibraryTechnology::AmdFsr);
        assert_eq!(
            grouped[&second][0].technology(),
            LibraryTechnology::IntelXeSs
        );
    }
}
