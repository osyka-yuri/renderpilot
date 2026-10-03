//! Canonical Xiph semantic members and reviewed DLL basenames.

/// DLL basename convention used by one Xiph member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XiphNameStyle {
    /// `ogg.dll` / `vorbis*.dll`.
    Plain,
    /// `libogg.dll` / `libvorbis*.dll`.
    Lib,
    /// Libtool ABI-major names such as `libvorbisfile-3.dll`.
    AbiMajor,
    /// Reviewed Unreal package names such as `libvorbis_64.dll`.
    ///
    /// Runtime parsing retains `Lib` with an opaque `_64` suffix.
    Unreal,
}

impl XiphNameStyle {
    /// Every reviewed basename convention in stable order.
    pub const ALL: [Self; 4] = [Self::Plain, Self::Lib, Self::AbiMajor, Self::Unreal];
}

/// Runtime base styles; Unreal names parse as `Lib` plus `_64`.
const RUNTIME_NAME_STYLES: [XiphNameStyle; 3] = [
    XiphNameStyle::Plain,
    XiphNameStyle::Lib,
    XiphNameStyle::AbiMajor,
];

/// Aggregate naming profile of a validated deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XiphNamingProfile {
    /// Every member uses the plain convention.
    Plain,
    /// Every member uses the unsuffixed `lib` convention.
    Lib,
    /// Every member uses the libtool ABI-major convention.
    AbiMajor,
    /// Every member uses [`XiphNameStyle::Unreal`] catalog names.
    Unreal,
    /// The runtime profile is unavailable or the deployment mixes conventions.
    Mixed,
}

impl XiphNamingProfile {
    /// Stable slug used by component discriminators and catalog variants.
    #[must_use]
    pub const fn as_slug(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Lib => "lib",
            Self::AbiMajor => "abi",
            Self::Unreal => "unreal",
            Self::Mixed => "mixed",
        }
    }

    /// Reduces member styles to one deployment profile.
    #[must_use]
    pub fn from_styles(styles: impl IntoIterator<Item = XiphNameStyle>) -> Self {
        let mut styles = styles.into_iter();
        let Some(first) = styles.next() else {
            return Self::Mixed;
        };
        if styles.any(|style| style != first) {
            return Self::Mixed;
        }
        match first {
            XiphNameStyle::Plain => Self::Plain,
            XiphNameStyle::Lib => Self::Lib,
            XiphNameStyle::AbiMajor => Self::AbiMajor,
            XiphNameStyle::Unreal => Self::Unreal,
        }
    }
}

/// Semantic member of the Xiph codec stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum XiphMember {
    /// High-level `ov_*` file API.
    VorbisFile,
    /// Convenience encoder-configuration API.
    VorbisEnc,
    /// Core Vorbis codec API.
    Vorbis,
    /// Ogg container API.
    Ogg,
}

impl XiphMember {
    /// Every semantic member in primary-first order.
    pub const ALL: [Self; 4] = [Self::VorbisFile, Self::VorbisEnc, Self::Vorbis, Self::Ogg];

    /// Stable catalog component key.
    #[must_use]
    pub const fn as_slug(self) -> &'static str {
        match self {
            Self::VorbisFile => "vorbisfile",
            Self::VorbisEnc => "vorbisenc",
            Self::Vorbis => "vorbis",
            Self::Ogg => "ogg",
        }
    }

    /// Stable primary-member preference used by grouping and projections.
    #[must_use]
    pub const fn primary_rank(self) -> u8 {
        match self {
            Self::Vorbis => 0,
            Self::VorbisFile => 1,
            Self::VorbisEnc => 2,
            Self::Ogg => 3,
        }
    }
}

/// Classifies one reviewed canonical Xiph DLL basename case-insensitively.
///
/// Accepts exact catalog names, including Unreal `_64` names. Use
/// [`parse_runtime_file_name`] for arbitrary vendor suffixes and runtime identity.
#[must_use]
pub fn classify_canonical_file_name(name: &str) -> Option<(XiphMember, XiphNameStyle)> {
    for member in XiphMember::ALL {
        for style in XiphNameStyle::ALL {
            if name.eq_ignore_ascii_case(file_name(member, style)) {
                return Some((member, style));
            }
        }
    }
    None
}

