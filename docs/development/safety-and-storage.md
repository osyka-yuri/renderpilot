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

The portable runtime release contract declares the current schema and its released compatibility floor. Declared migration steps define the supported intermediate schemas. Unreleased gaps, schemas below the floor, and databases from a newer schema are rejected.

A current portable catalog is validated without a snapshot or mutation. An older supported catalog is changed only after the supervisor has committed a verified snapshot and issued an authenticated migration permit; the signed App generation then applies its complete schema chain in one transaction. An absent catalog, or an empty SQLite v0 catalog without user objects, is initialized only after durable activation commit. A v0 catalog containing user objects is malformed and is rejected.

The stable supervisor owns snapshots, journals, receipts, publication, selection, and recovery. The signed App generation owns schema inspection and its schema-specific migration chain. This separation lets a compatible installed supervisor run a later signed generation without granting either process the other's authority. Portable migration never falls back to rebuilding user data. General, nonportable storage may rebuild a malformed schema only after creating and validating an SQLite backup.

The current release contract targets schema v17. Its v16→v17 step replaces weak global scan caches with owner-scoped file observations and typed, fail-closed scan authority. Schema changes must update the shared release contract, migration steps, physical contract, repository behavior, and tests together.

The database holds catalog entities, scan state, operation and pending-mutation records, add-on capabilities and installations, cover metadata, and related settings. A scan persists its game, components, and artifacts in one transaction. SQLite runs in WAL mode, and typed rollback baselines use a private storage encoding behind repository contracts. Large library payloads and covers live in filesystem storage rather than SQLite.

Catalog scans always traverse each authoritative installation root so an unchanged launcher manifest cannot hide an external DLL replacement. On a supported local filesystem, a warm scan re-proves the owner-scoped strong identity without reading or hashing file contents; unsupported, remote, or discontinuous identity sources deliberately fall back to one stable full read and publish no reusable key. Automatic scans use at most four workers. Local library verification loads artifact observations with one batch query and publishes all successfully verified owner scopes in one transaction.

## Storage locations

An authenticated portable child installs its supervisor-derived `RuntimePathsV1` before any durable consumer starts; those paths are authoritative for the catalog, caches, covers, WebView2 profile, generations, and recovery state. Outside that boundary, application-root resolution checks `RENDERPILOT_APP_DIR`, Windows local application data, Windows roaming application data, then development-oriented XDG or home fallbacks on other systems. `RENDERPILOT_DB_PATH` independently overrides the installed or development database path.

Portable startup derives the data root beside the raw supervisor and passes the complete authenticated path object to the managed app. Environment overrides remain compatibility and development inputs, not portable path authority. The database is `catalog.db`; the active library catalog is under `libraries/v1`; archives and extracted artifacts are content-addressed beneath that tree; covers and the WebView2 profile also remain below the portable data root.

## Sources of truth

- [Catalog execution](../../crates/renderpilot-orchestration/src/catalog/execute/mod.rs)
- [Game mutation lock](../../crates/renderpilot-orchestration/src/game_mutation_lock.rs)
- [Baseline handling](../../crates/renderpilot-orchestration/src/coordinated_files/baseline.rs)
- [Portable runtime release contract](../../data/contracts/portable-runtime-release.json)
- [Storage contract projection](../../crates/renderpilot-storage-sqlite/build.rs)
- [Portable layout](../../crates/renderpilot-orchestration/src/portable.rs)
