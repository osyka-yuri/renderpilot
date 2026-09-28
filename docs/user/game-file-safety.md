# Game-file safety

RenderPilot can replace rendering libraries, install optional components, and manage RenoDX, Luma, or OptiScaler. These operations can change files a game loads. Before a file-changing action, Game Details checks the selected installation and asks for confirmation when a safety notice is due.

## Confirmation before file changes

Opening Game Details does not run an anti-cheat scan. Before each action that can increase managed game-file changes, RenderPilot obtains a fresh assessment and asks for confirmation if needed. You can choose **Don't ask me to confirm the general risk again** to skip that general confirmation in future sessions. The risk sentence still appears in executable-change confirmations. Confirmations for executable changes and notices about detected anti-cheat markers remain; the preference does not skip fresh assessments or change backend authorization.

When supported anti-cheat markers are detected, the confirmation shows the associated engine names. If the check is incomplete, the same confirmation explains that some markers may have gone undetected. Specific notices are shown again during the session when the installation or scan result changes. Canceling skips the requested change.

When a prepared D3D12 plan patches or restores the executable, the same confirmation also shows the planned executable paths, backup details, and signature warning. Executable confirmation remains required even if the general warning was previously acknowledged or suppressed. Developer Mode recovery remains a separate prerequisite when needed.

Update All prepares the batch first, then uses one confirmation and one captured assessment for the whole batch. If the selected game installation changes or the assessment becomes stale, the batch stops instead of silently refreshing authorization or continuing with later actions.

## What detection means

The supplementary check searches selected file and directory names for supported Easy Anti-Cheat and BattlEye markers. It is limited and does not inventory every anti-cheat system; other systems and markers that do not match the supported names may not be detected. A limited result can also mean that part of the installation could not be read or the traversal limit was reached.

Detection provides additional context only. It does not determine whether a modification is permitted by the game, its anti-cheat provider, or its online service. A scan that finds no markers does not prove a modification is safe.

## Fresh checks and recovery

The assessment shown during an action is freshly obtained and uncached. The backend validates its opaque context token again while holding the final mutation lock, before durable mutation state is created or files are changed. The operation separately revalidates the files it intends to replace.

If the assessment is missing, belongs to another installation, or becomes stale while an archive is downloading, the requested change stops and an error is shown. RenderPilot does not automatically repeat the change. Rollback, uninstall, removal, recovery, and reconciliation remain available so users can restore or clean up managed files.

## Before modifying a multiplayer game

Check the rules and support documentation for the game and its online service. Consider keeping multiplayer installations unmodified when permission is unclear. RenderPilot can make supported changes reviewable and reversible, but it cannot guarantee that a game, service, or anti-cheat system will accept them.

## Sources of truth

- [Game Details toolbar](../../apps/desktop/ui/src/pages/game-details/ui/GameDetailsToolbar.svelte)
- [Confirmation behavior](../../apps/desktop/ui/src/pages/game-details/ui/FileSafetyConfirmationDialog.svelte)
- [Assessment details](../../apps/desktop/ui/src/pages/game-details/ui/FileSafetyAssessmentDetails.svelte)
- [Mutation confirmation coordinator](../../apps/desktop/ui/src/pages/game-details/model/create-mutation-confirmation.svelte.ts)
- [Notice signature policy](../../apps/desktop/ui/src/pages/game-details/model/file-safety-notice-policy.ts)
- [Session acknowledgments](../../apps/desktop/ui/src/pages/game-details/model/file-safety-notice-session.ts)
- [Anti-cheat detection](../../crates/renderpilot-detection/src/anticheat.rs)
- [File-safety authority](../../crates/renderpilot-orchestration/src/file_safety.rs)
- [Mutation safety policy](../../crates/renderpilot-domain/src/mutation_features.rs)
