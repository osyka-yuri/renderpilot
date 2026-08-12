# Contributing to RenderPilot

Thank you for improving RenderPilot. Keep changes focused, explain user-visible behavior, and preserve the safety boundaries around game-file mutations.

## Before coding

1. Read the [development setup](docs/development/setup.md) and use the pinned Rust, Node.js, and pnpm versions.
2. Review the [architecture](docs/development/architecture.md) before introducing a dependency or moving responsibility between crates or frontend layers.
3. For storage, catalog, IPC, localization, or filesystem changes, follow the relevant contract document in the [developer guide](docs/README.md#developer-guide).
4. Prefer an issue before a large behavioral or architectural change. Do not include copyrighted game files, private API keys, or third-party binaries in a contribution.

## Quality gates

Run the checks that match your change. Before opening a pull request, the expected complete set is documented in [Quality and release](docs/development/quality-and-release.md#local-quality-gates). It covers Rust formatting, build, Clippy, tests and documentation, plus frontend formatting, linting, tests, build, and documentation checks.

Add focused tests for changed contracts and failure cases. Update user documentation when behavior changes and developer documentation when a maintained boundary changes. Public Rust APIs need Rustdoc; private implementation details belong in code rather than duplicated prose.

## Pull requests

Describe the problem, the chosen behavior, safety implications, and verification performed. Keep generated files synchronized with their declared sources. A passing automated localization review does not substitute for native-language approval.

## Sources of truth

- [Development documentation](docs/README.md#developer-guide)
- [Quality workflow](.github/workflows/quality.yml)
- [Workspace manifest](Cargo.toml)
