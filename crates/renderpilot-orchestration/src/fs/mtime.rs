//! File mtime helpers and HTTP-date conversion for upstream Last-Modified.

use std::path::Path;
use std::time::{Duration, SystemTime};

use crate::ServiceError;

/// Best-effort mtime stamping helper for installed files whose upstream date is
/// meaningful to users. Callers decide whether a failure is fatal.
pub(crate) fn set_file_mtime(path: &Path, modified: SystemTime) -> Result<(), ServiceError> {
    filetime::set_file_mtime(path, filetime::FileTime::from_system_time(modified)).map_err(
        |error| {
            crate::failed(format!(
                "failed to set modified time for `{}`: {error}",
                path.display()
            ))
        },
    )
}

/// Parses an HTTP-date header into a [`SystemTime`].
pub(crate) fn parse_http_date(value: &str) -> Result<SystemTime, ServiceError> {
    httpdate::parse_http_date(value)
        .map_err(|error| crate::failed(format!("failed to parse HTTP date `{value}`: {error}")))
}

/// Formats a [`SystemTime`] as IMF-fixdate for DTO compatibility with existing UI
/// date parsing.
#[must_use]
pub(crate) fn format_http_date(time: SystemTime) -> String {
    httpdate::fmt_http_date(time)
}

/// Treat file mtimes far in the future as unreliable metadata.
#[must_use]
pub(crate) fn is_reasonable_file_mtime(time: SystemTime) -> bool {
    let future_slop = Duration::from_hours(24);
    time <= SystemTime::now()
        .checked_add(future_slop)
        .unwrap_or(SystemTime::now())
}

/// Best-effort mtime stamp for an installed file, from its upstream `Last-Modified`
/// HTTP-date (preferred) or a `fallback` time. A missing/unparseable date with no
/// fallback is a no-op; a stamping failure is logged, never fatal -- the file's
/// bytes are already in place, only the displayed date is affected.
pub(crate) fn stamp_mtime_best_effort(
    path: &Path,
    last_modified: Option<&str>,
    fallback: Option<SystemTime>,
) {
    let Some(time) = last_modified
        .and_then(|value| parse_http_date(value).ok())
        .or(fallback)
    else {
        return;
    };
    if let Err(error) = set_file_mtime(path, time) {
        log::warn!("mtime stamp skipped for `{}`: {error}", path.display());
    }
}
