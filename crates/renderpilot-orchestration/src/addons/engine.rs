//! Tool-agnostic install engine.
//!
//! Applies a serializable [`InstallPlan`] of file operations into a game folder,
//! running ops in list order and rolling back in strict reverse order if any step
//! fails, and reverses an install from the file lists it recorded. Each [`FileOp`]
//! declares its own backup policy: [`FileOp::Create`]/[`FileOp::BackupAndReplace`]/
//! [`FileOp::MergeText`] move a pre-existing file aside to `.bak` first (for an
//! artifact worth manually recovering); [`FileOp::Replace`]/[`FileOp::UpdateText`]
//! never do (for an artifact whose identity is never ambiguous — see their own
//! docs). A crash-safety **sentinel** is written before the first mutation and
//! removed once the folder is in a consistent state (a clean install or a fully
//! reverted rollback); a rollback that cannot complete leaves the sentinel behind
//! so a torn install is detectable on the next scan instead of silently
//! half-applied.
//!
//! The engine is pure over the filesystem (tempdir-testable) and knows nothing
//! tool-specific: a tool layer (RenoDX today, OptiScaler tomorrow) builds the plan —
//! which files to place, which config keys to merge — and maps the returned
//! [`InstallReceipt`] into its own persisted install record.

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use renderpilot_domain::AddonKind;

use crate::ServiceError;

fn invalid(message: impl Into<String>) -> ServiceError {
    ServiceError::InvalidInput(message.into())
}

fn failed(message: impl Into<String>) -> ServiceError {
    ServiceError::CommandFailed(message.into())
}

fn io_error(action: &str, path: &Path, error: &io::Error) -> ServiceError {
    failed(format!("failed to {action} `{}`: {error}", path.display()))
}

/// A named INI section and the `key=value` pairs to set in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IniSection {
    /// Section name without brackets, e.g. `ADDON`.
    pub name: String,
    /// Ordered `key=value` pairs to set.
    pub keys: Vec<(String, String)>,
}

/// A named INI section and the key names to remove from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IniSectionRemoval {
    /// Section name without brackets, e.g. `ADDON`.
    pub name: String,
    /// Key names to remove (matched case-insensitively). An empty list removes the
    /// entire section.
    pub keys: Vec<String>,
}

/// How a [`FileOp::MergeText`] folds new content into an existing (or default)
/// text-config file. An **enum of known strategies** — never a closure — so a plan
/// stays a serializable, inspectable value; a future tool with a different config
/// format adds its own variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeStrategy {
    /// INI: set each `(key, value)` under each section, replacing the key in place
    /// if present and appending it otherwise, while preserving every other section,
    /// key, comment, and blank line verbatim. Multiple sections are set in one
    /// atomic op. Output is CRLF-terminated.
    IniSetKeys {
        /// Sections to set keys in.
        sections: Vec<IniSection>,
    },
    /// INI: remove each named key from its section (case-insensitive), and remove
    /// any section left empty after key removal. An empty `keys` list removes the
    /// entire section. Preserves every other section, key, comment, and blank line
    /// verbatim. Output is CRLF-terminated.
    IniRemoveKeys {
        /// Sections to clean.
        sections: Vec<IniSectionRemoval>,
    },
}

impl MergeStrategy {
    /// Applies the strategy to `base` (the existing file contents, or the op's
    /// default when none), returning the new file contents.
    #[must_use]
    pub fn apply(&self, base: &str) -> String {
        match self {
            MergeStrategy::IniSetKeys { sections } => {
                let mut ini = super::ini::Ini::parse(base);
                for section in sections {
                    for (key, value) in &section.keys {
                        ini.set(&section.name, key, value);
                    }
                }
                ini.render()
            }
            MergeStrategy::IniRemoveKeys { sections } => {
                let mut ini = super::ini::Ini::parse(base);
                for section in sections {
                    if section.keys.is_empty() {
                        ini.remove_section(&section.name);
                    } else {
                        for key in &section.keys {
                            ini.remove_key(&section.name, key);
                        }
                    }
                }
                ini.render()
            }
        }
    }
}

/// A single file mutation in an [`InstallPlan`]. Plain data — never a closure — so a
/// plan is a serializable, testable value the engine can both apply and reason about.
#[derive(Debug, Clone)]
pub enum FileOp {
    /// Write a new payload file. If a file already exists at `name` it is backed up
    /// first (the engine never silently clobbers), but the author's intent is that
    /// none is expected — e.g. the add-on payload or the ownership marker.
    Create {
        /// Bare file name placed in the game folder.
        name: String,
        /// File contents.
        bytes: Vec<u8>,
    },
    /// Install a file *over* one that may already exist — e.g. a proxy DLL shadowing
    /// a game-shipped `dxgi.dll`. Mechanically identical to [`Create`](Self::Create)
    /// (back up any prior file, then write); the distinct name documents that
    /// shadowing a pre-existing file is the expected case.
    BackupAndReplace {
        /// Bare file name placed in the game folder.
        name: String,
        /// File contents.
        bytes: Vec<u8>,
    },
    /// Write a payload file with **no on-disk backup**, for an artifact whose
    /// identity is never ambiguous — a rolling upstream snapshot or an official
    /// redistributable RenderPilot fetched and PE-sanity-checked itself, so there is
    /// nothing about a prior version worth manually recovering. Rolled back, within
    /// the same [`install`] call, by restoring the pre-write bytes it captured in
    /// memory (or deleting the file if none existed) — but once an install commits,
    /// a later [`uninstall`] simply deletes the file; there is no `.bak` to restore
    /// from. Contrast [`BackupAndReplace`](Self::BackupAndReplace), which preserves
    /// whatever was there for the user to recover by hand.
    Replace {
        /// Bare file name placed in the game folder.
        name: String,
        /// File contents.
        bytes: Vec<u8>,
    },
    /// Merge content into a text-config file, resolving an existing file by name
    /// case-insensitively (config casing varies) and using `default` as the base
    /// when none is present. The pre-merge file, if any, is backed up.
    MergeText {
        /// Conventional bare file name (also the name created when none exists).
        name: String,
        /// Base contents used when no file is present.
        default: String,
        /// How to fold the required keys into the base.
        strategy: MergeStrategy,
    },
    /// Update content of a text-config file in-place, without clobbering its `.bak`
    /// file (useful for partial/companion updates that must preserve the main install's
    /// backup). Rolled back by restoring the file to its pre-update bytes.
    UpdateText {
        /// Conventional bare file name.
        name: String,
        /// Base contents used when no file is present.
        default: String,
        /// How to fold the required keys into the base.
        strategy: MergeStrategy,
    },
    /// Delete a file the install previously created. If the file exists, it is
    /// moved to a `.bak` first (so a rollback can restore it); a missing file is a
    /// no-op. The `.bak` is cleaned up on success — it exists only for rollback.
    Remove {
        /// Bare file name to remove from the game folder.
        name: String,
    },
}

