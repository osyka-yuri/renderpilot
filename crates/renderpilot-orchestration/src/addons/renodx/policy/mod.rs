//! RenoDX-specific title compatibility policy.
//!
//! DirectX proxy decisions (`HostKind`, `host_decision`, …) live in
//! [`crate::addons::reshade::proxy`] and are imported from there directly.

pub mod compatibility;

pub use compatibility::check_title_compatibility;
