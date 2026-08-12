# Data, network access, recovery, and troubleshooting

RenderPilot is local-first: game detection, settings, installed state, operation records, cached catalogs, downloaded artifacts, covers, and recovery material live on your computer. It has no user account and sends no telemetry.

## Local data and network sources

Standard installations use the Windows local application-data directory, with the roaming application-data directory as a fallback. Portable builds use the `data` folder beside the executable. The data includes the SQLite catalog, library catalog and content cache, cover files, settings, and recovery records.

Network access is used for these explicit product functions:

- library catalogs and library payloads from the configured RenderPilot CDN;
- RenoDX, Luma, ReShade, and other supported add-on sources;
- application update metadata and assets from GitHub Releases;
- Steam, GOG, SteamGridDB, or other enabled cover sources.

Cached catalog data and artifacts can remain usable when a source is temporarily unavailable. Operations that need a version or add-on not already cached cannot complete offline. A stale cache is an offline fallback, not a claim that upstream information is current.

## Recovery

At startup, RenderPilot checks for supported mutations that were recorded but did not reach a completed state. Recovery uses the durable record and current filesystem state to finish or reverse the interrupted operation where the state is recognizable. Do not manually delete temporary files, `.bak` baselines, or application data while recovery is pending.

If RenderPilot reports an ambiguous state, stop modifying that game, preserve its folder and RenderPilot data, and open an issue with the sanitized diagnostic information shown by the app. Operation history is useful context, but the current files and verified backup remain decisive.

## Common problems

- **The app window does not open:** repair or update Microsoft Edge WebView2, then retry.
- **A game is not found:** scan its launcher again, or add the real game root manually and review the executable.
- **Access is denied:** close the game and launcher, then relaunch RenderPilot with the permission required by that folder.
- **A plan became stale:** another process changed the files. Rescan and create a fresh plan.
- **No versions or add-ons are available:** check the network, refresh the source, and verify that the detected component or profile is supported.
- **A modified game no longer starts:** use the rollback preview if available. For third-party add-ons, also follow the upstream removal and dependency instructions.

Never download an installer or portable archive from an unofficial mirror when diagnosing installation trouble. Include the RenderPilot version, Windows version, game path with private account segments removed, and the displayed error code in a bug report; do not post API keys or raw personal paths.

## Sources of truth

- [Application data resolution](../../crates/renderpilot-orchestration/src/app_dir.rs)
- [File-mutation recovery](../../crates/renderpilot-orchestration/src/file_mutation/recover.rs)
- [Cover network policy](../../crates/renderpilot-orchestration/src/covers/policy.rs)
