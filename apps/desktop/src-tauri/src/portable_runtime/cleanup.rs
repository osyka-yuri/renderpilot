use std::path::Path;

use super::{
    error::{PortableRuntimeError, Result},
    journal::{JournalPhase, read_entries},
    provenance::{self, SealDomain},
};

/// A path-only cleanup is deliberately disabled.  The v1.2 contract permits
/// deletion only from a transaction-owned sealed closed-world manifest through
/// retained live handles (or a later no-follow file-ID enumeration).  This
/// entry point is retained for callers while both staged bytes and snapshots
/// are conservatively retained.
pub fn cleanup_staging_after_terminal(journal_path: &Path, staging: &Path) -> Result<()> {
    require_terminal(journal_path)?;
    if staging.file_name().is_none() {
        return Err(PortableRuntimeError::new(
            "portable_cleanup",
            "refused cleanup without an exact staging leaf",
        ));
    }
    let transaction = transaction_id(journal_path)?;
    provenance::intent(
        SealDomain::Cleanup,
        &format!("cleanup:{transaction}"),
        staging.as_os_str().to_string_lossy().as_bytes(),
    )?;
    // No raw path, recursive shared-root, or guessed manifest cleanup is
    // allowed.  Retention is safe and recoverable; an unsealed/unknown tree is
    // never made deletion authority.
    provenance::observe(
        SealDomain::Cleanup,
        &format!("cleanup:{transaction}"),
        b"retained-no-exact-live-handles",
    )?;
    Ok(())
}

/// Validates one canonical terminal identity and records cleanup intent plus a
/// retention observation. Rollback snapshots and the journal/receipt remain:
/// exact live-handle and closed-world deletion authority is unavailable.
pub fn cleanup_snapshot_after_terminal(journal_path: &Path) -> Result<()> {
    require_terminal(journal_path)?;
    if journal_path.file_name().and_then(|name| name.to_str()) != Some("journal.json") {
        return Err(PortableRuntimeError::new(
            "portable_cleanup",
            "refused snapshot cleanup for a non-canonical journal",
        ));
    }
    let transaction = transaction_id(journal_path)?;
    provenance::intent(
        SealDomain::Cleanup,
        &format!("cleanup:{transaction}"),
        b"snapshot-retention-review",
    )?;
    provenance::observe(
        SealDomain::Cleanup,
        &format!("cleanup:{transaction}"),
        b"retained-no-exact-live-handles",
    )?;
    Ok(())
}

fn transaction_id(journal_path: &Path) -> Result<&str> {
    journal_path
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .filter(|value| value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .ok_or_else(|| {
            PortableRuntimeError::new(
                "portable_cleanup",
                "cleanup required a canonical transaction nonce",
            )
        })
}

fn require_terminal(journal_path: &Path) -> Result<()> {
    let terminal = read_entries(journal_path)?.last().is_some_and(|entry| {
        matches!(
            entry.phase,
            JournalPhase::RolledBack | JournalPhase::CommitObserved
        )
    });
    if terminal {
        Ok(())
    } else {
        Err(PortableRuntimeError::new(
            "portable_cleanup",
            "transaction was not terminal",
        ))
    }
}
