# Finding and adding games

RenderPilot can discover games from supported launcher libraries and from folders you select yourself. A scan is read-only: it inspects candidate locations and records what it finds before any library or add-on operation is offered.

## Launcher scan

Launcher discovery supports Steam, Epic Games, GOG, EA App/Origin, and Ubisoft Connect. Start a scan from the Games page and review the resulting titles. A game's current location, launcher metadata, executable candidates, detected rendering components, and add-on capabilities are resolved from the installation that exists on disk.

Launcher metadata is useful evidence, not an instruction to trust a stale path. If a game was moved or imported, RenderPilot inspects the real folder and can retain its launcher identity when the files support that match.

## Manual folders

Use a manual folder for a standalone game, an unusual launcher layout, or a title that discovery did not find. Select the game's root rather than an individual DLL. RenderPilot inspects the folder first and presents executable candidates. Confirm the executable that actually starts the game, especially when a directory also contains launchers, crash reporters, benchmark tools, or redistributable installers.

The root and executable establish the boundary for later detection and mutation. If inspection indicates that the selected folder is above or below the likely root, RenderPilot requires an explicit correction. It does not silently redirect a planned operation to another directory.

## Reviewing results

Warnings distinguish usable results from uncertain ones. Common cases include more than one plausible executable, missing launcher evidence, nested game layouts, or a folder that does not contain a supported component yet. Review the displayed path before confirming a manually added game.

Detection recognizes supported NVIDIA, AMD, and Intel components, plus DirectStorage, Microsoft DXC, D3D12 Agility SDK, OpenVR, and the Xiph Ogg/Vorbis pair. A detected file does not automatically mean that every catalog version is compatible; compatibility is evaluated again when you plan a change.

## Sources of truth

- [Launcher and executable detection](../../crates/renderpilot-platform-windows/src/executable_detection.rs)
- [Filesystem detector](../../crates/renderpilot-detection/src/filesystem_detector/mod.rs)
- [Add-game warning contract](../../data/contracts/add-game-warnings.json)
