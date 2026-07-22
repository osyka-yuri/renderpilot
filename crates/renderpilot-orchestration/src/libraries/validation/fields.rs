use renderpilot_domain::Version;

use crate::ServiceError;

use super::super::library_error;

const SUPPORTED_SCHEMA_VERSION: u32 = 1;

pub(super) fn validate_schema(actual: u32, label: &str) -> Result<(), ServiceError> {
    if actual != SUPPORTED_SCHEMA_VERSION {
        return Err(library_error(format!(
            "unsupported {label} schema version: expected {SUPPORTED_SCHEMA_VERSION}, got {actual}"
        )));
    }
    Ok(())
}

pub(super) fn ensure_not_blank(field: &str, value: &str) -> Result<(), ServiceError> {
    if value.trim().is_empty() {
        return Err(library_error(format!(
            "catalog field `{field}` must not be empty"
        )));
    }
    Ok(())
}

pub(super) fn ensure_sha256(field: &str, value: &str) -> Result<(), ServiceError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(library_error(format!(
            "catalog field `{field}` must be a lowercase SHA-256 digest"
        )));
    }
    Ok(())
}

pub(super) fn ensure_id(field: &str, value: &str) -> Result<(), ServiceError> {
    let mut bytes = value.bytes();
    if !bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
        || !bytes.all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err(library_error(format!(
            "catalog field `{field}` is not a valid identifier: `{value}`"
        )));
    }
    Ok(())
}

pub(super) fn ensure_dll_name(field: &str, value: &str) -> Result<(), ServiceError> {
    if !value.to_ascii_lowercase().ends_with(".dll")
        || value.is_empty()
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
    {
        return Err(library_error(format!(
            "catalog field `{field}` is not a safe DLL basename: `{value}`"
        )));
    }
    Ok(())
}

pub(super) fn ensure_numeric_version(field: &str, value: &str) -> Result<(), ServiceError> {
    let version = Version::parse(value).map_err(|error| {
        library_error(format!(
            "catalog field `{field}` is not a dotted numeric version: {error}"
        ))
    })?;
    if version.as_str() != value {
        return Err(library_error(format!(
            "catalog field `{field}` is not in canonical form: `{value}`"
        )));
    }
    Ok(())
}
