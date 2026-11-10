# NVIDIA settings and artwork

## NVIDIA DLSS settings

**Game Details → NVIDIA** manages driver overrides for DLSS Super Resolution, Frame Generation, and Ray Reconstruction in the selected executable's profile. **Settings → NVIDIA** changes the base profile for games that inherit the setting. In-game support depends on the game, GPU, driver, and installed DLSS components.

RenderPilot rereads the driver when you open the NVIDIA tab, return to the app with that tab visible, or a write fails. If the game, driver, or another tool changes a value, you can select a supported value again while the profile binding can still be verified. The new write starts from the current driver state.

- **Reset to default** removes the override from the current profile and lets the driver choose the inherited value.
- **Restore original** reinstates the first saved state of a game profile setting, including whether it had an override. It requires the same verified executable binding.

RenderPilot blocks writes if it cannot verify the profile binding or an unresolved operation overlaps the profile or executable. Deleting a profile created by RenderPilot requires its applications and settings to match the last confirmed profile state.

DLL-dependent options use the game catalog. **DLL not found** means a scan found no matching DLL. **Version unavailable** means the DLL was found but its version could not be read. **Catalog not ready** means a fresh scan is needed. In that case, driver values stay visible and DLL-dependent changes stay disabled until a scan completes. **Rescan game** refreshes these details.

Enable **DLSS Indicator** in **Settings → NVIDIA** to see active DLSS information in supported games.

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
