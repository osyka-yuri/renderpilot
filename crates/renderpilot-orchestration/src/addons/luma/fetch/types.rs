//! In-memory Luma release payload types.

/// One extracted Luma payload file, relative to the add-on directory,
/// `/`-normalized (e.g. `Luma-Dishonored_2.addon`, `Luma/Global/Copy_PS.hlsl`,
/// `nvngx_dlss.dll`).
#[derive(Debug, Clone)]
pub(crate) struct LumaPayloadFile {
    pub(crate) relative_path: String,
    pub(crate) bytes: Vec<u8>,
}

/// Everything extracted from a Luma release asset, plus its upstream identity.
#[derive(Debug, Clone)]
pub(crate) struct LumaPayload {
    /// Every payload file to lay down, including the main `.addon` and, when
    /// present, `nvngx_dlss.dll`.
    pub(crate) files: Vec<LumaPayloadFile>,
    /// The relative path of the main `.addon` within `files` -- the record's
    /// primary/addon-file anchor.
    pub(crate) main_addon_rel: String,
    /// SHA-256 of the raw ZIP bytes -- the durable change-detection digest.
    pub(crate) zip_digest: String,
    /// HTTP cache validator for a cheap update pre-check.
    pub(crate) etag: Option<String>,
    /// Raw `Last-Modified` HTTP-date string, when the host sent one.
    pub(crate) last_modified: Option<String>,
    /// Rolling-release build number recovered from the redirect target, when
    /// the tag could be parsed (see [`super::super::source::parse_build_number`]).
    pub(crate) build_number: Option<u64>,
}
