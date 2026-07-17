//! Manifest validation primitives shared by every add-on tool.
//!
//! Each tool's own `validate.rs` still owns its manifest's *shape* (which
//! fields exist, which combinations are legal); the checks below --
//! blank/semver/file-name/hash shape, schema-version gating, a full match-rule
//! set (non-empty, positive tier, non-blank value, per-[`MatchKind`] value
//! shape), a manifest's `defaults` against the Rust-side fallback, and
//! title-id uniqueness -- read identically across tools and live here once.

use std::collections::HashSet;

use super::errors::failed;
use super::matching::{MatchKind, MatchRule};
use crate::ServiceError;

/// Asserts a string field is non-blank (after trimming).
pub(crate) fn ensure_not_blank(field: &str, value: &str) -> Result<(), ServiceError> {
    if value.trim().is_empty() {
        return Err(failed(format!("`{field}` must not be empty")));
    }
    Ok(())
}

/// Asserts a value is a dotted-triple version (e.g. `1.0.0`). `context` names
/// what's being checked (e.g. `"manifest"`, `` "title `{id}`" ``) and prefixes
/// the message ahead of `field`.
pub(crate) fn ensure_semver(context: &str, field: &str, value: &str) -> Result<(), ServiceError> {
    let ok = !value.is_empty()
        && value
            .split('.')
            .map(|part| part.parse::<u32>())
            .all(|part| part.is_ok());
    if !ok {
        return Err(failed(format!(
            "{context} {field} must be a dotted-triple version, got `{value}`"
        )));
    }
    Ok(())
}

/// Asserts a field is a bare, safe file name (non-blank, no path separators).
pub(crate) fn ensure_safe_file_name(field: &str, value: &str) -> Result<(), ServiceError> {
    ensure_not_blank(field, value)?;
    if !crate::paths::is_safe_file_name(value) {
        return Err(failed(format!(
            "`{field}` must be a bare file name, got `{value}`"
        )));
    }
    Ok(())
}

/// Whether `value` is a 64-character lowercase hex SHA-256 digest.
pub(crate) fn is_lowercase_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// Validates a match rule's `value` against the shape its `kind` requires
/// (hash, positive integer id, non-blank id). `title_id` only names the title
/// in the error message.
pub(crate) fn validate_match_rule_value(
    title_id: &str,
    rule: &MatchRule,
) -> Result<(), ServiceError> {
    match rule.kind {
        MatchKind::ExeSha256 if !is_lowercase_sha256_hex(&rule.value) => Err(failed(format!(
            "title `{title_id}` ExeSha256 rule value must be lowercase hex SHA-256"
        ))),
        MatchKind::SteamAppid if !rule.value.parse::<u64>().is_ok_and(|appid| appid > 0) => {
            Err(failed(format!(
                "title `{title_id}` SteamAppid rule value must be a positive integer"
            )))
        }
        MatchKind::EpicId | MatchKind::GogId if rule.value.trim().is_empty() => {
            Err(failed(format!(
                "title `{title_id}` {:?} rule value must not be empty",
                rule.kind
            )))
        }
        _ => Ok(()),
    }
}

/// Compatibility assertion for the legacy RenoDX v3 defaults document.
pub(crate) fn ensure_defaults_match<D: PartialEq>(
    tool: &str,
    actual: &D,
    expected: &D,
) -> Result<(), ServiceError> {
    if actual != expected {
        return Err(failed(format!(
            "{tool} manifest `defaults` do not match the validator's expected defaults \
             (min_app_version / channel drift between generator and parser)"
        )));
    }
    Ok(())
}

/// Validates a slice of match rules: every rule must have a positive tier, a
/// non-blank value unless it's a [`MatchKind::Generic`], and a per-kind valid
/// value shape (delegated to [`validate_match_rule_value`]).
pub(crate) fn validate_match_rules(
    title_id: &str,
    rules: &[MatchRule],
) -> Result<(), ServiceError> {
    if rules.is_empty() {
        return Err(failed(format!(
            "title `{title_id}` must declare at least one match rule"
        )));
    }
    for rule in rules {
        if rule.tier == 0 {
            return Err(failed(format!(
                "title `{title_id}` match rule tier must be greater than zero"
            )));
        }
        if rule.kind != MatchKind::Generic && rule.value.trim().is_empty() {
            return Err(failed(format!(
                "title `{title_id}` match rule of kind {:?} requires a value",
                rule.kind
            )));
        }
        validate_match_rule_value(title_id, rule)?;
    }
    Ok(())
}