/// An ordered, tool-agnostic description of the file mutations an install makes.
#[derive(Debug, Clone)]
pub struct InstallPlan {
    /// The add-on kind, used to namespace the crash-safety sentinel so concurrent
    /// installs of different tools never collide.
    pub kind: AddonKind,
    /// File operations, applied in order (so a `MergeText` always follows the op
    /// that created the file it edits) and rolled back in strict reverse order.
    pub ops: Vec<FileOp>,
}

/// What an install left on disk: every file written and every pre-existing file
/// backed up. The tool layer maps this into its persisted, reversible record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InstallReceipt {
    /// Every file the install wrote (removed on uninstall unless it shadowed a
    /// backed-up original, which is restored instead).
    pub created_files: Vec<PathBuf>,
    /// Every pre-existing file moved aside to a `.bak` before being overwritten
    /// (restored on uninstall).
    pub backed_up_files: Vec<PathBuf>,
}

/// Installs `plan` into `game_dir`, returning the receipt needed to reverse it.
///
/// Ops run in list order; any failure rolls every applied op back in reverse order.
/// A crash-safety sentinel guards the window: it is removed on success or after a
/// clean rollback, and left behind if a rollback step itself fails.
pub fn install(game_dir: &Path, plan: &InstallPlan) -> Result<InstallReceipt, ServiceError> {
    let sentinel = sentinel_path(game_dir, plan.kind);
    write_sentinel(&sentinel)?;

    let mut changes = InstallChanges::default();
    match apply_ops(game_dir, &plan.ops, &mut changes) {
        Ok(()) => {
            changes.sync_touched_dirs();
            changes.cleanup_remove_backups();
            remove_sentinel(&sentinel);
            Ok(changes.into_receipt())
        }
        Err(error) => {
            if changes.undo().is_complete() {
                // The folder is back to its prior state; the torn-install marker is
                // no longer warranted.
                remove_sentinel(&sentinel);
            } else {
                // A rollback step failed (e.g. an AV/anti-cheat `Access Denied`
                // while restoring a backed-up `dxgi.dll`): leave the sentinel so the
                // partial state is detectable rather than silently torn.
                log::warn!(
                    "addon install rollback was incomplete; leaving sentinel `{}` to flag a torn install",
                    sentinel.display()
                );
            }
            Err(error)
        }
    }
}

/// Reverses an install described by its recorded file lists, returning the folder to
/// its prior state. Idempotent and safe to re-run: missing files are ignored.
///
/// Deletes files the install created that did not shadow a pre-existing file, then
/// restores every backed-up original from its `.bak` (which also overwrites a file
/// written on top of it, such as a merged foreign config).
pub fn uninstall(
    created_files: &[PathBuf],
    backed_up_files: &[PathBuf],
) -> Result<(), ServiceError> {
    let backed_up: HashSet<&Path> = backed_up_files.iter().map(PathBuf::as_path).collect();

    for path in created_files {
        if !backed_up.contains(path.as_path()) {
            remove_file_if_exists(path)?;
        }
    }

    let mut touched_dirs: HashSet<PathBuf> = HashSet::new();
    for path in backed_up_files {
        let bak = bak_path(path);
        if !bak.exists() {
            // The original was replaced but its backup is gone (deleted by the user,
            // antivirus, etc.); we cannot restore it. Surface it rather than silently
            // leaving our overwritten content in place.
            log::warn!(
                "addon uninstall: backup `{}` is missing; cannot restore the original file",
                bak.display()
            );
            continue;
        }
        remove_file_if_exists(path)?;
        fs::rename(&bak, path).map_err(|error| io_error("restore backup", path, &error))?;
        insert_parent(&mut touched_dirs, path);
    }

    for path in created_files {
        insert_parent(&mut touched_dirs, path);
    }
    for dir in touched_dirs {
        crate::fs::sync_directory_best_effort(&dir);
    }
    Ok(())
}

/// Atomically replaces an installed file in place with new bytes (for an update),
/// fsyncing its directory. Every other tracked file is left untouched.
pub fn replace_file(path: &Path, bytes: &[u8]) -> Result<(), ServiceError> {
    crate::fs::write_file_atomically(path, bytes)?;
    if let Some(parent) = path.parent() {
        crate::fs::sync_directory_best_effort(parent);
    }
    Ok(())
}

