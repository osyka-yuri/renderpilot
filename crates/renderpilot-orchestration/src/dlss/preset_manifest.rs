//! Typed DLSS preset manifest + lookup logic.
//!
//! Three JSON files are bundled at compile time, one per DLSS DLL family:
//!
//!   bundled/dlss_presets.json   - Super Resolution (nvngx_dlss.dll)
//!   bundled/dlss_g_presets.json - Frame Generation (nvngx_dlssg.dll)
//!   bundled/dlss_d_presets.json - Ray Reconstruction (nvngx_dlssd.dll)
//!
//! All three share the schema documented in `renderpilot-libraries/
//! schemas/dlss_preset_manifest.schema.json`. The bundled snapshot is
//! the runtime source of truth; auto-update from GitHub Pages will be
//! added in a follow-up.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::LazyLock;

use renderpilot_nvapi::{DlssDllKind, DlssVersion};
use serde::Deserialize;

const BUNDLED_SR: &str = include_str!("bundled/dlss_presets.json");
const BUNDLED_FG: &str = include_str!("bundled/dlss_g_presets.json");
const BUNDLED_RR: &str = include_str!("bundled/dlss_d_presets.json");

static MANIFEST_SR: LazyLock<DlssPresetManifest> = LazyLock::new(|| {
    load_manifest_from_disk_or_fallback("dlss_presets.json", BUNDLED_SR, DlssDllKind::Sr)
});
static MANIFEST_FG: LazyLock<DlssPresetManifest> = LazyLock::new(|| {
    load_manifest_from_disk_or_fallback("dlss_g_presets.json", BUNDLED_FG, DlssDllKind::FrameGen)
});
static MANIFEST_RR: LazyLock<DlssPresetManifest> = LazyLock::new(|| {
    load_manifest_from_disk_or_fallback(
        "dlss_d_presets.json",
        BUNDLED_RR,
        DlssDllKind::RayReconstruction,
    )
});

fn empty_manifest(kind: DlssDllKind) -> DlssPresetManifest {
    DlssPresetManifest {
        schema_version: 1,
        library_family: kind.manifest_library_family().to_owned(),
        dll_kind: kind.manifest_tag().to_owned(),
        generated_at: None,
        presets: BTreeMap::new(),
        version_support: Vec::new(),
    }
}

fn load_manifest_from_disk_or_fallback(
    file_name: &str,
    bundled_json: &str,
    kind: DlssDllKind,
) -> DlssPresetManifest {
    let from_disk = crate::libraries::local_preset_manifest_path(file_name)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|json| parse_manifest_json(&json, kind).ok());

    if let Some(manifest) = from_disk {
        return manifest;
    }

    crate::util::load_bundled_asset_or_default(
        || parse_manifest_json(bundled_json, kind),
        || empty_manifest(kind),
        "DLSS preset manifest",
    )
}

// -----------------------------------------------------------------------------
// Schema types
// -----------------------------------------------------------------------------

/// Top-level preset manifest as published on GitHub Pages.
#[derive(Debug, Clone, Deserialize)]
pub struct DlssPresetManifest {
    /// Schema version. Currently always `1`.
    pub schema_version: u32,
    /// E.g. `"nvngx_dlss"` / `"nvngx_dlssg"` / `"nvngx_dlssd"`.
    /// Validated non-empty at parse time.
    pub library_family: String,
    /// `"sr"` / `"fg"` / `"rr"` — validated against the expected [`DlssDllKind`].
    pub dll_kind: String,
    /// ISO-8601 timestamp of when the manifest was last regenerated.
    /// When present, validated non-empty; used for operator diagnostics.
    #[serde(default)]
    pub generated_at: Option<String>,
    /// Master preset table: DWORD value (as decimal string) → label/metadata.
    /// `BTreeMap` so the JSON object's key ordering does not matter and the
    /// in-memory ordering is deterministic.
    pub presets: BTreeMap<String, PresetEntry>,
    /// Ordered list of `(version, supported preset DWORDs)` entries.
    pub version_support: Vec<VersionSupportEntry>,
}

/// One preset entry from the master table.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct PresetEntry {
    /// Human-readable label, e.g. `"Preset F"`.
    pub label: String,
    /// `true` if this preset exists in the driver but is no longer recommended.
    #[serde(default)]
    pub deprecated: bool,
}

