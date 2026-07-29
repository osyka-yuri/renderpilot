#[cfg(windows)]
mod auto_orphans;

#[cfg(windows)]
pub(crate) use auto_orphans::prune_auto_scan_orphans;
