use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use super::util::same_path;
use super::{APPS_INI_NAME, APPS_KEY};

/// The change a pure app-list planner asks its caller to publish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppListChange {
    /// The existing bytes already represent the requested operation.
    Unchanged,
    /// The caller must replace the file with these complete bytes.
    Replacement(Vec<u8>),
}

/// A deterministic, preservation-aware app-list mutation plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppListPlan {
    /// The byte-level publication requested by the plan.
    pub change: AppListChange,
    /// The executable paths represented by the resulting `Apps=` value.
    pub resulting_apps: Vec<PathBuf>,
}

impl AppListPlan {
    /// Whether the resulting `Apps=` value has no executable entries.
    #[must_use]
    pub fn resulting_list_is_empty(&self) -> bool {
        self.resulting_apps.is_empty()
    }
}

/// Fail-closed errors from the app-list parser and planner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppListPlanError {
    /// The file is not valid UTF-8 (apart from the optional UTF-8 BOM).
    InvalidUtf8,
    /// More than one unambiguous `Apps=` key was found.
    MultipleAppsKeys,
    /// An `Apps` key was present without a value delimiter.
    MalformedAppsKey,
    /// The requested path cannot be represented by ReShade's comma-delimited
    /// UTF-8 app-list format without changing its meaning.
    PathNotRepresentable,
    /// A lone carriage return was found instead of a supported line ending.
    UnsupportedLineEnding,
}

impl fmt::Display for AppListPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidUtf8 => "ReShadeApps.ini is not valid UTF-8",
            Self::MultipleAppsKeys => "ReShadeApps.ini contains multiple Apps keys",
            Self::MalformedAppsKey => "ReShadeApps.ini contains a malformed Apps key",
            Self::PathNotRepresentable => {
                "the executable path cannot be represented losslessly in ReShadeApps.ini"
            }
            Self::UnsupportedLineEnding => "ReShadeApps.ini contains an unsupported line ending",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for AppListPlanError {}

#[derive(Debug, Clone)]
struct AppEntry {
    segment: String,
    path: PathBuf,
}

#[derive(Debug, Clone, Copy)]
struct AppsLine {
    value_start: usize,
    value_end: usize,
}

#[derive(Debug, Clone)]
struct ParsedAppList {
    content: String,
    apps_line: Option<AppsLine>,
    entries: Vec<AppEntry>,
}

/// Plans registration of one executable without normalizing unrelated bytes.
///
/// `None` means that `ReShadeApps.ini` does not exist. An existing file with
/// no `Apps=` key is preserved and receives one appended key on registration.
pub fn plan_register_app(
    raw: Option<&[u8]>,
    exe_path: &Path,
) -> Result<AppListPlan, AppListPlanError> {
    let parsed = parse_raw(raw)?;
    let target = path_for_ini(exe_path)?;

    if parsed
        .entries
        .iter()
        .any(|entry| same_path(&entry.path, exe_path))
    {
        return Ok(AppListPlan {
            change: AppListChange::Unchanged,
            resulting_apps: parsed.entries.into_iter().map(|entry| entry.path).collect(),
        });
    }

    let replacement = match parsed.apps_line {
        Some(line) => {
            let existing = &parsed.content[line.value_start..line.value_end];
            let separator = if existing.is_empty() { "" } else { "," };
            replace_apps_value(
                &parsed.content,
                line,
                &format!("{existing}{separator}{target}"),
            )
        }
        None => append_apps_line(&parsed.content, &target),
    };
    let mut entries = parsed.entries;
    entries.push(AppEntry {
        segment: target,
        path: exe_path.to_path_buf(),
    });
    let resulting_apps = entries.into_iter().map(|entry| entry.path).collect();

    Ok(AppListPlan {
        change: if replacement.as_bytes() == raw.unwrap_or_default() {
            AppListChange::Unchanged
        } else {
            AppListChange::Replacement(replacement.into_bytes())
        },
        resulting_apps,
    })
}

