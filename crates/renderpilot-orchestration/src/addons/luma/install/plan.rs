use std::path::{Path, PathBuf};

use super::PreparedInstall;
use crate::addons::engine::FileOp;
use crate::addons::luma::fetch::types::LumaPayloadFile;
use crate::addons::luma::{dgvoodoo, dlss};
use crate::addons::reshade::host_policy::HostAssessment;

/// A `CreateNested` op per payload file -- root-level files (the main `.addon`
/// and `Luma/**` tree). DLSS is handled by the managed-file planner, not here.
/// Consumes payload bytes so they are not cloned into the engine plan.
pub(super) fn payload_ops(payload: Vec<LumaPayloadFile>) -> Vec<FileOp> {
    payload
        .into_iter()
        .filter(|file| !dlss::is_dlss_relative_path(&file.relative_path))
        .map(|file| FileOp::CreateNested {
            relative_path: file.relative_path,
            bytes: file.bytes,
        })
        .collect()
}

/// Operations that must land beside the game's executable rather than in
/// ReShade's effective add-on directory: the proxy host and managed wrappers.
///
/// Moves host DLL bytes out of `prepared` when a host write is required.
pub(crate) fn game_dir_ops(prepared: &mut PreparedInstall, host: &HostAssessment) -> Vec<FileOp> {
    let mut ops = Vec::new();
    if host.initial_writes_host() {
        ops.push(FileOp::Replace {
            name: prepared.proxy_dll_name.clone(),
            bytes: std::mem::take(&mut prepared.reshade_dll_bytes),
        });
    }
    ops.extend(dgvoodoo_ops(prepared));
    ops
}

/// Live paths under the game directory that install may touch (no byte clones).
/// Destinations match [`game_dir_ops`] (plus absolute adopted dgVoodoo paths).
pub(crate) fn game_dir_live_paths(
    game_dir: &Path,
    prepared: &PreparedInstall,
    host: &HostAssessment,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if host.initial_writes_host() {
        paths.push(game_dir.join(&prepared.proxy_dll_name));
    }
    match prepared.dgvoodoo.as_ref() {
        Some(dgvoodoo::DgVoodooInstall::Managed(managed)) => {
            for file in &managed.files {
                paths.push(game_dir.join(&file.dest));
            }
            paths.push(game_dir.join(&managed.config_file));
        }
        Some(dgvoodoo::DgVoodooInstall::Reused(reused)) => {
            paths.push(game_dir.join(&reused.config_file));
        }
        Some(dgvoodoo::DgVoodooInstall::Adopted(adopted)) => {
            paths.push(game_dir.join(&adopted.config.config_file));
            paths.extend(adopted.existing_paths.iter().cloned());
        }
        None => {}
    }
    paths
}

fn dgvoodoo_ops(prepared: &mut PreparedInstall) -> Vec<FileOp> {
    match prepared.dgvoodoo.as_mut() {
        Some(dgvoodoo::DgVoodooInstall::Managed(managed)) => dgvoodoo::install_ops(managed),
        Some(dgvoodoo::DgVoodooInstall::Reused(reused)) => dgvoodoo::reuse_ops(reused),
        Some(dgvoodoo::DgVoodooInstall::Adopted(adopted)) => dgvoodoo::reuse_ops(&adopted.config),
        None => Vec::new(),
    }
}
