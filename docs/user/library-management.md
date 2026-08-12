# Managing libraries

The Libraries view brings detected components and available catalog packages together. Treat each operation as a change to a particular game state, not as a universal recommendation that a newer version is always better.

## Versions and compatibility

Select a game and component to compare the installed version with compatible choices. RenderPilot uses the component family, binary metadata, architecture, location, and package metadata to reject choices that do not belong to the detected target. Update selects a newer compatible candidate; downgrade lets you choose an older compatible release. Availability still depends on the current catalog and network or cache state.

Before applying a choice, review the affected paths and any warnings. Games can carry modified, bundled, or engine-specific files whose names resemble a supported library. If RenderPilot cannot establish a safe target, it stops and asks for review instead of guessing.

## Apply and rollback

Planning reads the current files and prepares a state-specific operation. Applying repeats the important checks, saves the original baseline when one does not already exist, and replaces the selected files. If the files changed after planning, create a new plan.

Rollback has its own preview and lists every affected path. It restores the verified original baseline, not merely the version that happened to be installed immediately before the most recent update. The Operations view retains a readable history of completed swaps and rollbacks. Keep RenderPilot's data until you no longer need that recovery path.

For the implementation protocol behind preflight, baselines, and recovery, see [Mutation safety and storage](../development/safety-and-storage.md).

## D3D12 Agility SDK

D3D12 updates coordinate `D3D12Core.dll` with the selected game's `D3D12SDKVersion` export. RenderPilot reports whether the executable already matches, needs a managed patch, can be returned to its original value, or requires manual repair. The first managed executable patch and a user-selected restore require explicit confirmation tied to the current state.

Windows Developer Mode may be required for the managed executable step. Enable it only if you understand the system-wide setting and the game is suitable for modification. Ambiguous exports, unsupported layouts, stale files, or repair states stop the operation.

## Sources of truth

- [Catalog planning](../../crates/renderpilot-orchestration/src/catalog/execute/mod.rs)
- [Mutation engine](../../crates/renderpilot-orchestration/src/file_mutation/mod.rs)
- [D3D12 compatibility handling](../../crates/renderpilot-orchestration/src/catalog/runtime_compatibility.rs)
