# Data, network access, recovery, and troubleshooting

RenderPilot is local-first: game detection, settings, installed state, operation records, cached catalogs, downloaded artifacts, covers, and recovery material live on your computer. It has no user account and sends no telemetry.

## Local data and network sources

Standard installations use `RENDERPILOT_APP_DIR` when it is set; otherwise they use the Windows local application-data directory, with the roaming application-data directory as a fallback. Portable builds use the authenticated `data` folder beside the executable. The data includes the SQLite catalog, library catalog and content cache, cover files, settings, and recovery records.

### Diagnostic logs

- Installed: `%RENDERPILOT_APP_DIR%\logs\installed\app\` when set, otherwise `%LOCALAPPDATA%\RenderPilot\logs\installed\app\` (or `%APPDATA%\RenderPilot\logs\installed\app\`).
- Portable App: `<portable folder>\data\logs\portable\app\`.
- Portable supervisor: the adjacent `supervisor` folder.

Filenames begin with the UTC start time and a short ID; App files also have a segment number. Sort by name to find the latest run and choose its highest segment. If two runs began in the same second, compare modification times. Each record has readable `timestamp_utc` and numeric `unix_ms`.

Successful cleanup keeps up to eight verified completed files plus the active file in each folder. Unrecognized or incomplete files are left untouched; a cleanup failure does not stop the current log. Logs contain event codes and selected game or affected-file paths, which may include your Windows account name. Review a log before sharing it. A second installed launch focuses the existing window. Portable App failures before runtime-path authentication do not appear in its file.

### Network access

Network access is used for these explicit product functions:

- library catalogs and library payloads from the configured RenderPilot CDN;
- RenoDX, Luma, OptiScaler, ReShade, and other supported add-on sources;
- application update metadata and assets from GitHub Releases;
- Steam, GOG, SteamGridDB, or other enabled cover sources.

Cached catalog data and artifacts can remain usable when a source is temporarily unavailable. Operations that need a version or add-on not already cached cannot complete offline. A stale cache is an offline fallback, not a claim that upstream information is current.

## Recovery

At startup, RenderPilot checks for supported mutations that were recorded but did not reach a completed state. Recovery uses the durable record and current filesystem state to finish or reverse the interrupted operation where the state is recognizable. Do not manually delete temporary files, `.bak` baselines, or application data while recovery is pending.

If RenderPilot reports an ambiguous state, stop modifying that game, preserve its folder and RenderPilot data, and open an issue with the sanitized diagnostic information shown by the app. If asked for a diagnostic file, review it first and remove personal account-name segments from any game paths before attaching it. Operation history is useful context, but the current files and verified backup remain decisive.

## Common problems

- **The app window does not open:** if a WebView2 Runtime warning appears, install or update the runtime using the link in the prompt. Otherwise, include any displayed error and a diagnostic log, if one was created, in a bug report.
- **A game is not found:** check the search, filters, and hidden games on the Games page, then use **Refresh** to scan launcher libraries. If it is still missing, use **Add Game** to select its installation folder and review the game executable if prompted.
- **Access is denied:** check which operation failed. Production builds already request administrator access at startup; if access is still denied, include the error code and diagnostic log, if available, in a bug report.
- **An action is out of date:** review the updated result and start the action again. If a downloaded version changed, select it again; RenderPilot refreshes an outdated file-safety assessment before the next attempt.
- **No versions or add-ons are available:** use **Refresh** to update game detection and catalog data. Check the network if a source reports a connection error; otherwise, the detected game or component may have no compatible choice.
- **A game stopped launching after a RenderPilot change:** use **Restore original** if available for the affected library component, or **Remove** for the affected add-on. If removal is blocked by another managed add-on, follow the dependency order shown in the app.

Never download an installer or portable archive from an unofficial mirror when diagnosing installation trouble. Include the RenderPilot version, Windows version, game path with private account segments removed, and the displayed error code in a bug report; do not post API keys or raw personal paths.

## Sources of truth

- [Application data resolution](../../crates/renderpilot-orchestration/src/app_dir.rs)
- [File-mutation recovery](../../crates/renderpilot-orchestration/src/file_mutation/recover.rs)
- [Cover network policy](../../crates/renderpilot-orchestration/src/covers/policy.rs)
