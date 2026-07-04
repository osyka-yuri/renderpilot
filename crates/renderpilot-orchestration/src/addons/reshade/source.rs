//! Resolving the add-on-enabled ReShade host archive URL for a channel + arch.

use renderpilot_domain::Architecture;

use super::types::{ReshadeChannel, ReshadeConfig};
use crate::ServiceError;

/// A concrete ReShade host source for a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReshadeSource {
    pub(crate) channel: ReshadeChannel,
    pub(crate) url: String,
}

/// The add-on-enabled ReShade host archive URL for a channel and architecture.
#[must_use]
pub(crate) fn reshade_source(
    config: &ReshadeConfig,
    channel: ReshadeChannel,
    arch: Architecture,
) -> Option<ReshadeSource> {
    match channel {
        ReshadeChannel::Stable => config.stable.as_ref().map(|stable| ReshadeSource {
            channel,
            url: stable.url.clone(),
        }),
        ReshadeChannel::Nightly => Some(ReshadeSource {
            channel,
            url: match arch {
                Architecture::X64 => config.nightly.url64.clone(),
                Architecture::X86 => config.nightly.url32.clone(),
            },
        }),
    }
}

/// Like [`reshade_source`], but rejects a channel absent from the manifest
/// with the shared "channel not available" error instead of returning `None`.
pub(crate) fn require_reshade_source(
    config: &ReshadeConfig,
    channel: ReshadeChannel,
    arch: Architecture,
) -> Result<ReshadeSource, ServiceError> {
    reshade_source(config, channel, arch).ok_or_else(|| channel_unavailable(channel))
}

/// The manifest has no ReShade source for the requested channel. Shared by every
/// flow that resolves a channel to a downloadable source.
pub(crate) fn channel_unavailable(channel: ReshadeChannel) -> ServiceError {
    ServiceError::InvalidInput(format!(
        "ReShade channel `{}` is not available",
        channel.as_str()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::reshade::types::{ReshadeNightly, ReshadeStable};

    #[test]
    fn stable_source_is_manifest_current_locator() {
        let config = ReshadeConfig {
            stable: Some(ReshadeStable {
                url: "https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe".to_owned(),
            }),
            nightly: ReshadeNightly {
                url64: "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip"
                    .to_owned(),
                url32: "https://nightly.link/crosire/reshade/workflows/build/main/x86.zip"
                    .to_owned(),
            },
        };

        assert_eq!(
            reshade_source(&config, ReshadeChannel::Stable, Architecture::X64)
                .map(|source| source.url),
            Some("https://reshade.me/downloads/ReShade_Setup_6.7.3_Addon.exe".to_owned())
        );

        let without_stable = ReshadeConfig {
            stable: None,
            nightly: config.nightly,
        };
        assert_eq!(
            without_stable.effective_install_channel(ReshadeChannel::Stable),
            ReshadeChannel::Nightly
        );
        assert!(
            reshade_source(&without_stable, ReshadeChannel::Stable, Architecture::X64).is_none()
        );
    }

    #[test]
    fn require_reshade_source_rejects_absent_channel_with_shared_message() {
        let config = ReshadeConfig {
            stable: None,
            nightly: ReshadeNightly {
                url64: "https://nightly.link/crosire/reshade/workflows/build/main/x64.zip"
                    .to_owned(),
                url32: "https://nightly.link/crosire/reshade/workflows/build/main/x86.zip"
                    .to_owned(),
            },
        };

        let error = require_reshade_source(&config, ReshadeChannel::Stable, Architecture::X64)
            .expect_err("stable channel is absent from the manifest");

        match error {
            crate::ServiceError::InvalidInput(message) => {
                assert_eq!(message, "ReShade channel `stable` is not available");
            }
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }
}
