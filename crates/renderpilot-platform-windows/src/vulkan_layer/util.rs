use std::path::Path;

pub(crate) fn manifest_path_looks_reshade(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.to_ascii_lowercase().contains("reshade"))
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
