//! Shared timestamp formatting and strict canonical diagnostic filename parsing.

use std::time::SystemTime;

use jiff::Timestamp;

const FILE_TIMESTAMP_FORMAT: &str = "%Y-%m-%d_%H-%M-%SZ";
const EVENT_TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3f%z";
const FILE_TIMESTAMP_LEN: usize = 20;
const DISPLAY_ID_LEN: usize = 16;

#[cfg(all(windows, feature = "portable"))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PortableDiagnosticName {
    Supervisor { display_id: String },
    App { display_id: String, segment: u32 },
}

#[cfg(not(all(windows, feature = "portable")))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstalledDiagnosticName {
    pub(crate) display_id: String,
    pub(crate) segment: u32,
}

/// Formats one event timestamp as RFC3339 UTC with exactly millisecond precision.
pub(crate) fn format_timestamp_utc(time: SystemTime) -> Option<String> {
    let timestamp = Timestamp::try_from(time).ok()?;
    Some(format!("{timestamp:.3}"))
}

/// Formats the stable, Windows-safe session timestamp used in filenames.
/// A clock outside Jiff's representable range does not disable file logging.
pub(crate) fn format_filename_timestamp(time: SystemTime) -> String {
    Timestamp::try_from(time)
        .ok()
        .map(|timestamp| timestamp.strftime(FILE_TIMESTAMP_FORMAT).to_string())
        .filter(|value| is_valid_filename_timestamp(value))
        .unwrap_or_else(|| "undated".to_owned())
}

/// Returns the compact prefix of a full lowercase-hex identity of the expected size.
pub(crate) fn display_id(identity: &str, full_length: usize) -> Option<&str> {
    if !is_hex(identity, full_length) {
        return None;
    }
    identity.get(..DISPLAY_ID_LEN)
}

/// Builds the common timestamp and display-ID prefix for a canonical filename.
pub(crate) fn canonical_filename_prefix(
    timestamp: &str,
    identity: &str,
    full_identity_length: usize,
) -> Option<String> {
    if !is_valid_filename_timestamp(timestamp) {
        return None;
    }
    let display_id = display_id(identity, full_identity_length)?;
    Some(format!("{timestamp}-{display_id}"))
}

/// Confirms that a parsed filename display ID is the prefix of a full identity.
pub(crate) fn display_id_matches(display: &str, identity: &str, full_length: usize) -> bool {
    display_id(identity, full_length) == Some(display)
}

/// Validates the exact RFC3339 UTC millisecond representation written by the profiles.
pub(crate) fn is_valid_timestamp_utc(value: &str) -> bool {
    let original = value;
    let Some(value) = value.strip_suffix('Z') else {
        return false;
    };
    let value = format!("{value}+0000");
    let Ok(timestamp) = Timestamp::strptime(EVENT_TIMESTAMP_FORMAT, &value) else {
        return false;
    };
    format!("{timestamp:.3}") == original
}

#[cfg(not(all(windows, feature = "portable")))]
pub(crate) fn parse_installed_diagnostic_name(name: &str) -> Option<InstalledDiagnosticName> {
    let stem = name.strip_suffix(".log")?;
    let (_timestamp, display_id, suffix) = split_filename_prefix(stem)?;
    let segment = suffix.strip_prefix("-s")?;
    Some(InstalledDiagnosticName {
        display_id: display_id.to_owned(),
        segment: parse_segment(segment)?,
    })
}

#[cfg(all(windows, feature = "portable"))]
pub(crate) fn parse_portable_diagnostic_name(name: &str) -> Option<PortableDiagnosticName> {
    let stem = name.strip_suffix(".log")?;
    let (_timestamp, display_id, suffix) = split_filename_prefix(stem)?;
    if suffix.is_empty() {
        return Some(PortableDiagnosticName::Supervisor {
            display_id: display_id.to_owned(),
        });
    }
    let segment = suffix.strip_prefix("-s")?;
    Some(PortableDiagnosticName::App {
        display_id: display_id.to_owned(),
        segment: parse_segment(segment)?,
    })
}

fn split_filename_prefix(stem: &str) -> Option<(&str, &str, &str)> {
    let (timestamp, remainder) = if let Some(remainder) = stem.strip_prefix("undated-") {
        ("undated", remainder)
    } else {
        let timestamp = stem.get(..FILE_TIMESTAMP_LEN)?;
        if !is_valid_filename_timestamp(timestamp) {
            return None;
        }
        (
            timestamp,
            stem.get(FILE_TIMESTAMP_LEN + 1..)
                .filter(|_| stem.as_bytes().get(FILE_TIMESTAMP_LEN) == Some(&b'-'))?,
        )
    };
    let display_id = remainder.get(..DISPLAY_ID_LEN)?;
    if !is_hex(display_id, DISPLAY_ID_LEN) {
        return None;
    }
    let suffix = remainder.get(DISPLAY_ID_LEN..)?;
    if !suffix.is_empty() && !suffix.starts_with('-') {
        return None;
    }
    Some((timestamp, display_id, suffix))
}

