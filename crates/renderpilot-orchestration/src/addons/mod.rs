//! Tool-agnostic framework for add-ons RenderPilot *introduces* into a game folder
//! (a proxy DLL plus a config, tracked for reversal and upstream updates), with each
//! tool a thin module over the shared mechanics.
//!
//! * [`engine`] applies a serializable [`engine::InstallPlan`] of file operations,
//!   each per its own backup policy (some back up a pre-existing file, some never
//!   do — see [`engine::FileOp`]), with ordered execution, reverse-order rollback,
//!   and a crash-safety sentinel — and reverses an install from the file lists it
//!   recorded.
//! * [`record`] maps an engine receipt into the persisted, reversible
//!   `InstalledAddon` (created/backed-up files + the upstream sources to track).
//! * [`update`] owns the generic update-verdict vocabulary (HEAD/ETag fast-path,
//!   digest compare, combine) every tool shares.
//! * [`renodx`] is the reference tool: it resolves a game, fetches the add-on and
//!   ReShade host, and builds the [`engine::InstallPlan`] the framework executes.
//!
//! A future sibling (e.g. `optiscaler`) adds its own resolver/fetch/plan-builder and
//! reuses `engine`/`record` (and the shared `net`/`cdn`/`fs` infrastructure) with no
//! changes to the framework.

pub mod engine;
mod ini;
pub mod record;
pub mod renodx;
pub mod update;
