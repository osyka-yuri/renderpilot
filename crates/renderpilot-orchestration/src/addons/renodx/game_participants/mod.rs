//! Pure, touched-path planning for RenoDX game-file participants.
//!
//! The generic install engine remains the executor for ordinary game-only
//! installs.  Combined Vulkan transactions need a before/after snapshot before
//! their first write, so this planner interprets only the two operations RenoDX
//! emits for that flow (`Replace` and `UpdateText`).  It deliberately never
//! enumerates the game directory or reads an unrelated file.

use std::fs;
use std::path::{Path, PathBuf};

use crate::ServiceError;
use crate::addons::engine::{
    FileOp, InstallPlan, InstallReceipt, ensure_bare_file_name, existing_case_insensitive,
    safe_join,
};

/// One exact game-file transition owned by the RenoDX planner.
///
/// The coordinator translates this tool-level value into its durable protocol
/// representation. Keeping the types separate prevents the planner from
/// depending on a particular transaction implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GameFileIntent {
    pub(crate) live_path: PathBuf,
    pub(crate) before: Option<Vec<u8>>,
    pub(crate) after: Option<Vec<u8>>,
}

/// Exact game participants and receipt projection for one RenoDX Vulkan plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GameParticipantPlan {
    files: Vec<GameFileIntent>,
    created_dirs: Vec<PathBuf>,
    receipt: InstallReceipt,
    config_receipt: Option<renderpilot_domain::RenoDxConfigReceipt>,
}

impl GameParticipantPlan {
    /// Moves the exact game participants and directory authorities into the
    /// durable mutation composer. The receipt is only needed while building
    /// the install record and is intentionally dropped after that projection.
    pub(crate) fn into_parts(self) -> (Vec<GameFileIntent>, Vec<PathBuf>) {
        let Self {
            files,
            created_dirs,
            receipt: _,
            config_receipt: _,
        } = self;
        (files, created_dirs)
    }

    #[must_use]
    pub(crate) fn receipt(&self) -> &InstallReceipt {
        &self.receipt
    }

    pub(crate) fn config_receipt(&self) -> Option<&renderpilot_domain::RenoDxConfigReceipt> {
        self.config_receipt.as_ref()
    }

    /// Combines independently rooted RenoDX participants while preserving the
    /// engine receipt ordering and the single typed config receipt invariant.
    pub(crate) fn merge(mut self, mut other: Self) -> Result<Self, ServiceError> {
        if self.config_receipt.is_some() && other.config_receipt.is_some() {
            return Err(crate::addons::renodx::errors::invalid(
                "RenoDX install plan contains duplicate config operations".to_owned(),
            ));
        }
        self.files.append(&mut other.files);
        self.created_dirs.append(&mut other.created_dirs);
        self.receipt
            .created_files
            .append(&mut other.receipt.created_files);
        self.receipt
            .backed_up_files
            .append(&mut other.receipt.backed_up_files);
        if self.config_receipt.is_none() {
            self.config_receipt = other.config_receipt.take();
        }
        Ok(self)
    }
}

/// Plans the exact before/after bytes for the touched files in `plan`.
///
/// This preserves the generic engine's relevant semantics: `Replace` uses the
/// validated exact name and `UpdateText` resolves an existing file
/// case-insensitively, reads update bytes with lossy UTF-8, and uses the default
/// only when the resolved path is absent.  Any other operation is rejected so a
/// future RenoDX operation cannot silently acquire combined-transaction support
/// without an explicit planner rule.
pub(crate) fn build(
    game_dir: &Path,
    plan: InstallPlan,
) -> Result<GameParticipantPlan, ServiceError> {
    let mut files = Vec::with_capacity(plan.ops.len());
    let mut receipt = InstallReceipt::default();
    let mut config_receipt = None;

    for operation in plan.ops {
        let (path, before, after, record_as_created, planned_config_receipt) = match operation {
            FileOp::Replace { name, bytes } => {
                let path = exact_path(game_dir, "file name", &name)?;
                let before = read_replace_before(&path)?;
                (path, before, Some(bytes), true, None)
            }
            FileOp::UpdateText {
                name,
                default,
                strategy,
            } => {
                ensure_bare_file_name("update file name", &name)?;
                let path = existing_case_insensitive(game_dir, &name)
                    .unwrap_or_else(|| game_dir.join(name));
                let before = read_update_before(&path)?;
                let current = before
                    .as_deref()
                    .map(String::from_utf8_lossy)
                    .map_or_else(|| default, |value| value.into_owned());
                let after = strategy.apply(&current).into_bytes();
                let record_as_created = before.is_none();
                (path, before, Some(after), record_as_created, None)
            }
            FileOp::RenoDxConfig {
                name,
                expected_before,
                after,
                receipt,
            } => {
                ensure_bare_file_name("RenoDX config file name", &name)?;
                let path = existing_case_insensitive(game_dir, &name)
                    .unwrap_or_else(|| game_dir.join(name));
                let current_before = read_update_before(&path)?;
                if current_before != expected_before {
                    return Err(crate::addons::errors::invalid(format!(
                        "RenoDX ReShade.ini changed after preparation: {}",
                        path.display()
                    )));
                }
                let record_as_created = expected_before.is_none();
                (
                    path,
                    expected_before,
                    Some(after),
                    record_as_created,
                    Some(receipt),
                )
            }
            _ => {
                return Err(crate::addons::errors::invalid(
                    "RenoDX Vulkan combined planning supports only Replace, UpdateText, and RenoDxConfig operations",
                ));
            }
        };
        if let Some(planned_config_receipt) = planned_config_receipt
            && config_receipt.replace(planned_config_receipt).is_some()
        {
            return Err(crate::addons::errors::invalid(
                "RenoDX Vulkan plan contains duplicate config operations",
            ));
        }

        if record_as_created {
            receipt.created_files.push(path.clone());
        }
        files.push(GameFileIntent {
            live_path: path,
            before,
            after,
        });
    }

    Ok(GameParticipantPlan {
        files,
        created_dirs: Vec::new(),
        receipt,
        config_receipt,
    })
}

