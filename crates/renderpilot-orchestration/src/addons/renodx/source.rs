//! Deriving upstream download sources for a RenoDX install.
//!
//! Add-ons are fetched live from upstream, so the URL is derived from a title's
//! slug, an engine-generic's canonical slug, or an explicit per-arch generic URL.
//! The add-on-enabled ReShade host can come from the manifest-current reshade.me
//! stable add-on installer or the nightly.link CI zip.

use renderpilot_domain::Architecture;

use super::types::{Generic, ReshadeChannel, ReshadeConfig};

/// GitHub Pages host serving the per-game RenoDX add-ons.
const RENODX_BASE: &str = "https://clshortfuse.github.io/renodx";

/// On-disk add-on file stem (`renodx-<slug>`). A manual generic install with no
/// catalogue slug falls back to `renodx-manual` so the path stays well-formed.
#[must_use]
pub(super) fn addon_file_stem(slug: &str) -> String {
    let base = if slug.is_empty() { "manual" } else { slug };
    format!("renodx-{base}")
}

/// On-disk add-on file name for a canonical slug and architecture.
#[must_use]
pub(super) fn addon_file_name(slug: &str, arch: Architecture) -> String {
    format!("{}.{}", addon_file_stem(slug), arch.addon_extension())
}

/// Canonical local slug for an engine-generic add-on. New manifests provide an
/// explicit slug; legacy explicit-URL generics fall back to the engine key.
#[must_use]
pub(super) fn generic_local_slug(generic: &Generic) -> &str {
    generic
        .slug
        .as_deref()
        .unwrap_or_else(|| generic.engine.as_str())
}

/// The upstream URL for a per-game add-on, derived from its slug.
#[must_use]
pub(super) fn addon_url(slug: &str, arch: Architecture) -> String {
    format!("{RENODX_BASE}/{}", addon_file_name(slug, arch))
}

/// The upstream URL for an engine-generic add-on: an explicit per-arch URL wins
/// when the generic is hosted elsewhere, else the canonical slug maps to the
/// default clshortfuse host.
#[must_use]
pub(super) fn generic_addon_url(generic: &Generic, arch: Architecture) -> Option<String> {
    let explicit = match arch {
        Architecture::X64 => generic.url64.clone(),
        Architecture::X86 => generic.url32.clone(),
    };
    explicit.or_else(|| generic.slug.as_deref().map(|slug| addon_url(slug, arch)))
}

/// The upstream URL for the DLSS-Fix companion add-on, derived from the `dlssfix`
/// slug. Architecture-specific (`renodx-dlssfix.addon64` / `.addon32`).
#[must_use]
pub(super) fn dlss_fix_url(arch: Architecture) -> String {
    addon_url("dlssfix", arch)
}

/// A concrete ReShade host source for a channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReshadeSource {
    pub channel: ReshadeChannel,
    pub url: String,
}

/// The add-on-enabled ReShade host archive URL for a channel and architecture.
#[must_use]
pub(super) fn reshade_source(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::renodx::types::{ReshadeNightly, ReshadeStable};

    #[test]
    fn addon_url_derives_from_slug_and_arch() {
        assert_eq!(
            addon_url("cp2077", Architecture::X64),
            "https://clshortfuse.github.io/renodx/renodx-cp2077.addon64"
        );
        assert_eq!(
            addon_url("oldgame", Architecture::X86),
            "https://clshortfuse.github.io/renodx/renodx-oldgame.addon32"
        );
    }

    #[test]
    fn addon_file_name_is_derived_only_from_canonical_slug_and_arch() {
        assert_eq!(
            addon_file_name("cp2077", Architecture::X64),
            "renodx-cp2077.addon64"
        );
        assert_eq!(
            addon_file_name("oldgame", Architecture::X86),
            "renodx-oldgame.addon32"
        );
        assert_eq!(
            addon_file_name("", Architecture::X64),
            "renodx-manual.addon64"
        );
    }

    #[test]
    fn generic_url_prefers_explicit_override_then_slug() {
        let explicit = Generic {
            engine: super::super::types::Engine::Unity,
            status: super::super::types::Status::Unknown,
            slug: Some("unityengine".to_owned()),
            url64: Some("https://github.com/x/y/renodx-unityengine.addon64".to_owned()),
            url32: Some("https://github.com/x/y/renodx-unityengine.addon32".to_owned()),
            label_key: None,
        };
        assert_eq!(
            generic_addon_url(&explicit, Architecture::X64).as_deref(),
            Some("https://github.com/x/y/renodx-unityengine.addon64")
        );
        assert_eq!(
            generic_addon_url(&explicit, Architecture::X86).as_deref(),
            Some("https://github.com/x/y/renodx-unityengine.addon32")
        );

        let slugged = Generic {
            engine: super::super::types::Engine::Unreal,
            status: super::super::types::Status::Unknown,
            slug: Some("_univ".to_owned()),
            url64: None,
            url32: None,
            label_key: None,
        };
        assert_eq!(
            generic_addon_url(&slugged, Architecture::X64).as_deref(),
            Some("https://clshortfuse.github.io/renodx/renodx-_univ.addon64")
        );
    }

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
}