/// Whether a crash-safety sentinel for `kind` is present in `game_dir` — i.e. an
/// install or rollback did not complete cleanly and the folder may be torn.
#[must_use]
pub fn is_install_torn(game_dir: &Path, kind: AddonKind) -> bool {
    sentinel_path(game_dir, kind).exists()
}

fn apply_ops(
    game_dir: &Path,
    ops: &[FileOp],
    changes: &mut InstallChanges,
) -> Result<(), ServiceError> {
    for op in ops {
        apply_op(game_dir, op, changes)?;
    }
    Ok(())
}

fn apply_op(
    game_dir: &Path,
    op: &FileOp,
    changes: &mut InstallChanges,
) -> Result<(), ServiceError> {
    match op {
        FileOp::Create { name, bytes } | FileOp::BackupAndReplace { name, bytes } => {
            let path = safe_join(game_dir, "file name", name)?;
            place_file(&path, bytes, changes)
        }
        FileOp::Replace { name, bytes } => {
            let path = safe_join(game_dir, "file name", name)?;
            replace_file_tracked(&path, bytes, changes)
        }
        FileOp::MergeText {
            name,
            default,
            strategy,
        } => {
            ensure_bare_file_name("merge file name", name)?;
            let path =
                existing_case_insensitive(game_dir, name).unwrap_or_else(|| game_dir.join(name));
            let base = fs::read_to_string(&path).unwrap_or_else(|_| default.clone());
            let merged = strategy.apply(&base);
            place_file(&path, merged.as_bytes(), changes)
        }
        FileOp::UpdateText {
            name,
            default,
            strategy,
        } => {
            ensure_bare_file_name("update file name", name)?;
            let path =
                existing_case_insensitive(game_dir, name).unwrap_or_else(|| game_dir.join(name));

            let (original_bytes, current) = if path.exists() {
                let bytes =
                    fs::read(&path).map_err(|error| io_error("read for update", &path, &error))?;
                let text = String::from_utf8_lossy(&bytes).into_owned();
                (Some(bytes), text)
            } else {
                (None, default.clone())
            };

            let merged = strategy.apply(&current);
            crate::fs::write_file_atomically(&path, merged.as_bytes())?;
            changes.actions.push(Action::Updated {
                path,
                original_bytes,
                whole_file_owned: false,
            });
            Ok(())
        }
        FileOp::Remove { name } => {
            let path = safe_join(game_dir, "file name", name)?;
            remove_file_with_backup(&path, changes)
        }
    }
}

/// Tracks the filesystem actions an install takes, as one ordered log so rollback
/// is the log replayed in reverse.
#[derive(Default)]
struct InstallChanges {
    actions: Vec<Action>,
}

/// One reversible filesystem action.
enum Action {
    /// A file written where none existed (removed on rollback).
    Created(PathBuf),
    /// A pre-existing file moved to `bak`, then overwritten at `path` (rolled back
    /// by removing `path` and renaming `bak` back).
    Replaced { path: PathBuf, bak: PathBuf },
    /// A file moved to `bak`, then the original deleted (rolled back by renaming
    /// `bak` back to `path`). The `bak` is cleaned up on success.
    Removed { path: PathBuf, bak: PathBuf },
    /// A file updated in-place with no on-disk `.bak` (rolled back by writing the
    /// original bytes back, or deleting the file if none existed before). Reported
    /// in the receipt's `created_files` when either `whole_file_owned` is set (this
    /// op replaces the *entire* file — e.g. an addon/host DLL `Replace`, whose
    /// content, whoever wrote it originally, is now this artifact's identity — so
    /// `uninstall()` deletes it outright) or the file didn't exist before (a config
    /// merge that created it from empty is just as much this install's own file as
    /// one written fresh). A merge into a file that already existed is never
    /// reported either way — only specific keys were touched, not the whole file,
    /// so the caller (not a blanket `uninstall()` delete) is responsible for
    /// reversing just those keys.
    Updated {
        path: PathBuf,
        original_bytes: Option<Vec<u8>>,
        whole_file_owned: bool,
    },
}

/// The outcome of a rollback: how many actions could not be reverted.
struct UndoOutcome {
    failures: usize,
}

impl UndoOutcome {
    /// Whether every recorded action was reverted (the folder is consistent again).
    fn is_complete(&self) -> bool {
        self.failures == 0
    }
}

impl InstallChanges {
    /// Best-effort reversal of every action so far, in strict reverse order.
    /// Failures are logged (this already runs on an error path) and counted so the
    /// caller can decide whether the install is cleanly reverted or torn.
    fn undo(&self) -> UndoOutcome {
        let mut failures = 0;
        for action in self.actions.iter().rev() {
            match action {
                Action::Created(path) => {
                    if let Err(error) = remove_existing(path) {
                        log::warn!(
                            "addon rollback: failed to remove `{}`: {error}",
                            path.display()
                        );
                        failures += 1;
                    }
                }
                Action::Replaced { path, bak } => {
                    let _ = remove_existing(path);
                    if let Err(error) = fs::rename(bak, path) {
                        log::warn!(
                            "addon rollback: failed to restore `{}` from backup: {error}",
                            path.display()
                        );
                        failures += 1;
                    }
                }
                Action::Removed { path, bak } => {
                    let _ = remove_existing(path);
                    if let Err(error) = fs::rename(bak, path) {
                        log::warn!(
                            "addon rollback: failed to restore removed file `{}`: {error}",
                            path.display()
                        );
                        failures += 1;
                    }
                }
                Action::Updated {
                    path,
                    original_bytes,
                    ..
                } => match original_bytes {
                    Some(bytes) => {
                        if let Err(error) = crate::fs::write_file_atomically(path, bytes) {
                            log::warn!(
                                "addon rollback: failed to restore updated file `{}`: {error}",
                                path.display()
                            );
                            failures += 1;
                        }
                    }
                    None => {
                        if let Err(error) = remove_existing(path) {
                            log::warn!(
                                "addon rollback: failed to remove newly updated file `{}`: {error}",
                                path.display()
                            );
                            failures += 1;
                        }
                    }
                },
            }
        }
        UndoOutcome { failures }
    }

