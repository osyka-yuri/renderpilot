# Changelog

All notable changes to this project will be documented in this file.

## [1.4.0] - 2026-07-03

### Added
- **Game Details → RenoDX**: Vulkan games can now get RenoDX HDR too, through a single, system-wide ReShade Vulkan layer shared across every Vulkan title — installed once, with your explicit consent, rather than per game. A layer you already have installed yourself (or from another tool) is detected and left untouched; RenderPilot only manages the one it installs.
- **Game Details → RenoDX**: Removing the add-on or applying an update now asks for confirmation first, so a misclick can't silently undo an install.
- **Settings → RenoDX**: A new section for the shared Vulkan layer — see its status, switch it between stable and nightly, or install/remove it independently of any single game.

### Changed
- **Game Details → RenoDX**: Clearer wording for the compatibility badge ("Works" / "In progress" / "Unverified") and the freshness indicator, and a redesigned channel switch control.

### Changed (Refactoring & Maintenance)
- **RenoDX engine**: Replaced the single install/update service with one focused module per operation (install, uninstall, channel switch, update, availability, status), built on a shared anti-cheat detection database and split host/compatibility/install-risk policy rules.
- **Storage**: Added a persistence layer for the shared Vulkan layer's install/detection state, so it survives restarts the same way per-game installs already do.

## [1.3.0] - 2026-06-28

### Added
- **Game Details → RenoDX**: One-click HDR for supported games through the RenoDX ReShade add-on. RenderPilot detects the game, installs the add-on (and an add-on-enabled ReShade host — stable or nightly — when one is missing), and can fully reverse the change. Before installing, each game shows a confidence badge (verified / experimental / untested) and an anti-cheat risk note, so online titles are flagged up front. Add-ons are fetched live from upstream, so you always get the latest build.
  - **Updates**: A per-game update check with a freshness indicator and the add-on's publish date, so you can tell when a newer build exists and refresh it in place. The ReShade host is updated alongside the add-on, and you can switch it between stable and nightly at any time. A host that already supports add-ons is reused untouched — including one you installed yourself — and is only replaced (reversibly) when it lacks the add-on support RenoDX needs.
  - **External add-ons**: For games whose add-on is distributed off-GitHub, the card links to the source and lets you install the downloaded file directly — via file picker or drag-and-drop — while RenderPilot still performs the real install and tracks it for clean removal.
  - **DLSS Frame Generation fix**: An optional companion fix that stops ReShade from drawing over generated frames, resolving flicker when DLSS Frame Generation is active.
- **Localization**: The RenoDX experience is fully translated across all seven supported languages.
- **Game Details → executable selector**: A per-game control to choose which executable RenderPilot targets — shared by the NVIDIA driver profile and RenoDX — with smart auto-detection (launcher metadata plus the binary's imported graphics APIs) and a one-click manual override for the rare miss. The "install your own add-on file" path now works for any DirectX game, not only off-GitHub titles.

### Changed (Refactoring & Maintenance)
- **Add-on framework**: Added a tool-agnostic install/update engine — atomic install, strict ordered rollback, and upstream-source tracking — with RenoDX as its first consumer, so future add-ons reuse the same mechanics.
- **Internal structure**: Extracted shared HTTP/CDN/filesystem infrastructure into reusable modules, restructured executable (PE) parsing into a dedicated module, and reshaped installed-add-on storage around a single tracked-sources record.
- **Tooling**: Consolidated desktop linting on a single ESLint configuration (removing a redundant second linter) and enforced consistent block braces across the UI.

## [1.2.0] - 2026-06-20

### Added
- **Settings → NVIDIA**: Global DLSS driver settings (Super Resolution, Frame Generation, Ray Reconstruction) written to NVIDIA's base driver profile, so they apply to every game without a per-game override. Reverting a global setting clears it back to the driver default.
- **Game Details**: "Update all to latest" toolbar action that upgrades every graphics component — NVIDIA, AMD, Intel, and the NVIDIA Streamline bundle — to its newest version in a single pass, with one aggregate progress bar and per-component failure isolation. Streamline plugins upgrade together so the bundle never lands on mixed versions. Shows an "up to date" hint when there is nothing to update.
- **Libraries**: "Download latest" toolbar action that fetches the newest stable build of every not-yet-downloaded library, with an aggregate progress bar and a single summary toast (done / partial / nothing to do).
- **Libraries**: The Streamline tab shows the DLL file name beneath the version for multi-DLL groups, so otherwise-identical `sl_*.dll` rows are distinguishable.

### Fixed
- **Streamline**: The version selector keeps the correct version after a bulk swap, instead of going blank or showing the previous version until the app was restarted.

### Changed
- **Settings**: Reworked the settings page UX — tabs gain icons and remember the active tab, the theme picker becomes a System/Light/Dark segmented control, the SteamGridDB API-key field appears only when its source is enabled (with show/hide, save-on-Enter, and a "get a key" link), and NVIDIA cards gain loading skeletons, an empty state, and error alerts.
- **Bulk actions**: Unified the Libraries and Game Details bulk-action buttons (consistent style, icon, and labels) and moved notification toasts to bottom-center so they no longer cover the selects and buttons beneath them.
- **UI**: Consistent layout spacing and scrolling across the Libraries and Settings pages.

### Changed (Refactoring & Maintenance)
- **Internal structure**: Split several large modules into focused submodules — SQLite schema, the filesystem detector, catalog cards, and catalog execute.
- **Libraries**: Converged the UI on shared primitives (bulk-download button, batch progress bar, concurrency pool).
- **Dependencies**: Updated Rust and frontend dependencies and CI actions to their latest versions.

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
