//! Deriving upstream download sources for a RenoDX install.
//!
//! Add-ons are fetched live from upstream, so the URL is *derived* from a title's
//! slug (or an engine-generic's slug/explicit URL) rather than pinned in the
//! manifest. The add-on-enabled ReShade host is the nightly.link CI zip (a plain
//! zip); the reshade.me "stable" installer is an NSIS archive that cannot be
//! extracted without bundling 7-Zip, so it is intentionally not supported.

use renderpilot_domain::Architecture;

use super::types::{Generic, ReshadeConfig};

/// GitHub Pages host serving the per-game RenoDX add-ons.
const RENODX_BASE: &str = "https://clshortfuse.github.io/renodx";

/// The upstream URL for a per-game add-on, derived from its slug.
#[must_use]
pub(super) fn addon_url(slug: &str, arch: Architecture) -> String {
    format!("{RENODX_BASE}/renodx-{slug}.{}", arch.addon_extension())
}

/// The upstream URL for an engine-generic add-on: an explicit per-arch URL when
/// the generic is hosted elsewhere, else a clshortfuse slug.
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

/// The nightly add-on-enabled ReShade host zip URL for an architecture.
#[must_use]
pub(super) fn reshade_nightly_url(config: &ReshadeConfig, arch: Architecture) -> String {
    match arch {
        Architecture::X64 => config.nightly.url64.clone(),
        Architecture::X86 => config.nightly.url32.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn generic_url_prefers_explicit_then_slug() {
        let explicit = Generic {
            engine: super::super::types::Engine::Unity,
            slug: None,
            url64: Some("https://github.com/x/y/renodx-unityengine.addon64".to_owned()),
            url32: None,
            label_key: None,
        };
        assert_eq!(
            generic_addon_url(&explicit, Architecture::X64).as_deref(),
            Some("https://github.com/x/y/renodx-unityengine.addon64")
        );
        // No explicit url32 → falls through to None (no slug either).
        assert_eq!(generic_addon_url(&explicit, Architecture::X86), None);

        let slugged = Generic {
            engine: super::super::types::Engine::Unreal,
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
}
