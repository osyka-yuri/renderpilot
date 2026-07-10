## [1.4.1] - 2026-07-03

### Fixed

- **Game Details → RenoDX**: Fixed installation failing outright for certain games that share a universal, engine-wide add-on instead of having their own.
- **Game Details → RenoDX**: Updating no longer shows a warning written for replacing the ReShade host, or leaves behind an unnecessary backup file, when only the add-on itself has a new version.
- **Game Details → RenoDX**: Uninstalling no longer restores `ReShade.ini` to its pre-install snapshot — which could silently discard any ReShade settings changed after installing RenoDX — and instead removes only the keys RenoDX itself added.
- **Game Details → RenoDX**: A ReShade installation that's actually a different, compatible build (such as GShade) is now clearly recognized and left alone — RenderPilot won't offer to install, repair, or update it.

### Changed (Refactoring & Maintenance)

- **RenoDX engine**: Every in-place file replacement (add-on, host, DLSS-Fix) now rolls back to its previous bytes if a later step in the same operation fails, and the custom-build recognition check now lives at a single shared resolution point instead of being re-implemented per call site.
