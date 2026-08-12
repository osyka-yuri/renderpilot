# Advanced CLI

`renderpilot-cli` is an advanced source-only adapter for inspection, scripting, and focused maintenance. Release drafts do not publish a standalone CLI binary. Build or run it from the same source revision as the behavior and output contract you depend on.

```powershell
cargo run -p renderpilot-cli -- --help
```

## Commands

| Command | Purpose |
| --- | --- |
| `add-game` | Inspect and add a game root and selected executable |
| `list-artifacts` | List detected or stored rendering artifacts |
| `list-operations` | Read completed operation history |
| `candidates` | List compatible catalog choices for a target |
| `plan-swap` | Produce a reviewable library replacement plan |
| `apply` | Apply a fresh planned replacement |
| `plan-rollback` | Produce a rollback preview with affected paths |
| `rollback` | Restore the verified original baseline |
| `renodx status`, `uninstall` | Inspect or remove a managed RenoDX installation |
| `renodx check-update`, `check-updates` | Check one or all supported RenoDX installations |
| `luma status`, `uninstall` | Inspect or remove a managed Luma installation |
| `luma check-update [--deep]`, `check-updates` | Check one or all supported Luma installations |
| `--version`, `--help` | Print version or the complete command summary |

Use the top-level `--help` output from the exact source revision for required IDs, paths, confirmation values, and cursors. Subcommands do not currently expose separate help pages. Mutating commands preserve the same planning, current-state verification, locking, baseline, and recovery rules as the desktop application; the CLI is not a bypass for unsupported targets.

## JSON output

Operational commands return JSON on stdout; help and version output are plain text. A `plan-swap` result includes blockers, warnings, the complete file mutation list, an optional D3D12 executable action, and a fresh `confirmation_token`. If the action requires confirmation, pass that token unchanged to `apply --confirmation-token`; apply rebuilds the current preflight and rejects a token whose bound state changed. `plan-rollback` provides the equivalent affected-file preview for managed rollback.

Treat stderr as diagnostics. For example, capture and inspect a command at the current revision:

```powershell
$result = cargo run -q -p renderpilot-cli -- list-artifacts |
  ConvertFrom-Json
$result
```

Field names, enum values, pagination shapes, and error projections form a contract for the specific RenderPilot release that emitted them. They are not guaranteed to remain compatible across releases. Pin automation to a RenderPilot source revision or release, validate the command result, tolerate additive fields where practical, and review changelog and CLI tests before upgrading.

Do not parse human-readable tables. Do not expose full CLI output publicly without checking paths and diagnostic context for private information. A zero exit code indicates command success; scripts must still interpret domain results such as an empty candidate list or a state that needs confirmation.

## Sources of truth

- [CLI arguments](../../crates/renderpilot-cli/src/args/command.rs)
- [JSON renderer](../../crates/renderpilot-cli/src/output/json.rs)
- [CLI command tests](../../crates/renderpilot-cli/src/commands/tests/mod.rs)
- [CLI package manifest](../../crates/renderpilot-cli/Cargo.toml)
