//! Durable file-mutation target computation for RenoDX commands.
//!
//! Each command snapshots every live/sidecar path it may touch so the outer
//! `DurableFileTransaction` can restore the exact before-state after a crash.
//! Targets are over-inclusive — an untouched path is only snapshotted.

use std::path::{Path, PathBuf};

use renderpilot_domain::InstalledAddon;

use crate::addons::mutation_targets::MutationTargets;
use crate::addons::renodx::install::PreparedInstall;
use crate::addons::reshade::proxy::HostKind;
use crate::addons::reshade::scan as reshade;
use crate::addons::reshade::split_install::InstallRoots;

/// Live/sidecar paths a RenoDX install (proxy or Vulkan) may touch.
pub(crate) fn install_targets(
    game_dir: &Path,
    prepared: &PreparedInstall,
) -> Result<MutationTargets, crate::ServiceError> {
    match prepared.host_kind {
        HostKind::Proxy => {
            let roots = InstallRoots::resolve_from_ini(game_dir);
            let addon_path = roots.addon_dir.join(&prepared.addon_file_name);
            let host_path = roots.game_dir.join(&prepared.proxy_dll_name);
            let ini_path = reshade::reshade_ini_path(&roots.game_dir)
                .unwrap_or_else(|| roots.game_dir.join(reshade::RESHADE_INI_FILE_NAME));
            Ok(MutationTargets::from_roots_and_live_paths(
                [roots.game_dir, roots.addon_dir],
                [addon_path, host_path, ini_path],
            ))
        }
        HostKind::Vulkan => {
            let addon_path = game_dir.join(&prepared.addon_file_name);
            let ini_path = reshade::reshade_ini_path(game_dir)
                .unwrap_or_else(|| game_dir.join(reshade::RESHADE_INI_FILE_NAME));
            Ok(MutationTargets::from_roots_and_live_paths(
                [game_dir.to_path_buf()],
                [addon_path, ini_path],
            ))
        }
    }
}

/// Live/sidecar paths a RenoDX update may touch.
pub(crate) fn update_targets(
    record: &InstalledAddon,
    replacement_paths: &[PathBuf],
    host_install_path: Option<&Path>,
) -> Result<MutationTargets, crate::ServiceError> {
    let binding = super::dlss_fix_binding::resolve(record);
    if binding.main_payload_collides() {
        return Err(super::errors::invalid(
            "RenoDX main payload collides with the reserved DLSS-Fix companion target".to_owned(),
        ));
    }
    let mut extra: Vec<PathBuf> = replacement_paths.to_vec();
    if let Some(host_path) = host_install_path {
        extra.push(host_path.to_path_buf());
    }
    Ok(MutationTargets::for_record_excluding(
        record,
        std::iter::empty(),
        extra,
        binding.isolation_paths,
    ))
}

/// Live/sidecar paths a proxy ReShade channel switch may touch.
pub(crate) fn channel_switch_targets(target_path: &Path, game_dir: &Path) -> MutationTargets {
    MutationTargets::for_live_paths([game_dir.to_path_buf()], [target_path.to_path_buf()])
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{AddonKind, GameId, ManagedAddonFile, PathRef, Sha256Hash};

    use super::*;

    #[test]
    fn renodx_update_excludes_every_recorded_dlss_candidate_and_sidecar_but_keeps_main_and_host() {
        let root = PathBuf::from(r"C:\Games\Target");
        let addon = root.join("renodx-game.addon64");
        let host = root.join("dxgi.dll");
        let companion = root.join("renodx-dlssfix.addon64");
        let wrong_arch = root.join("RENODX-DLSSFIX.ADDON32");
        let backed_candidate = root.join("renodx-dlssfix.legacy");
        let managed_candidate = root.join("RENODX-DLSSFIX.MANAGED");
        let managed_hash = Sha256Hash::new("a".repeat(64)).expect("hash");
        let record = InstalledAddon::new(
            GameId::new("manual:renodx-update-targets").expect("game id"),
            AddonKind::RenoDx,
            PathRef::new(addon.to_string_lossy().into_owned()).expect("addon"),
        )
        .with_created_file(PathRef::new(host.to_string_lossy().into_owned()).expect("host"))
        .with_created_file(
            PathRef::new(companion.to_string_lossy().into_owned()).expect("companion"),
        )
        .with_created_file(PathRef::new(wrong_arch.to_string_lossy().into_owned()).expect("wrong"));
        let record = record
            .with_backed_up_file(
                PathRef::new(backed_candidate.to_string_lossy().into_owned()).expect("backed"),
            )
            .try_with_managed_files(vec![ManagedAddonFile::reused(
                PathRef::new(managed_candidate.to_string_lossy().into_owned()).expect("managed"),
                managed_hash,
            )])
            .expect("managed files");
        let targets = update_targets(&record, std::slice::from_ref(&addon), None).expect("targets");
        let keys: Vec<String> = targets
            .paths
            .iter()
            .map(|path| crate::paths::normalized_key(path))
            .collect();

        assert!(keys.contains(&crate::paths::normalized_key(&addon)));
        assert!(keys.contains(&crate::paths::normalized_key(&host)));
        assert!(keys.contains(&crate::paths::normalized_key(
            &host.with_extension("dll.bak")
        )));
        assert!(!keys.contains(&crate::paths::normalized_key(&companion)));
        assert!(!keys.contains(&crate::paths::normalized_key(
            &companion.with_extension("addon64.bak")
        )));
        assert!(!keys.contains(&crate::paths::normalized_key(&wrong_arch)));
        assert!(!keys.contains(&crate::paths::normalized_key(
            &wrong_arch.with_extension("addon32.bak")
        )));
        for candidate in [&backed_candidate, &managed_candidate] {
            assert!(!keys.contains(&crate::paths::normalized_key(candidate)));
            let sidecar = crate::fs::backup_path(candidate).expect("sidecar");
            assert!(!keys.contains(&crate::paths::normalized_key(&sidecar)));
        }
    }

    #[test]
    fn renodx_update_rejects_a_legacy_main_companion_collision_before_durable_work() {
        let root = PathBuf::from(r"C:\Games\Target");
        let main = root.join("renodx-dlssfix.addon64");
        let record = InstalledAddon::new(
            GameId::new("manual:renodx-update-collision").expect("game id"),
            AddonKind::RenoDx,
            PathRef::new(main.to_string_lossy().into_owned()).expect("addon"),
        );

        assert!(update_targets(&record, std::slice::from_ref(&main), None).is_err());
    }
}