/// One `(DLL version, supported presets)` entry.
#[derive(Debug, Clone, Deserialize)]
pub struct VersionSupportEntry {
    /// DLL version string, e.g. `"3.1.1.0"`.
    pub version: String,
    /// Preset DWORD values supported for DLL versions >= this entry
    /// (until the next entry overrides them).
    pub supported_presets: Vec<u32>,
    /// Human-readable version label, e.g. `"DLSS 3.1"`.
    pub label: String,
}

// -----------------------------------------------------------------------------
// Error type
// -----------------------------------------------------------------------------

/// Errors that can arise when parsing or using a preset manifest.
#[derive(Debug)]
pub enum PresetManifestError {
    /// The bundled / fetched JSON is malformed.
    InvalidJson(String),
    /// The manifest references a preset DWORD not declared in `presets`.
    UnknownPresetInSupportList {
        /// The version entry that references the unknown preset.
        version: String,
        /// The DWORD value that has no matching entry in the `presets` table.
        dword: u32,
    },
    /// `schema_version` is newer than the version this build understands.
    UnsupportedSchemaVersion(u32),
    /// `dll_kind` does not match the DLL family this manifest was loaded for.
    MismatchedDllKind {
        /// Expected wire tag (`sr` / `fg` / `rr`).
        expected: String,
        /// Value from the document.
        found: String,
    },
    /// `library_family` is missing or blank.
    EmptyLibraryFamily,
    /// `library_family` does not match the DLL family this manifest was loaded for.
    MismatchedLibraryFamily {
        /// Expected manifest library-family name.
        expected: String,
        /// Value from the document.
        found: String,
    },
    /// `generated_at` is present but blank.
    EmptyGeneratedAt,
}

impl fmt::Display for PresetManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(detail) => {
                write!(formatter, "invalid preset manifest JSON: {detail}")
            }
            Self::UnknownPresetInSupportList { version, dword } => write!(
                formatter,
                "version_support[{version}] references unknown preset dword {dword}"
            ),
            Self::UnsupportedSchemaVersion(version) => {
                write!(
                    formatter,
                    "preset manifest schema_version {version} is newer than supported (1)"
                )
            }
            Self::MismatchedDllKind { expected, found } => write!(
                formatter,
                "preset manifest dll_kind `{found}` does not match expected `{expected}`"
            ),
            Self::EmptyLibraryFamily => {
                write!(formatter, "preset manifest library_family is empty")
            }
            Self::MismatchedLibraryFamily { expected, found } => write!(
                formatter,
                "preset manifest library_family `{found}` does not match expected `{expected}`"
            ),
            Self::EmptyGeneratedAt => {
                write!(
                    formatter,
                    "preset manifest generated_at is present but empty"
                )
            }
        }
    }
}

impl std::error::Error for PresetManifestError {}

// -----------------------------------------------------------------------------
// Loading
// -----------------------------------------------------------------------------

/// Retrieves the static, compile-time bundled manifest corresponding to the specified `kind`.
///
/// Upon initial access, the manifest is parsed and subsequently cached in memory
/// for the entire duration of the application's lifecycle.
pub fn bundled_manifest(kind: DlssDllKind) -> &'static DlssPresetManifest {
    match kind {
        DlssDllKind::Sr => &MANIFEST_SR,
        DlssDllKind::FrameGen => &MANIFEST_FG,
        DlssDllKind::RayReconstruction => &MANIFEST_RR,
    }
}

fn parse_manifest_json(
    json: &str,
    expected_kind: DlssDllKind,
) -> Result<DlssPresetManifest, PresetManifestError> {
    let manifest: DlssPresetManifest =
        serde_json::from_str(json).map_err(|e| PresetManifestError::InvalidJson(e.to_string()))?;
    validate_manifest(&manifest, expected_kind)?;
    Ok(manifest)
}

