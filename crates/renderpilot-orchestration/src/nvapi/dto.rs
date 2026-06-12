use renderpilot_nvapi::setting::NvapiValueType;
use renderpilot_nvapi::NvapiSetting;
use serde::Serialize;

/// Serializable descriptor for a single NVAPI setting (key, label, value type, DLL family).
#[derive(Debug, Serialize)]
pub struct SettingDescriptorDto {
    /// Wire-stable setting identifier.
    pub setting_key: String,
    /// Short UI label for the setting.
    pub setting_label: String,
    /// Value type string: `"dword"` or `"wstring"`.
    pub value_type: String,
    /// DLL family tag if the setting is DLL-version constrained.
    pub dll_kind: Option<String>,
    /// Grouping family tag for the UI (`"sr"` / `"fg"` / `"rr"`).
    pub family: Option<String>,
}

/// Serializable descriptor for a candidate game executable.
#[derive(Debug, Serialize)]
pub struct ExecutableCandidateDto {
    /// Relative path from the game install root.
    pub relative_path: String,
    /// File name only.
    pub file_name: String,
    /// Absolute path with forward slashes.
    pub absolute_path: String,
    /// File size in bytes.
    pub size_bytes: u64,
    /// Directory depth from the install root.
    pub depth: u32,
    /// Scoring rank (higher = more likely primary executable).
    pub rank_score: i32,
    /// Human-readable rejection reason, if any.
    pub rejection: Option<String>,
    /// Machine-readable rejection token, if any.
    pub rejection_token: Option<String>,
}

/// Serializable descriptor for one selectable value of an NVAPI setting.
#[derive(Debug, Serialize)]
pub struct ValueOptionDto {
    /// Wire string for this value.
    pub wire: String,
    /// Human-readable label.
    pub label: String,
    /// Whether this value is supported by the current DLL version.
    pub supported: bool,
}

/// Serializable wire+label+dword triple for an NVAPI setting value.
#[derive(Debug, Serialize)]
pub struct ValueDescriptorDto {
    /// Wire string for this value.
    pub wire: String,
    /// Human-readable label.
    pub label: String,
    /// Raw DWORD stored in the NVIDIA driver profile.
    pub dword: u32,
}

/// Serializable baseline snapshot captured before RenderPilot first modified a setting.
#[derive(Debug, Serialize)]
pub struct BaselineDto {
    /// Wire string of the baseline value, if known.
    pub wire: Option<String>,
    /// Human-readable label of the baseline value, if known.
    pub label: Option<String>,
    /// Raw DWORD of the baseline value.
    pub dword: u32,
    /// `true` if the setting was at the driver predefined default when first captured.
    pub was_predefined: bool,
    /// Unix timestamp (seconds) when the baseline was captured.
    pub captured_at: i64,
    /// Executable that was in use when the baseline was captured.
    pub captured_exe: String,
}

/// Serializable info about a DLSS DLL found in the game installation directory.
#[derive(Debug, Serialize)]
pub struct DllInfoDto {
    /// DLL kind tag (`"sr"`, `"fg"`, `"rr"`).
    pub kind: String,
    /// DLL version string.
    pub version: String,
    /// Absolute path with forward slashes.
    pub path: String,
    /// Human-readable label from the preset manifest for this DLL version.
    pub manifest_label: Option<String>,
}

