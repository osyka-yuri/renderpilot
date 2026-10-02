//! Read-only current loading-chain proof for persisted Proxy add-on records.
//!
//! This stays separate from the tool registry policy in `tool.rs`: raw records
//! continue to own cleanup, while presentation requires exact current host and
//! payload evidence.

use std::path::{Path, PathBuf};

use renderpilot_domain::{
    Architecture, FileReceipt, GameProxyTopology, InstalledAddon, PathRef, ProxyImplementation,
};

/// Verifies a Proxy installation against the current, bounded loading chain.
/// A validated OptiScaler topology or recorded current host takes precedence;
/// stale evidence never falls through to a different host. Legacy records
/// without a host path use a unique compatible ReShade DLL observed in the
/// current runtime root.
pub(super) fn proxy_binding_matches_current_loading_chain(
    record: &InstalledAddon,
    runtime_root: &Path,
    game_architecture: Option<Architecture>,
    topology: Option<&GameProxyTopology>,
) -> bool {
    let Some(game_architecture) = game_architecture else {
        return false;
    };

    if let Some(topology) = topology {
        return topology_binding_is_current(record, runtime_root, game_architecture, topology);
    }

    match recorded_proxy_host_paths(record, runtime_root, game_architecture) {
        RecordedHostPaths::One(path) => {
            return host_binding_is_current(record, runtime_root, game_architecture, &path);
        }
        RecordedHostPaths::Ambiguous | RecordedHostPaths::Stale => return false,
        RecordedHostPaths::None => {}
    }

    let Some(candidates) = immediate_dll_children(runtime_root) else {
        return false;
    };
    let mut active_hosts = candidates.into_iter().filter(|path| {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        // These are downstream/engine DLLs; they are active only through an
        // explicit validated topology, never by folder discovery.
        if name.eq_ignore_ascii_case("ReShade64.dll") || name.eq_ignore_ascii_case("ReShade32.dll")
        {
            return false;
        }
        host_binding_is_current(record, runtime_root, game_architecture, path)
    });

    let Some(host_path) = active_hosts.next() else {
        return false;
    };
    // Reobserve the selected host after checking the rest of the candidates to
    // preserve the final freshness check for the unique match.
    active_hosts.next().is_none()
        && host_binding_is_current(record, runtime_root, game_architecture, &host_path)
}

enum RecordedHostPaths {
    None,
    One(PathBuf),
    Ambiguous,
    Stale,
}

fn recorded_proxy_host_paths(
    record: &InstalledAddon,
    runtime_root: &Path,
    game_architecture: Architecture,
) -> RecordedHostPaths {
    let payload = Path::new(record.addon_file().as_str());
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut unresolved_host_receipt = false;
    for path in record
        .created_files()
        .iter()
        .chain(record.backed_up_files())
    {
        let path = Path::new(path.as_str());
        if crate::paths::same_path(path, payload)
            || !path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            continue;
        }
        let is_dgvoodoo_dependency =
            crate::addons::luma::tool::is_recorded_dgvoodoo_dependency(record, path);

        // Created/backed-up DLLs are not typed host receipts: Luma may also
        // own dgVoodoo or other support DLLs. Require current PE evidence to
        // identify actual ReShade hosts, while an unresolved recorded host
        // remains authoritative stale evidence and cannot switch to a scan.
        if !is_direct_runtime_child(runtime_root, path) {
            unresolved_host_receipt |= recorded_path_may_be_host(record, is_dgvoodoo_dependency);
            continue;
        }
        let Some(snapshot) = observe_runtime_file(runtime_root, path) else {
            unresolved_host_receipt |= recorded_path_may_be_host(record, is_dgvoodoo_dependency);
            continue;
        };
        let Some(bytes) = snapshot.bytes() else {
            unresolved_host_receipt |= recorded_path_may_be_host(record, is_dgvoodoo_dependency);
            continue;
        };
        match classify_reshade_host(bytes, game_architecture) {
            ReshadeHostClassification::Compatible => {
                if !paths
                    .iter()
                    .any(|existing| crate::paths::same_path(existing, path))
                {
                    paths.push(path.to_path_buf());
                }
            }
            ReshadeHostClassification::Incompatible => unresolved_host_receipt = true,
            ReshadeHostClassification::Other => {
                // Record-level source roles do not identify which DLL a
                // HostBinary came from. A readable non-ReShade DLL can
                // therefore resolve only when another recorded current
                // ReShade host has been proven below.
                unresolved_host_receipt |=
                    recorded_path_may_be_host(record, is_dgvoodoo_dependency);
            }
        }
    }

    match paths.len() {
        0 if unresolved_host_receipt => RecordedHostPaths::Stale,
        0 => RecordedHostPaths::None,
        1 => RecordedHostPaths::One(paths.pop().expect("one compatible host remains")),
        _ => RecordedHostPaths::Ambiguous,
    }
}

fn recorded_path_may_be_host(record: &InstalledAddon, is_dgvoodoo_dependency: bool) -> bool {
    !is_dgvoodoo_dependency || record.has_host_binary_provenance()
}

fn immediate_dll_children(runtime_root: &Path) -> Option<Vec<PathBuf>> {
    let mut candidates = Vec::new();
    for entry in std::fs::read_dir(runtime_root).ok()? {
        let entry = entry.ok()?;
        if !entry.file_type().ok()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_str()?;
        if Path::new(name)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        {
            candidates.push(entry.path());
        }
    }
    Some(candidates)
}

