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
}

impl XiphNameStyle {
    /// Every reviewed basename convention in stable order.
    pub const ALL: [Self; 3] = [Self::Plain, Self::Lib, Self::AbiMajor];
}

/// Aggregate naming profile of a validated deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XiphNamingProfile {
    /// Every member uses the plain convention.
    Plain,
    /// Every member uses the unsuffixed `lib` convention.
    Lib,
    /// Every member uses the libtool ABI-major convention.
    AbiMajor,
    /// The import graph is valid but the deployment mixes conventions.
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

/// Classifies one reviewed Xiph DLL basename case-insensitively.
#[must_use]
pub fn classify_file_name(name: &str) -> Option<(XiphMember, XiphNameStyle)> {
    match name.to_ascii_lowercase().as_str() {
        "vorbisfile.dll" => Some((XiphMember::VorbisFile, XiphNameStyle::Plain)),
        "vorbisenc.dll" => Some((XiphMember::VorbisEnc, XiphNameStyle::Plain)),
        "vorbis.dll" => Some((XiphMember::Vorbis, XiphNameStyle::Plain)),
        "ogg.dll" => Some((XiphMember::Ogg, XiphNameStyle::Plain)),
        "libvorbisfile.dll" => Some((XiphMember::VorbisFile, XiphNameStyle::Lib)),
        "libvorbisenc.dll" => Some((XiphMember::VorbisEnc, XiphNameStyle::Lib)),
        "libvorbis.dll" => Some((XiphMember::Vorbis, XiphNameStyle::Lib)),
        "libogg.dll" => Some((XiphMember::Ogg, XiphNameStyle::Lib)),
        "libvorbisfile-3.dll" => Some((XiphMember::VorbisFile, XiphNameStyle::AbiMajor)),
        "libvorbisenc-2.dll" => Some((XiphMember::VorbisEnc, XiphNameStyle::AbiMajor)),
        "libvorbis-0.dll" => Some((XiphMember::Vorbis, XiphNameStyle::AbiMajor)),
        "libogg-0.dll" => Some((XiphMember::Ogg, XiphNameStyle::AbiMajor)),
        _ => None,
    }
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
    }
}
