//! Proxy-slot discovery and ReShade peer-host planning.

use super::*;
use renderpilot_application::InstalledAddonRepository;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlanningMode {
    ExistingInstall,
    Candidate,
}

pub(super) fn plan(
    context: &Context,
    manifest: &OptiScalerManifest,
    game_id: &GameId,
    target_dir: Option<&Path>,
    rule: Option<&crate::addons::optiscaler::compatibility_catalog::ResolvedVariant>,
    mode: PlanningMode,
) -> Result<EvaluatedProxyPlan, ServiceError> {
    if mode == PlanningMode::ExistingInstall
        && let Some(topology) = context.storage().get_proxy_topology(game_id)?
    {
        return Ok(EvaluatedProxyPlan {
            slot: PathBuf::from(topology.root_slot.as_str()),
            chain_reshade: topology.downstream.is_some(),
            downstream_path: topology
                .downstream
                .as_ref()
                .map(|link| PathBuf::from(link.path.as_str())),
            conflict: None,
            reshade_source_path: None,
            reshade_source_sha256: None,
        });
    }
    let explicit_rule_slot = rule.and_then(|rule| match &rule.proxy {
        crate::addons::optiscaler::compatibility_catalog::ProxyPolicy::Automatic => None,
        crate::addons::optiscaler::compatibility_catalog::ProxyPolicy::Exact { slot } => {
            Some(slot.clone())
        }
    });
    let configured_slot_name = explicit_rule_slot
        .clone()
        .unwrap_or_else(|| "dxgi.dll".to_owned());
    let known_slots = target_dir
        .map(|dir| known_optiscaler_proxy_slots(manifest, dir))
        .unwrap_or_default();
    let (mut slot_name, mut conflict) = match known_slots.as_slice() {
        [] => (configured_slot_name, None),
        [known] => (
            known
                .file_name()
                .and_then(|name| name.to_str())
                .map(str::to_owned)
                .ok_or_else(|| failed("known OptiScaler proxy has no valid file name"))?,
            None,
        ),
        _ => (
            configured_slot_name,
            Some(
                "multiple manifest-verified OptiScaler proxy loaders were found; the active root slot is ambiguous"
                    .to_owned(),
            ),
        ),
    };
    let addons = context
        .storage()
        .list_installed_addons()?
        .into_iter()
        .filter(|record| record.game_id() == game_id)
        .collect::<Vec<_>>();
    let managed_hosts = addons
        .iter()
        .filter(|record| matches!(record.kind(), AddonKind::RenoDx | AddonKind::Luma))
        .map(crate::addons::tracking::host_proxy_path)
        .collect::<Vec<_>>();
    let has_managed_peer = !managed_hosts.is_empty();
    let managed_source = managed_hosts
        .iter()
        .flatten()
        .find(|host| host.is_file() && crate::addons::reshade::scan::is_reshade_proxy_file(host))
        .cloned();
    let scanned_source = if managed_source.is_none() {
        target_dir.and_then(|dir| {
            let scan = crate::addons::reshade::scan::scan_reshade_hosts(dir, Some(&slot_name));
            if !scan.hosts.is_empty()
                && crate::addons::reshade::scan::is_known_custom_build(dir, None)
            {
                conflict = Some(
                    "a recognized custom ReShade build cannot be chained automatically"
                        .to_owned(),
                );
                return None;
            }
            let hosts = scan
                .reshade_hosts()
                .into_iter()
                .filter(|host| {
                    host.as_present().is_some_and(|host| {
                        !known_slots
                            .iter()
                            .any(|outer| crate::paths::same_path(outer, host.path))
                    })
                })
                .collect::<Vec<_>>();
            if hosts.len() > 1 {
                conflict = Some(
                    "multiple ReShade hosts were found; select a single proxy slot before installing OptiScaler"
                        .to_owned(),
                );
                return None;
            }
            hosts
                .first()
                .and_then(|host| host.as_present())
                .filter(|host| crate::addons::reshade::scan::is_reshade_proxy_file(host.path))
                .map(|host| host.path.to_path_buf())
        })
    } else {
        None
    };
    let reshade_source = managed_source.or(scanned_source);
    // A discovered ReShade host is the loader slot the game actually uses.
    // With no explicit compatibility override and no already-known OptiScaler
    // outer, the new outer must take that exact slot.  Choosing the generic
    // default (`dxgi.dll`) while moving a host from (for example) `d3d12.dll`
    // would create a topology whose return target disagrees with the peer
    // receipt — and, more importantly, would not preserve the game's active
    // loader path.
    if known_slots.is_empty()
        && let Some(source_name) = reshade_source
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
    {
        match explicit_rule_slot.as_deref() {
            None => source_name.clone_into(&mut slot_name),
            Some(expected) if !expected.eq_ignore_ascii_case(source_name) => {
                conflict = Some(format!(
                    "the active ReShade host uses {source_name}, but the OptiScaler compatibility rule requires {expected}; automatic chaining would change the game's loader slot"
                ));
            }
            Some(_) => {}
        }
    }
    let slot = target_dir.map_or_else(|| PathBuf::from(&slot_name), |dir| dir.join(&slot_name));
    let slot_path = target_dir.map(|dir| dir.join(&slot_name));
    let slot_is_known_optiscaler = slot_path
        .as_deref()
        .is_some_and(|path| is_known_optiscaler_proxy(manifest, path));
    if has_managed_peer && reshade_source.is_none() && conflict.is_none() {
        conflict = Some("the ReShade proxy used by the managed peer add-on is missing".to_owned());
    }
    if conflict.is_none()
        && let (Some(root), Some(source)) = (slot_path.as_deref(), reshade_source.as_deref())
        && root.exists()
        && !crate::paths::same_path(root, source)
        && !slot_is_known_optiscaler
    {
        conflict = Some(proxy_conflict_message(root, &slot_name));
    }
    let downstream = target_dir.map(|dir| dir.join("ReShade64.dll"));
    if conflict.is_none()
        && reshade_source.is_none()
        && downstream.as_deref().is_some_and(Path::exists)
    {
        conflict = Some(
            "ReShade64.dll exists but is not a recognized ReShade host; the proxy chain was left unchanged"
                .to_owned(),
        );
    }
    if conflict.is_none()
        && let (Some(source), Some(downstream)) = (reshade_source.as_deref(), downstream.as_deref())
        && downstream.exists()
        && !crate::paths::same_path(source, downstream)
    {
        conflict = Some("ReShade64.dll already exists; proxy chain is ambiguous".to_owned());
    }
    let mut chain_reshade = reshade_source.is_some() && conflict.is_none();
    if chain_reshade
        && let (Some(source), Some(downstream)) = (reshade_source.as_deref(), downstream.as_deref())
        && let Err(error) = super::super::lifecycle::preflight_peer_host_transition(
            context, game_id, source, downstream,
        )
    {
        conflict = Some(format!(
            "the active ReShade peer host transition is not provable: {error}"
        ));
        chain_reshade = false;
    }
    let reshade_source_sha256 = if chain_reshade {
        let source = reshade_source
            .as_deref()
            .ok_or_else(|| failed("ReShade chain source disappeared during proxy planning"))?;
        match renderpilot_detection::sha256_file(source) {
            Ok(hash) => Some(hash),
            Err(error) => {
                tracing::warn!(
                    "failed to hash ReShade chain source {}: {error}",
                    source.display()
                );
                conflict =
                    Some("the detected ReShade chain source could not be hashed safely".to_owned());
                chain_reshade = false;
                None
            }
        }
    } else {
        None
    };
    let downstream_path = if chain_reshade { downstream } else { None };
    let reshade_source_path = if chain_reshade { reshade_source } else { None };
    Ok(EvaluatedProxyPlan {
        slot,
        chain_reshade,
        downstream_path,
        conflict,
        reshade_source_path,
        reshade_source_sha256,
    })
}

/// Identifies an already-installed OptiScaler outer loader by immutable
/// release payload identity. File names and PE metadata are not sufficient:
/// another proxy may legitimately use the same slot and must remain a
/// conflict until its exact bytes are known to the manifest.
fn is_known_optiscaler_proxy(manifest: &OptiScalerManifest, path: &Path) -> bool {
    let Ok(actual) = renderpilot_detection::sha256_file(path) else {
        return false;
    };
    manifest.releases.iter().any(|release| {
        release.members.iter().any(|member| {
            member.module == "core"
                && member.target == "$proxy"
                && Sha256Hash::new(member.sha256.clone()).is_ok_and(|expected| expected == actual)
        })
    })
}

fn known_optiscaler_proxy_slots(manifest: &OptiScalerManifest, target_dir: &Path) -> Vec<PathBuf> {
    super::super::tool::PROXY_NAMES
        .iter()
        .map(|name| target_dir.join(name))
        .filter(|path| is_known_optiscaler_proxy(manifest, path))
        .collect()
}
