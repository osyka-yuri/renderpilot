# RenderPilot documentation

This directory is the documentation hub for RenderPilot. Start with the user guide if you installed a release. Use the developer guide when building from source, changing contracts, or preparing a release.

## User guide

- [Installation and updates](user/installation.md) covers the installer, portable build, WebView2, Windows launch authorization, the updater, and data removal.
- [Finding and adding games](user/game-discovery.md) explains launcher scans, manual folders, game roots, and executable selection.
- [Managing libraries](user/library-management.md) covers versions, compatibility, planned operations, D3D12, Developer Mode, and rollback.
- [Game-file safety](user/game-file-safety.md) explains the Game Details notice, anti-cheat detection limits, and fresh mutation contexts.
- [RenoDX and Luma](user/addons.md) covers third-party add-ons, dependencies, updates, and removal.
- [NVIDIA settings and artwork](user/nvidia-and-covers.md) covers per-game DLSS controls, cover sources, SteamGridDB, and custom artwork.
- [Data and troubleshooting](user/data-and-troubleshooting.md) explains local data, network access, offline behavior, recovery, and common problems.

## Developer guide

- [Development setup](development/setup.md) covers the pinned toolchain, desktop and CLI builds, browser preview, and environment overrides.
- [Architecture](development/architecture.md) describes the Rust crates, frontend layers, boundaries, and principal data flows.
- [Mutation safety and storage](development/safety-and-storage.md) documents planning, locking, baselines, journals, recovery, SQLite, migrations, and portable storage.
- [Catalogs and producer contracts](development/catalogs-and-contracts.md) documents Libraries V2, CDN validation, receipts, add-on manifests, and producer checks.
- [Errors and diagnostics](development/errors-and-diagnostics.md) describes error manifests, IPC projection, warning contracts, and logging ownership.
- [Localization](development/localization.md) covers locale packs, generated contracts, editorial policy, and Luma/NVAPI review.
- [Accessibility](development/accessibility.md) defines the WCAG target, Chromium browser coverage, and the separate packaged Windows NVDA/Narrator release matrix.
- [Quality and release](development/quality-and-release.md) lists CI gates, signing, updater assets, portable packaging, and the release lifecycle.
- [Advanced CLI](development/cli.md) is the source-only command reference, including JSON output and its stability boundary.

Public APIs should still be documented in Rustdoc. These guides explain system contracts and contributor workflows; they are not a line-by-line restatement of private code.

## Sources of truth

- [Workspace manifest](../Cargo.toml)
- [Desktop package manifest](../apps/desktop/package.json)
- [Quality workflow](../.github/workflows/quality.yml)
