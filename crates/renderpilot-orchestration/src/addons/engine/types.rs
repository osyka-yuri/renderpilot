//! Core serializable types for the engine: Ini sections, MergeStrategy, FileOp, InstallPlan, InstallReceipt.

use std::path::PathBuf;

use renderpilot_domain::AddonKind;

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
                let mut ini = crate::addons::ini::Ini::parse(base);
                for section in sections {
                    for (key, value) in &section.keys {
                        ini.set(&section.name, key, value);
                    }
                }
                ini.render()
            }
            MergeStrategy::IniRemoveKeys { sections } => {
                let mut ini = crate::addons::ini::Ini::parse(base);
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
    /// the same `install` call, by restoring the pre-write bytes it captured in
    /// memory (or deleting the file if none existed) — but once an install commits,
    /// a later `uninstall` simply deletes the file; there is no `.bak` to restore
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
    /// Write a new payload file at a validated multi-component path relative to
    /// the game folder (e.g. `Luma/Global/Copy_PS.hlsl`), creating any missing
    /// parent directories. Backup policy is identical to [`Create`](Self::Create):
    /// a pre-existing file at that path is backed up first. Missing parent
    /// directories are created one level at a time and each recorded for rollback
    /// (removed, deepest first, if they end up empty); a directory that already
    /// existed before the install is never a rollback candidate. Directories are
    /// not tracked in [`InstallReceipt`] — a later `uninstall_tree` re-derives
    /// cleanup candidates from the recorded files' parent chains instead.
    CreateNested {
        /// Relative path under the game folder, using `/` or `\` as the
        /// separator; every component is validated the same way a bare
        /// [`Create`](Self::Create) file name is.
        relative_path: String,
        /// File contents.
        bytes: Vec<u8>,
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

/// Options controlling per-call engine behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstallOptions {
    /// When `true` (default), the engine writes and clears its own crash-safety
    /// sentinel in `game_dir`. When `false`, an outer orchestrator owns the sentinel.
    pub manage_sentinel: bool,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            manage_sentinel: true,
        }
    }
}
