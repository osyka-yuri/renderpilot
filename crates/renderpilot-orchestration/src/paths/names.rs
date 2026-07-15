//! Pure bare path-component / file-name safety (no filesystem I/O).

/// Returns `true` if `value` is a safe bare file name: non-empty, no path
/// separators, no parent-directory references, no trailing dots or spaces, and not
/// a Windows reserved device name.
pub(crate) fn is_safe_file_name(value: &str) -> bool {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value.contains("..")
        || value.ends_with('.')
        || value.ends_with(' ')
    {
        return false;
    }

    let stem = value.split('.').next().unwrap_or(value);
    !is_windows_reserved_name(stem)
}

/// Sanitizes an arbitrary string into a safe bare path component (used to derive
/// storage directory names from untrusted identifiers).
pub(crate) fn sanitize_path_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => character,
            _ => '_',
        })
        .collect();

    let sanitized = sanitized
        .trim_matches(|c| c == '.' || c == ' ' || c == '_')
        .to_owned();

    if sanitized.is_empty() {
        return "unknown".to_owned();
    }

    let stem = sanitized.split('.').next().unwrap_or_default();
    if is_windows_reserved_name(stem) {
        format!("_{sanitized}")
    } else {
        sanitized
    }
}

fn is_windows_reserved_name(value: &str) -> bool {
    matches!(
        value.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_file_names() {
        assert!(is_safe_file_name("dxgi.dll"));
        assert!(!is_safe_file_name(""));
        assert!(!is_safe_file_name("a/b"));
        assert!(!is_safe_file_name("a\\b"));
        assert!(!is_safe_file_name(".."));
        assert!(!is_safe_file_name("trailing."));
        assert!(!is_safe_file_name("CON"));
        assert!(!is_safe_file_name("nul.txt"));
    }
}
