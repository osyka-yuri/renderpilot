//! DLSS preset manifest loading and lookup.
//!
//! Loads compile-time bundled snapshots of the SR/FG/RR preset manifests
//! from `renderpilot-libraries` (see `bundled/`) and exposes a typed API
//! for the CLI orchestration layer. Runtime fetching from GitHub Pages
//! is a planned future enhancement; today the bundled copy is the
//! source of truth at runtime.

pub(crate) mod preset_manifest;