/// Compatibility wrapper for code that historically classified canonical names
/// with [`classify_file_name`]. It is deliberately canonical-only.
#[must_use]
pub fn classify_file_name(name: &str) -> Option<(XiphMember, XiphNameStyle)> {
    classify_canonical_file_name(name)
}

/// A runtime Xiph DLL basename, optionally carrying an opaque vendor suffix.
///
/// The suffix is not interpreted: strings such as `_vs2008_x64_rwdi` and
/// `_vs2010_x64_rwdi` are distinct values and are retained only so a loader
/// alias can be preserved exactly. All exposed names are normalized to ASCII
/// lowercase.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct XiphRuntimeFileName {
    member: XiphMember,
    style: XiphNameStyle,
    normalized_name: String,
    suffix: Option<String>,
}

impl XiphRuntimeFileName {
    /// Returns the semantic Xiph member represented by this basename.
    #[must_use]
    pub const fn member(&self) -> XiphMember {
        self.member
    }

    /// Returns the canonical naming style before any vendor suffix.
    #[must_use]
    pub const fn base_style(&self) -> XiphNameStyle {
        self.style
    }

    /// Alias for [`Self::base_style`].
    #[must_use]
    pub const fn style(&self) -> XiphNameStyle {
        self.style
    }

    /// Returns the normalized complete runtime basename.
    #[must_use]
    pub fn normalized_name(&self) -> &str {
        &self.normalized_name
    }

    /// Returns the normalized opaque vendor suffix, including its leading `_`.
    #[must_use]
    pub fn vendor_suffix(&self) -> Option<&str> {
        self.suffix.as_deref()
    }

    /// Returns whether this basename uses a vendor suffix.
    #[must_use]
    pub const fn is_vendor(&self) -> bool {
        self.suffix.is_some()
    }

    /// Returns whether a catalog name satisfies this runtime member's naming contract.
    ///
    /// Canonical names require an exact match. Vendor aliases accept plain names
    /// for the same member or Unreal names matching the installed basename.
    #[must_use]
    pub fn accepts_candidate_file_name(&self, candidate_name: &str) -> bool {
        let Some((member, style)) = classify_canonical_file_name(candidate_name) else {
            return false;
        };
        if member != self.member {
            return false;
        }

        if self.is_vendor() {
            style == XiphNameStyle::Plain
                || (style == XiphNameStyle::Unreal
                    && candidate_name.eq_ignore_ascii_case(&self.normalized_name))
        } else {
            candidate_name.eq_ignore_ascii_case(&self.normalized_name)
        }
    }
}

/// A malformed candidate runtime Xiph basename.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XiphRuntimeFileNameError {
    /// Runtime names are basenames, not paths.
    PathLike,
    /// Runtime names and suffixes are restricted to ASCII.
    NonAscii,
    /// The candidate has no exact `.dll` extension.
    MissingDllExtension,
    /// A suffix was present but did not contain valid `_token` segments.
    InvalidVendorSuffix,
    /// A suffix exceeded the bounded grammar.
    VendorSuffixTooLong,
}

impl std::fmt::Display for XiphRuntimeFileNameError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::PathLike => "Xiph runtime names must be basenames",
            Self::NonAscii => "Xiph runtime names must contain ASCII only",
            Self::MissingDllExtension => "Xiph runtime names must end in .dll",
            Self::InvalidVendorSuffix => "Xiph vendor suffix is malformed",
            Self::VendorSuffixTooLong => "Xiph vendor suffix is too long",
        })
    }
}

impl std::error::Error for XiphRuntimeFileNameError {}

