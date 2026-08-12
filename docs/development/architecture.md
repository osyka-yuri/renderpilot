# Architecture

RenderPilot keeps policy and data contracts independent from delivery mechanisms. The desktop UI and CLI call application-facing services; orchestration coordinates filesystem, network, and storage adapters; domain types carry invariants without depending on Tauri or Svelte.

## Rust workspace

| Package | Responsibility |
| --- | --- |
| `renderpilot-domain` | IDs, versions, games, components, packages, receipts, rollback baselines, and add-on state |
| `renderpilot-application` | Use-case ports, repository traits, persistence records, and application errors |
| `renderpilot-detection` | Filesystem and PE inspection, component classification, normalization, and anti-cheat evidence |
| `renderpilot-storage-sqlite` | SQLite repositories, schema validation, backup, and linear migrations |
| `renderpilot-platform-windows` | Windows launcher and executable discovery plus platform integrations |
| `renderpilot-nvapi` | Narrow NVIDIA NVAPI boundary and driver-setting errors |
| `renderpilot-orchestration` | Catalogs, scans, downloads, mutations, add-ons, covers, recovery, and runtime composition |
| `renderpilot-api` | Stable application-facing API models and cover operations |
| `renderpilot-cli` | Advanced command-line adapter and text/JSON presentation |
| `renderpilot-desktop` | Tauri bootstrap, command boundary, Windows manifest selection, updater, portable supervisor, and frontend host |

Dependencies should point inward: adapters may depend on application and domain contracts, while domain code must not know about SQLite, HTTP, Tauri, or frontend representation. The domain crate keeps a small direct runtime dependency set: `serde`, `sha2`, and `ulid`. Public APIs belong in Rustdoc; this document describes ownership rather than every private module.

The `stable_enum!` macro keeps serialization, display, and parsing representations synchronized for wire-stable enums such as `Launcher`, `ComponentKind`, and `AddonKind`. `LibraryTechnology` uses equivalent explicit serialization names and is shared across persistence and presentation. Additions or representation changes require coordinated migration and contract review. Do not substitute similarly named view models, such as graphics presentation groupings, for that persisted identity.

## Desktop frontend

The Svelte 5 frontend follows Feature-Sliced Design:

| Layer | Purpose |
| --- | --- |
| `app` | Bootstrap, providers, routing, global styles, mocks, and composition |
| `pages` | Page-level orchestration for games, libraries, settings, and details |
| `widgets` | Reusable composed screen regions |
| `features` | User actions such as scans, mutations, cover sync, and add-on operations |
| `entities` | Game, library, add-on, operation, and settings models and UI |
| `shared` | IPC access, contracts, errors, localization, primitives, and utilities |

Import boundaries are checked by lint rules and bundle tests. Features should not bypass entity or shared contracts to reach page internals. The browser preview supplies mock commands at the same frontend boundary used by Tauri.

The implementation uses Svelte 5 runes, Tailwind CSS 4, bits-ui, Lucide icons, TanStack virtual and table primitives, and the Tauri API. Vitest covers frontend behavior and `svelte-check` validates component and TypeScript contracts.

## Principal data flows

**Scan:** launcher and manual sources produce candidate roots; platform discovery and detection inspect executable and library evidence; orchestration reconciles the result into repositories; API models cross IPC to entity stores and pages.

**Library mutation:** the UI requests candidates and a plan; orchestration resolves a catalog receipt, captures current file identity, and returns a reviewable operation; apply revalidates under a game lock, records durable state, changes files, and projects a sanitized result.

**Remote catalog:** orchestration fetches a host-pinned index and vendor snapshots, validates structure and content identities, commits a last-known-good local catalog, then downloads content-addressed payloads on demand.

**Errors:** internal errors retain diagnostic context in Rust; the desktop command boundary maps them to an allowlisted IPC shape; frontend presentation resolves user-facing severity, actions, and localized copy.

## Sources of truth

- [Workspace manifest](../../Cargo.toml)
- [Domain crate](../../crates/renderpilot-domain/src/lib.rs)
- [Orchestration crate](../../crates/renderpilot-orchestration/src/lib.rs)
- [Frontend source](../../apps/desktop/ui/src)
