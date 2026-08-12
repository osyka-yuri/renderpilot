# Development setup

RenderPilot is a Rust workspace with a Tauri desktop application, a Svelte frontend, and a source-only CLI. The minimum supported Rust version is 1.97 and `rust-toolchain.toml` pins 1.97.1; the desktop package requires Node.js 24 or later and declares pnpm 11.18.0 as the package-manager source of truth.

## Toolchain

On Windows, install the Rust prerequisites for MSVC development, Node.js 24, and the Microsoft Edge WebView2 Runtime. Let Corepack or the pinned pnpm setup resolve the package-manager version from `apps/desktop/package.json`.

```powershell
rustup show
cd apps/desktop
corepack enable
pnpm install --frozen-lockfile
```

Linux CI also installs the WebKitGTK, JavaScriptCoreGTK, Soup, GTK, AppIndicator, SVG, and `pkg-config` development packages needed to compile the Tauri target.

## Desktop development

Run the browser-based UI during ordinary frontend work:

```powershell
cd apps/desktop
pnpm dev
```

When the page does not find `__TAURI_INTERNALS__`, it loads the maintained mock command adapter. This preview is useful for UI states and frontend tests; it is not a substitute for testing real filesystem, SQLite, NVAPI, Windows manifest launch behavior, or updater behavior.

Run the full desktop shell when the change crosses the IPC boundary:

```powershell
cd apps/desktop
pnpm tauri dev
```

Official production Apps embed a `requireAdministrator` manifest, so Windows must grant an administrator token before it creates the process. Development and release-tooling builds deliberately embed `asInvoker` instead. To exercise the full desktop shell locally, open an elevated terminal first, then run `pnpm tauri dev`; the development manifest never opens its own UAC prompt. Do not treat an unelevated development run as coverage for protected filesystem, NVAPI, portable update, or recovery effects.

Build the frontend with `pnpm build`. On Windows, build the release-tooling desktop bundle through the manifest helper so the Tauri CLI receives its required selector and `beforeBuildCommand` is applied. The helper restores the prior manifest environment after the command finishes:

```powershell
cd apps/desktop
. ./scripts/windows-manifest-common.ps1
Invoke-RenderPilotWithWindowsManifest -Selector release-tooling -Command { pnpm tauri build }
```

## Rust workspace and CLI

Build all Rust targets and features with the locked dependency graph:

```powershell
cargo build --workspace --locked --all-features
```

The CLI is an advanced source-only tool. Run it from the workspace rather than expecting a published binary:

```powershell
cargo run -p renderpilot-cli -- --help
```

## Environment overrides

- `RENDERPILOT_APP_DIR` replaces the resolved application-data root outside an authenticated portable child.
- `RENDERPILOT_DB_PATH` replaces only the SQLite catalog path outside an authenticated portable child.
- `RENDERPILOT_LIBRARIES_FIXTURE` points the ignored producer-contract test at its golden V2 fixture.
- `TAURI_DEV_HOST` controls the Vite/Tauri development host.
- `WEBVIEW2_USER_DATA_FOLDER` overrides the WebView2 profile location when set before desktop startup.
- `RUST_LOG` controls backend logging through `env_logger` where the launched target enables it.
- `RENDERPILOT_WINDOWS_MANIFEST` is build-only. Windows release builds accept only `production` or `release-tooling`; non-release Windows builds accept only `development` or an unset value. Use `Invoke-RenderPilotWithWindowsManifest` to scope a local selector instead of exporting the variable; the helper restores any previous value. The official production workflow remains the release authority and scopes its selector itself.

For the complete application-data resolution order and portable layout, see [Storage locations](safety-and-storage.md#storage-locations).

Use task-specific temporary directories for storage overrides. Do not aim development tests at a real installation's data directory or a game library you cannot restore.

## Sources of truth

- [Rust toolchain](../../rust-toolchain.toml)
- [Desktop package manifest](../../apps/desktop/package.json)
- [Vite configuration](../../apps/desktop/vite.config.ts)
- [Application directory resolution](../../crates/renderpilot-orchestration/src/app_dir.rs)
