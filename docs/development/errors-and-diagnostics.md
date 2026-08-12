# Errors and diagnostics

Desktop errors cross a trust boundary. Internal chains may contain paths, URLs, operating-system text, or implementation details that are useful in backend logs but inappropriate for a stable frontend contract.

## Error boundary

The effective mapping is `AppError` to `ServiceError` to `ApiError` to `CommandError`. The Tauri command boundary exposes only an allowlisted code and structured fields for the recognized failure. Severity, supported actions, and presentation metadata come from `data/contracts/desktop-command-errors.json`; the generated frontend projection is checked against that manifest and normalized once when a response enters the UI.

Raw error causes remain development-only and are not serialized across IPC. User-facing messages must be selected from a stable code plus sanitized parameters, not from `Display` output or arbitrary upstream text. A new desktop failure therefore needs coordinated Rust mapping, manifest entry, generated/frontend contract, localization, and tests.

## Add-game warnings

Non-fatal game inspection outcomes use the separate `data/contracts/add-game-warnings.json` manifest. Stable warning IDs let the backend communicate evidence without embedding UI prose. Their generated frontend bindings remain inside the scan feature, while desktop command-error bindings belong to the shared error layer. The frontend owns localized presentation and recognized actions. Do not reuse a warning as an error merely because both appear near the same scan flow.

## Diagnostic ownership

Technical logging has one owner: the backend records diagnostic error chains. The desktop command boundary registers a mapped command failure once. The frontend receives and presents the sanitized projection and must not duplicate the same raw technical event. This avoids three near-identical log records while preserving the most useful cause chain where it can be protected.

Use structured context such as operation ID, game ID, technology, phase, and safe state classification. Avoid secrets, API keys, full private paths, download query credentials, or unfiltered third-party response bodies. Frontend notifications should remain actionable without asking the user to interpret Rust or Windows error text.

## Contract workflow

After editing either manifest, regenerate localization and frontend contracts, run their tests, and verify both a recognized error and the unknown-code fallback. Manifest generation should reject duplicate codes, unsupported actions, missing default copy, unsafe placeholder shapes, and frontend drift.

## Sources of truth

- [Desktop error manifest](../../data/contracts/desktop-command-errors.json)
- [Add-game warning manifest](../../data/contracts/add-game-warnings.json)
- [Rust command mapping](../../apps/desktop/src-tauri/src/commands/error/mapping.rs)
- [Frontend error presentation](../../apps/desktop/ui/src/shared/error-presentation/presenter.ts)
