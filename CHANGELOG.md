# Changelog

All notable changes to this project will be documented in this file.

## [1.8.0] - 2026-07-30

### Added

- **Guided game installation**: RenderPilot now inspects a selected folder before adding it, recommends the correct installation root, and asks for an executable when one cannot be chosen safely. Existing manual entries can be corrected without silently discarding managed state, with excluded data preserved in a recovery bundle.
- **Safer D3D12 previews**: Preview D3D12 Agility SDK swaps now verify Windows Developer Mode before modifying a game. If action is required, RenderPilot explains the issue and provides direct links to the relevant Windows setting and documentation.
- **CLI add-game workflow**: `add-game <install-root>` replaces the former `scan-folder` command and exposes executable selection, root review, and root-correction controls.

### Changed

- **Games catalog performance**: Games load and refresh faster, especially in large collections. Updating favorites, visibility, covers, or removing a game no longer reloads unrelated catalog data.
- **Desktop startup and navigation**: Secondary pages and non-English languages now load only when needed, reducing initial work while keeping navigation and language changes consistent.
- **Locale-aware presentation**: Dates, times, relative durations, byte sizes, numbers, lists, and updater progress now use shared locale-aware formatters with strict timestamp parsing at application boundaries.
- **Installation tracking**: RenderPilot now identifies installation roots more reliably. During an upgrade, only verified duplicate entries are consolidated; ambiguous or conflicting state is left untouched.

## [1.7.2] - 2026-07-27

### Added

- **Preview library releases**: Preview versions are now available in Libraries and compatible replacement selectors. They remain manual choices and are never included in automatic updates.
- **Local library releases**: Downloaded versions remain visible and manageable if they are no longer available in the active catalog.

### Changed

- **Library versions**: RenderPilot now preserves complete package versions, making preview and stable releases easier to identify and manage reliably.
- **Executable selection**: Unavailable executable selectors now remain visible with a clear explanation. Candidate grouping and switching back to automatic detection are also clearer.

### Fixed

- **App updates**: Failed update downloads are retried automatically, with reliable progress reporting and cancellation.

## [1.7.1] - 2026-07-26

### Fixed

- **D3D12 executable detection**: Games such as Cyberpunk 2077 and Split Fiction no longer show a false repair-required state when RenderPilot has never managed their executable. The scanner now chooses the unique executable that exports `D3D12SDKVersion` and accepts a valid target export even when unrelated PE data exports live only in virtual memory.
- **D3D12 recovery status**: The Game Details recovery warning shows the immutable backup path only when that backup actually exists, so a planned sidecar is never presented as recoverable data.

## [1.7.0] - 2026-07-25

### Added

- **D3D12 Agility SDK**: Detect and manage versioned `D3D12Core.dll` packages together with the selected game's `D3D12SDKVersion` export. Swap plans classify the executable action as no change, managed patch, original restore, or repair required. The first user-selected managed patch and a user-selected restore require the fresh, state-bound plan token; apply fails closed when the executable, package, or backup has changed.
- **DirectX Shader Compiler**: Discover and replace DXC runtimes from the versioned library catalog, including complete `dxcompiler.dll`/`dxil.dll` packages and games that ship only the standalone compiler. Package identity, architecture, and installed-pair coverage are validated before a candidate is offered.
- **OpenVR**: Detect package-backed OpenVR loaders and safely move between compatible releases with PE architecture, export-surface, source-integrity, and rollback validation.
- **Library legal documents**: View package license and notice documents directly from the Libraries page. Legal metadata is validated as part of the catalog and remains available across page navigation.
- **Operation preflights**: The CLI and desktop API now expose typed swap and rollback previews. `plan-swap` returns blockers, warnings, affected files, the D3D12 executable action, and any required confirmation token; `plan-rollback` provides the corresponding read-only rollback preview.

### Changed

- **Runtime catalog**: Replaced runtime-specific package handling with a versioned, package-aware catalog model shared by DXC, OpenVR, D3D12 Agility SDK, Streamline, FSR, and existing single-file libraries. Candidate identity now follows the exact files written by each transition instead of relying on display versions alone.
- **D3D12 workflows**: Game Details separates ordinary candidates from executable-changing actions, shows the current managed executable state, requests confirmation for a first managed patch or restore when required, and propagates the state-bound token through both single and bulk updates.
- **Bulk updates**: Update-all planning, confirmation, cancellation, and progress now use one testable workflow, while unrelated component failures remain isolated.
- **Libraries experience**: Page state and package data survive navigation, action feedback stays aligned with the initiating control, tooltips forward their trigger properties correctly, and download progress is more compact.

### Fixed

- **Disk reconciliation**: Catalog, add-on, and rollback state are reconciled against current files before mutation, preventing stale observations from being treated as valid managed state.
- **App updates**: Stabilized the update dialog lifecycle and action state so download/install feedback remains consistent while the app prepares to exit.

### Changed (Refactoring & Maintenance)

- **Rollback and recovery**: Original component and D3D12 executable identities are immutable once captured; only the verified expected-active identity advances. SQLite schema v11/v12 migrations preserve existing data, and interrupted apply/rollback operations recover through the durable journal and immutable sidecars.
- **Typed boundaries**: Consolidated action, status, swap-plan, rollback-plan, and operation-metadata DTOs across orchestration, CLI, API, Tauri, and TypeScript. Confirmation tokens live only on fresh swap plans rather than executable actions.
- **Graphics model**: Removed the unused standalone NVIDIA Reflex technology classification from the domain model, filters, and desktop presentation.
- **Tooling and dependencies**: Pinned Rust 1.97.1, updated hashing and desktop dependencies, refreshed both lockfiles, and documented the current architecture and quality gates.

