//! Resolves the executable-side and payload-side roots used by Luma update.

use std::path::{Path, PathBuf};

use renderpilot_domain::{GameId, InstalledAddon};

use crate::addons::luma::errors;
use crate::addons::luma::types::LumaManifest;
use crate::addons::luma::use_cases::update_target;
use crate::addons::reshade::InstallRoots;
use crate::paths::same_path;
use crate::{Context, ServiceError};

pub(super) fn addon_parent_dir(record: &InstalledAddon) -> Result<PathBuf, ServiceError> {
    Path::new(record.addon_file().as_str())
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| {
            errors::failed("Luma install record's add-on file has no parent directory".to_owned())
        })
}

/// Sentinel/host writes use `game_dir`; the release set-diff uses `payload_dir`.
#[derive(Debug, Clone)]
pub(super) struct UpdateLayout {
    pub(super) game_dir: PathBuf,
    pub(super) payload_dir: PathBuf,
    pub(super) roots: InstallRoots,
}

impl UpdateLayout {
    #[must_use]
    pub(super) fn sentinel_dir(&self) -> &Path {
        debug_assert!(same_path(&self.game_dir, self.roots.sentinel_dir()));
        self.game_dir.as_path()
    }

    pub(super) fn scan_dir_paths(&self) -> Vec<&Path> {
        let mut dirs = self.roots.scan_dir_paths();
        if !dirs.iter().any(|dir| same_path(dir, &self.payload_dir)) {
            dirs.push(self.payload_dir.as_path());
        }
        dirs
    }
}

/// Resolves update layout from the recorded payload root and a fresh live
/// manifest match. Recorded ownership paths cannot replace the matcher-owned
/// executable, proxy-slot, and dependency decisions.
pub(super) fn resolve_update_layout(
    context: &Context,
    manifest: &LumaManifest,
    game_id: &GameId,
    record: &InstalledAddon,
) -> Result<UpdateLayout, ServiceError> {
    let payload_dir = addon_parent_dir(record)?;
    let game_dir = update_target::require_update_target(context, manifest, game_id)?.game_dir;
    let roots = InstallRoots::resolve_from_ini(&game_dir);
    Ok(UpdateLayout {
        game_dir,
        payload_dir,
        roots,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use renderpilot_domain::{AddonKind, GameId, InstalledAddon, PathRef};
    use tempfile::tempdir;

    use super::*;
    use crate::addons::engine;

    fn path_ref(path: &Path) -> PathRef {
        PathRef::new(path.to_string_lossy().into_owned()).expect("path")
    }

    #[test]
    fn split_layout_anchors_sentinel_at_game_dir() {
        let game = tempdir().expect("game");
        let addon = tempdir().expect("addon");
        let addon_file = addon.path().join("Luma-Game.addon");
        fs::write(&addon_file, b"addon").expect("write addon");
        let record = InstalledAddon::new(
            GameId::new("steam:1").expect("id"),
            AddonKind::Luma,
            path_ref(&addon_file),
        );
        let layout = UpdateLayout {
            game_dir: game.path().to_path_buf(),
            payload_dir: addon_parent_dir(&record).expect("payload"),
            roots: InstallRoots::resolve_from_ini(game.path()),
        };

        let commit =
            engine::PendingInstallCommit::begin(game.path(), AddonKind::Luma).expect("sentinel");
        assert!(engine::is_install_torn(
            layout.sentinel_dir(),
            AddonKind::Luma
        ));
        assert!(!engine::is_install_torn(
            &layout.payload_dir,
            AddonKind::Luma
        ));
        assert!(
            layout
                .scan_dir_paths()
                .iter()
                .any(|dir| same_path(dir, &layout.payload_dir))
        );
        commit.finish_committed();
    }
}
