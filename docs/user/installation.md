# Installation and updates

RenderPilot is distributed for Windows x64 as an installer and a portable build. Official files are available only from the project's [GitHub Releases](https://github.com/osyka-yuri/renderpilot/releases) page.

## Installer

The NSIS installer is the recommended option. Download the `x64-setup.exe` asset from the latest release, run it, and choose whether to install for the current user or for all users when Windows offers that choice. The installed app requests an administrator token when Windows creates its process; this is required for protected game folders and graphics settings. If you cancel, deny, or Windows blocks that consent, RenderPilot does not start or make any changes. Start it again after administrator access is available.

RenderPilot uses Microsoft Edge WebView2 for its interface. The configured minimum runtime is 136.0.3240.44. Current Windows installations normally service WebView2 automatically; if the window cannot start, install or repair the Evergreen WebView2 Runtime from Microsoft, then reopen RenderPilot.

The installed Windows application checks the signed `latest.json` published with GitHub releases and uses the normal NSIS/Tauri updater. Existing versioned installer names, installer signatures, and updater metadata are part of that installed-product contract.

## Portable build

For a new portable installation, download the portable ZIP and extract the complete archive to a writable folder. Do not run the executable from inside the ZIP. The public `.rpu` release asset is used by the portable updater and is not an application. Persistent data and recovery state remain beside the executable inside the extracted folder.

Portable mode keeps RenderPilot data with the application, but it does not remove Windows permission requirements for game folders. Windows requests administrator approval when the portable application starts; if approval is canceled, denied, or blocked, RenderPilot does not start or make changes. Operations explicitly requested against games or system-wide graphics settings still modify those external locations.

Close RenderPilot completely before moving a portable installation, then move the entire extracted folder. This preserves the database, downloaded catalogs, libraries, covers, WebView2 profile, and update recovery state. It does not move games or files that RenderPilot has already installed into game folders.

### Updating an existing portable installation

If you are specifically installing 1.9.0 from an earlier 1.x release, close RenderPilot, download `RenderPilot_1.9.0_x64-portable.zip`, and extract it into the existing portable folder. Keep the `data` folder unchanged. The first launch upgrades the preserved data, and keeping the existing executable filename preserves shortcuts and intentionally renamed launchers.

The first release using the current portable runtime requires one complete package replacement. You do not need to install 1.9.0 first. Fully close every RenderPilot process, download the new portable ZIP, and extract the complete archive into the existing folder, replacing the shipped files when prompted. Do not delete `data` or the hidden `.renderpilot-runtime-authority`, `.renderpilot-generations`, and `.renderpilot-update` directories. The 1.9.0 in-app updater cannot perform this one transition. After the compatible package has started successfully, later compatible portable updates work normally from the application.

If a portable startup or update is interrupted, close every RenderPilot process and retry with the same complete package. Do not delete the data or recovery directories listed above. If the retry still fails, keep the folder unchanged so its retained recovery information can be included in a support report.

## Uninstall and data removal

The installer exposes a separate option to delete application data. When selected during a normal uninstall, the hook removes RenderPilot data from the standard local and roaming application-data locations. It does not perform this deletion during an update. Leave the option clear if you want to preserve the catalog, settings, operation records, cached files, and backups for a later reinstall.

A portable installation can be removed completely by closing every RenderPilot process and deleting the complete extracted folder. Delete only its `data` folder if you intentionally want to reset user data while retaining supervisor recovery state. Before deleting either standard or portable data, restore any game files you may still want RenderPilot to roll back: removing the application database and backups cannot undo modifications already present in a game directory.

## Sources of truth

- [Tauri configuration](../../apps/desktop/src-tauri/tauri.conf.json)
- [NSIS installer hooks](../../apps/desktop/src-tauri/nsis-hooks.nsh)
- [Portable supervisor](../../apps/desktop/src-tauri/src/portable_runtime/supervisor.rs)
- [Portable path authority](../../crates/renderpilot-orchestration/src/portable.rs)