/// Parses a canonical or vendor-suffixed Xiph runtime basename.
///
/// `Ok(None)` means that the basename is unrelated to the reviewed Xiph
/// family. A name that looks like an Xiph basename but violates the runtime
/// grammar returns `Err`, allowing PE import validation to fail closed. An
/// exact Unreal `_64` package spelling remains the `Lib` runtime style with
/// the opaque `_64` vendor suffix.
pub fn parse_runtime_file_name(
    name: &str,
) -> Result<Option<XiphRuntimeFileName>, XiphRuntimeFileNameError> {
    if name.contains('/') || name.contains('\\') || name.contains(':') {
        return Err(XiphRuntimeFileNameError::PathLike);
    }
    if !name.is_ascii() {
        return Err(XiphRuntimeFileNameError::NonAscii);
    }

    let normalized = name.to_ascii_lowercase();
    if let Some((member, style)) = classify_runtime_canonical_file_name(&normalized) {
        return Ok(Some(XiphRuntimeFileName {
            member,
            style,
            normalized_name: normalized,
            suffix: None,
        }));
    }

    let Some(stem_end) = normalized.len().checked_sub(4) else {
        return if looks_like_xiph_name(&normalized) {
            Err(XiphRuntimeFileNameError::MissingDllExtension)
        } else {
            Ok(None)
        };
    };
    if !normalized.ends_with(".dll") {
        return if looks_like_xiph_name(&normalized)
            || has_malformed_canonical_extension(&normalized)
        {
            Err(XiphRuntimeFileNameError::MissingDllExtension)
        } else {
            Ok(None)
        };
    }
    let stem = &normalized[..stem_end];

    let Some((member, style, base)) = runtime_base_for_stem(stem) else {
        return if looks_like_xiph_name(stem) {
            Err(XiphRuntimeFileNameError::InvalidVendorSuffix)
        } else {
            Ok(None)
        };
    };

    if stem == base {
        return Ok(Some(XiphRuntimeFileName {
            member,
            style,
            normalized_name: normalized,
            suffix: None,
        }));
    }

    let suffix = &stem[base.len()..];
    validate_vendor_suffix(suffix)?;
    let suffix = suffix.to_owned();
    Ok(Some(XiphRuntimeFileName {
        member,
        style,
        normalized_name: normalized,
        suffix: Some(suffix),
    }))
}

fn validate_vendor_suffix(suffix: &str) -> Result<(), XiphRuntimeFileNameError> {
    if suffix.is_empty() || suffix.len() > 128 || !suffix.starts_with('_') {
        return Err(if suffix.len() > 128 {
            XiphRuntimeFileNameError::VendorSuffixTooLong
        } else {
            XiphRuntimeFileNameError::InvalidVendorSuffix
        });
    }
    let mut segment_count = 0;
    for segment in suffix[1..].split('_') {
        segment_count += 1;
        if segment_count > 8
            || !(1..=32).contains(&segment.len())
            || !segment.bytes().enumerate().all(|(index, byte)| {
                if index == 0 || index + 1 == segment.len() {
                    byte.is_ascii_alphanumeric()
                } else {
                    byte.is_ascii_alphanumeric() || byte == b'-'
                }
            })
        {
            return Err(XiphRuntimeFileNameError::InvalidVendorSuffix);
        }
    }
    Ok(())
}

fn looks_like_xiph_name(name: &str) -> bool {
    XiphMember::ALL.into_iter().any(|member| {
        XiphNameStyle::ALL.into_iter().any(|style| {
            looks_like_canonical_or_vendor_stem(name, member, style)
                || looks_like_abi_major_stem(name, member, style)
        })
    })
}

/// Classifies runtime base names without catalog-only Unreal spellings.
fn classify_runtime_canonical_file_name(name: &str) -> Option<(XiphMember, XiphNameStyle)> {
    for member in XiphMember::ALL {
        for style in RUNTIME_NAME_STYLES {
            if name.eq_ignore_ascii_case(file_name(member, style)) {
                return Some((member, style));
            }
        }
    }
    None
}

/// Recognizes a reviewed canonical stem and a possible vendor-suffixed form.
fn looks_like_canonical_or_vendor_stem(
    name: &str,
    member: XiphMember,
    style: XiphNameStyle,
) -> bool {
    file_name(member, style)
        .strip_suffix(".dll")
        .is_some_and(|base| {
            name == base
                || name
                    .strip_prefix(base)
                    .is_some_and(|suffix| suffix.starts_with('_'))
        })
}