/// Full serializable response for a single NVAPI setting's live state.
#[derive(Debug, Serialize)]
pub struct SettingStateResponse {
    /// Wire-stable setting identifier.
    pub setting_key: String,
    /// Short UI label for the setting.
    pub setting_label: String,
    /// Value type string: `"dword"` or `"wstring"`.
    pub value_type: String,
    /// Grouping family for the UI: `"sr"` / `"fg"` / `"rr"` (or `None`).
    pub family: Option<String>,
    /// Human-readable family label derived from `family` (display only).
    pub category: Option<String>,
    /// Optional help text shown beside the control.
    pub description: Option<String>,
    /// Optional minimum driver-version hint (display only; not enforced).
    pub min_driver: Option<String>,
    /// Current live value.
    pub current: ValueDescriptorDto,
    /// Driver predefined default, if available.
    pub predefined: Option<ValueDescriptorDto>,
    /// Baseline snapshot, if one was captured.
    pub baseline: Option<BaselineDto>,
    /// `true` if the current value equals the driver predefined default.
    pub is_current_predefined: bool,
    /// `true` if the setting was modified by a tool other than RenderPilot.
    pub is_modified_outside_renderpilot: bool,
    /// Effective executable used for NVIDIA profile lookup.
    pub effective_exe: Option<String>,
    /// Source of the effective exe: `"override"` or `"auto"`.
    pub effective_exe_source: Option<String>,
    /// `true` if an NVIDIA driver profile exists for the effective exe.
    pub has_profile_for_exe: bool,
    /// Whether NVAPI (the NVIDIA driver) is present on this machine. When false the
    /// UI hides driver-profile affordances and keeps only the on-disk DLL swap.
    pub nvapi_available: bool,
    /// All selectable values, each marked as supported or not.
    pub available_values: Vec<ValueOptionDto>,
    /// Info about the DLSS DLL detected in the install directory.
    pub dll_info: Option<DllInfoDto>,
    /// Non-fatal warnings surfaced to the UI.
    pub warnings: Vec<NvapiWarningDto>,
}

/// Strongly typed warning code for UI localization and categorization.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum NvapiWarningDto {
    /// No DLSS DLL detected in the install directory.
    NoDll,
    /// Manifest has no entry for this DLL version.
    NoManifest,
    /// No executable resolved for this game.
    NoExecutable,
    /// NVAPI unavailable.
    NvapiUnavailable,
    /// NVAPI initialize failed.
    NvapiInitFailed,
    /// DRS session could not be created.
    DrsFailed,
}

/// Builds a [`SettingDescriptorDto`] from a dynamic [`NvapiSetting`] reference.
pub fn setting_descriptor_dto(setting: &dyn NvapiSetting) -> SettingDescriptorDto {
    SettingDescriptorDto {
        setting_key: setting.key().to_owned(),
        setting_label: setting.label().to_owned(),
        value_type: value_type_str(setting.value_type()).to_owned(),
        dll_kind: setting.dll_kind().map(|k| k.manifest_tag().to_owned()),
        family: setting.family().map(str::to_owned),
    }
}

/// Maps a grouping `family` tag to a human-readable category label for the UI.
pub fn category_for_family(family: &str) -> Option<String> {
    let label = match family {
        "sr" => "DLSS Super Resolution",
        "fg" => "DLSS Frame Generation",
        "rr" => "DLSS Ray Reconstruction",
        _ => return None,
    };
    Some(label.to_owned())
}

/// Converts a platform-windows [`ExecutableCandidate`](renderpilot_platform_windows::ExecutableCandidate) into a serializable DTO.
#[cfg(windows)]
pub fn executable_candidate_dto(
    candidate: renderpilot_platform_windows::ExecutableCandidate,
) -> ExecutableCandidateDto {
    let (rejection, rejection_token) = match candidate.rejection.as_ref() {
        Some(r) => (Some(r.kind().to_owned()), Some(r.token().to_owned())),
        None => (None, None),
    };
    ExecutableCandidateDto {
        relative_path: candidate.relative_path,
        file_name: candidate.file_name,
        absolute_path: candidate.absolute_path.to_string_lossy().replace('\\', "/"),
        size_bytes: candidate.size_bytes,
        depth: candidate.depth,
        rank_score: candidate.rank_score,
        rejection,
        rejection_token,
    }
}

/// Non-Windows stub: executable candidate conversion is only supported on Windows.
#[cfg(not(windows))]
pub fn executable_candidate_dto(_candidate: ()) -> ExecutableCandidateDto {
    unreachable!()
}

/// Returns the wire string for an [`NvapiValueType`].
pub fn value_type_str(v: NvapiValueType) -> &'static str {
    match v {
        NvapiValueType::Dword => "dword",
        NvapiValueType::WString => "wstring",
    }
}