/// Rejects a manifest whose `schema_version` doesn't match the version this
/// build understands; `tool` names the add-on in the error message (e.g.
/// `"Luma"`, `"RenoDX"`).
pub(crate) fn ensure_schema_version(
    tool: &str,
    actual: u32,
    supported: u32,
) -> Result<(), ServiceError> {
    if actual != supported {
        return Err(failed(format!(
            "unsupported {tool} manifest schema version: expected {supported}, got {actual}"
        )));
    }
    Ok(())
}

/// Rejects a manifest whose titles reuse an id -- installs and tracking index
/// by title id, so a collision would silently shadow one title.
pub(crate) fn ensure_unique_title_ids<'a>(
    ids: impl IntoIterator<Item = &'a str>,
) -> Result<(), ServiceError> {
    let mut seen: HashSet<&str> = HashSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(failed(format!("duplicate title id `{id}` in manifest")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_blank_rejects_empty_and_whitespace() {
        assert!(ensure_not_blank("field", "value").is_ok());
        assert!(ensure_not_blank("field", "").is_err());
        assert!(ensure_not_blank("field", "   ").is_err());
    }

    #[test]
    fn semver_requires_dotted_integer_parts() {
        assert!(ensure_semver("manifest", "version", "1.2.3").is_ok());
        assert!(ensure_semver("manifest", "version", "1.2").is_ok());
        assert!(ensure_semver("manifest", "version", "").is_err());
        assert!(ensure_semver("manifest", "version", "not-a-version").is_err());
    }

    #[test]
    fn safe_file_name_rejects_path_separators() {
        assert!(ensure_safe_file_name("field", "dgVoodoo.conf").is_ok());
        assert!(ensure_safe_file_name("field", "../dgVoodoo.conf").is_err());
        assert!(ensure_safe_file_name("field", "sub/dgVoodoo.conf").is_err());
    }

    #[test]
    fn sha256_hex_requires_64_lowercase_hex_chars() {
        assert!(is_lowercase_sha256_hex(&"a".repeat(64)));
        assert!(!is_lowercase_sha256_hex(&"A".repeat(64)));
        assert!(!is_lowercase_sha256_hex(&"a".repeat(63)));
    }

    #[test]
    fn match_rule_value_is_checked_per_kind() {
        let rule = |kind: MatchKind, value: &str| MatchRule {
            kind,
            value: value.to_owned(),
            tier: 100,
        };
        assert!(
            validate_match_rule_value("t", &rule(MatchKind::ExeSha256, &"a".repeat(64))).is_ok()
        );
        assert!(validate_match_rule_value("t", &rule(MatchKind::ExeSha256, "not-a-hash")).is_err());
        assert!(validate_match_rule_value("t", &rule(MatchKind::SteamAppid, "100")).is_ok());
        assert!(validate_match_rule_value("t", &rule(MatchKind::SteamAppid, "0")).is_err());
        assert!(validate_match_rule_value("t", &rule(MatchKind::EpicId, "abc")).is_ok());
        assert!(validate_match_rule_value("t", &rule(MatchKind::EpicId, "")).is_err());
    }

    #[test]
    fn unique_title_ids_rejects_repeats() {
        assert!(ensure_unique_title_ids(["a", "b", "c"]).is_ok());
        assert!(ensure_unique_title_ids(["a", "b", "a"]).is_err());
    }

    #[test]
    fn schema_version_accepts_match() {
        assert!(ensure_schema_version("Luma", 1, 1).is_ok());
    }

    #[test]
    fn schema_version_rejects_mismatch() {
        assert!(ensure_schema_version("Luma", 1, 2).is_err());
    }

    #[test]
    fn match_rules_rejects_empty() {
        assert!(validate_match_rules("t", &[]).is_err());
    }

    #[test]
    fn match_rules_rejects_zero_tier() {
        let rule = MatchRule {
            kind: MatchKind::Generic,
            value: String::new(),
            tier: 0,
        };
        assert!(validate_match_rules("t", &[rule]).is_err());
    }

    #[test]
    fn match_rules_requires_nonblank_value_for_non_generic() {
        let rule = MatchRule {
            kind: MatchKind::SteamAppid,
            value: String::new(),
            tier: 1,
        };
        assert!(validate_match_rules("t", &[rule]).is_err());
    }

    #[test]
    fn match_rules_accepts_generic_with_empty_value() {
        let rule = MatchRule {
            kind: MatchKind::Generic,
            value: String::new(),
            tier: 1,
        };
        assert!(validate_match_rules("t", &[rule]).is_ok());
    }

    #[test]
    fn match_rules_accepts_valid_steam_appid() {
        let rule = MatchRule {
            kind: MatchKind::SteamAppid,
            value: "730".to_owned(),
            tier: 100,
        };
        assert!(validate_match_rules("t", &[rule]).is_ok());
    }
}
