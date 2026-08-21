# Game-file safety

RenderPilot can replace rendering libraries, install optional components, and manage RenoDX or Luma. These features all change files that a game may load, so their safety guidance belongs to the game installation rather than to one component or add-on.

## The Game Details notice

Game Details shows one compact notice for file-changing features. The notice always explains that changing files used by a multiplayer game may result in account restrictions or a ban. It is not repeated beside every version, add-on, or action.

When RenderPilot detects a known anti-cheat engine, the same notice names it. When none is detected, the notice keeps the general multiplayer warning without claiming that anti-cheat is absent or that a modification is safe. A technical confirmation required for a particular operation, such as a D3D12 executable change, remains separate because it describes a different, operation-specific risk.

## What detection means

Anti-cheat detection uses bounded filesystem heuristics against known markers in the selected game installation. A scan can be incomplete because the installation root, a directory, or an entry could not be read, or because the traversal limit was reached. Unknown and newly introduced anti-cheat systems may also have no known marker.

Detection therefore provides additional context only. It does not determine whether a modification is permitted by the game, its anti-cheat provider, or its online service. The absence of a detected engine is never presented as proof of safety.

## Fresh context before a change

Risk-increasing operations use a fresh, uncached assessment of the selected game. Its opaque context token is bound to that game installation and its anti-cheat scan observation. RenderPilot validates it again while holding the final mutation lock, before durable mutation state is created or the first file is changed. The operation's own plan separately revalidates the files it intends to replace.

If the assessment is missing, belongs to another resource, or became stale while an archive was downloading, the operation stops and Game Details refreshes the assessment. RenderPilot does not automatically repeat the requested change. Rollback, uninstall, removal, recovery, and reconciliation remain available without this context so a user can restore or clean up files.

## Before modifying a multiplayer game

Check the rules and support documentation for the game and its online service. Consider keeping multiplayer installations unmodified when permission is unclear. RenderPilot can make supported changes reviewable and reversible, but it cannot guarantee that a game, service, or anti-cheat system will accept them.

## Sources of truth

- [Anti-cheat detection](../../crates/renderpilot-detection/src/anticheat.rs)
- [File-safety authority](../../crates/renderpilot-orchestration/src/file_safety.rs)
- [Mutation safety policy](../../crates/renderpilot-domain/src/mutation_features.rs)
- [Game Details safety notice](../../apps/desktop/ui/src/entities/game/ui/GameFileSafetyRow.svelte)
