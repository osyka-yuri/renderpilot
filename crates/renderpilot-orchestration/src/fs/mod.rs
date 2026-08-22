//! Crash-safe filesystem primitives shared across orchestration features.
//!
//! Provider- and domain-neutral: the library swapper and the add-on installers
//! both build on these. Nothing here knows about a concrete add-on kind.
//!
//! ## Modules
//!
//! - [`durability`] -- directory-entry fsync (batched, best-effort)
//! - [`atomic`] -- content-durable write/copy and platform no-replace rename
//! - [`cache`] -- cache transactions and bounded corrupt-file quarantine
//! - [`io`] -- read, remove, BOM strip
//! - [`mtime`] -- HTTP-date parse/format and best-effort mtime stamping
//! - [`hash`] -- readable-file probes and non-empty regular-file SHA-256
//! - [`sidecar`] -- classic `.bak` path naming and verify/create/restore
//!
//! Call sites use the flat `crate::fs::*` surface re-exported below. Internal
//! layout documents dependency rules only.
//!
//! ## Dependency rules
//!
//! ```text
//! durability, io, mtime, hash  ->  (std / domain only)
//! atomic                       ->  (std + target filesystem API)
//! cache                        ->  atomic + io + sidecar::naming
//! sidecar::naming              ->  (std only)
//! sidecar::ops                 ->  atomic + hash + naming
//! ```
//!
//! Pure bare-name safety lives in [`crate::paths`] (no I/O).
//! Multi-file protocols live in `file_mutation` / `coordinated_files`.
//!
//! Writing a file with `std::fs::write` only schedules the data for the OS page
//! cache; a crash before the cache is flushed can leave a torn file.
//! [`write_file_atomically`] makes a write **content-durable** via a temp file,
//! `sync_all`, and an atomic rename. Directory-entry durability is a separate,
//! explicit step ([`sync_directory_best_effort`]) that callers invoke once over
//! the dirs they touched.

#[cfg(all(
    not(any(windows, target_os = "linux")),
    not(feature = "development-host-fallback")
))]
compile_error!(
    "renderpilot-orchestration filesystem publication supports Windows and Linux; enable `development-host-fallback` only for an unsupported development host"
);

mod atomic;
mod cache;
mod durability;
mod hash;
mod io;
mod mtime;
mod sidecar;

pub(crate) use atomic::{
    copy_file_atomically, move_file_no_replace, publish_staged_replace, write_file_atomically,
};
pub(crate) use cache::{
    CacheGeneration, CacheObservation, CachePublication, MatchingCurrentPolicy,
    commit_cache_candidate, observe_cache_file,
};
pub(crate) use durability::{
    sync_directory, sync_directory_best_effort, sync_parent_directory_best_effort,
};
pub(crate) use hash::{
    NonEmptyFileError, is_readable_file, is_readable_non_empty_file, sha256_of_non_empty_file,
};
pub(crate) use io::{read_file, remove_file_if_exists, strip_utf8_bom};
pub(crate) use mtime::{format_http_date, is_reasonable_file_mtime, stamp_mtime_best_effort};
pub(crate) use sidecar::naming::{
    backup_path, expand_with_sidecars, original_path_from_backup, with_added_extension,
};
pub(crate) use sidecar::ops::{
    SidecarVerifyError, create_sidecar, restore_from_sidecar, verify_sidecar,
};

#[cfg(test)]
mod platform_contract_tests {
    #[test]
    fn production_hosts_select_a_native_publication_backend() {
        assert!(matches!(std::env::consts::OS, "windows" | "linux"));
    }

    #[cfg(all(
        not(any(windows, target_os = "linux")),
        feature = "development-host-fallback"
    ))]
    #[test]
    fn unsupported_development_hosts_require_the_explicit_fallback_feature() {
        assert!(cfg!(feature = "development-host-fallback"));
    }
}
