use renderpilot_nvapi::setting::NvapiValueType;
use renderpilot_nvapi::NvapiSetting;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct SettingDescriptorDto {
    pub setting_key: String,
    pub setting_label: String,
    pub value_type: String,
    pub dll_kind: Option<String>,
    pub family: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExecutableCandidateDto {
    pub relative_path: String,
    pub file_name: String,
    pub absolute_path: String,
    pub size_bytes: u64,
    pub depth: u32,
    pub rank_score: i32,
    pub rejection: Option<String>,
    pub rejection_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ValueOptionDto {
    pub wire: String,
    pub label: String,
    pub supported: bool,
}

#[derive(Debug, Serialize)]
pub struct ValueDescriptorDto {
    pub wire: String,
    pub label: String,
    pub dword: u32,
}

#[derive(Debug, Serialize)]
pub struct BaselineDto {
    pub wire: Option<String>,
    pub label: Option<String>,
    pub dword: u32,
    pub was_predefined: bool,
    pub captured_at: i64,
    pub captured_exe: String,
}

#[derive(Debug, Serialize)]
pub struct DllInfoDto {
    pub kind: String,
    pub version: String,
    pub path: String,
    pub manifest_label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SettingStateResponse {
    pub setting_key: String,
    pub setting_label: String,
    pub value_type: String,
    /// Grouping family for the UI: `"sr"` / `"fg"` / `"rr"` (or `None`).
    pub family: Option<String>,
    /// Human-readable family label derived from `family` (display only).
    pub category: Option<String>,
    /// Optional help text shown beside the control.
    pub description: Option<String>,
    /// Optional minimum driver-version hint (display only; not enforced).
    pub min_driver: Option<String>,
    pub current: ValueDescriptorDto,
    pub predefined: Option<ValueDescriptorDto>,
    pub baseline: Option<BaselineDto>,
    pub is_current_predefined: bool,
    pub is_modified_outside_renderpilot: bool,
    pub effective_exe: Option<String>,
    pub effective_exe_source: Option<String>,
    pub has_profile_for_exe: bool,
    pub available_values: Vec<ValueOptionDto>,
    pub dll_info: Option<DllInfoDto>,
    pub warnings: Vec<String>,
}

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

#[cfg(not(windows))]
pub fn executable_candidate_dto(_candidate: ()) -> ExecutableCandidateDto {
    unreachable!()
}

pub fn value_type_str(v: NvapiValueType) -> &'static str {
    match v {
        NvapiValueType::Dword => "dword",
        NvapiValueType::WString => "wstring",
    }
}
