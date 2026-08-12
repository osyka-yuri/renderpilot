# Mutation safety and storage

Filesystem mutation is a transaction-like workflow across a filesystem and SQLite, not a single file copy. The maintained contract separates reviewable planning from guarded application and keeps enough durable evidence to recover supported interrupted states.

## Preflight and apply

A plan records the selected game, component, targets, observed hashes and metadata, source package receipt, and any state-bound confirmation. Apply acquires the game/path mutation lock and repeats source and target checks. If the executable, DLL, selected SDK state, package, or confirmation context no longer matches, the operation is rejected and must be planned again.

Locks cover all mutation paths that can overlap, including libraries, coordinated D3D12 files, and managed add-on artifacts. New mutation features must declare their targets before work begins and must not introduce an uncoordinated write path.

## Baselines and rollback

The first managed change captures the original file as a `.bak` baseline. That first baseline is not replaced by later updates or downgrades. Its SHA-256 identity is checked before restoration. Rollback has its own preflight and enumerates every affected path, including coordinated files such as a D3D12 library and executable state.

D3D12 operations keep `D3D12Core.dll` and the selected executable's `D3D12SDKVersion` consistent. Planning classifies unchanged, managed-patch, original-restore, ambiguous, and repair states. The first managed patch and a user-selected restore require fresh state-bound confirmation. Developer Mode requirements are surfaced before apply.

## Journal and recovery

Pending file mutations are recorded durably in SQLite around filesystem work. Startup recovery reconciles the record with the current and staged files and completes or reverses only recognizable states. Unknown or conflicting states are reported for manual handling.

The completed operation journal is best-effort, informational history. Failure to append history after a successful filesystem result must not turn a completed change into a destructive retry. Correctness comes from current hashes, verified baselines, pending-mutation state, and package receipts rather than from treating history as an undo log.

## SQLite and migrations

The current schema version is 16. Migrations are linear and validated. Portable startup accepts the released 1.x schema boundaries v4 and v8 through v16, applies the complete declared chain in one transaction after a verified snapshot, and never falls back to rebuilding user data. Unreleased gaps, older schemas, and databases from a newer schema are rejected. Before a general-storage migration or required rebuild, storage uses SQLite's online backup API and validates the backup. Schema changes must update the declared version, migration step, physical contract, repository behavior, and tests together.

The database holds catalog entities, scan state, operation and pending-mutation records, add-on capabilities and installations, cover metadata, and related settings. A scan persists its game, components, and artifacts in one transaction. SQLite runs in WAL mode, and typed rollback baselines use a private storage encoding behind repository contracts. Large library payloads and covers live in filesystem storage rather than SQLite.

## Storage locations

An authenticated portable child installs its supervisor-derived `RuntimePathsV1` before any durable consumer starts; those paths are authoritative for the catalog, caches, covers, WebView2 profile, generations, and recovery state. Outside that boundary, application-root resolution checks `RENDERPILOT_APP_DIR`, Windows local application data, Windows roaming application data, then development-oriented XDG or home fallbacks on other systems. `RENDERPILOT_DB_PATH` independently overrides the installed or development database path.

Portable startup derives the data root beside the raw supervisor and passes the complete authenticated path object to the managed app. Environment overrides remain compatibility and development inputs, not portable path authority. The database is `catalog.db`; the active library catalog is under `libraries/v1`; archives and extracted artifacts are content-addressed beneath that tree; covers and the WebView2 profile also remain below the portable data root.

## Sources of truth

- [Catalog execution](../../crates/renderpilot-orchestration/src/catalog/execute/mod.rs)
- [Game mutation lock](../../crates/renderpilot-orchestration/src/game_mutation_lock.rs)
- [Baseline handling](../../crates/renderpilot-orchestration/src/coordinated_files/baseline.rs)
- [Schema version](../../crates/renderpilot-storage-sqlite/src/schema/version.rs)
- [Portable layout](../../crates/renderpilot-orchestration/src/portable.rs)
