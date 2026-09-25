use super::{EngineIniEntry, EngineIniError, IniEncoding};
use sha2::{Digest, Sha256};

pub(super) struct IniDocument {
    pub(super) bytes: Vec<u8>,
    pub(super) encoding: IniEncoding,
    pub(super) lines: Vec<IniLine>,
}

impl IniDocument {
    pub(super) fn empty() -> Self {
        Self {
            bytes: Vec::new(),
            encoding: IniEncoding::Utf8,
            lines: Vec::new(),
        }
    }

    pub(super) fn sections(&self) -> Result<IniSections<'_>, EngineIniError> {
        let mut groups = Vec::new();
        let mut current: Option<(String, usize)> = None;
        for (index, line) in self.lines.iter().enumerate() {
            if let Some(name) = section_name(&line.text) {
                if let Some(prev) = current.take() {
                    groups.push(prev);
                }
                current = Some((name.to_owned(), index));
            }
        }
        if let Some(section) = current {
            groups.push(section);
        }
        Ok(IniSections {
            document: self,
            groups,
        })
    }
}

#[derive(Debug, Clone)]
pub(super) struct IniLine {
    pub(super) start: usize,
    pub(super) text: String,
}

pub(super) struct IniSections<'a> {
    document: &'a IniDocument,
    groups: Vec<(String, usize)>,
}

pub(super) enum FindValue {
    Exact,
    Missing,
    Conflict,
    Ambiguous,
}

impl IniSections<'_> {
    pub(super) fn find_value(&self, section: &str, key: &str, desired: &str) -> FindValue {
        let mut matches = self
            .groups
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(section));
        let Some((_, start)) = matches.next() else {
            return FindValue::Missing;
        };
        if matches.next().is_some() {
            return FindValue::Ambiguous;
        }
        let end = self
            .groups
            .iter()
            .filter(|(_, index)| index > start)
            .map(|(_, index)| *index)
            .min()
            .unwrap_or(self.document.lines.len());
        let mut found_value = None;
        for line in &self.document.lines[*start + 1..end] {
            if let Some((candidate_key, value)) = assignment(&line.text)
                && candidate_key.eq_ignore_ascii_case(key)
            {
                if found_value.is_some() {
                    return FindValue::Ambiguous;
                }
                found_value = Some(value);
            }
        }
        match found_value {
            None => FindValue::Missing,
            Some(value) if value.trim() == desired.trim() => FindValue::Exact,
            Some(_) => FindValue::Conflict,
        }
    }

    pub(super) fn has_section(&self, section: &str) -> Result<bool, EngineIniError> {
        let count = self
            .groups
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(section))
            .count();
        if count > 1 {
            return Err(EngineIniError::AmbiguousTarget(format!(
                "duplicate section [{}]",
                section
            )));
        }
        Ok(count == 1)
    }

    pub(super) fn insertion_offset(&self, section: &str) -> Result<usize, EngineIniError> {
        let mut matched = self
            .groups
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(section));
        let start = match (matched.next(), matched.next()) {
            (Some((_, start)), None) => *start,
            _ => {
                return Err(EngineIniError::AmbiguousTarget(format!(
                    "section [{}] is not unique",
                    section
                )));
            }
        };
        let end = self
            .groups
            .iter()
            .filter(|(_, index)| *index > start)
            .map(|(_, index)| *index)
            .min()
            .unwrap_or(self.document.lines.len());
        Ok(self
            .document
            .lines
            .get(end)
            .map_or(self.document.bytes.len(), |line| line.start))
    }

    pub(super) fn preferred_newline(&self) -> &'static str {
        match self.document.encoding {
            IniEncoding::Utf16Le => {
                if self
                    .document
                    .bytes
                    .windows(4)
                    .any(|window| window == [13, 0, 10, 0])
                {
                    "\r\n"
                } else if self
                    .document
                    .bytes
                    .windows(2)
                    .any(|window| window == [13, 0])
                {
                    "\r"
                } else {
                    "\n"
                }
            }
            IniEncoding::Utf16Be => {
                if self
                    .document
                    .bytes
                    .windows(4)
                    .any(|window| window == [0, 13, 0, 10])
                {
                    "\r\n"
                } else if self
                    .document
                    .bytes
                    .windows(2)
                    .any(|window| window == [0, 13])
                {
                    "\r"
                } else {
                    "\n"
                }
            }
            IniEncoding::Utf8 | IniEncoding::Utf8Bom => {
                if self
                    .document
                    .bytes
                    .windows(2)
                    .any(|window| window == b"\r\n")
                {
                    "\r\n"
                } else if self.document.bytes.contains(&b'\r') {
                    "\r"
                } else {
                    "\n"
                }
            }
        }
    }
}