    /// Fsyncs each distinct parent directory the install touched.
    fn sync_touched_dirs(&self) {
        let mut synced: HashSet<PathBuf> = HashSet::new();
        for action in &self.actions {
            let path = match action {
                Action::Created(path)
                | Action::Replaced { path, .. }
                | Action::Removed { path, .. }
                | Action::Updated { path, .. } => path,
            };
            if let Some(parent) = path.parent()
                && synced.insert(parent.to_path_buf())
            {
                crate::fs::sync_directory_best_effort(parent);
            }
        }
    }

    /// Deletes `.bak` files left by `Remove` ops — they exist only for rollback and
    /// are not needed after a successful plan.
    fn cleanup_remove_backups(&self) {
        for action in &self.actions {
            if let Action::Removed { bak, .. } = action
                && let Err(error) = remove_existing(bak)
            {
                log::warn!(
                    "addon install: failed to clean up remove backup `{}`: {error}",
                    bak.display()
                );
            }
        }
    }

    /// Maps the action log into the public receipt.
    fn into_receipt(self) -> InstallReceipt {
        let mut created_files = Vec::with_capacity(self.actions.len());
        let mut backed_up_files = Vec::new();
        for action in self.actions {
            match action {
                Action::Created(path) => created_files.push(path),
                Action::Replaced { path, .. } => {
                    backed_up_files.push(path.clone());
                    created_files.push(path);
                }
                Action::Updated {
                    path,
                    original_bytes,
                    whole_file_owned,
                } => {
                    // No `.bak` regardless — this path never enters `backed_up_files`.
                    // It's `created_files` (so a later `uninstall()` deletes it
                    // outright) when this op owns the whole file, or when there was
                    // nothing here before it ran; a merge into a pre-existing file
                    // is left for the caller to reverse key-by-key instead.
                    if whole_file_owned || original_bytes.is_none() {
                        created_files.push(path);
                    }
                }
                Action::Removed { .. } => {
                    // Removed files don't appear in the receipt; the caller knows
                    // which file it asked to remove and updates its record directly.
                }
            }
        }
        InstallReceipt {
            created_files,
            backed_up_files,
        }
    }
}

/// Writes `bytes` to `path`, first moving any pre-existing regular file aside to
/// `.bak`, recording the action for rollback.
fn place_file(path: &Path, bytes: &[u8], changes: &mut InstallChanges) -> Result<(), ServiceError> {
    if path.exists() {
        if !path.is_file() {
            return Err(invalid(format!(
                "cannot back up `{}`: not a regular file",
                path.display()
            )));
        }
        let bak = bak_path(path);
        if bak.exists() {
            fs::remove_file(&bak).map_err(|error| io_error("clear stale backup", &bak, &error))?;
        }
        fs::rename(path, &bak).map_err(|error| io_error("back up", path, &error))?;
        crate::fs::write_file_atomically(path, bytes)?;
        changes.actions.push(Action::Replaced {
            path: path.to_path_buf(),
            bak,
        });
    } else {
        crate::fs::write_file_atomically(path, bytes)?;
        changes.actions.push(Action::Created(path.to_path_buf()));
    }
    Ok(())
}

/// Writes `bytes` to `path` with no on-disk backup, capturing any pre-write bytes
/// in memory so a same-call rollback ([`InstallChanges::undo`]) can restore them.
/// Unlike [`place_file`], a pre-existing file is never moved aside to `.bak` — for
/// [`FileOp::Replace`], the caller has already decided the artifact's identity is
/// unambiguous enough that nothing here is worth preserving for manual recovery.
fn replace_file_tracked(
    path: &Path,
    bytes: &[u8],
    changes: &mut InstallChanges,
) -> Result<(), ServiceError> {
    let original_bytes = if path.exists() {
        if !path.is_file() {
            return Err(invalid(format!(
                "cannot replace `{}`: not a regular file",
                path.display()
            )));
        }
        Some(fs::read(path).map_err(|error| io_error("read before replace", path, &error))?)
    } else {
        None
    };
    crate::fs::write_file_atomically(path, bytes)?;
    changes.actions.push(Action::Updated {
        path: path.to_path_buf(),
        original_bytes,
        whole_file_owned: true,
    });
    Ok(())
}

/// Moves `path` to a `.bak` and then deletes the original, recording the action
/// for rollback. A missing file is a no-op. The `.bak` is cleaned up on success by
/// [`InstallChanges::cleanup_remove_backups`]; it exists only so a failed plan can
/// restore the removed file.
fn remove_file_with_backup(path: &Path, changes: &mut InstallChanges) -> Result<(), ServiceError> {
    if !path.exists() {
        return Ok(());
    }
    if !path.is_file() {
        return Err(invalid(format!(
            "cannot remove `{}`: not a regular file",
            path.display()
        )));
    }
    let bak = bak_path(path);
    if bak.exists() {
        fs::remove_file(&bak).map_err(|error| io_error("clear stale backup", &bak, &error))?;
    }
    fs::rename(path, &bak).map_err(|error| io_error("back up before remove", path, &error))?;
    changes.actions.push(Action::Removed {
        path: path.to_path_buf(),
        bak,
    });
    Ok(())
}

