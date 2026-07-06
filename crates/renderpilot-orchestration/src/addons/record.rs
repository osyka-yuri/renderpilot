//! Maps an engine [`InstallReceipt`] into the persisted, reversible
//! [`InstalledAddon`] record — the tool-agnostic glue between "what was written" and
//! "how to reverse and update it".
//!
//! The tool layer supplies the primary add-on file and the upstream
//! [`TrackedSource`]s it manages; this assembles the record (primary file +
//! every other created/backed-up file + sources), preserving the invariant that the
//! add-on file is always a member of `created_files`.

use std::path::Path;

use renderpilot_domain::{AddonKind, GameId, InstalledAddon, PathRef, TrackedSource};

use super::engine::InstallReceipt;
use crate::ServiceError;

/// Builds the install record from a receipt.
///
/// `addon_file` is the primary payload (recorded as the record's add-on file and
/// always present in `created_files`); `tracked_sources` are the upstream sources
/// the update system will check.
pub fn build(
    game_id: GameId,
    kind: AddonKind,
    addon_file: &Path,
    receipt: &InstallReceipt,
    tracked_sources: Vec<TrackedSource>,
) -> Result<InstalledAddon, ServiceError> {
    let mut record = InstalledAddon::new(game_id, kind, to_path_ref(addon_file)?)
        .with_tracked_sources(tracked_sources);

    // `new` already records the add-on file; add every *other* written file.
    for path in &receipt.created_files {
        if path.as_path() != addon_file {
            record = record.with_created_file(to_path_ref(path)?);
        }
    }
    for path in &receipt.backed_up_files {
        record = record.with_backed_up_file(to_path_ref(path)?);
    }

    Ok(record)
}

fn to_path_ref(path: &Path) -> Result<PathRef, ServiceError> {
    PathRef::new(path.to_string_lossy().into_owned())
        .map_err(|error| ServiceError::CommandFailed(format!("invalid install path: {error}")))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use renderpilot_domain::TrackedSourceRole;
    use tempfile::tempdir;

    use super::*;
    use crate::addons::engine::{self, FileOp, IniSection, InstallPlan, MergeStrategy};
    use crate::addons::path_bufs;

    /// The extensibility seam: a *second* tool with a different shape — an
    /// OptiScaler-style proxy DLL plus an `OptiScaler.ini` (a different section and
    /// keys) plus its own marker — installs, records, and fully round-trips through
    /// the **same** [`engine`] and [`build`] with no framework changes. Shipping the
    /// real tool is then one `AddonKind` variant + one `addons::optiscaler` module +
    /// thin commands; nothing here, in the engine, or in the record changes.
    #[test]
    fn a_second_tool_shape_installs_records_and_reverses_through_the_shared_framework() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        fs::write(game.join("dxgi.dll"), b"game-shipped-dxgi").expect("write");

        let plan = InstallPlan {
            // A real OptiScaler tool would add its own `AddonKind` variant; the
            // engine is kind-agnostic beyond namespacing its sentinel, so the seam
            // is provable without polluting the production enum.
            kind: AddonKind::RenoDx,
            ops: vec![
                FileOp::BackupAndReplace {
                    name: "dxgi.dll".to_owned(),
                    bytes: b"optiscaler-proxy".to_vec(),
                },
                FileOp::MergeText {
                    name: "OptiScaler.ini".to_owned(),
                    default: String::new(),
                    strategy: MergeStrategy::IniSetKeys {
                        sections: vec![IniSection {
                            name: "Upscalers".to_owned(),
                            keys: vec![("Dx12Upscaler".to_owned(), "fsr31".to_owned())],
                        }],
                    },
                },
                FileOp::Create {
                    name: "optiscaler-marker.json".to_owned(),
                    bytes: b"{}".to_vec(),
                },
            ],
        };

        let receipt = engine::install(game, &plan).expect("install");

        // The proxy DLL is the primary file; the record carries a host binary entry.
        let proxy = game.join("dxgi.dll");
        let record = build(
            GameId::new("steam:42").expect("id"),
            AddonKind::RenoDx,
            &proxy,
            &receipt,
            vec![TrackedSource::new(
                TrackedSourceRole::HostBinary,
                "https://example.com/optiscaler.zip",
                None,
                "opti-digest",
            )],
        )
        .expect("record");

        assert!(record.has_host_binary_provenance());
        assert_eq!(record.tracked_sources().len(), 1);
        // proxy + ini + marker, with the original dxgi.dll backed up.
        assert_eq!(record.created_files().len(), 3);
        assert_eq!(record.backed_up_files().len(), 1);

        engine::uninstall(
            &path_bufs(record.created_files()),
            &path_bufs(record.backed_up_files()),
        )
        .expect("uninstall");

        // The game folder is restored exactly.
        assert_eq!(
            fs::read(game.join("dxgi.dll")).unwrap(),
            b"game-shipped-dxgi"
        );
        assert!(!game.join("OptiScaler.ini").exists());
        assert!(!game.join("optiscaler-marker.json").exists());
    }
}
