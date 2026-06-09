use std::fmt::Display;

/// Attempts to load a bundled asset; logs and falls back to a default on failure.
///
/// Bundled assets are `include_str!`-ed JSON files shipped inside the binary.
/// They should always be valid for the current build, but a corrupted build or
/// a version skew between crate updates can produce a parse failure. Prefer
/// graceful degradation (empty state) over a hard panic — the app continues
/// working with reduced functionality rather than crashing at startup.
pub(super) fn load_bundled_asset_or_default<T, E: Display>(
    load: impl FnOnce() -> Result<T, E>,
    fallback: impl FnOnce() -> T,
    name: &str,
) -> T {
    match load() {
        Ok(value) => value,
        Err(error) => {
            log::error!("Bundled asset `{name}` is invalid: {error}; using fallback");
            fallback()
        }
    }
}