## [1.6.0] - 2026-07-20

### Added

- **Game Details → Luma Framework**: Install, update, repair, and remove Luma Framework for supported DirectX 11 games. Managed installs use the nightly ReShade host and can provide DLSS or FSR upscaling, HDR, and shader replacement according to each game's catalog profile.
- **Luma guidance**: Game-specific cards show anti-cheat risk, required launch arguments, Visual C++ runtime guidance, and managed dgVoodoo2 dependencies. The complete experience is localized across all seven supported languages.
- **CLI**: Inspect per-game Luma status, remove an installation, run passive or deep update checks, and audit updates across the catalog.

### Changed

- **Add-ons**: Luma and RenoDX are mutually exclusive for a game and now share coordinated ReShade host policy, catalog refresh, operation locking, progress, and recovery behavior.
- **Bulk updates**: Game Details coordinates library, RenoDX, and Luma updates through one exclusive action so overlapping mutations cannot run against the same game.

### Changed (Refactoring & Maintenance)

- **Add-on engine**: Introduced a durable shared lifecycle for managed files, set-diff updates, repair, rollback, and recovery after interrupted mutations or lost tracking data.
- **Storage**: Added the schema v10 migration for coordinated add-on files and crash-recoverable game-file mutations, including validation, additive upgrades, and a backup before destructive schema rebuilds.
- **Desktop performance**: Split third-party frontend dependencies into a vendor chunk; current production JavaScript chunks stay below 500 kB.
- **Documentation and tooling**: Updated the feature overview, architecture, runtime behavior, Rust/Tauri/Svelte dependencies, and internal module boundaries for Luma and the shared add-on platform.

## [1.5.2] - 2026-07-14

### Fixed

- **Libraries**: Before swapping a DLL, RenderPilot verifies that the replacement file is still present and matches what the catalog expects. If you restored game files outside the app, or a cached copy was corrupted, the swap is blocked with a clear recovery message instead of writing the wrong file. After a rescan, outdated local copies no longer appear as valid replacements.
- **Streamline**: Updating Streamline now applies a matching multi-file package for the plugins already in the game, instead of updating only a single DLL. When plugins are on different versions, the UI shows a mixed state so you can bring the whole set back in sync.
- **App updates**: After installing an update on Windows, the app can request administrator rights again on restart instead of staying without them. The update dialog shows a completed download and an installing step before the window closes for the installer, with clearer wording about what happens next.

## [1.5.1] - 2026-07-12

### Fixed

- **RenoDX / ReShade**: Existing ReShade installations are now classified safely. Empty, compatible hosts can be recovered or repaired; custom, incompatible, or unclear setups are left untouched.
- **RenoDX / ReShade**: Improved recovery after tracked install data is lost. RenderPilot adopts only verifiable RenoDX files, preserves user ReShade configuration and logs, and never removes a reused runtime during uninstall.
- **Games**: Corrected catalog filter logic to prevent programs without any supported technologies from appearing when all filters are checked.
- **UI**: Improved alignment and spacing in addon views and action buttons. The "Check for updates" button now spins smoothly instead of jumping text.

## [1.5.0] - 2026-07-11

### Added

- **App updates**: RenderPilot now checks for new releases from the header and Settings. Review the release notes, download and verify an update with live progress, retry a failed attempt, and restart the app to finish installing it.
- **Games**: Filter your library by available add-ons and see each supported add-on directly on the game card.

### Changed

- **Refresh**: The main Refresh button now fetches the latest Libraries, RenoDX, and ReShade catalog data before it rescans your system, so newly published versions and compatibility changes do not wait for the cache to expire.

### Fixed

- **Games**: Centered the scan action in an empty library.
- **Settings → RenoDX**: The shared Vulkan layer now displays the ReShade host consistently.

### Changed (Refactoring & Maintenance)

- **Add-ons**: Consolidated the RenoDX and ReShade install/update foundations and strengthened file replacement, database validation, catalog handling, and native operation boundaries for more reliable recovery from interrupted work.
- **Tooling**: Raised the minimum Rust version to 1.97 and updated the release build action and desktop toolchain.

## [1.4.1] - 2026-07-03

### Fixed

- **Game Details → RenoDX**: Fixed installation failing outright for certain games that share a universal, engine-wide add-on instead of having their own.
- **Game Details → RenoDX**: Updating no longer shows a warning written for replacing the ReShade host, or leaves behind an unnecessary backup file, when only the add-on itself has a new version.
- **Game Details → RenoDX**: Uninstalling no longer restores `ReShade.ini` to its pre-install snapshot — which could silently discard any ReShade settings changed after installing RenoDX — and instead removes only the keys RenoDX itself added.
- **Game Details → RenoDX**: A ReShade installation that's actually a different, compatible build (such as GShade) is now clearly recognized and left alone — RenderPilot won't offer to install, repair, or update it.

### Changed (Refactoring & Maintenance)

- **RenoDX engine**: Every in-place file replacement (add-on, host, DLSS-Fix) now rolls back to its previous bytes if a later step in the same operation fails, and the custom-build recognition check now lives at a single shared resolution point instead of being re-implemented per call site.

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
