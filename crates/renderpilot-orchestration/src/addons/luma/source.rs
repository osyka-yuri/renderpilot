//! Deriving the Luma Framework release download source.
//!
//! Luma has no per-game upstream repository: every asset is a file on the
//! single rolling GitHub Release of `Filoppi/Luma-Framework`, fetched through
//! the `releases/latest/download/<asset>` alias so the client never needs to
//! know the current tag. The tag redirect target *does* encode a build number
//! (`releases/download/latest-<n>/<asset>`), which [`parse_build_number`]
//! recovers from the final response URL for the "Build N" label shown in the UI
//! (see [`build_label`]) — the only versioning signal Luma offers, since its
//! `.addon` PE resources are unset (verified: `0.0.0.0`) and it publishes no
//! per-file checksums.
//!
//! The add-on-enabled ReShade host source resolution is shared
//! ([`crate::addons::reshade::source`]); Luma always requests the nightly
//! channel.

use reqwest::Url;

/// GitHub Releases "latest" alias base for the Luma Framework repository.
const LUMA_RELEASE_BASE: &str =
    "https://github.com/Filoppi/Luma-Framework/releases/latest/download";

/// The stable "always current" download URL for a release `asset` file name.
///
/// Path-segment encoding keeps the URL valid even if asset stems ever include
/// reserved characters; current manifest charset already produces the same
/// bytes as a plain join for the allowed stems.
#[must_use]
pub(super) fn asset_url(asset: &str) -> String {
    // Trailing slash is required: without it `Url::join` replaces the final
    // `download` path segment instead of appending under it.
    let base = format!("{LUMA_RELEASE_BASE}/");
    match Url::parse(&base).and_then(|base| base.join(asset)) {
        Ok(url) => url.to_string(),
        Err(_) => format!("{LUMA_RELEASE_BASE}/{asset}"),
    }
}

/// Recovers the rolling release's build number from the final URL a
/// `releases/latest/download/<asset>` request redirected to
/// (`releases/download/latest-<n>/<asset>`), if present. `None` when the
/// redirect target does not carry a recognizable `latest-<digits>` tag segment
/// — the caller records the install without a build-number label rather than
/// failing (see the module's `LumaInstallState::version` doc).
#[must_use]
pub(super) fn parse_build_number(final_url: &Url) -> Option<u64> {
    final_url
        .path_segments()?
        .find_map(|segment| segment.strip_prefix("latest-")?.parse::<u64>().ok())
}

/// Formats a build number recovered by [`parse_build_number`] into the UI-facing
/// version label ("Build N") recorded as a Luma install's `addon_version`.
#[must_use]
pub(crate) fn build_label(build_number: u64) -> String {
    format!("Build {build_number}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_url_appends_to_the_latest_alias_base() {
        assert_eq!(
            asset_url("Luma-Dishonored_2.zip"),
            "https://github.com/Filoppi/Luma-Framework/releases/latest/download/Luma-Dishonored_2.zip"
        );
    }

    #[test]
    fn asset_url_encodes_reserved_characters_in_the_asset_segment() {
        let url = asset_url("Luma Game.zip");
        assert!(
            url.contains("Luma%20Game.zip") || url.ends_with("Luma%20Game.zip"),
            "expected percent-encoded space in asset segment, got {url}"
        );
        assert!(url.starts_with(LUMA_RELEASE_BASE));
    }

    #[test]
    fn parse_build_number_reads_the_tag_path_segment() {
        let url = Url::parse(
            "https://github.com/Filoppi/Luma-Framework/releases/download/latest-515/Luma-Dishonored_2.zip",
        )
        .expect("valid url");
        assert_eq!(parse_build_number(&url), Some(515));
    }

    #[test]
    fn parse_build_number_is_none_without_a_latest_tag_segment() {
        let url = Url::parse(
            "https://github.com/Filoppi/Luma-Framework/releases/latest/download/Luma-Dishonored_2.zip",
        )
        .expect("valid url");
        assert_eq!(parse_build_number(&url), None);
    }

    #[test]
    fn parse_build_number_rejects_a_non_numeric_suffix() {
        let url = Url::parse(
            "https://github.com/Filoppi/Luma-Framework/releases/download/latest-abc/Luma-Dishonored_2.zip",
        )
        .expect("valid url");
        assert_eq!(parse_build_number(&url), None);
    }

    #[test]
    fn build_label_formats_the_build_number() {
        assert_eq!(build_label(515), "Build 515");
    }
}