/// Recognizes an ABI-major stem even when its major-version suffix is malformed.
fn looks_like_abi_major_stem(name: &str, member: XiphMember, style: XiphNameStyle) -> bool {
    style == XiphNameStyle::AbiMajor
        && file_name(member, style)
            .strip_suffix(".dll")
            .and_then(|base| base.rsplit_once('-').map(|(prefix, _)| prefix))
            .is_some_and(|prefix| {
                name.strip_prefix(prefix)
                    .is_some_and(|suffix| suffix.starts_with('-'))
            })
}

/// A reviewed canonical DLL name followed by extra bytes is not an unrelated
/// executable: it is a malformed spelling of a known runtime member.
fn has_malformed_canonical_extension(name: &str) -> bool {
    XiphMember::ALL.into_iter().any(|member| {
        XiphNameStyle::ALL.into_iter().any(|style| {
            name.strip_prefix(file_name(member, style))
                .is_some_and(|suffix| !suffix.is_empty())
        })
    })
}

/// Finds the longest reviewed canonical stem that can own `stem`.
///
/// Iterating the fixed 4×3 catalogue avoids allocating and sorting a temporary
/// list on every runtime-name parse.
fn runtime_base_for_stem(stem: &str) -> Option<(XiphMember, XiphNameStyle, &'static str)> {
    let mut selected: Option<(XiphMember, XiphNameStyle, &'static str)> = None;
    for member in XiphMember::ALL {
        for style in RUNTIME_NAME_STYLES {
            let Some(base) = file_name(member, style).strip_suffix(".dll") else {
                continue;
            };
            let matches = stem == base
                || stem
                    .strip_prefix(base)
                    .is_some_and(|suffix| suffix.starts_with('_'));
            if matches
                && selected
                    .as_ref()
                    .is_none_or(|(_, _, previous)| base.len() > previous.len())
            {
                selected = Some((member, style, base));
            }
        }
    }
    selected
}

/// Returns the canonical basename for a member in a reviewed style.
#[must_use]
pub const fn file_name(member: XiphMember, style: XiphNameStyle) -> &'static str {
    match (member, style) {
        (XiphMember::VorbisFile, XiphNameStyle::Plain) => "vorbisfile.dll",
        (XiphMember::VorbisEnc, XiphNameStyle::Plain) => "vorbisenc.dll",
        (XiphMember::Vorbis, XiphNameStyle::Plain) => "vorbis.dll",
        (XiphMember::Ogg, XiphNameStyle::Plain) => "ogg.dll",
        (XiphMember::VorbisFile, XiphNameStyle::Lib) => "libvorbisfile.dll",
        (XiphMember::VorbisEnc, XiphNameStyle::Lib) => "libvorbisenc.dll",
        (XiphMember::Vorbis, XiphNameStyle::Lib) => "libvorbis.dll",
        (XiphMember::Ogg, XiphNameStyle::Lib) => "libogg.dll",
        (XiphMember::VorbisFile, XiphNameStyle::AbiMajor) => "libvorbisfile-3.dll",
        (XiphMember::VorbisEnc, XiphNameStyle::AbiMajor) => "libvorbisenc-2.dll",
        (XiphMember::Vorbis, XiphNameStyle::AbiMajor) => "libvorbis-0.dll",
        (XiphMember::Ogg, XiphNameStyle::AbiMajor) => "libogg-0.dll",
        (XiphMember::VorbisFile, XiphNameStyle::Unreal) => "libvorbisfile_64.dll",
        (XiphMember::VorbisEnc, XiphNameStyle::Unreal) => "libvorbisenc_64.dll",
        (XiphMember::Vorbis, XiphNameStyle::Unreal) => "libvorbis_64.dll",
        (XiphMember::Ogg, XiphNameStyle::Unreal) => "libogg_64.dll",
    }
}
