use renderpilot_domain::{InstalledAddon, TrackedSource, TrackedSourceRole};

use super::types::ReshadeChannel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChannelReadIssue {
    DuplicateHostSources,
}

pub(super) fn host_sources(record: &InstalledAddon) -> Vec<&TrackedSource> {
    record
        .tracked_sources()
        .iter()
        .filter(|source| source.role() == TrackedSourceRole::Host)
        .collect()
}

pub(super) fn single_host_source(
    record: &InstalledAddon,
) -> Result<Option<&TrackedSource>, ChannelReadIssue> {
    let sources = host_sources(record);
    match sources.as_slice() {
        [] => Ok(None),
        [source] => Ok(Some(*source)),
        _ => Err(ChannelReadIssue::DuplicateHostSources),
    }
}

pub(super) fn installed_channel(
    record: &InstalledAddon,
) -> Result<Option<ReshadeChannel>, ChannelReadIssue> {
    let Some(source) = single_host_source(record)? else {
        return Ok(None);
    };
    if let Some(channel) = source.channel() {
        return match channel {
            "stable" => Ok(Some(ReshadeChannel::Stable)),
            "nightly" => Ok(Some(ReshadeChannel::Nightly)),
            other => {
                log::warn!(
                    "RenoDX install record has unknown ReShade channel `{other}`; falling back to unknown"
                );
                Ok(None)
            }
        };
    }
    Ok(infer_legacy_channel_from_url(source.url()))
}

pub(super) fn infer_legacy_channel_from_url(url: &str) -> Option<ReshadeChannel> {
    // Reuse the net layer's URL parsing so reqwest stays encapsulated there; the
    // recorded host URLs are always HTTPS. This is a legacy-only fallback for
    // records written before the `channel` provenance tag existed.
    let parsed = crate::net::parse_https_url(url, "reshade host url").ok()?;
    match parsed.host_str()? {
        "reshade.me" => Some(ReshadeChannel::Stable),
        "nightly.link" => Some(ReshadeChannel::Nightly),
        _ => None,
    }
}

pub(super) fn with_host_channel(source: &TrackedSource, channel: ReshadeChannel) -> TrackedSource {
    TrackedSource::new(
        source.role(),
        source.url().to_owned(),
        source.etag().map(str::to_owned),
        source.digest().to_owned(),
    )
    .with_last_modified(source.last_modified().map(str::to_owned))
    .with_channel(channel.as_str())
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{AddonKind, GameId, PathRef};

    use super::*;

    fn record_with_sources(sources: Vec<TrackedSource>) -> InstalledAddon {
        let addon = PathRef::new(r"C:\Games\Test\renodx-test.addon64").expect("path");
        InstalledAddon::from_parts(
            GameId::new("steam:42").expect("id"),
            AddonKind::RenoDx,
            addon.clone(),
            None,
            vec![addon],
            Vec::new(),
            sources,
        )
        .expect("record")
    }

    fn host(url: &str) -> TrackedSource {
        TrackedSource::new(TrackedSourceRole::Host, url, None, "digest")
    }

    #[test]
    fn typed_channel_wins_over_url_classification() {
        let record = record_with_sources(vec![
            host("https://nightly.link/crosire/reshade/workflows/build/main/x64.zip")
                .with_channel("stable"),
        ]);

        assert_eq!(installed_channel(&record), Ok(Some(ReshadeChannel::Stable)));
    }

    #[test]
    fn unknown_channel_is_recoverable_unknown() {
        let record = record_with_sources(vec![
            host("https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe")
                .with_channel("canary"),
        ]);

        assert_eq!(installed_channel(&record), Ok(None));
    }

    #[test]
    fn legacy_single_host_source_falls_back_to_url() {
        let stable = record_with_sources(vec![host(
            "https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe",
        )]);
        let nightly = record_with_sources(vec![host(
            "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip",
        )]);

        assert_eq!(installed_channel(&stable), Ok(Some(ReshadeChannel::Stable)));
        assert_eq!(
            installed_channel(&nightly),
            Ok(Some(ReshadeChannel::Nightly))
        );
    }

    #[test]
    fn duplicate_host_sources_are_explicit_conflict() {
        let record = record_with_sources(vec![
            host("https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe"),
            host("https://nightly.link/crosire/reshade/workflows/build/main/x64.zip"),
        ]);

        assert_eq!(
            installed_channel(&record),
            Err(ChannelReadIssue::DuplicateHostSources)
        );
    }
}