pub(super) fn group_by_section(mut entries: Vec<EngineIniEntry>) -> Vec<Vec<EngineIniEntry>> {
    entries.sort_by_key(|entry| (ascii_key(&entry.section), ascii_key(&entry.key)));
    let mut groups: Vec<Vec<EngineIniEntry>> = Vec::new();
    for entry in entries {
        if let Some(group) = groups
            .last_mut()
            .filter(|group| group[0].section.eq_ignore_ascii_case(&entry.section))
        {
            group.push(entry);
        } else {
            groups.push(vec![entry]);
        }
    }
    groups
}

pub(super) fn decode_document(bytes: &[u8]) -> Result<(IniEncoding, IniDocument), EngineIniError> {
    if bytes.starts_with(&[0xFF, 0xFE, 0, 0]) || bytes.starts_with(&[0, 0, 0xFE, 0xFF]) {
        return Err(EngineIniError::InvalidEncoding(
            "UTF-32 is unsupported".to_owned(),
        ));
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return decode_utf8(bytes, IniEncoding::Utf8Bom, 3);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16(bytes, IniEncoding::Utf16Le, true, 2);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16(bytes, IniEncoding::Utf16Be, false, 2);
    }
    if bytes.contains(&0) {
        return Err(EngineIniError::InvalidEncoding(
            "NUL without a supported BOM".to_owned(),
        ));
    }
    decode_utf8(bytes, IniEncoding::Utf8, 0)
}

pub(super) fn decode_utf8(
    bytes: &[u8],
    encoding: IniEncoding,
    offset: usize,
) -> Result<(IniEncoding, IniDocument), EngineIniError> {
    // Lossless for valid UTF-8; arbitrary single-byte content remains byte
    // preserved and is compared through a replacement string only.
    let body = &bytes[offset..];
    let lines = byte_lines(body, offset);
    Ok((
        encoding,
        IniDocument {
            bytes: bytes.to_vec(),
            encoding,
            lines,
        },
    ))
}

pub(super) fn decode_utf16(
    bytes: &[u8],
    encoding: IniEncoding,
    little_endian: bool,
    offset: usize,
) -> Result<(IniEncoding, IniDocument), EngineIniError> {
    let body = &bytes[offset..];
    if !body.len().is_multiple_of(2) {
        return Err(EngineIniError::InvalidEncoding(
            "odd UTF-16 byte length".to_owned(),
        ));
    }
    let mut units = Vec::with_capacity(body.len() / 2);
    for pair in body.as_chunks::<2>().0 {
        units.push(if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        });
    }
    if std::char::decode_utf16(units.iter().copied()).any(|value| value.is_err()) {
        return Err(EngineIniError::InvalidEncoding(
            "malformed UTF-16".to_owned(),
        ));
    }
    let lines = utf16_lines(&units, offset);
    Ok((
        encoding,
        IniDocument {
            bytes: bytes.to_vec(),
            encoding,
            lines,
        },
    ))
}

fn byte_lines(body: &[u8], base: usize) -> Vec<IniLine> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < body.len() {
        let newline_len = if body[index] == b'\r' {
            if body.get(index + 1) == Some(&b'\n') {
                2
            } else {
                1
            }
        } else if body[index] == b'\n' {
            1
        } else {
            0
        };
        if newline_len != 0 {
            lines.push(IniLine {
                start: base + start,
                text: String::from_utf8_lossy(&body[start..index]).into_owned(),
            });
            index += newline_len;
            start = index;
        } else {
            index += 1;
        }
    }
    if start < body.len() || body.is_empty() {
        lines.push(IniLine {
            start: base + start,
            text: String::from_utf8_lossy(&body[start..]).into_owned(),
        });
    }
    lines
}

fn utf16_lines(units: &[u16], base: usize) -> Vec<IniLine> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < units.len() {
        let newline_len = if units[index] == 13 {
            if units.get(index + 1) == Some(&10) {
                2
            } else {
                1
            }
        } else if units[index] == 10 {
            1
        } else {
            0
        };
        if newline_len != 0 {
            lines.push(utf16_line(units, start, index, base));
            index += newline_len;
            start = index;
        } else {
            index += 1;
        }
    }
    if start < units.len() || units.is_empty() {
        lines.push(utf16_line(units, start, units.len(), base));
    }
    lines
}

fn utf16_line(units: &[u16], start: usize, end: usize, base: usize) -> IniLine {
    let text = String::from_utf16(&units[start..end]).unwrap_or_default();
    IniLine {
        start: base + start * 2,
        text,
    }
}

pub(crate) fn encode_text(text: &str, encoding: IniEncoding) -> Vec<u8> {
    match encoding {
        IniEncoding::Utf8 => text.as_bytes().to_vec(),
        IniEncoding::Utf8Bom => text.as_bytes().to_vec(),
        IniEncoding::Utf16Le => text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
        IniEncoding::Utf16Be => text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
    }
}

