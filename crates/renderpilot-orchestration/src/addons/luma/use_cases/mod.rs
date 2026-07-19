/// Commands (state-changing operations).
pub mod commands;
/// Queries (read-only operations).
pub mod queries;
/// Shared pure policy: resolve live update target + host rewrite/check status.
mod update_target;