fn parse_segment(value: &str) -> Option<u32> {
    if !is_hex(value, 8) {
        return None;
    }
    u32::from_str_radix(value, 16).ok()
}

fn is_valid_filename_timestamp(value: &str) -> bool {
    if value == "undated" {
        return true;
    }
    if value.len() != FILE_TIMESTAMP_LEN {
        return false;
    }
    let Some((date, time)) = value.split_once('_') else {
        return false;
    };
    let Some(time) = time.strip_suffix('Z') else {
        return false;
    };
    let mut parts = time.split('-');
    let (Some(hour), Some(minute), Some(second), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let rfc3339 = format!("{date}T{hour}:{minute}:{second}+0000");
    let Ok(timestamp) = Timestamp::strptime("%Y-%m-%dT%H:%M:%S%z", rfc3339) else {
        return false;
    };
    timestamp.strftime(FILE_TIMESTAMP_FORMAT).to_string() == value
}

fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::{
        canonical_filename_prefix, display_id_matches, format_filename_timestamp,
        format_timestamp_utc, is_valid_timestamp_utc,
    };

    const START: &str = "2026-09-23_14-35-12Z";

    #[cfg(not(all(windows, feature = "portable")))]
    #[test]
    fn installed_names_require_a_valid_timestamp_compact_id_and_segment() {
        use super::{InstalledDiagnosticName, parse_installed_diagnostic_name};

        let id = "a".repeat(16);
        assert_eq!(
            parse_installed_diagnostic_name(&format!("{START}-{id}-s0000000f.log")),
            Some(InstalledDiagnosticName {
                display_id: id.clone(),
                segment: 15,
            })
        );
        assert!(parse_installed_diagnostic_name(&format!("undated-{id}-s00000000.log")).is_some());
        for name in [
            format!("{}.log", "a".repeat(32)),
            format!("{START}-{}-s0000000F.log", id),
            format!("2026-02-30_14-35-12Z-{id}-s00000000.log"),
            format!("{START}-{}-s00000000.tmp", id),
        ] {
            assert!(
                parse_installed_diagnostic_name(&name).is_none(),
                "unexpectedly accepted {name}"
            );
        }
    }

    #[test]
    fn event_and_filename_timestamps_are_human_readable_utc() {
        let time = UNIX_EPOCH + Duration::from_millis(123);
        assert_eq!(
            format_timestamp_utc(time),
            Some("1970-01-01T00:00:00.123Z".to_owned())
        );
        assert!(is_valid_timestamp_utc("1970-01-01T00:00:00.123Z"));
        assert!(!is_valid_timestamp_utc("1970-01-01 00:00:00.123Z"));
        assert_eq!(format_filename_timestamp(time), "1970-01-01_00-00-00Z");
    }

    #[test]
    fn filename_prefix_uses_the_compact_prefix_of_a_full_identity() {
        let identity = "a".repeat(64);
        let prefix = canonical_filename_prefix(START, &identity, 64).expect("canonical prefix");
        assert_eq!(prefix, format!("{START}-{}", "a".repeat(16)));
        assert!(display_id_matches(&"a".repeat(16), &identity, 64));
        assert!(!display_id_matches(&"b".repeat(16), &identity, 64));
    }

    #[cfg(all(windows, feature = "portable"))]
    #[test]
    fn portable_names_accept_only_current_canonical_timestamped_shapes() {
        use super::{PortableDiagnosticName, parse_portable_diagnostic_name};

        let id = "a".repeat(16);
        assert_eq!(
            parse_portable_diagnostic_name(&format!("{START}-{id}.log")),
            Some(PortableDiagnosticName::Supervisor {
                display_id: id.clone(),
            })
        );
        assert_eq!(
            parse_portable_diagnostic_name(&format!("undated-{id}.log")),
            Some(PortableDiagnosticName::Supervisor {
                display_id: id.clone(),
            })
        );
        assert_eq!(
            parse_portable_diagnostic_name(&format!("undated-{id}-s00000000.log")),
            Some(PortableDiagnosticName::App {
                display_id: id.clone(),
                segment: 0,
            })
        );
        assert_eq!(
            parse_portable_diagnostic_name(&format!("{START}-{id}-s0000000f.log")),
            Some(PortableDiagnosticName::App {
                display_id: id,
                segment: 15,
            })
        );
    }

    #[cfg(all(windows, feature = "portable"))]
    #[test]
    fn portable_parser_rejects_old_malformed_and_wrong_role_shapes() {
        use super::parse_portable_diagnostic_name;

        let id = "a".repeat(16);
        for name in [
            format!("{}.log", "a".repeat(64)),
            format!("{START}-{}-s00000000.log", "a".repeat(64)),
            format!("2026-99-23_14-35-12Z-{id}.log"),
            format!("{START}-{}-s0000000F.log", id),
            format!("{START}-{}-s0000000.log", id),
            format!("{START}-{}-s000000000.log", id),
            format!("{START}-{}-extra.log", id),
            format!("{START}-{}-{}.log", id, "b".repeat(16)),
        ] {
            assert!(
                parse_portable_diagnostic_name(&name).is_none(),
                "unexpectedly accepted {name}"
            );
        }
    }
}