pub(super) fn encode_line(text: &str, newline: &str, encoding: IniEncoding) -> Vec<u8> {
    encode_text(&format!("{text}{newline}"), encoding)
}

pub(super) fn encode_assignment(
    key: &str,
    value: &str,
    newline: &str,
    encoding: IniEncoding,
) -> Vec<u8> {
    encode_text(&format!("{key}={value}{newline}"), encoding)
}

pub(super) fn render_new_document(entries: &[EngineIniEntry], encoding: IniEncoding) -> Vec<u8> {
    let mut out = match encoding {
        IniEncoding::Utf8Bom => vec![0xEF, 0xBB, 0xBF],
        IniEncoding::Utf16Le => vec![0xFF, 0xFE],
        IniEncoding::Utf16Be => vec![0xFE, 0xFF],
        IniEncoding::Utf8 => Vec::new(),
    };
    for (index, group) in group_by_section(entries.to_vec()).into_iter().enumerate() {
        if index > 0 {
            out.extend(encode_text("\r\n", encoding));
        }
        out.extend(encode_line(
            &format!("[{}]", group[0].section),
            "\r\n",
            encoding,
        ));
        for entry in group {
            out.extend(encode_assignment(
                &entry.key,
                &entry.value,
                "\r\n",
                encoding,
            ));
        }
    }
    out
}

pub(super) fn encode_document(document: &IniDocument) -> Vec<u8> {
    document.bytes.clone()
}

fn section_name(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    trimmed
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
}

fn assignment(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim();
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim();
    (!key.is_empty()).then_some((key, value.trim()))
}

/// Returns exact line spans for one scalar assignment in one section.
///
/// The section/key/value checks are deliberately repeated here instead of
/// relying on the byte line alone.  This keeps receipts syntactic: a copied
/// assignment in a different section, a comment, or a duplicate target never
/// becomes removable ownership by accident.
pub(super) fn contribution_ranges_in_document(
    bytes: &[u8],
    document: &IniDocument,
    section: &str,
    key: &str,
    value: &str,
    expected_line: &[u8],
) -> Vec<(usize, usize)> {
    let mut current_section = None::<&str>;
    let mut matches = Vec::new();
    for (index, line) in document.lines.iter().enumerate() {
        if let Some(name) = section_name(&line.text) {
            current_section = Some(name);
            continue;
        }
        let Some(current_section) = current_section else {
            continue;
        };
        let Some((actual_key, actual_value)) = assignment(&line.text) else {
            continue;
        };
        if !current_section.eq_ignore_ascii_case(section)
            || !actual_key.eq_ignore_ascii_case(key)
            || actual_value != value
        {
            continue;
        }
        let start = line.start;
        let end = document
            .lines
            .get(index + 1)
            .map_or(bytes.len(), |next| next.start);
        if bytes.get(start..end) == Some(expected_line) {
            matches.push((start, end));
        }
    }
    matches
}

fn ascii_key(value: &str) -> String {
    value
        .bytes()
        .map(|byte| byte.to_ascii_lowercase() as char)
        .collect()
}

pub(super) fn ends_with_newline(bytes: &[u8], encoding: IniEncoding) -> bool {
    match encoding {
        IniEncoding::Utf8 | IniEncoding::Utf8Bom => {
            bytes.ends_with(b"\n") || bytes.ends_with(b"\r")
        }
        IniEncoding::Utf16Le => bytes.ends_with(&[10, 0]) || bytes.ends_with(&[13, 0]),
        IniEncoding::Utf16Be => bytes.ends_with(&[0, 10]) || bytes.ends_with(&[0, 13]),
    }
}

pub(super) fn is_bom_only(bytes: &[u8], encoding: IniEncoding) -> bool {
    match encoding {
        IniEncoding::Utf8 => bytes.is_empty(),
        IniEncoding::Utf8Bom => bytes == [0xEF, 0xBB, 0xBF],
        IniEncoding::Utf16Le => bytes == [0xFF, 0xFE],
        IniEncoding::Utf16Be => bytes == [0xFE, 0xFF],
    }
}

pub(super) fn header_body_is_empty(bytes: &[u8], header_start: usize) -> bool {
    let Ok((_, document)) = decode_document(bytes) else {
        return false;
    };
    let Some(header_line) = document
        .lines
        .iter()
        .position(|line| line.start == header_start)
    else {
        return false;
    };
    let header_end = header_line + 1;
    let next_section = document.lines[header_end..]
        .iter()
        .position(|line| section_name(&line.text).is_some())
        .map_or(document.lines.len(), |offset| header_end + offset);
    document.lines[header_end..next_section]
        .iter()
        .all(|line| line.text.trim().is_empty())
}

pub(super) fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
