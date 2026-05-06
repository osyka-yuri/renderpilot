use super::{read_windows_file_version_from_bytes, version_info::parse_version_text};

#[test]
fn non_pe_bytes_have_no_version() {
    assert_eq!(read_windows_file_version_from_bytes(b"not a PE file"), None);
}

#[test]
fn parses_plain_file_version_text() {
    let version = parse_version_text("31.0.15.5244").expect("version should parse");

    assert_eq!(version.as_str(), "31.0.15.5244");
}

#[test]
fn parses_comma_separated_file_version_text() {
    let version = parse_version_text("1, 2, 3, 4").expect("version should parse");

    assert_eq!(version.as_str(), "1.2.3.4");
}

#[test]
fn parses_numeric_prefix_from_decorated_version_text() {
    let version = parse_version_text("3.7.20 beta").expect("version should parse");

    assert_eq!(version.as_str(), "3.7.20");
}

#[test]
fn parses_numeric_version_after_leading_label() {
    let version =
        parse_version_text("File version: 3.7.20 beta").expect("decorated version should parse");

    assert_eq!(version.as_str(), "3.7.20");
}

#[test]
fn returns_none_when_text_contains_no_version() {
    assert_eq!(parse_version_text("beta release"), None);
}
