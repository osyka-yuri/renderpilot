# Changelog

All notable changes to this project will be documented in this file.

## [1.1.0] - 2026-06-12

### Added
- **Libraries**: Parallel library downloads with type-safe, throttled download progress.
  - Downloads of different libraries run concurrently, each with a live progress bar on the control that started it (`LibraryActionsCell`, `ComponentVersionRow`, `StreamlineComponentCard`).
  - Prevents IPC flooding to the frontend by throttling Tauri event emissions to 100ms.
  - Mitigates OOM vulnerabilities with strict archive size validation (`MAX_ARCHIVE_SIZE` of 500 MiB) and accurate `Vec` preallocation.
- **Libraries**: Switched downloads to zstd archives hosted on Cloudflare R2 for faster, more reliable distribution.
- **UI**: Added a new deterministic `Progress` component.

### Fixed
- **Updater**: Granted the `dialog:allow-ask` capability so the update confirmation prompt can be shown. In 1.0.0 the update check failed with an error as soon as an update was available, so automatic updates never worked — 1.0.0 users must install this release manually.
- **Operation Journal**: Prevented panics by providing a fallback timestamp during operation execution.
- **Manifest**: Enabled `reqwest` gzip decompression for manifest fetch to handle compressed server responses gracefully.
- **UI**: Clipped document overflow so that native Windows scrollbars cannot appear over the application styling.
- **Components**: Stabilized the visual replacement candidate order (version-descending).
- **FSR**: Correctly respects entry-point lineage in mixed FSR directories to avoid file clashes.

### Changed (Refactoring & Maintenance)
- **Error Handling**: Improved error propagation boundaries and significantly reduced allocations during processing.
- **Tooling**: Enforced Rust ownership and memory lints workspace-wide for better long-term code quality.
- **Docs**: Updated `README.md` to perfectly align with current architecture, features, and technology stack. Fixed all `rustdoc` warnings.
