# RenderPilot

RenderPilot is currently a minimal Rust workspace scaffold.

The workspace keeps the domain and application crates independent from UI,
SQLite implementation details, and Windows API calls. Platform and storage
crates are present as future adapter boundaries only.

## Commands

```powershell
cargo check --workspace
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p renderpilot-cli -- scan-folder "D:\Games\SomeGame"
```

The CI workflow runs the stricter gate variant:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Crates

| Crate | Responsibility |
| --- | --- |
| `renderpilot-domain` | Pure domain types and value objects. |
| `renderpilot-application` | Application-facing metadata and future use-case boundary. |
| `renderpilot-detection` | Future detection pipeline boundary. |
| `renderpilot-storage-sqlite` | Future SQLite storage adapter boundary. |
| `renderpilot-platform-windows` | Future Windows platform adapter boundary. |
| `renderpilot-cli` | CLI entry point over the application crate. |
