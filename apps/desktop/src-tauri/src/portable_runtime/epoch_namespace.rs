use std::path::{Path, PathBuf};

use super::{
    error::{PortableRuntimeError, Result},
    win32::directory::{
        EntryKind, ensure_plain_directory, expect_exact_addition, require_known_entries_skipping,
        stable_scan, stable_scan_skipping,
    },
};

const AUTHORITY_ENTRIES: [(&str, EntryKind); 3] = [
    ("admission.lock", EntryKind::File),
    ("epochs", EntryKind::Directory),
    ("provenance", EntryKind::Directory),
];
const SCAN_SKIPS: [&str; 1] = ["admission.lock"];

/// D17's total, fail-closed namespace classifier. Every known authority leaf
/// is pinned no-follow, unknown/reparse/nonregular/multilink/unopenable leaves
/// are retained and block mutation, and epoch publication proves one Scan A/B
/// delta instead of trusting path observations or counters.
pub fn establish_epoch(authority_root: &Path, epoch: &str) -> Result<PathBuf> {
    if !canonical_epoch(epoch) {
        return Err(PortableRuntimeError::new(
            "portable_epoch_invalid",
            "epoch name was not canonical",
        ));
    }
    ensure_plain_directory(authority_root)?;
    let authority_scan = stable_scan_skipping(authority_root, &SCAN_SKIPS)?;
    require_known_entries_skipping(
        authority_root,
        &authority_scan,
        &AUTHORITY_ENTRIES,
        &SCAN_SKIPS,
    )?;
    let epochs = authority_root.join("epochs");
    ensure_plain_directory(&epochs)?;
    let before = stable_scan(&epochs)?;
    for (raw_name, kind) in before.entries() {
        if *kind != EntryKind::Directory
            || !String::from_utf16(raw_name)
                .map(|name| canonical_epoch(&name))
                .unwrap_or(false)
        {
            return Err(PortableRuntimeError::new(
                "portable_namespace_unknown",
                "epoch namespace contained an unknown or malformed leaf",
            ));
        }
    }
    let epoch_root = epochs.join(epoch);
    if epoch_root.exists() {
        return Err(PortableRuntimeError::new(
            "portable_epoch_collision",
            "epoch already existed",
        ));
    }
    std::fs::create_dir(&epoch_root)?;
    let after = stable_scan(&epochs)?;
    expect_exact_addition(&before, &after, epoch, EntryKind::Directory)?;
    Ok(epoch_root)
}

fn canonical_epoch(epoch: &str) -> bool {
    epoch.len() == 64
        && epoch
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}
