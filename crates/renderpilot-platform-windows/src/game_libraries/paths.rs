//! Path discovery and normalization helpers shared by launcher discovery:
//! existence/dedup filtering, verbatim-prefix stripping, `%ENV%` expansion, and
//! Steam `libraryfolders.vdf` parsing.

use std::{
    collections::HashSet,
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

pub(super) fn existing_unique_dirs(paths: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();

    for path in paths {
        let Some(path) = normalize_existing_dir(&path) else {
            continue;
        };

        let key = comparable_path_key(&path);

        if seen.insert(key) {
            unique.push(path);
        }
    }

    unique
}

fn normalize_existing_dir(path: &Path) -> Option<PathBuf> {
    let metadata = fs::metadata(path).ok()?;

    if !metadata.is_dir() {
        return None;
    }

    let normalized = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());

    Some(strip_verbatim_prefix(normalized))
}

pub(super) fn comparable_path_key(path: &Path) -> String {
    let mut value = path.to_string_lossy().replace('/', "\\");

    while value.ends_with('\\') {
        value.pop();
    }

    value.to_ascii_lowercase()
}

pub(super) fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let replacement = {
        let value = path.to_string_lossy();

        if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
            Some(PathBuf::from(format!(r"\\{rest}")))
        } else {
            value.strip_prefix(r"\\?\").map(PathBuf::from)
        }
    };

    replacement.unwrap_or(path)
}

pub(super) fn path_from_string(value: &str) -> Option<PathBuf> {
    let value = value.trim_matches('\0').trim().trim_matches('"').trim();

    if value.is_empty() {
        return None;
    }

    Some(PathBuf::from(expand_percent_env_vars(value)))
}

fn expand_percent_env_vars(value: &str) -> String {
    let mut result = String::new();
    let mut rest = value;

    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);

        let after_start = &rest[start + 1..];

        let Some(end) = after_start.find('%') else {
            result.push_str(&rest[start..]);
            return result;
        };

        let name = &after_start[..end];

        if name.is_empty() {
            result.push_str("%%");
        } else if let Some(env_value) = env::var_os(name) {
            result.push_str(&env_value.to_string_lossy());
        } else {
            result.push('%');
            result.push_str(name);
            result.push('%');
        }

        rest = &after_start[end + 1..];
    }

    result.push_str(rest);
    result
}

pub(super) fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name).map(PathBuf::from)
}

pub(super) fn push_env_join(paths: &mut Vec<PathBuf>, env_name: &str, components: &[&str]) {
    let Some(mut path) = env_path(env_name) else {
        return;
    };

    for component in components {
        path.push(component);
    }

    paths.push(path);
}

pub(super) fn has_extension_ignore_ascii_case(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case(expected))
}

pub(super) fn parse_steam_library_roots(content: &str) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for line in content.lines().map(str::trim) {
        let tokens = quoted_tokens(line);

        if tokens.len() < 2 {
            continue;
        }

        let key = &tokens[0];
        let value = &tokens[1];

        // Current Steam format:
        // "path" "D:\\SteamLibrary"
        if key.eq_ignore_ascii_case("path") {
            if let Some(path) = path_from_string(value) {
                roots.push(path);
            }

            continue;
        }

        // Legacy Steam format:
        // "1" "D:\\SteamLibrary"
        if key.parse::<u32>().is_ok() && looks_like_windows_path(value) {
            if let Some(path) = path_from_string(value) {
                roots.push(path);
            }
        }
    }

    roots
}

fn quoted_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut token = String::new();

    let mut in_quotes = false;
    let mut escaped = false;

    for ch in line.chars() {
        if !in_quotes {
            if ch == '"' {
                in_quotes = true;
                token.clear();
                escaped = false;
            }

            continue;
        }

        if escaped {
            match ch {
                '\\' => token.push('\\'),
                '"' => token.push('"'),
                'n' => token.push('\n'),
                't' => token.push('\t'),
                other => {
                    token.push('\\');
                    token.push(other);
                }
            }

            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => {
                tokens.push(std::mem::take(&mut token));
                in_quotes = false;
            }
            other => token.push(other),
        }
    }

    tokens
}

fn looks_like_windows_path(value: &str) -> bool {
    let bytes = value.as_bytes();

    let drive_absolute = bytes.len() >= 3 && bytes[1] == b':' && matches!(bytes[2], b'\\' | b'/');

    drive_absolute || value.starts_with(r"\\")
}
