//! Record projection for a committed RenoDX channel switch.

use renderpilot_domain::{InstalledAddon, TrackedSource, TrackedSourceRole};

use crate::ServiceError;
use crate::addons::engine::InstallReceipt;
use crate::addons::renodx::tracking;
use crate::addons::reshade::types::ReshadeChannel;

pub(super) fn replace_or_append_host_source(
    record: &InstalledAddon,
    new_source: TrackedSource,
) -> Vec<TrackedSource> {
    let mut sources = record.tracked_sources().to_vec();
    let mut replaced = false;
    for entry in &mut sources {
        if entry.role() == TrackedSourceRole::HostBinary {
            *entry = new_source.clone();
            replaced = true;
        }
    }
    if !replaced {
        sources.push(new_source);
    }
    sources
}

pub(super) fn rebuild_proxy_switch_record(
    record: &InstalledAddon,
    new_source: TrackedSource,
    receipt: Option<&InstallReceipt>,
    target_channel: ReshadeChannel,
) -> Result<InstalledAddon, ServiceError> {
    tracking::rebuild_with_sources_and_receipt(
        record,
        replace_or_append_host_source(record, new_source),
        receipt,
        "RenoDX channel switch",
    )
    .map(|updated| updated.with_reshade_channel(target_channel.as_str()))
}
