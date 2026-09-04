//! Deriving upstream download sources for a RenoDX install.
//!
//! Add-ons are fetched live from upstream, so the URL is derived from a title's
//! slug, an engine-generic's canonical slug, or an explicit per-arch generic URL.
//! The add-on-enabled ReShade host source resolution lives in
//! [`crate::addons::reshade::source`]; callers use it directly from there.

use renderpilot_domain::Architecture;

use super::types::RenoDxGeneric;

/// Upstream host serving the per-game RenoDX add-ons.
const RENODX_BASE: &str = "https://github.com/clshortfuse/renodx/releases/download/snapshot";

/// Reserved slug for the DLSS-Fix companion. Both its canonical source URL and
/// on-disk file name derive from this single value, so it cannot become a main
/// RenoDX payload through manifest drift.
pub(super) const DLSS_FIX_SLUG: &str = "dlssfix";

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

/// Canonical DLSS-Fix companion file name for `arch`.
#[must_use]
pub(super) fn dlss_fix_file_name(arch: Architecture) -> String {
    addon_file_name(DLSS_FIX_SLUG, arch)
}

/// Whether a file name is a DLSS-Fix-shaped candidate. This deliberately
/// recognizes every extension after the reserved stem: legacy records with a
/// wrong architecture or malformed suffix still need to remain opaque to a
/// generic RenoDX mutation.
#[must_use]
pub(super) fn is_dlss_fix_candidate_file_name(file_name: &str) -> bool {
    let prefix = format!("{}.", addon_file_stem(DLSS_FIX_SLUG));
    file_name
        .get(..prefix.len())
        .is_some_and(|prefix_in_name| prefix_in_name.eq_ignore_ascii_case(&prefix))
}

/// Canonical local slug for an engine-generic add-on. New manifests provide an
/// explicit slug; legacy explicit-URL generics fall back to the engine key.
#[must_use]
pub(super) fn generic_local_slug(generic: &RenoDxGeneric) -> &str {
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
pub(super) fn generic_addon_url(generic: &RenoDxGeneric, arch: Architecture) -> Option<String> {
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
    addon_url(DLSS_FIX_SLUG, arch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addon_url_derives_from_slug_and_arch() {
        assert_eq!(
            addon_url("cp2077", Architecture::X64),
            "https://github.com/clshortfuse/renodx/releases/download/snapshot/renodx-cp2077.addon64"
        );
        assert_eq!(
            addon_url("oldgame", Architecture::X86),
            "https://github.com/clshortfuse/renodx/releases/download/snapshot/renodx-oldgame.addon32"
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
    fn dlss_fix_source_and_file_name_share_the_reserved_slug() {
        assert_eq!(
            dlss_fix_file_name(Architecture::X64),
            addon_file_name(DLSS_FIX_SLUG, Architecture::X64)
        );
        assert_eq!(
            dlss_fix_url(Architecture::X86),
            addon_url(DLSS_FIX_SLUG, Architecture::X86)
        );
        assert!(is_dlss_fix_candidate_file_name("RENODX-DLSSFIX.ADDON64"));
        assert!(is_dlss_fix_candidate_file_name("renodx-dlssfix.invalid"));
        assert!(!is_dlss_fix_candidate_file_name("renodx-dlssfixer.addon64"));
    }

    #[test]
    fn generic_url_prefers_explicit_override_then_slug() {
        let explicit = RenoDxGeneric {
            engine: super::super::types::Engine::Unity,
            status: super::super::types::Status::Unknown,
            slug: Some("unityengine".to_owned()),
            url64: Some("https://github.com/x/y/renodx-unityengine.addon64".to_owned()),
            url32: Some("https://github.com/x/y/renodx-unityengine.addon32".to_owned()),
            message: crate::addons::CatalogMessage::new(
                "renodx.generic.unity",
                "Generic Unity profile",
            ),
        };
        assert_eq!(
            generic_addon_url(&explicit, Architecture::X64).as_deref(),
            Some("https://github.com/x/y/renodx-unityengine.addon64")
        );
        assert_eq!(
            generic_addon_url(&explicit, Architecture::X86).as_deref(),
            Some("https://github.com/x/y/renodx-unityengine.addon32")
        );

        let slugged = RenoDxGeneric {
            engine: super::super::types::Engine::Unreal,
            status: super::super::types::Status::Unknown,
            slug: Some("_univ".to_owned()),
            url64: None,
            url32: None,
            message: crate::addons::CatalogMessage::new(
                "renodx.generic.universal",
                "Generic Unreal profile",
            ),
        };
        assert_eq!(
            generic_addon_url(&slugged, Architecture::X64).as_deref(),
            Some(
                "https://github.com/clshortfuse/renodx/releases/download/snapshot/renodx-_univ.addon64"
            )
        );
    }
}
