# Errors and diagnostics

Desktop errors cross a trust boundary. Internal chains may contain paths, URLs, operating-system text, or implementation details that are useful in backend logs but inappropriate for a stable frontend contract.

## Error boundary

The effective mapping is `AppError` to `ServiceError` to `ApiError` to `CommandError`. The Tauri command boundary exposes only an allowlisted code and structured fields for the recognized failure. Severity, supported actions, and presentation metadata come from `data/contracts/desktop-command-errors.json`; the generated frontend projection is checked against that manifest and normalized once when a response enters the UI.

Raw error causes are not serialized across IPC. User-facing messages must be selected from a stable code plus sanitized parameters, not from `Display` output or arbitrary upstream text. A new desktop failure therefore needs coordinated Rust mapping, manifest entry, generated/frontend contract, localization, and tests.

## Add-game warnings

Non-fatal game inspection outcomes use the separate `data/contracts/add-game-warnings.json` manifest. Stable warning IDs let the backend communicate evidence without embedding UI prose. Their generated frontend bindings remain inside the scan feature, while desktop command-error bindings belong to the shared error layer. The frontend owns localized presentation and recognized actions. Do not reuse a warning as an error merely because both appear near the same scan flow.

## Diagnostic ownership

`tracing-subscriber` writes Rust diagnostics to stderr. `RUST_LOG` controls its filter (default `info`), and `tracing-log` forwards `log` events from dependencies.

File diagnostics are independent of `RUST_LOG`. A sealed, typed observer records native workspace `Warn` and `Error` callsites with a fixed category, Rust module, line, and deterministic site ID. Selected recovery and uninstall callsites may include a validated game or affected-file path. The file never stores the original message, raw error chain, URL, response body, or arbitrary tracing fields.

Desktop command failures are recorded once at the Rust command boundary. Other backend observations use typed events; a callsite marker prevents their generic tracing projection from duplicating them. The frontend bridge starts before i18n and application bootstrap. It sends fixed i18n metadata, coarse client-boundary and IPC transport failures, and recognized frontend-only errors without their original causes. Preview mode remains console-only.

## Diagnostic files

Files are NDJSON, with a 4 KiB line limit and 2 MiB segment limit. Each normal event has `timestamp_utc` (RFC3339 UTC, milliseconds) and `unix_ms`.

| Build | Directory | Filename |
| --- | --- | --- |
| Installed App | `<app data>/logs/installed/app/` | `YYYY-MM-DD_HH-MM-SSZ-<16-hex-session-prefix>-s<8-hex-segment>.log` |
| Portable App | `<portable root>/data/logs/portable/app/` | `YYYY-MM-DD_HH-MM-SSZ-<16-hex-transaction-prefix>-s<8-hex-segment>.log` |
| Portable supervisor | `<portable root>/data/logs/portable/supervisor/` | `YYYY-MM-DD_HH-MM-SSZ-<16-hex-session-prefix>.log` |

For installed builds, `RENDERPILOT_APP_DIR` overrides the default `%LOCALAPPDATA%\RenderPilot` (with `%APPDATA%\RenderPilot` as fallback). A second installed launch focuses the existing window. Portable App logging begins after runtime-path authentication. Both portable roles use schema version 1; every App record includes its segment. Segments of one run keep the same filename time and ID. Sort by filename to find the latest run, compare first-record timestamps for starts in the same second, then choose the highest segment.

Successful cleanup keeps up to eight verified completed files plus the active file per role. It reads at most 4 KiB from each candidate's first record and checks its schema, role, segment, and whether the filename's short ID matches the full identity prefix. Unrecognized, incomplete, or mismatching files are left untouched. Scanning stops at 256 directory entries or 64 canonical candidates; a cleanup failure leaves the active writer running and reports `installed_diagnostics_retention_failed` or `portable_diagnostics_retention_failed` on stderr. Open, write, sync, or rollover failure disables that file observer and reports a fixed diagnostic marker.

Keep new durable events typed and bounded. Never put secrets, API keys, raw error text, URLs, or response bodies in them. A selected game path can contain a Windows account name; review it before sharing a log publicly.

## Contract workflow

After editing either manifest, regenerate localization and frontend contracts, run their tests, and verify both a recognized error and the unknown-code fallback. Manifest generation should reject duplicate codes, unsupported actions, missing default copy, unsafe placeholder shapes, and frontend drift.

## Sources of truth

- [Desktop error manifest](../../data/contracts/desktop-command-errors.json)
- [Add-game warning manifest](../../data/contracts/add-game-warnings.json)
- [Rust command mapping](../../apps/desktop/src-tauri/src/commands/error/mapping.rs)
- [Frontend error presentation](../../apps/desktop/ui/src/shared/error-presentation/presenter.ts)
