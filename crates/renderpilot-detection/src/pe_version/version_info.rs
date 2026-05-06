use renderpilot_domain::Version;

use super::binary::{
    align4, checked_range, read_u16, read_u32, read_utf16_null_terminated, read_utf16_value,
};

const VS_FIXEDFILEINFO_SIGNATURE: u32 = 0xfeef_04bd;
const STRING_VERSION_KEYS: [&str; 2] = ["FileVersion", "ProductVersion"];
const FIXED_VERSION_OFFSETS: [usize; 2] = [8, 16];

#[derive(Debug, Clone)]
pub(super) struct VersionInfo<'a> {
    bytes: &'a [u8],
    root: VersionBlock,
}

impl<'a> VersionInfo<'a> {
    pub(super) fn parse(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            root: VersionBlock::parse(bytes, 0, bytes.len()),
        }
    }

    pub(super) fn version(&self) -> Option<Version> {
        if self.root.key != "VS_VERSION_INFO" {
            return None;
        }

        self.string_version().or_else(|| self.fixed_version())
    }

    fn string_version(&self) -> Option<Version> {
        STRING_VERSION_KEYS.into_iter().find_map(|key| {
            self.string_value(key)
                .and_then(|value| parse_version_text(&value))
        })
    }

    fn fixed_version(&self) -> Option<Version> {
        FIXED_VERSION_OFFSETS
            .into_iter()
            .find_map(|offset| self.fixed_version_at(offset))
    }

    fn string_value(&self, key: &str) -> Option<String> {
        find_string_value(self.bytes, self.root.children_offset, self.root.end, key)
    }

    fn fixed_version_at(&self, offset: usize) -> Option<Version> {
        let value = checked_range(self.bytes, self.root.value_offset, self.root.value_length)?;

        if read_u32(value, 0)? != VS_FIXEDFILEINFO_SIGNATURE {
            return None;
        }

        let version_ms = read_u32(value, offset)?;
        let version_ls = read_u32(value, offset.checked_add(4)?)?;
        let parts = [
            version_ms >> 16,
            version_ms & 0xffff,
            version_ls >> 16,
            version_ls & 0xffff,
        ];

        Version::parse(format!(
            "{}.{}.{}.{}",
            parts[0], parts[1], parts[2], parts[3]
        ))
        .ok()
    }
}

#[derive(Debug, Clone)]
struct VersionBlock {
    length: usize,
    value_length: usize,
    value_type: u16,
    key: String,
    value_offset: usize,
    children_offset: usize,
    end: usize,
}

impl VersionBlock {
    fn parse(bytes: &[u8], offset: usize, limit: usize) -> Self {
        Self::try_parse(bytes, offset, limit).unwrap_or_else(Self::empty)
    }

    fn try_parse(bytes: &[u8], offset: usize, limit: usize) -> Option<Self> {
        let offset = align4(offset)?;
        checked_range(bytes, offset, 6)?;

        let length = usize::from(read_u16(bytes, offset)?);
        let value_length = usize::from(read_u16(bytes, offset.checked_add(2)?)?);
        let value_type = read_u16(bytes, offset.checked_add(4)?)?;

        if length < 6 {
            return None;
        }

        let end = offset.checked_add(length)?;

        if end > limit || end > bytes.len() {
            return None;
        }

        let (key, after_key) = read_utf16_null_terminated(bytes, offset.checked_add(6)?, end)?;
        let value_offset = align4(after_key)?;
        let value_length_bytes = if value_type == 1 {
            value_length.checked_mul(2)?
        } else {
            value_length
        };
        let value_end = value_offset.checked_add(value_length_bytes)?;

        if value_end > end {
            return None;
        }

        let children_offset = align4(value_end)?;

        Some(Self {
            length,
            value_length: value_length_bytes,
            value_type,
            key,
            value_offset,
            children_offset,
            end,
        })
    }

    fn empty() -> Self {
        Self {
            length: 0,
            value_length: 0,
            value_type: 0,
            key: String::new(),
            value_offset: 0,
            children_offset: 0,
            end: 0,
        }
    }
}

fn find_string_value(bytes: &[u8], offset: usize, limit: usize, key: &str) -> Option<String> {
    let mut cursor = offset;

    while cursor < limit {
        cursor = align4(cursor)?;

        if cursor.checked_add(6)? > limit {
            break;
        }

        let block = VersionBlock::try_parse(bytes, cursor, limit)?;

        if block.key.eq_ignore_ascii_case(key) && block.value_type == 1 && block.value_length > 0 {
            let value = read_utf16_value(bytes, block.value_offset, block.value_length / 2)?;
            let trimmed = value.trim();

            if !trimmed.is_empty() {
                return Some(trimmed.to_owned());
            }
        }

        if let Some(value) = find_string_value(bytes, block.children_offset, block.end, key) {
            return Some(value);
        }

        cursor = cursor.checked_add(block.length)?;
    }

    None
}

pub(super) fn parse_version_text(value: &str) -> Option<Version> {
    normalized_version_text(value)
        .and_then(|normalized| Version::parse(&normalized).ok())
        .or_else(|| parse_first_numeric_version(value))
}

fn normalized_version_text(value: &str) -> Option<String> {
    let normalized: String = value
        .chars()
        .filter_map(|character| match character {
            ',' => Some('.'),
            character if character.is_whitespace() => None,
            character => Some(character),
        })
        .collect();

    if normalized.is_empty() {
        return None;
    }

    Some(normalized)
}

fn parse_first_numeric_version(value: &str) -> Option<Version> {
    let mut candidate = String::new();

    for character in value.chars() {
        if character.is_ascii_digit() || character == '.' || character == ',' {
            candidate.push(if character == ',' { '.' } else { character });
            continue;
        }

        if let Some(version) = parse_candidate_version(&candidate) {
            return Some(version);
        }

        candidate.clear();
    }

    parse_candidate_version(&candidate)
}

fn parse_candidate_version(candidate: &str) -> Option<Version> {
    let trimmed = candidate.trim_matches('.');

    if trimmed.is_empty() {
        return None;
    }

    Version::parse(trimmed).ok()
}