/// Returns `path` with a `.bak` suffix appended (preserving its extension, e.g.
/// `dxgi.dll` → `dxgi.dll.bak`).
fn bak_path(path: &Path) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(".bak");
    PathBuf::from(name)
}

fn remove_file_if_exists(path: &Path) -> Result<(), ServiceError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error("remove", path, &error)),
    }
}

/// Like [`remove_file_if_exists`] but returns the raw `io` error, for the rollback
/// path that counts failures rather than short-circuiting.
fn remove_existing(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn insert_parent(dirs: &mut HashSet<PathBuf>, path: &Path) {
    if let Some(parent) = path.parent() {
        dirs.insert(parent.to_path_buf());
    }
}

/// Returns the path of an existing file in `game_dir` whose name equals `name`
/// case-insensitively, if any — so a foreign config saved under a different casing
/// is found and merged rather than duplicated.
fn existing_case_insensitive(game_dir: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(game_dir).ok()?;
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_file())
            && entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(name)
        {
            return Some(entry.path());
        }
    }
    None
}

/// Validates a bare, safe file name and joins it onto `game_dir`.
fn safe_join(game_dir: &Path, field: &str, name: &str) -> Result<PathBuf, ServiceError> {
    ensure_bare_file_name(field, name)?;
    Ok(game_dir.join(name))
}

/// Defense in depth: refuse a path component that is not a bare file name before it
/// is joined onto the game folder, so a (future) unvalidated source can never escape
/// it. Tool manifests already validate this; the engine guards independently.
fn ensure_bare_file_name(field: &str, name: &str) -> Result<(), ServiceError> {
    if !crate::fs::is_safe_file_name(name) {
        return Err(invalid(format!(
            "unsafe {field} `{name}`: must be a bare file name"
        )));
    }
    Ok(())
}

fn sentinel_path(game_dir: &Path, kind: AddonKind) -> PathBuf {
    game_dir.join(format!(
        "renderpilot-{}-install.lock",
        kind.as_str().to_ascii_lowercase()
    ))
}

fn write_sentinel(path: &Path) -> Result<(), ServiceError> {
    crate::fs::write_file_atomically(path, b"")
}

