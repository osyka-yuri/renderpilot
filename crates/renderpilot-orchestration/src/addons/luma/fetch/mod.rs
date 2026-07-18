//! Downloading and extracting a Luma Framework release asset.
//!
//! Unlike RenoDX's single add-on file, a Luma asset is a ZIP tree: a root
//! `.addon`, an optional root `nvngx_dlss.dll`, and a `Luma/` shader tree the
//! add-on reads at runtime. Nothing is hashed against the manifest (Luma
//! publishes no per-file checksums); instead every extracted file is
//! zip-slip-safe and size-capped, the root `.addon` and optional `nvngx_dlss.dll`
//! are PE-sanity-checked and architecture-checked against the resolved title
//! arch, and the raw ZIP's own digest is recorded for update *detection*. The
//! bundled `dxgi.dll` (Luma's own ReShade build) and any bundled `ReShade.ini`
//! are deliberately never extracted -- the shared ReShade host subsystem owns
//! that file, fetched separately (see [`prepare`] and
//! [`crate::addons::reshade`]).
//!
//! ## Pipeline stages
//!
//! ```text
//! types     -- LumaPayload / LumaPayloadFile
//! extract   -- ZIP allow-list + PE gates (no network)
//! download  -- HTTP -> LumaPayload
//! digest    -- recovery payload identity (memory / disk)
//! prepare   -- orchestrate payload + dgVoodoo + ReShade -> PreparedInstall
//! ```
//!
//! ## Dependency rules
//!
//! - `extract` must not use network or know about dgVoodoo/ReShade.
//! - `download` depends on `extract` + `types` only.
//! - `digest` depends on `types` (+ disk I/O for recovery).
//! - `prepare` is the only stage that composes external fetches.

pub(crate) mod digest;
pub(crate) mod download;
mod extract;
pub(crate) mod types;
