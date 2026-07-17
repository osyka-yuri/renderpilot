//! Shared ReShade host subsystem used by every add-on tool.
//!
//! An add-on tool (RenoDX, Luma) is a ReShade add-on that needs a ReShade build
//! with full add-on support present in the game folder. This subsystem owns the
//! tool-agnostic model of that host: detecting it ([`scan`]), deciding what to do
//! about it ([`host_policy`]), resolving and fetching the channel build
//! ([`source`], [`fetch`]), the `ReShade.ini` schema and merge ([`ini_schema`]),
//! reading the recorded channel ([`channel`]), the host source configuration
//! ([`types`]), the DirectX proxy decision ([`proxy`]), the public host DTOs
//! ([`dto`]), and the recorded host-source provenance ([`update`]). Each tool
//! layers only its own manifest-shaped resolution on top.

pub(crate) mod channel;
pub(crate) mod dto;
pub(crate) mod fetch;
pub(crate) mod host_policy;
pub(crate) mod ini_schema;
pub(crate) mod manifest;
pub(crate) mod manifest_store;
pub(crate) mod proxy;
pub(crate) mod report;
pub(crate) mod scan;
pub(crate) mod source;
pub(crate) mod split_install;
pub(crate) mod types;
pub(crate) mod update;

pub(crate) use split_install::InstallRoots;
pub(crate) use scan::same_path;
