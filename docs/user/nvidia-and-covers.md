# NVIDIA settings and artwork

RenderPilot includes two optional conveniences that do not change its core library workflow: per-game NVIDIA DLSS settings and locally stored game artwork.

## NVIDIA DLSS settings

On supported NVIDIA systems, RenderPilot can read and manage driver-level settings for DLSS Super Resolution, Frame Generation, and Ray Reconstruction, including available presets. Settings are applied to the selected game's NVIDIA profile through NVAPI; they do not replace a game's own graphics menu and cannot add a feature the game, installed library, GPU, or driver does not support.

Review the selected executable and current value before applying an override. Driver updates, NVIDIA profile changes, or game updates may alter effective behavior. Return a setting to its default when troubleshooting differences between RenderPilot, the game menu, and the driver.

The NVIDIA DLSS indicator can also be toggled to display the active upscaler information in a supported game. It is a diagnostic overlay, not a compatibility test, and still depends on the driver, game, and installed DLSS component.

## Covers

Cover discovery can use first-party Steam information, Steam CDN artwork, GOG CDN artwork, and the optional SteamGridDB service. Remote cover sources can be enabled or disabled in settings. SteamGridDB requests require your own API key; the key is stored in RenderPilot's local settings and sent only to that service for requests you enable.

You can replace a downloaded cover with an image from your computer. Custom artwork is copied into RenderPilot's local cover storage and can be removed without touching the game installation. If remote sources are unavailable, already cached or custom covers continue to work.

Artwork remains subject to the source provider's terms. RenderPilot's GPL license does not relicense images returned by Steam, GOG, SteamGridDB, or a file you supplied.

## Catalog organization

The game catalog can be searched and filtered by launcher or detected library. Favorites and hidden games are local organizational choices. Light, dark, and system-following themes are also stored locally and do not change a game's files.

## Sources of truth

- [NVAPI integration](../../crates/renderpilot-nvapi/src/lib.rs)
- [Cover providers](../../crates/renderpilot-orchestration/src/covers/providers/mod.rs)
- [Cover settings](../../apps/desktop/ui/src/entities/settings/api/cover-policy.ts)