/// Plans removal of one executable without pruning stale or unrelated entries.
///
/// An empty `Apps=` value remains in the file. This is intentional: callers
/// may use [`AppListPlan::resulting_list_is_empty`] to decide what the shared
/// layer means, while the file itself remains byte-preserving and recoverable.
pub fn plan_unregister_app(
    raw: Option<&[u8]>,
    exe_path: &Path,
) -> Result<AppListPlan, AppListPlanError> {
    let parsed = parse_raw(raw)?;
    let mut entries = parsed.entries;
    let original_len = entries.len();
    entries.retain(|entry| !same_path(&entry.path, exe_path));

    let Some(line) = parsed.apps_line else {
        return Ok(AppListPlan {
            change: AppListChange::Unchanged,
            resulting_apps: entries.into_iter().map(|entry| entry.path).collect(),
        });
    };
    if original_len == entries.len() {
        return Ok(AppListPlan {
            change: AppListChange::Unchanged,
            resulting_apps: entries.into_iter().map(|entry| entry.path).collect(),
        });
    }

    let joined = entries
        .iter()
        .map(|entry| entry.segment.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let replacement = replace_apps_value(&parsed.content, line, &joined);
    let resulting_apps = entries.into_iter().map(|entry| entry.path).collect();
    Ok(AppListPlan {
        change: if replacement.as_bytes() == raw.unwrap_or_default() {
            AppListChange::Unchanged
        } else {
            AppListChange::Replacement(replacement.into_bytes())
        },
        resulting_apps,
    })
}

/// Parses a complete app-list byte sequence without touching the filesystem.
pub fn parse_app_list(raw: &[u8]) -> Result<Vec<PathBuf>, AppListPlanError> {
    Ok(parse_raw(Some(raw))?
        .entries
        .into_iter()
        .map(|entry| entry.path)
        .collect())
}

/// Reads the list of registered app executable paths from `ReShadeApps.ini`.
/// Returns an empty vector if the file does not exist.
pub fn read_app_list(layer_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let raw = read_app_list_bytes(layer_dir)?;
    raw.as_deref()
        .map(parse_app_list)
        .transpose()
        .map_err(plan_error_to_io)
        .map(|apps| apps.unwrap_or_default())
}

/// Reads the complete `ReShadeApps.ini` byte sequence for a participant
/// snapshot. `None` means the file is absent; an empty vector means an existing
/// zero-byte file.
pub fn read_app_list_bytes(layer_dir: &Path) -> io::Result<Option<Vec<u8>>> {
    let ini_path = layer_dir.join(APPS_INI_NAME);
    match std::fs::read(&ini_path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Adds `exe_path` to `ReShadeApps.ini` if not already present.
/// Creates the file (and directory) if needed. The planner is deliberately
/// applied only after the complete previous byte sequence has been read.
pub fn register_app(layer_dir: &Path, exe_path: &Path) -> io::Result<()> {
    let raw = read_app_list_bytes(layer_dir)?;
    let plan = plan_register_app(raw.as_deref(), exe_path).map_err(plan_error_to_io)?;
    apply_plan(layer_dir, plan.change)
}

/// Removes `exe_path` from `ReShadeApps.ini` and returns whether the resulting
/// `Apps=` value is empty. Missing files and files without an `Apps=` key are
/// left untouched and report `true`.
pub fn unregister_app(layer_dir: &Path, exe_path: &Path) -> io::Result<bool> {
    let raw = read_app_list_bytes(layer_dir)?;
    let plan = plan_unregister_app(raw.as_deref(), exe_path).map_err(plan_error_to_io)?;
    let is_empty = plan.resulting_list_is_empty();
    apply_plan(layer_dir, plan.change)?;
    Ok(is_empty)
}

fn apply_plan(layer_dir: &Path, change: AppListChange) -> io::Result<()> {
    let AppListChange::Replacement(bytes) = change else {
        return Ok(());
    };
    write_app_list_bytes(layer_dir, &bytes)
}

/// Writes complete app-list bytes through a same-directory temporary file.
/// Transaction staging and publication remain the responsibility of the
/// orchestration layer; this helper only preserves the existing compatibility
/// API's safe non-truncating write behavior.
fn write_app_list_bytes(layer_dir: &Path, bytes: &[u8]) -> io::Result<()> {
    let ini_path = layer_dir.join(APPS_INI_NAME);
    let tmp_path = layer_dir.join(format!("{APPS_INI_NAME}.tmp"));
    std::fs::create_dir_all(layer_dir)?;
    std::fs::write(&tmp_path, bytes)?;
    std::fs::rename(&tmp_path, &ini_path)
}

/// Compatibility helper used by existing platform tests and callers that
/// intentionally create a canonical app list. New combined operations should
/// use [`plan_register_app`] and publish the returned bytes as one participant.
#[cfg(test)]
pub(crate) fn write_app_list(layer_dir: &Path, apps: &[PathBuf]) -> io::Result<()> {
    let joined = apps
        .iter()
        .map(|path| path_for_ini(path))
        .collect::<Result<Vec<_>, _>>()
        .map_err(plan_error_to_io)?
        .join(",");
    write_app_list_bytes(layer_dir, format!("{APPS_KEY}={joined}\n").as_bytes())
}

fn parse_raw(raw: Option<&[u8]>) -> Result<ParsedAppList, AppListPlanError> {
    let bytes = raw.unwrap_or_default();
    let content = std::str::from_utf8(bytes)
        .map_err(|_| AppListPlanError::InvalidUtf8)?
        .to_owned();
    let mut apps_line = None;
    let mut entries = Vec::new();
    let mut cursor = 0;

    for segment in content.split_inclusive('\n') {
        let segment_start = cursor;
        cursor += segment.len();
        let body = if let Some(stripped) = segment.strip_suffix("\r\n") {
            stripped
        } else if let Some(stripped) = segment.strip_suffix('\n') {
            stripped
        } else {
            segment
        };
        if body.contains('\r') {
            return Err(AppListPlanError::UnsupportedLineEnding);
        }
        parse_line(segment_start, body, &mut apps_line, &mut entries)?;
    }
    if !content.ends_with('\n') && content.contains('\r') {
        return Err(AppListPlanError::UnsupportedLineEnding);
    }

    Ok(ParsedAppList {
        content,
        apps_line,
        entries,
    })
}

fn parse_line(
    segment_start: usize,
    body: &str,
    apps_line: &mut Option<AppsLine>,
    entries: &mut Vec<AppEntry>,
) -> Result<(), AppListPlanError> {
    let key_body = body.strip_prefix('\u{feff}').unwrap_or(body);
    let key_body_offset = body.len() - key_body.len();
    let trimmed = key_body.trim();
    let Some(equal_offset) = key_body.find('=') else {
        if looks_like_apps_key(trimmed) {
            return Err(AppListPlanError::MalformedAppsKey);
        }
        return Ok(());
    };
    let key = key_body[..equal_offset].trim();
    if key != APPS_KEY {
        if looks_like_apps_key(trimmed) {
            return Err(AppListPlanError::MalformedAppsKey);
        }
        return Ok(());
    }
    if apps_line.is_some() {
        return Err(AppListPlanError::MultipleAppsKeys);
    }

    let value_start = segment_start + key_body_offset + equal_offset + 1;
    let value_end = segment_start + body.len();
    let raw_value = &key_body[equal_offset + 1..];
    for segment in raw_value.split(',') {
        let value = segment.trim();
        if value.is_empty() {
            continue;
        }
        if value.contains(['\r', '\n', '\0']) {
            return Err(AppListPlanError::PathNotRepresentable);
        }
        entries.push(AppEntry {
            segment: segment.to_owned(),
            path: PathBuf::from(value),
        });
    }
    *apps_line = Some(AppsLine {
        value_start,
        value_end,
    });
    Ok(())
}

fn looks_like_apps_key(line: &str) -> bool {
    line.strip_prefix(APPS_KEY).is_some_and(|suffix| {
        if suffix.is_empty() {
            return true;
        }
        let normalized = suffix.trim_start();
        if normalized.starts_with('=') {
            return false;
        }
        suffix != normalized
            || normalized
                .chars()
                .next()
                .is_some_and(|character| !character.is_ascii_alphanumeric() && character != '_')
    })
}

fn path_for_ini(path: &Path) -> Result<String, AppListPlanError> {
    let value = path
        .to_str()
        .ok_or(AppListPlanError::PathNotRepresentable)?;
    if value.is_empty() || value.contains([',', '\r', '\n', '\0']) {
        return Err(AppListPlanError::PathNotRepresentable);
    }
    Ok(value.replace('/', "\\"))
}

fn replace_apps_value(content: &str, line: AppsLine, value: &str) -> String {
    let mut replacement = String::with_capacity(content.len() + value.len());
    replacement.push_str(&content[..line.value_start]);
    replacement.push_str(value);
    replacement.push_str(&content[line.value_end..]);
    replacement
}

fn append_apps_line(content: &str, value: &str) -> String {
    let newline = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    if content.is_empty() {
        format!("{APPS_KEY}={value}{newline}")
    } else if content.ends_with('\n') {
        format!("{content}{APPS_KEY}={value}{newline}")
    } else {
        format!("{content}{newline}{APPS_KEY}={value}{newline}")
    }
}

fn plan_error_to_io(error: AppListPlanError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}