fn exact_path(game_dir: &Path, field: &str, name: &str) -> Result<PathBuf, ServiceError> {
    safe_join(game_dir, field, name)
}

fn read_replace_before(path: &Path) -> Result<Option<Vec<u8>>, ServiceError> {
    if !path.exists() {
        return Ok(None);
    }
    if !path.is_file() {
        return Err(crate::addons::errors::invalid(format!(
            "cannot replace `{}`: not a regular file",
            path.display()
        )));
    }
    fs::read(path)
        .map(Some)
        .map_err(|error| crate::addons::errors::io("read before replace", path, &error))
}

fn read_update_before(path: &Path) -> Result<Option<Vec<u8>>, ServiceError> {
    if !path.exists() {
        return Ok(None);
    }
    fs::read(path)
        .map(Some)
        .map_err(|error| crate::addons::errors::io("read for update", path, &error))
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;
    use crate::addons::engine::{IniSection, MergeStrategy};
    use renderpilot_domain::{AddonKind, RenoDxSetPathValue};

    fn plan(ops: Vec<FileOp>) -> InstallPlan {
        InstallPlan {
            kind: AddonKind::RenoDx,
            ops,
        }
    }

    #[test]
    fn replace_reads_only_its_touched_path() {
        let directory = tempdir().expect("tempdir");
        fs::write(directory.path().join("unrelated.bin"), [0xff, 0xfe]).expect("unrelated");
        let participants = build(
            directory.path(),
            plan(vec![FileOp::Replace {
                name: "renodx.addon".to_owned(),
                bytes: vec![1, 2, 3],
            }]),
        )
        .expect("touched path plan");

        assert_eq!(
            participants.files[0].live_path,
            directory.path().join("renodx.addon")
        );
        assert_eq!(participants.files[0].before, None);
        assert_eq!(participants.files[0].after, Some(vec![1, 2, 3]));
        assert_eq!(
            participants.receipt.created_files,
            vec![directory.path().join("renodx.addon")]
        );
    }

    #[test]
    fn replace_records_existing_target_as_owned_like_engine() {
        let directory = tempdir().expect("tempdir");
        let target = directory.path().join("renodx.addon");
        fs::write(&target, [9, 8]).expect("existing addon");
        let participants = build(
            directory.path(),
            plan(vec![FileOp::Replace {
                name: "renodx.addon".to_owned(),
                bytes: vec![1, 2, 3],
            }]),
        )
        .expect("touched path plan");

        assert_eq!(participants.files[0].before, Some(vec![9, 8]));
        assert_eq!(participants.receipt.created_files, vec![target]);
        assert!(participants.receipt.backed_up_files.is_empty());
    }

    #[test]
    fn update_uses_existing_case_and_lossy_utf8_like_engine() {
        let directory = tempdir().expect("tempdir");
        let existing = directory.path().join("reshade.INI");
        let original = vec![b'[', 0xff, b']', b'\n'];
        fs::write(&existing, &original).expect("existing ini");
        let participants = build(
            directory.path(),
            plan(vec![FileOp::UpdateText {
                name: "ReShade.ini".to_owned(),
                default: String::new(),
                strategy: MergeStrategy::IniSetKeys {
                    sections: vec![IniSection {
                        name: "ADDON".to_owned(),
                        keys: vec![("Path".to_owned(), "addons".to_owned())],
                    }],
                },
            }]),
        )
        .expect("update plan");

        assert_eq!(participants.files[0].live_path, existing);
        assert_eq!(participants.files[0].before, Some(original));
        assert!(String::from_utf8(participants.files[0].after.clone().expect("after")).is_ok());
        assert!(participants.receipt.created_files.is_empty());
    }

    #[test]
    fn typed_set_path_uses_planner_bytes_and_captures_receipt() {
        let directory = tempdir().expect("tempdir");
        let existing = directory.path().join("reshade.INI");
        let original = b"; keep this comment\n[renodx]\nSet_Path = arbitrary\n";
        fs::write(&existing, original).expect("existing ini");
        let path_ref = renderpilot_domain::PathRef::new(existing.to_string_lossy().into_owned())
            .expect("path");
        let prepared = crate::addons::renodx::reshade_ini::plan_set_path(
            path_ref,
            original,
            RenoDxSetPathValue::One,
        )
        .expect("set path prepare");
        let participants = build(
            directory.path(),
            plan(vec![FileOp::RenoDxConfig {
                name: "ReShade.ini".to_owned(),
                expected_before: Some(original.to_vec()),
                after: prepared.after,
                receipt: prepared.receipt,
            }]),
        )
        .expect("typed config plan");

        assert_eq!(
            participants.files[0].after,
            Some(b"; keep this comment\n[renodx]\nSet_Path = 1\n".to_vec())
        );
        assert_eq!(
            participants
                .config_receipt
                .as_ref()
                .expect("receipt")
                .baseline,
            renderpilot_domain::RenoDxSetPathBaseline::Present {
                value: "arbitrary".to_owned()
            }
        );
    }

    #[test]
    fn unsupported_operations_fail_closed() {
        let directory = tempdir().expect("tempdir");
        let error = build(
            directory.path(),
            plan(vec![FileOp::Create {
                name: "unexpected.bin".to_owned(),
                bytes: vec![1],
            }]),
        )
        .expect_err("unsupported operation");
        assert!(matches!(error, ServiceError::InvalidInput(_)));
    }
}