fn validate_manifest(
    manifest: &DlssPresetManifest,
    expected_kind: DlssDllKind,
) -> Result<(), PresetManifestError> {
    if manifest.schema_version != 1 {
        return Err(PresetManifestError::UnsupportedSchemaVersion(
            manifest.schema_version,
        ));
    }
    if manifest.library_family.trim().is_empty() {
        return Err(PresetManifestError::EmptyLibraryFamily);
    }
    let expected_family = expected_kind.manifest_library_family();
    if manifest.library_family != expected_family {
        return Err(PresetManifestError::MismatchedLibraryFamily {
            expected: expected_family.to_owned(),
            found: manifest.library_family.clone(),
        });
    }
    let expected_tag = expected_kind.manifest_tag();
    if manifest.dll_kind != expected_tag {
        return Err(PresetManifestError::MismatchedDllKind {
            expected: expected_tag.to_owned(),
            found: manifest.dll_kind.clone(),
        });
    }
    if let Some(generated_at) = &manifest.generated_at
        && generated_at.trim().is_empty()
    {
        return Err(PresetManifestError::EmptyGeneratedAt);
    }
    // Every preset referenced in version_support must exist in `presets`.
    let known: std::collections::HashSet<u32> = manifest
        .presets
        .keys()
        .filter_map(|k| k.parse::<u32>().ok())
        .collect();
    for entry in &manifest.version_support {
        for dword in &entry.supported_presets {
            if !known.contains(dword) {
                return Err(PresetManifestError::UnknownPresetInSupportList {
                    version: entry.version.clone(),
                    dword: *dword,
                });
            }
        }
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// Inheritance lookup
// -----------------------------------------------------------------------------

/// Retrieves the `version_support` entry applicable to the specified `version`.
/// Returns `None` if no entry satisfies the condition `entry_version <= version`
/// (meaning the provided DLL version is older than any documented entry).
///
/// Version comparison relies on [`DlssVersion`]'s component-wise lexicographical
/// ordering, ensuring that versions like `3.10.2` are properly evaluated as older
/// than `310.1.0`.
pub fn resolve_entry<'a>(
    manifest: &'a DlssPresetManifest,
    version: &DlssVersion,
) -> Option<&'a VersionSupportEntry> {
    manifest
        .version_support
        .iter()
        .filter_map(|entry| DlssVersion::parse(&entry.version).map(|v| (v, entry)))
        .filter(|(v, _)| v <= version)
        .max_by_key(|(v, _)| *v)
        .map(|(_, entry)| entry)
}

