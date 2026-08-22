use std::path::Path;

pub(crate) fn manifest_path_looks_reshade(path: &Path) -> bool {
    path.to_string_lossy()
        .rsplit(['/', '\\'])
        .next()
        .is_some_and(|name| name.to_ascii_lowercase().contains("reshade"))
}

/// Recognizes both the host's absolute-path syntax and absolute Windows paths.
/// The Vulkan registry and manifests always carry Windows paths, even when the
/// pure parser and planner are exercised by a non-Windows build host.
pub(crate) fn is_absolute(path: &Path) -> bool {
    if path.is_absolute() {
        return true;
    }
    let value = path.to_string_lossy();
    let bytes = value.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\'))
        || value.starts_with(r"\\")
}

/// Compares two paths as the same file. NTFS is case-insensitive but
/// case-preserving, so a lowercased string compare after best-effort
/// canonicalization is enough — no need to open either file.
pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    canonicalize_best_effort(a) == canonicalize_best_effort(b)
}

/// Canonicalizes when possible (resolves `.`/`..` and symlinks); falls back to
/// the path as given when it doesn't exist yet, which is routine here — e.g.
/// comparing a registered manifest path against one we're about to write.
fn canonicalize_best_effort(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}
