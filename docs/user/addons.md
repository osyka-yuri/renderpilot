# RenoDX and Luma

RenderPilot can manage supported RenoDX and Luma installations. Both are independent third-party projects with their own licenses, compatibility boundaries, update channels, and risks. ReShade may be installed as a shared host. Read the upstream documentation for the selected profile before modifying a game.

## RenoDX

RenoDX profiles can add HDR and related rendering controls to supported games. RenderPilot evaluates the selected game, displays match confidence and host requirements, and prepares the required RenoDX and ReShade files. Some profiles can use an optional DLSS Frame Generation fix so the ReShade output is handled correctly with generated frames.

Where a profile is not distributed from an automatically supported source, RenderPilot can validate a file you downloaded yourself through the file picker or drag-and-drop flow. That path does not make an unknown archive trusted: obtain it from the profile's official distribution channel and verify its instructions.

## Luma

Luma profiles can provide DirectX 11 upscaling, HDR, and shader replacement for supported games. New managed installations use the nightly ReShade host. Their requirements can include a particular host layout, Microsoft Visual C++ runtime, launch arguments, DLSS bindings, or dgVoodoo components. RenderPilot reports the dependencies and planned files that apply to the chosen title.

Luma and RenoDX are treated as mutually exclusive managed add-ons for a game. Remove the active one before installing the other. Shared ReShade files are tracked so removal can distinguish an add-on's managed files from files that another supported feature still needs.

## Updates and removal

Status and update checks use current manifests and, when supported, the add-on's upstream source. A failed remote check can leave the last known information available, but it should not be mistaken for proof that no update exists. Removal previews and reverses files tracked by RenderPilot; unrelated files are not intentionally deleted.

Installing, updating, or repairing an add-on changes files in the selected game and therefore participates in RenderPilot's general [game-file safety](game-file-safety.md) flow. Add-on removal remains available so managed files can be restored even when a fresh safety context cannot be acquired.

## Sources of truth

- [RenoDX orchestration](../../crates/renderpilot-orchestration/src/addons/renodx/mod.rs)
- [Luma orchestration](../../crates/renderpilot-orchestration/src/addons/luma/mod.rs)