/// Convenience: returns the supported preset DWORDs for `version`.
/// Empty slice if no `version_support` entry applies.
pub fn supported_presets_for<'a>(
    manifest: &'a DlssPresetManifest,
    version: &DlssVersion,
) -> &'a [u32] {
    resolve_entry(manifest, version)
        .map(|entry| entry.supported_presets.as_slice())
        .unwrap_or(&[])
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;

    #[test]
    fn bundled_sr_manifest_parses() {
        let m = bundled_manifest(DlssDllKind::Sr);
        assert_eq!(m.schema_version, 1);
        assert_eq!(m.library_family, "nvngx_dlss");
        assert_eq!(m.dll_kind, "sr");
        assert!(!m.version_support.is_empty());
        assert!(m.presets.contains_key("0"));
    }

    #[test]
    fn bundled_fg_manifest_parses() {
        let m = bundled_manifest(DlssDllKind::FrameGen);
        assert_eq!(m.dll_kind, "fg");
        assert!(!m.version_support.is_empty());
    }

    #[test]
    fn bundled_rr_manifest_parses() {
        let m = bundled_manifest(DlssDllKind::RayReconstruction);
        assert_eq!(m.dll_kind, "rr");
        assert!(!m.version_support.is_empty());
    }

    #[test]
    fn sr_dlss_4_resolves_to_310_entry() {
        let m = bundled_manifest(DlssDllKind::Sr);
        let v = DlssVersion::new(310, 1, 0, 0);
        let entry = resolve_entry(m, &v).expect("should resolve");
        assert_eq!(entry.version, "310.1.0.0");
        // DLSS 4 ships with a much smaller supported preset set.
        assert!(entry.supported_presets.contains(&0));
        assert!(entry.supported_presets.contains(&6));
        assert!(entry.supported_presets.contains(&10));
        assert!(entry.supported_presets.contains(&11));
        assert!(!entry.supported_presets.contains(&1));
    }

    #[test]
    fn sr_dlss_3_1_resolves_to_3_1_entry_not_310() {
        // The bug we have to avoid: string-compare would order
        // 3.1.1.0 above 310.1.0.0 (because '3' < '3'? actually `3.` < `31`).
        // semver compare puts 3.x correctly *below* 310.x.
        let m = bundled_manifest(DlssDllKind::Sr);
        let v = DlssVersion::new(3, 1, 1, 0);
        let entry = resolve_entry(m, &v).expect("should resolve");
        assert_eq!(entry.version, "3.1.1.0");
        // 3.1 still has all twelve presets including K.
        assert!(entry.supported_presets.contains(&11));
    }

    #[test]
    fn sr_intermediate_version_inherits_from_below() {
        let m = bundled_manifest(DlssDllKind::Sr);
        // 3.10.2 sits between 3.1.1 and 310.1.0. Semver compare puts
        // 3.10.2 < 310.1.0, so it should inherit from the highest
        // entry <= 3.10.2, which is 3.1.1.
        let v = DlssVersion::new(3, 10, 2, 0);
        let entry = resolve_entry(m, &v).expect("should resolve");
        assert_eq!(entry.version, "3.1.1.0");
    }

    #[test]
    fn sr_version_below_oldest_returns_none() {
        let m = bundled_manifest(DlssDllKind::Sr);
        let v = DlssVersion::new(0, 5, 0, 0);
        assert!(resolve_entry(m, &v).is_none());
        assert!(supported_presets_for(m, &v).is_empty());
    }

    #[test]
    fn supported_presets_for_returns_expected_for_known_version() {
        let m = bundled_manifest(DlssDllKind::Sr);
        let v = DlssVersion::new(3, 1, 1, 0);
        let presets = supported_presets_for(m, &v);
        // 3.1 supports the full classic set.
        assert!(presets.contains(&0));
        assert!(presets.contains(&6));
        assert!(presets.contains(&11));
    }

    #[test]
    fn unknown_preset_in_support_list_is_rejected() {
        let bad = r#"{
            "schema_version": 1,
            "library_family": "nvngx_dlss",
            "dll_kind": "sr",
            "presets": {"0": {"label": "Default"}},
            "version_support": [
                {"version": "1.0.0.0", "supported_presets": [0, 999], "label": "x"}
            ]
        }"#;
        let err = parse_manifest_json(bad, DlssDllKind::Sr).unwrap_err();
        assert_matches!(
            err,
            PresetManifestError::UnknownPresetInSupportList { dword: 999, .. }
        );
    }

    #[test]
    fn future_schema_version_rejected() {
        let bad = r#"{
            "schema_version": 99,
            "library_family": "nvngx_dlss",
            "dll_kind": "sr",
            "presets": {},
            "version_support": []
        }"#;
        let err = parse_manifest_json(bad, DlssDllKind::Sr).unwrap_err();
        assert_matches!(err, PresetManifestError::UnsupportedSchemaVersion(99));
    }

    #[test]
    fn mismatched_dll_kind_is_rejected() {
        let bad = r#"{
            "schema_version": 1,
            "library_family": "nvngx_dlss",
            "dll_kind": "fg",
            "presets": {"0": {"label": "Default"}},
            "version_support": []
        }"#;
        let err = parse_manifest_json(bad, DlssDllKind::Sr).unwrap_err();
        assert_matches!(err, PresetManifestError::MismatchedDllKind { .. });
    }

    #[test]
    fn mismatched_library_family_is_rejected() {
        let bad = r#"{
            "schema_version": 1,
            "library_family": "nvngx_dlssg",
            "dll_kind": "sr",
            "presets": {"0": {"label": "Default"}},
            "version_support": []
        }"#;
        let err = parse_manifest_json(bad, DlssDllKind::Sr).unwrap_err();
        assert_matches!(err, PresetManifestError::MismatchedLibraryFamily { .. });
    }

    #[test]
    fn fallback_manifest_keeps_kind_identity() {
        for kind in [
            DlssDllKind::Sr,
            DlssDllKind::FrameGen,
            DlssDllKind::RayReconstruction,
        ] {
            let manifest = empty_manifest(kind);
            assert_eq!(manifest.library_family, kind.manifest_library_family());
            assert_eq!(manifest.dll_kind, kind.manifest_tag());
        }
    }

    #[test]
    fn empty_library_family_is_rejected() {
        let bad = r#"{
            "schema_version": 1,
            "library_family": "  ",
            "dll_kind": "sr",
            "presets": {},
            "version_support": []
        }"#;
        let err = parse_manifest_json(bad, DlssDllKind::Sr).unwrap_err();
        assert_matches!(err, PresetManifestError::EmptyLibraryFamily);
    }

    #[test]
    fn empty_generated_at_is_rejected() {
        let bad = r#"{
            "schema_version": 1,
            "library_family": "nvngx_dlss",
            "dll_kind": "sr",
            "generated_at": "  ",
            "presets": {},
            "version_support": []
        }"#;
        let err = parse_manifest_json(bad, DlssDllKind::Sr).unwrap_err();
        assert_matches!(err, PresetManifestError::EmptyGeneratedAt);
    }

    #[test]
    fn invalid_json_returns_parser_error() {
        let err = parse_manifest_json("not json", DlssDllKind::Sr).unwrap_err();
        assert_matches!(err, PresetManifestError::InvalidJson(_));
    }
}
