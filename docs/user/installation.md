# Installation and updates

RenderPilot is distributed for Windows x64 as an installer and a portable build. Official files are available only from the project's [GitHub Releases](https://github.com/osyka-yuri/renderpilot/releases) page.

## Installer

The NSIS installer is the recommended option. Download the `x64-setup.exe` asset from the latest release, run it, and choose whether to install for the current user or for all users when Windows offers that choice. The installed app requests an administrator token when Windows creates its process; this is required for protected game folders and graphics settings. If you cancel, deny, or Windows blocks that consent, RenderPilot does not start or make any changes. Start it again after administrator access is available.

RenderPilot uses Microsoft Edge WebView2 for its interface. The configured minimum runtime is 136.0.3240.44. Current Windows installations normally service WebView2 automatically; if the window cannot start, install or repair the Evergreen WebView2 Runtime from Microsoft, then reopen RenderPilot.

The installed Windows application checks the signed `latest.json` published with GitHub releases and uses the normal NSIS/Tauri updater. Existing versioned installer names, installer signatures, and updater metadata are part of that installed-product contract.

## Portable build

For a new portable installation, download the portable ZIP and extract the complete archive to a writable folder. Do not run the executable from inside the ZIP. Its single executable entry is byte-identical to the signed raw portable supervisor asset. The raw supervisor owns portable startup; the public `.rpu` release asset is an updater payload, not an executable. Persistent portable state remains beside the raw executable in `data`, including the database, downloaded catalogs, libraries, covers, and WebView2 profile.

Portable mode changes where RenderPilot stores its persistent application data; it does not remove Windows permission requirements for game folders. Moving the complete extracted folder moves its data and all supervisor-owned recovery state, but not games or files already installed into games. During portable startup, the administrator-token supervisor owns append-only generation, selection, journal, snapshot, migration-receipt, provenance, and authority roots beside the portable executable. RenderPilot-managed portable startup and update state stays inside the extracted folder; it is not stored under `%ProgramData%`, `%LocalAppData%`, or `%TEMP%`. Operations explicitly requested against games or a system-wide graphics layer still write to those external targets.

### Updating an existing portable installation

Portable installations from any earlier 1.x release need one manual upgrade to 1.9.0. Close RenderPilot, download the `RenderPilot_1.9.0_x64-portable.zip` asset, extract its raw supervisor over the existing portable executable, and keep the adjacent `data` folder unchanged. The first launch migrates the preserved 1.x data to the current format. Keeping the old executable filename preserves shortcuts and intentionally renamed launchers.

Starting with 1.9.0, replacement of the stable raw supervisor remains manual: replace it with the signed raw executable from the portable ZIP while preserving the sibling `data` folder. The raw supervisor embeds the exact signed public RPU it activates. While that supervisor is running, the portable update UI can request a newer signed `.rpu`; the supervisor, rather than the managed app, downloads, verifies, stages, publishes, and activates that payload. The installed updater never consumes portable RPU, portable startup, generation, selection, or authority state.

Start a portable copy normally after replacing its raw supervisor. Both the raw supervisor and the private managed app embed `requireAdministrator`; Windows therefore grants the administrator token at each production process-creation boundary before any RenderPilot code can run. The managed app accepts only the supervisor's exact inherited-handle startup contract. Canceling, denying, or blocking consent means no corresponding RenderPilot process starts or makes any changes; start it again after access is available.

If portable startup is interrupted, do not delete `data`, `.renderpilot-runtime-authority`, `.renderpilot-generations`, or `.renderpilot-update`. The supervisor retains uncertain state rather than guessing. After all RenderPilot processes have stopped, retry the same raw supervisor or replace it manually with the signed raw executable from the corresponding release; keep retained journal material for a support report.

## Uninstall and data removal

The installer exposes a separate option to delete application data. When selected during a normal uninstall, the hook removes RenderPilot data from the standard local and roaming application-data locations. It does not perform this deletion during an update. Leave the option clear if you want to preserve the catalog, settings, operation records, cached files, and backups for a later reinstall.

A portable installation can be removed completely by closing every RenderPilot process and deleting the complete extracted folder. Delete only its `data` folder if you intentionally want to reset user data while retaining supervisor recovery state. Before deleting either standard or portable data, restore any game files you may still want RenderPilot to roll back: removing the application database and backups cannot undo modifications already present in a game directory.

## Sources of truth

- [Tauri configuration](../../apps/desktop/src-tauri/tauri.conf.json)
- [NSIS installer hooks](../../apps/desktop/src-tauri/nsis-hooks.nsh)
- [Portable supervisor](../../apps/desktop/src-tauri/src/portable_runtime/supervisor.rs)
- [Portable path authority](../../crates/renderpilot-orchestration/src/portable.rs)