fn topology_binding_is_current(
    record: &InstalledAddon,
    runtime_root: &Path,
    game_architecture: Architecture,
    topology: &GameProxyTopology,
) -> bool {
    if topology.validate().is_err()
        || topology.game_id != *record.game_id()
        || topology.outer.implementation != ProxyImplementation::OptiScaler
        || !is_direct_runtime_child(runtime_root, Path::new(topology.root_slot.as_str()))
        || !crate::addons::reshade::scan::is_proxy_slot(
            Path::new(topology.root_slot.as_str())
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default(),
        )
    {
        return false;
    }

    let Some(downstream) = topology.downstream.as_ref() else {
        return false;
    };
    let host_path = Path::new(downstream.path.as_str());
    if downstream.implementation != ProxyImplementation::ReShade
        || !host_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("ReShade64.dll"))
        || !is_direct_runtime_child(runtime_root, host_path)
    {
        return false;
    }

    let outer_path = Path::new(topology.outer.path.as_str());
    if !is_direct_runtime_child(runtime_root, outer_path) {
        return false;
    }
    let Some(outer_snapshot) = observe_runtime_file(runtime_root, outer_path) else {
        return false;
    };
    if !snapshot_matches_receipt(&outer_snapshot, &topology.outer.receipt) {
        return false;
    }

    let Some(host_snapshot) = observe_runtime_file(runtime_root, host_path) else {
        return false;
    };
    if !snapshot_matches_receipt(&host_snapshot, &downstream.receipt) {
        return false;
    }
    let Some(bytes) = host_snapshot.bytes() else {
        return false;
    };
    is_compatible_reshade_host(bytes, game_architecture)
        && config_binds_payload(record, runtime_root, host_path)
}

fn host_binding_is_current(
    record: &InstalledAddon,
    runtime_root: &Path,
    game_architecture: Architecture,
    host_path: &Path,
) -> bool {
    if !is_direct_runtime_child(runtime_root, host_path)
        || host_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.eq_ignore_ascii_case("ReShade64.dll")
                    || name.eq_ignore_ascii_case("ReShade32.dll")
            })
    {
        return false;
    }
    let Some(snapshot) = observe_runtime_file(runtime_root, host_path) else {
        return false;
    };
    let Some(bytes) = snapshot.bytes() else {
        return false;
    };
    is_compatible_reshade_host(bytes, game_architecture)
        && config_binds_payload(record, runtime_root, host_path)
}

fn is_compatible_reshade_host(bytes: &[u8], game_architecture: Architecture) -> bool {
    classify_reshade_host(bytes, game_architecture) == ReshadeHostClassification::Compatible
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ReshadeHostClassification {
    Other,
    Incompatible,
    Compatible,
}

fn classify_reshade_host(
    bytes: &[u8],
    game_architecture: Architecture,
) -> ReshadeHostClassification {
    let inspection = renderpilot_detection::inspect_pe_bytes(bytes);
    let exports = inspection.export_names.as_deref();
    let has_reshade_export = exports.is_some_and(|names| {
        names
            .iter()
            .any(|name| name.eq_ignore_ascii_case("ReShadeVersion"))
    });
    let recognized = has_reshade_export
        || crate::addons::reshade::scan::version_strings_point_to_reshade(&inspection.identity);
    if !recognized || crate::addons::reshade::scan::is_known_custom_identity(&inspection.identity) {
        return ReshadeHostClassification::Other;
    }
    if inspection.architecture != Some(game_architecture)
        || crate::addons::reshade::scan::addon_support_from_exports_for_topology(
            exports,
            has_reshade_export,
        ) != crate::addons::reshade::scan::ReshadeAddonSupport::Full
    {
        return ReshadeHostClassification::Incompatible;
    }
    ReshadeHostClassification::Compatible
}

fn config_binds_payload(record: &InstalledAddon, runtime_root: &Path, host_path: &Path) -> bool {
    let Ok(snapshot) =
        crate::addons::reshade::scan::resolve_strict_snapshot(runtime_root, Some(host_path))
    else {
        return false;
    };
    let payload = Path::new(record.addon_file().as_str());
    let Some(payload_dir) = payload.parent() else {
        return false;
    };
    payload.is_absolute()
        && crate::paths::same_path(payload_dir, &snapshot.paths().effective_addon_path)
}

fn observe_runtime_file(
    runtime_root: &Path,
    path: &Path,
) -> Option<crate::peer_mutation_executor::PeerPathSnapshot> {
    let root = PathRef::new(runtime_root.to_string_lossy()).ok()?;
    let path = PathRef::new(path.to_string_lossy()).ok()?;
    crate::peer_mutation_executor::observe_peer_path_snapshot(&path, &root).ok()
}

fn snapshot_matches_receipt(
    snapshot: &crate::peer_mutation_executor::PeerPathSnapshot,
    receipt: &FileReceipt,
) -> bool {
    snapshot.file().is_some_and(|file| {
        file.identity() == receipt.identity() && file.digest() == receipt.digest()
    })
}

fn is_direct_runtime_child(runtime_root: &Path, candidate: &Path) -> bool {
    if !candidate.is_absolute()
        || candidate.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        return false;
    }
    candidate
        .parent()
        .is_some_and(|parent| crate::paths::same_path(parent, runtime_root))
}