fn remove_sentinel(path: &Path) {
    if let Err(error) = remove_existing(path) {
        log::warn!(
            "addon install: failed to remove sentinel `{}`: {error}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn renodx_like_plan() -> InstallPlan {
        InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![
                FileOp::Create {
                    name: "renodx-cp2077.addon64".to_owned(),
                    bytes: b"addon-bytes".to_vec(),
                },
                FileOp::BackupAndReplace {
                    name: "dxgi.dll".to_owned(),
                    bytes: b"reshade-dll".to_vec(),
                },
                FileOp::MergeText {
                    name: "ReShade.ini".to_owned(),
                    default: String::new(),
                    strategy: MergeStrategy::IniSetKeys {
                        sections: vec![IniSection {
                            name: "ADDON".to_owned(),
                            keys: vec![
                                (
                                    "DisabledAddons".to_owned(),
                                    "Generic Depth,Effect Runtime Sync".to_owned(),
                                ),
                                ("AddonPath".to_owned(), ".".to_owned()),
                            ],
                        }],
                    },
                },
                FileOp::Create {
                    name: "renderpilot-renodx.json".to_owned(),
                    bytes: b"{}".to_vec(),
                },
            ],
        }
    }

    fn read(path: &Path) -> Vec<u8> {
        fs::read(path).expect("file should exist")
    }

    fn receipt_paths(paths: &[PathBuf]) -> Vec<String> {
        paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn lays_down_every_op_and_round_trips_to_clean_folder() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        fs::write(game.join("game.exe"), b"game").expect("write");

        let receipt = install(game, &renodx_like_plan()).expect("install");

        assert_eq!(read(&game.join("renodx-cp2077.addon64")), b"addon-bytes");
        assert_eq!(read(&game.join("dxgi.dll")), b"reshade-dll");
        let ini = String::from_utf8(read(&game.join("ReShade.ini"))).unwrap();
        assert_eq!(
            ini,
            "[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\nAddonPath=.\r\n"
        );
        assert!(game.join("renderpilot-renodx.json").is_file());
        // addon + proxy + ini + marker, no backups (clean folder).
        assert_eq!(receipt.created_files.len(), 4);
        assert!(receipt.backed_up_files.is_empty());
        // The sentinel is gone after a clean install.
        assert!(!is_install_torn(game, AddonKind::RenoDx));

        uninstall(&receipt.created_files, &receipt.backed_up_files).expect("uninstall");
        assert!(!game.join("renodx-cp2077.addon64").exists());
        assert!(!game.join("dxgi.dll").exists());
        assert!(!game.join("ReShade.ini").exists());
        assert!(!game.join("renderpilot-renodx.json").exists());
        assert_eq!(read(&game.join("game.exe")), b"game");
    }

    #[test]
    fn backs_up_and_restores_a_preexisting_file() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        fs::write(game.join("dxgi.dll"), b"game-shipped").expect("write");

        let receipt = install(game, &renodx_like_plan()).expect("install");
        assert_eq!(read(&game.join("dxgi.dll")), b"reshade-dll");
        assert_eq!(receipt_paths(&receipt.backed_up_files), vec!["dxgi.dll"]);

        uninstall(&receipt.created_files, &receipt.backed_up_files).expect("uninstall");
        assert_eq!(read(&game.join("dxgi.dll")), b"game-shipped");
    }

    #[test]
    fn merge_text_resolves_a_foreign_config_case_insensitively() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        // A foreign config under a different casing must be found, merged, and backed
        // up — not duplicated under the conventional name.
        fs::write(game.join("reshade.ini"), "[GENERAL]\r\nPreset=mine.ini\r\n").expect("write");

        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![FileOp::MergeText {
                name: "ReShade.ini".to_owned(),
                default: String::new(),
                strategy: MergeStrategy::IniSetKeys {
                    sections: vec![IniSection {
                        name: "ADDON".to_owned(),
                        keys: vec![("AddonPath".to_owned(), ".".to_owned())],
                    }],
                },
            }],
        };
        let receipt = install(game, &plan).expect("install");

        // The merge targeted the existing lower-cased file (its `.bak` proves it),
        // not a fresh `ReShade.ini`, and preserved the foreign keys.
        let merged = String::from_utf8(read(&game.join("reshade.ini"))).unwrap();
        assert!(merged.contains("Preset=mine.ini"));
        assert!(merged.contains("AddonPath=."));
        assert_eq!(receipt_paths(&receipt.backed_up_files), vec!["reshade.ini"]);
        assert!(game.join("reshade.ini.bak").exists());
    }

    #[test]
    fn replace_over_a_missing_file_creates_it_with_no_backup() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();

        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![FileOp::Replace {
                name: "renodx-cp2077.addon64".to_owned(),
                bytes: b"addon-v1".to_vec(),
            }],
        };
        let receipt = install(game, &plan).expect("install");

        assert_eq!(read(&game.join("renodx-cp2077.addon64")), b"addon-v1");
        assert_eq!(
            receipt_paths(&receipt.created_files),
            vec!["renodx-cp2077.addon64"]
        );
        assert!(receipt.backed_up_files.is_empty());
        assert!(!game.join("renodx-cp2077.addon64.bak").exists());
    }

    #[test]
    fn replace_over_an_existing_file_overwrites_it_with_no_backup() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        fs::write(game.join("dxgi.dll"), b"old-reshade").expect("write");

        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![FileOp::Replace {
                name: "dxgi.dll".to_owned(),
                bytes: b"new-reshade".to_vec(),
            }],
        };
        let receipt = install(game, &plan).expect("install");

        assert_eq!(read(&game.join("dxgi.dll")), b"new-reshade");
        // The overwritten file counts as created (an uninstall deletes it outright),
        // never as backed-up (there is no `.bak` to restore from).
        assert_eq!(receipt_paths(&receipt.created_files), vec!["dxgi.dll"]);
        assert!(receipt.backed_up_files.is_empty());
        assert!(!game.join("dxgi.dll.bak").exists());
    }

    #[test]
    fn replace_rolls_back_to_pre_write_bytes_when_a_later_op_fails() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        fs::write(game.join("dxgi.dll"), b"old-reshade").expect("write");

        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![
                FileOp::Replace {
                    name: "dxgi.dll".to_owned(),
                    bytes: b"new-reshade".to_vec(),
                },
                FileOp::Create {
                    name: "../escape.dll".to_owned(),
                    bytes: b"evil".to_vec(),
                },
            ],
        };
        install(game, &plan).expect_err("unsafe op should fail");

        // Rolled back to the pre-write bytes, in place — no `.bak` was ever involved.
        assert_eq!(read(&game.join("dxgi.dll")), b"old-reshade");
        assert!(!game.join("dxgi.dll.bak").exists());
        assert!(!is_install_torn(game, AddonKind::RenoDx));
    }

    #[test]
    fn replace_rolls_back_to_absent_when_it_created_the_file() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();

        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![
                FileOp::Replace {
                    name: "renodx-cp2077.addon64".to_owned(),
                    bytes: b"addon".to_vec(),
                },
                FileOp::Create {
                    name: "../escape.dll".to_owned(),
                    bytes: b"evil".to_vec(),
                },
            ],
        };
        install(game, &plan).expect_err("unsafe op should fail");

        assert!(!game.join("renodx-cp2077.addon64").exists());
        assert!(!game.join("renodx-cp2077.addon64.bak").exists());
    }

    #[test]
    fn uninstall_deletes_a_replaced_file_outright() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        fs::write(game.join("dxgi.dll"), b"old-reshade").expect("write");

        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![FileOp::Replace {
                name: "dxgi.dll".to_owned(),
                bytes: b"new-reshade".to_vec(),
            }],
        };
        let receipt = install(game, &plan).expect("install");

        // Unlike `BackupAndReplace`, uninstalling a `Replace`'d file does not bring
        // the old game-shipped bytes back — there was never a `.bak` to restore.
        uninstall(&receipt.created_files, &receipt.backed_up_files).expect("uninstall");
        assert!(!game.join("dxgi.dll").exists());
    }

    #[test]
    fn ops_run_in_order_so_a_later_merge_sees_an_earlier_one() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();

        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![
                FileOp::MergeText {
                    name: "cfg.ini".to_owned(),
                    default: String::new(),
                    strategy: MergeStrategy::IniSetKeys {
                        sections: vec![IniSection {
                            name: "S".to_owned(),
                            keys: vec![("a".to_owned(), "1".to_owned())],
                        }],
                    },
                },
                FileOp::MergeText {
                    name: "cfg.ini".to_owned(),
                    default: String::new(),
                    strategy: MergeStrategy::IniSetKeys {
                        sections: vec![IniSection {
                            name: "S".to_owned(),
                            keys: vec![("b".to_owned(), "2".to_owned())],
                        }],
                    },
                },
            ],
        };
        install(game, &plan).expect("install");

        let cfg = String::from_utf8(read(&game.join("cfg.ini"))).unwrap();
        assert_eq!(cfg, "[S]\r\na=1\r\nb=2\r\n");
    }

    #[test]
    fn a_failed_op_rolls_back_every_prior_op_in_reverse() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        fs::write(game.join("dxgi.dll"), b"game-shipped").expect("write");

        // The third op has an unsafe name and is rejected; the first two (the addon
        // and the backup-and-replace of dxgi.dll) must be fully reverted.
        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![
                FileOp::Create {
                    name: "renodx.addon64".to_owned(),
                    bytes: b"addon".to_vec(),
                },
                FileOp::BackupAndReplace {
                    name: "dxgi.dll".to_owned(),
                    bytes: b"reshade-dll".to_vec(),
                },
                FileOp::Create {
                    name: "../escape.dll".to_owned(),
                    bytes: b"evil".to_vec(),
                },
            ],
        };
        let error = install(game, &plan).expect_err("unsafe op should fail");
        assert!(matches!(error, ServiceError::InvalidInput(_)));

        // Folder restored: addon removed, original dxgi.dll back, no leftovers.
        assert!(!game.join("renodx.addon64").exists());
        assert_eq!(read(&game.join("dxgi.dll")), b"game-shipped");
        assert!(!game.join("dxgi.dll.bak").exists());
        // A clean rollback clears the sentinel.
        assert!(!is_install_torn(game, AddonKind::RenoDx));
    }

    #[test]
    fn ini_set_keys_creates_section_then_replaces_in_place() {
        let create = MergeStrategy::IniSetKeys {
            sections: vec![IniSection {
                name: "ADDON".to_owned(),
                keys: vec![
                    (
                        "DisabledAddons".to_owned(),
                        "Generic Depth,Effect Runtime Sync".to_owned(),
                    ),
                    ("AddonPath".to_owned(), ".".to_owned()),
                ],
            }],
        };
        assert_eq!(
            create.apply(""),
            "[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\nAddonPath=.\r\n"
        );

        // Replaces an existing key in place (no duplication) and preserves foreign
        // sections, comments, and blank lines.
        let existing =
            "; mine\r\n[GENERAL]\r\nPreset=foo.ini\r\n\r\n[ADDON]\r\nDisabledAddons=Old\r\n";
        let merged = create.apply(existing);
        assert!(merged.contains("; mine"));
        assert!(merged.contains("[GENERAL]\r\nPreset=foo.ini"));
        assert_eq!(merged.matches("DisabledAddons=").count(), 1);
        assert!(merged.contains("DisabledAddons=Generic Depth,Effect Runtime Sync"));
        assert!(merged.contains("AddonPath=."));
    }

    #[test]
    fn undo_outcome_drives_sentinel_retention() {
        assert!(UndoOutcome { failures: 0 }.is_complete());
        assert!(!UndoOutcome { failures: 1 }.is_complete());
    }

    #[test]
    fn update_text_creates_a_missing_file_from_default() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![FileOp::UpdateText {
                name: "ReShade.ini".to_owned(),
                default: String::new(),
                strategy: MergeStrategy::IniSetKeys {
                    sections: vec![IniSection {
                        name: "ADDON".to_owned(),
                        keys: vec![("AddonPath".to_owned(), ".".to_owned())],
                    }],
                },
            }],
        };
        let receipt = install(game, &plan).expect("install");
        let ini = String::from_utf8(read(&game.join("ReShade.ini"))).unwrap();
        assert_eq!(ini, "[ADDON]\r\nAddonPath=.\r\n");
        // UpdateText created this file from empty, so it's just as much this
        // install's own file as one written fresh — it's in created_files (for
        // uninstall to find), never backed up (there was nothing to back up).
        assert_eq!(receipt_paths(&receipt.created_files), vec!["ReShade.ini"]);
        assert!(receipt.backed_up_files.is_empty());
        assert!(!game.join("ReShade.ini.bak").exists());
    }

    #[test]
    fn update_text_rolls_back_to_absent_when_it_created_the_file() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![
                FileOp::UpdateText {
                    name: "ReShade.ini".to_owned(),
                    default: String::new(),
                    strategy: MergeStrategy::IniSetKeys {
                        sections: vec![IniSection {
                            name: "ADDON".to_owned(),
                            keys: vec![("AddonPath".to_owned(), ".".to_owned())],
                        }],
                    },
                },
                FileOp::Create {
                    name: "../escape.dll".to_owned(),
                    bytes: b"evil".to_vec(),
                },
            ],
        };
        install(game, &plan).expect_err("unsafe op should fail");
        // The file was absent before the plan; rollback of the UpdateText that
        // created it removes it again (no stub left behind, no `.bak`).
        assert!(!game.join("ReShade.ini").exists());
        assert!(!game.join("ReShade.ini.bak").exists());
    }

    #[test]
    fn update_text_preserves_the_primary_bak_and_rolls_back_to_pre_update_bytes() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        // A pre-existing foreign config: a MergeText install backs it up and rewrites it.
        fs::write(game.join("ReShade.ini"), "[GENERAL]\r\nPreset=mine.ini\r\n").expect("write");

        let first = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![FileOp::MergeText {
                name: "ReShade.ini".to_owned(),
                default: String::new(),
                strategy: MergeStrategy::IniSetKeys {
                    sections: vec![IniSection {
                        name: "ADDON".to_owned(),
                        keys: vec![("AddonPath".to_owned(), ".".to_owned())],
                    }],
                },
            }],
        };
        let first_receipt = install(game, &first).expect("first install");
        assert!(game.join("ReShade.ini.bak").exists());
        assert_eq!(first_receipt.backed_up_files.len(), 1);

        // A companion UpdateText adds a section without touching the existing `.bak`.
        let second = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![FileOp::UpdateText {
                name: "ReShade.ini".to_owned(),
                default: String::new(),
                strategy: MergeStrategy::IniSetKeys {
                    sections: vec![IniSection {
                        name: "RENODX-DLSSFIX".to_owned(),
                        keys: vec![("DLSSPath".to_owned(), "C:\\dlss.dll".to_owned())],
                    }],
                },
            }],
        };
        let second_receipt = install(game, &second).expect("companion update");
        let ini = String::from_utf8(read(&game.join("ReShade.ini"))).unwrap();
        assert!(ini.contains("Preset=mine.ini"));
        assert!(ini.contains("AddonPath=."));
        assert!(ini.contains("[RENODX-DLSSFIX]"));
        assert!(ini.contains("DLSSPath=C:\\dlss.dll"));
        assert!(second_receipt.created_files.is_empty());
        assert!(second_receipt.backed_up_files.is_empty());
        // The primary install's `.bak` survives the companion update.
        assert!(game.join("ReShade.ini.bak").exists());

        // A failed follow-up op rolls the companion update back to its pre-update
        // bytes: `[RENODX-DLSSFIX]` stays (it was there before this plan), `Extra=`
        // (added by this plan) is gone, and the primary `.bak` is still intact.
        let third = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![
                FileOp::UpdateText {
                    name: "ReShade.ini".to_owned(),
                    default: String::new(),
                    strategy: MergeStrategy::IniSetKeys {
                        sections: vec![IniSection {
                            name: "ADDON".to_owned(),
                            keys: vec![("Extra".to_owned(), "1".to_owned())],
                        }],
                    },
                },
                FileOp::Create {
                    name: "../escape.dll".to_owned(),
                    bytes: b"evil".to_vec(),
                },
            ],
        };
        install(game, &third).expect_err("unsafe op should fail");
        let ini = String::from_utf8(read(&game.join("ReShade.ini"))).unwrap();
        assert!(ini.contains("AddonPath=."));
        assert!(ini.contains("[RENODX-DLSSFIX]"));
        assert!(!ini.contains("Extra="));
        assert!(game.join("ReShade.ini.bak").exists());
    }

    #[test]
    fn remove_deletes_a_file_and_cleans_up_its_backup_on_success() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        fs::write(game.join("renodx-dlssfix.addon64"), b"fix-bytes").expect("write");

        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![FileOp::Remove {
                name: "renodx-dlssfix.addon64".to_owned(),
            }],
        };
        let receipt = install(game, &plan).expect("install");
        assert!(!game.join("renodx-dlssfix.addon64").exists());
        // The `.bak` existed for rollback but is cleaned up on success.
        assert!(!game.join("renodx-dlssfix.addon64.bak").exists());
        assert!(receipt.created_files.is_empty());
        assert!(receipt.backed_up_files.is_empty());
    }

    #[test]
    fn remove_is_a_noop_for_a_missing_file() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![FileOp::Remove {
                name: "absent.addon64".to_owned(),
            }],
        };
        install(game, &plan).expect("missing file is a no-op");
        assert!(!game.join("absent.addon64").exists());
        assert!(!game.join("absent.addon64.bak").exists());
    }

    #[test]
    fn remove_rolls_back_restoring_the_deleted_file() {
        let dir = tempdir().expect("tempdir");
        let game = dir.path();
        fs::write(game.join("renodx-dlssfix.addon64"), b"fix-bytes").expect("write");

        // The Remove succeeds, then an unsafe op fails; rollback must restore the file.
        let plan = InstallPlan {
            kind: AddonKind::RenoDx,
            ops: vec![
                FileOp::Remove {
                    name: "renodx-dlssfix.addon64".to_owned(),
                },
                FileOp::Create {
                    name: "../escape.dll".to_owned(),
                    bytes: b"evil".to_vec(),
                },
            ],
        };
        install(game, &plan).expect_err("unsafe op should fail");
        assert_eq!(read(&game.join("renodx-dlssfix.addon64")), b"fix-bytes");
        assert!(!game.join("renodx-dlssfix.addon64.bak").exists());
    }

    #[test]
    fn ini_remove_keys_strips_named_keys_and_whole_sections() {
        let base = "[ADDON]\r\nAddonPath=.\r\nLoadFromDllMain=x.addon64\r\n\
                    [RENODX-DLSSFIX]\r\nDLSSPath=C:\\d.dll\r\nStreamlinePath=C:\\s.dll\r\n";
        let strategy = MergeStrategy::IniRemoveKeys {
            sections: vec![
                IniSectionRemoval {
                    name: "ADDON".to_owned(),
                    keys: vec!["LoadFromDllMain".to_owned()],
                },
                IniSectionRemoval {
                    name: "RENODX-DLSSFIX".to_owned(),
                    keys: Vec::new(),
                },
            ],
        };
        let merged = strategy.apply(base);
        assert_eq!(merged, "[ADDON]\r\nAddonPath=.\r\n");
    }
}
