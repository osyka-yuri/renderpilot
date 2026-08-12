# RenoDX and Luma

RenderPilot can manage supported RenoDX and Luma installations. Both are independent third-party projects with their own licenses, compatibility boundaries, update channels, and risks. ReShade may be installed as a shared host. Read the upstream documentation for the selected profile before modifying a game.

## RenoDX

RenoDX profiles can add HDR and related rendering controls to supported games. RenderPilot evaluates the selected game, displays match confidence and risk information, and prepares the required RenoDX and ReShade files. Some profiles can use an optional DLSS Frame Generation fix so the ReShade output is handled correctly with generated frames.

Where a profile is not distributed from an automatically supported source, RenderPilot can validate a file you downloaded yourself through the file picker or drag-and-drop flow. That path does not make an unknown archive trusted: obtain it from the profile's official distribution channel and verify its instructions.

## Luma

Luma profiles can provide DirectX 11 upscaling, HDR, and shader replacement for supported games. New managed installations use the nightly ReShade host. Their requirements can include a particular host layout, Microsoft Visual C++ runtime, launch arguments, DLSS bindings, or dgVoodoo components. RenderPilot reports the dependencies and planned files that apply to the chosen title.

Luma and RenoDX are treated as mutually exclusive managed add-ons for a game. Remove the active one before installing the other. Shared ReShade files are tracked so removal can distinguish an add-on's managed files from files that another supported feature still needs.

## Updates, removal, and anti-cheat

Status and update checks use current manifests and, when supported, the add-on's upstream source. A failed remote check can leave the last known information available, but it should not be mistaken for proof that no update exists. Removal previews and reverses files tracked by RenderPilot; unrelated files are not intentionally deleted.

Injectors, shader replacements, DLL proxies, and executable-adjacent modifications may be prohibited by an anti-cheat system or a game's terms. Do not install them for protected multiplayer use unless the game and service explicitly permit the modification. A warning is information, not a guarantee of safety.

## Sources of truth

- [RenoDX orchestration](../../crates/renderpilot-orchestration/src/addons/renodx/mod.rs)
- [Luma orchestration](../../crates/renderpilot-orchestration/src/addons/luma/mod.rs)
- [Anti-cheat detection](../../crates/renderpilot-detection/src/anticheat.rs)
