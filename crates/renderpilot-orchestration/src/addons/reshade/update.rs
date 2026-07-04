//! Shared host-source provenance for update tracking.
//!
//! The per-tool `HostUpdateTarget` resolver stays with each tool (it is
//! manifest-typed), but building the recorded [`TrackedSource`] for a freshly
//! downloaded or reused ReShade host is identical across tools.

use renderpilot_domain::{TrackedSource, TrackedSourceRole};

use super::types::ReshadeChannel;

/// Builds the tracked ReShade host-binary source recorded after a host download,
/// stamping the channel that produced it when known.
pub(crate) fn host_binary_source(
    url: String,
    etag: Option<String>,
    digest: String,
    last_modified: Option<String>,
    channel: Option<ReshadeChannel>,
) -> TrackedSource {
    let mut source = TrackedSource::new(TrackedSourceRole::HostBinary, url, etag, digest)
        .with_last_modified(last_modified);
    if let Some(channel) = channel {
        source = source.with_channel(channel.as_str());
    }
    source
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_binary_source_sets_channel_when_provided() {
        let source = host_binary_source(
            "https://reshade.me/downloads/ReShade_Setup.exe".to_owned(),
            Some("etag".to_owned()),
            "digest".to_owned(),
            Some("Tue, 17 Jun 2026 09:00:00 GMT".to_owned()),
            Some(ReshadeChannel::Nightly),
        );

        assert_eq!(source.role(), TrackedSourceRole::HostBinary);
        assert_eq!(source.channel(), Some("nightly"));
        assert_eq!(source.digest(), "digest");
    }

    #[test]
    fn host_binary_source_omits_channel_when_not_provided() {
        let source = host_binary_source(
            "https://reshade.me/downloads/ReShade_Setup.exe".to_owned(),
            None,
            "digest".to_owned(),
            None,
            None,
        );

        assert_eq!(source.channel(), None);
    }
}
