use crate::addons::engine::{FileOp, IniSection, MergeStrategy};

use super::super::types::ExternalConfigSection;
use super::model::{PreparedDgVoodoo, ReusedDgVoodoo};

/// Engine operations for adding this dependency to a game directory.
///
/// Consumes managed file bodies into ops so they are not cloned. Metadata
/// fields on `prepared` (source URL, digests, config defaults) remain for
/// record provenance after this call.
pub(crate) fn install_ops(prepared: &mut PreparedDgVoodoo) -> Vec<FileOp> {
    let files = std::mem::take(&mut prepared.files);
    let mut ops: Vec<FileOp> = files
        .into_iter()
        .map(|file| FileOp::BackupAndReplace {
            name: file.dest,
            bytes: file.bytes,
        })
        .collect();
    ops.push(FileOp::MergeText {
        name: prepared.config_file.clone(),
        default: prepared.config_default.clone(),
        strategy: merge_strategy(prepared),
    });
    ops
}

/// Non-destructive config merge for an existing compatible dgVoodoo runtime.
/// `UpdateText` deliberately produces no `.bak`; it also preserves every key
/// outside the manifest's explicit sections.
pub(crate) fn reuse_ops(reused: &ReusedDgVoodoo) -> Vec<FileOp> {
    vec![FileOp::UpdateText {
        name: reused.config_file.clone(),
        default: reused.config_default.clone(),
        strategy: MergeStrategy::IniSetKeys {
            sections: reused.config_sections.clone(),
        },
    }]
}

/// Applies the dependency's config merge strategy to an existing config string.
#[must_use]
pub(crate) fn merged_config(prepared: &PreparedDgVoodoo, current: &str) -> String {
    merge_strategy(prepared).apply(current)
}

fn merge_strategy(prepared: &PreparedDgVoodoo) -> MergeStrategy {
    MergeStrategy::IniSetKeys {
        sections: prepared.config_sections.clone(),
    }
}

pub(super) fn config_sections(config: &[ExternalConfigSection]) -> Vec<IniSection> {
    config
        .iter()
        .map(|section| IniSection {
            name: section.section.clone(),
            keys: section
                .entries
                .iter()
                .map(|entry| (entry.key.clone(), entry.value.clone()))
                .collect(),
        })
        .collect()
}

pub(super) fn managed_config_default(config: &[ExternalConfigSection]) -> String {
    let mut out = String::new();
    for (index, section) in config.iter().enumerate() {
        if index > 0 {
            out.push_str("\r\n");
        }
        out.push('[');
        out.push_str(&section.section);
        out.push_str("]\r\n");
        for entry in &section.entries {
            out.push_str(&entry.key);
            out.push_str(" = ");
            out.push_str(&entry.value);
            out.push_str("\r\n");
        }
    }
    out
}
