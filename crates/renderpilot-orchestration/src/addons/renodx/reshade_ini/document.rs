//! Byte-preserving ReShade.ini document abstraction.
//!
//! [`IniDocument`] encapsulates raw byte parsing, UTF-8 BOM preservation,
//! line endings (`\r\n` vs `\n`), safe section and key mutations, and
//! trailing newline anchor management for byte-exact roundtrips.

use renderpilot_domain::{RenoDxConfigEntry, RenoDxManagedBaseline};

use super::RenoDxConfigError;
#[cfg(test)]
use super::SET_PATH_KEY;

pub(super) const RENODX_SECTION: &str = "renodx";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PlannedKey {
    pub(super) baseline: RenoDxManagedBaseline,
    pub(super) newline_anchor: Option<String>,
    pub(super) changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RemovalPolicy {
    Strict,
    Tolerant,
}

#[derive(Debug, Clone)]
struct ParsedLine {
    raw: String,
    body: String,
    ending: &'static str,
}

impl ParsedLine {
    fn generated(body: String, ending: &'static str) -> Self {
        let mut raw = String::with_capacity(body.len() + ending.len());
        raw.push_str(&body);
        raw.push_str(ending);
        Self { raw, body, ending }
    }

    fn refresh_raw(&mut self) {
        self.raw.clear();
        self.raw.push_str(&self.body);
        self.raw.push_str(self.ending);
    }
}

/// In-memory representation of an INI file that preserves original formatting,
/// comments, indentation, line endings, and BOM.
#[derive(Debug, Clone)]
pub(super) struct IniDocument {
    has_bom: bool,
    lines: Vec<ParsedLine>,
}

impl IniDocument {
    /// Parses raw bytes into an [`IniDocument`], validating UTF-8 and rejecting NUL bytes.
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, RenoDxConfigError> {
        if bytes.contains(&0) {
            return Err(RenoDxConfigError::Nul);
        }
        let (has_bom, bytes) = if let Some(stripped) = bytes.strip_prefix(crate::addons::UTF8_BOM) {
            (true, stripped)
        } else {
            (false, bytes)
        };
        let text = std::str::from_utf8(bytes).map_err(|_| RenoDxConfigError::NonUtf8)?;
        let lines = text
            .split_inclusive('\n')
            .map(|raw| {
                let (body, ending) = if let Some(body) = raw.strip_suffix('\n') {
                    if let Some(body) = body.strip_suffix('\r') {
                        (body, "\r\n")
                    } else {
                        (body, "\n")
                    }
                } else {
                    (raw, "")
                };
                ParsedLine {
                    raw: raw.to_owned(),
                    body: body.to_owned(),
                    ending,
                }
            })
            .collect();
        Ok(Self { has_bom, lines })
    }

    /// Serializes lines back to bytes, prepending the UTF-8 BOM if it was originally present.
    pub(super) fn to_bytes(&self) -> Vec<u8> {
        let bom_len = if self.has_bom {
            crate::addons::UTF8_BOM.len()
        } else {
            0
        };
        let total_len: usize =
            bom_len + self.lines.iter().map(|line| line.raw.len()).sum::<usize>();
        let mut out = Vec::with_capacity(total_len);
        if self.has_bom {
            out.extend_from_slice(crate::addons::UTF8_BOM);
        }
        for line in &self.lines {
            out.extend_from_slice(line.raw.as_bytes());
        }
        out
    }

    /// Returns the predominant newline ending used in the document (defaults to `\r\n`).
    fn newline(&self) -> &'static str {
        self.lines
            .iter()
            .find_map(|line| (!line.ending.is_empty()).then_some(line.ending))
            .unwrap_or("\r\n")
    }

    /// Returns true if the document contains a valid `[renodx]` section.
    pub(super) fn section_exists(&self) -> Result<bool, RenoDxConfigError> {
        self.target_range().map(|range| range.is_some())
    }

    /// Finds line index bounds `[start, end)` of the `[renodx]` section.
    fn target_range(&self) -> Result<Option<(usize, usize)>, RenoDxConfigError> {
        self.target_range_with_keys(&[])
    }

    /// Finds line index bounds `[start, end)` of the `[renodx]` section while
    /// validating that candidate managed keys have valid assignment delimiters.
    fn target_range_with_keys(
        &self,
        managed_keys: &[&str],
    ) -> Result<Option<(usize, usize)>, RenoDxConfigError> {
        let mut start = None;
        let mut end = self.lines.len();
        for (index, line) in self.lines.iter().enumerate() {
            if start.is_none() {
                if malformed_target_header(&line.body) {
                    return Err(RenoDxConfigError::Ambiguous("malformed section header"));
                }
                if is_target_section(&line.body) {
                    start = Some(index);
                }
                continue;
            }

            if end == self.lines.len() {
                if malformed_section_header(&line.body) {
                    return Err(RenoDxConfigError::Ambiguous("malformed section header"));
                }
                if malformed_managed_key(&line.body, managed_keys) {
                    return Err(RenoDxConfigError::Ambiguous(
                        "managed RenoDX configuration key has no assignment delimiter",
                    ));
                }
                if section_header(&line.body).is_some() {
                    if is_target_section(&line.body) {
                        return Err(RenoDxConfigError::Ambiguous("multiple [renodx] sections"));
                    }
                    end = index;
                }
            } else {
                if malformed_target_header(&line.body) {
                    return Err(RenoDxConfigError::Ambiguous("malformed section header"));
                }
                if is_target_section(&line.body) {
                    return Err(RenoDxConfigError::Ambiguous("multiple [renodx] sections"));
                }
            }
        }
        Ok(start.map(|start| (start, end)))
    }

    /// Plans the addition or modification of a single managed key in `[renodx]`.
    pub(super) fn plan_key(
        &mut self,
        key: &str,
        desired: i32,
    ) -> Result<PlannedKey, RenoDxConfigError> {
        if key.is_empty() || key.contains(['\0', '\r', '\n', '=']) {
            return Err(RenoDxConfigError::Ambiguous("unsafe configuration key"));
        }
        let range = self.target_range_with_keys(&[key])?;
        let (_section_start, section_end) = range.unwrap_or((self.lines.len(), self.lines.len()));
        let mut key_index = None;
        let mut baseline = RenoDxManagedBaseline::Absent;
        if let Some((start, end)) = range {
            for (index, line) in self.lines.iter().enumerate().take(end).skip(start + 1) {
                if let Some((_lhs, rhs)) = key_assignment(&line.body, key) {
                    if key_index.replace(index).is_some() {
                        return Err(RenoDxConfigError::Ambiguous("duplicate configuration keys"));
                    }
                    baseline = RenoDxManagedBaseline::Present {
                        value: rhs.trim().to_owned(),
                    };
                }
            }
        }
        let mut changed = false;
        let mut newline_anchor = None;
        let desired_str = desired.to_string();
        if let Some(index) = key_index {
            let line = &mut self.lines[index];
            let replacement = replace_key_value(&line.body, &desired_str)?;
            if replacement != line.body {
                changed = true;
                line.body = replacement;
                line.refresh_raw();
            }
        } else {
            changed = true;
            let newline = self.newline();
            if range.is_some() {
                let insert_at = section_end;
                if insert_at > 0 && self.lines[insert_at - 1].ending.is_empty() {
                    newline_anchor = Some(self.lines[insert_at - 1].body.clone());
                    self.lines[insert_at - 1].ending = newline;
                    self.lines[insert_at - 1].refresh_raw();
                }
                self.lines.insert(
                    insert_at,
                    ParsedLine::generated(format!("{key}={desired_str}"), newline),
                );
            } else {
                if let Some(last) = self.lines.last_mut().filter(|line| line.ending.is_empty()) {
                    newline_anchor = Some(last.body.clone());
                    last.ending = newline;
                    last.refresh_raw();
                }
                self.lines.push(ParsedLine::generated(
                    format!("[{RENODX_SECTION}]"),
                    newline,
                ));
                self.lines.push(ParsedLine::generated(
                    format!("{key}={desired_str}"),
                    newline,
                ));
            }
        }
        Ok(PlannedKey {
            baseline,
            newline_anchor,
            changed,
        })
    }

    /// Verifies that all receipt-owned entries still match RenderPilot's last-written
    /// values within `[renodx]`. Fails closed on any discrepancy, duplication, or corruption.
    pub(super) fn verify_owned_entries(
        &self,
        entries: &[RenoDxConfigEntry],
    ) -> Result<(), RenoDxConfigError> {
        let Some((section_start, section_end)) = self.target_range()? else {
            return Err(RenoDxConfigError::Ambiguous(
                "receipt-owned RenoDX section is missing",
            ));
        };
        for entry in entries {
            if self
                .lines
                .iter()
                .take(section_end)
                .skip(section_start + 1)
                .any(|line| malformed_managed_key(&line.body, &[entry.key.as_str()]))
            {
                return Err(RenoDxConfigError::Ambiguous(
                    "managed RenoDX configuration key has no assignment delimiter",
                ));
            }
            let mut index = None;
            let mut observed = None;
            for (line_index, line) in self
                .lines
                .iter()
                .enumerate()
                .take(section_end)
                .skip(section_start + 1)
            {
                if let Some((_lhs, rhs)) = key_assignment(&line.body, &entry.key) {
                    if index.replace(line_index).is_some() {
                        return Err(RenoDxConfigError::Ambiguous("duplicate configuration keys"));
                    }
                    observed = Some(rhs.trim());
                }
            }
            let expected = entry.last_written.to_string();
            if observed != Some(expected.as_str()) {
                return Err(RenoDxConfigError::Ambiguous(
                    "receipt-owned RenoDX configuration was edited outside RenderPilot",
                ));
            }
        }
        Ok(())
    }

    /// Removes or restores an owned key according to the given policy.
    /// Under Strict policy, external edits or missing elements fail closed.
    /// Under Tolerant policy, non-matching or missing elements are safely skipped.
    pub(super) fn remove_owned_key_with_policy(
        &mut self,
        entry: &RenoDxConfigEntry,
        section_preexisted: bool,
        policy: RemovalPolicy,
    ) -> Result<bool, RenoDxConfigError> {
        let Some((section_start, section_end)) = self.target_range()? else {
            return match policy {
                RemovalPolicy::Strict => Err(RenoDxConfigError::Ambiguous(
                    "receipt-owned RenoDX section is missing",
                )),
                RemovalPolicy::Tolerant => Ok(false),
            };
        };
        if self
            .lines
            .iter()
            .take(section_end)
            .skip(section_start + 1)
            .any(|line| malformed_managed_key(&line.body, &[entry.key.as_str()]))
        {
            return match policy {
                RemovalPolicy::Strict => Err(RenoDxConfigError::Ambiguous(
                    "managed RenoDX configuration key has no assignment delimiter",
                )),
                RemovalPolicy::Tolerant => Ok(false),
            };
        }
        let mut index = None;
        let mut current = None;
        for (line_index, line) in self
            .lines
            .iter()
            .enumerate()
            .take(section_end)
            .skip(section_start + 1)
        {
            if let Some((_lhs, rhs)) = key_assignment(&line.body, &entry.key) {
                if index.replace(line_index).is_some() {
                    return match policy {
                        RemovalPolicy::Strict => {
                            Err(RenoDxConfigError::Ambiguous("duplicate configuration keys"))
                        }
                        RemovalPolicy::Tolerant => Ok(false),
                    };
                }
                current = Some(rhs.trim());
            }
        }
        let expected = entry.last_written.to_string();
        if current != Some(expected.as_str()) {
            return match policy {
                RemovalPolicy::Strict => Err(RenoDxConfigError::Ambiguous(
                    "receipt-owned RenoDX configuration was edited outside RenderPilot",
                )),
                RemovalPolicy::Tolerant => Ok(false),
            };
        }
        let Some(index) = index else {
            return match policy {
                RemovalPolicy::Strict => Err(RenoDxConfigError::Ambiguous(
                    "receipt-owned RenoDX configuration was edited outside RenderPilot",
                )),
                RemovalPolicy::Tolerant => Ok(false),
            };
        };
        match &entry.baseline {
            RenoDxManagedBaseline::Present { value } => {
                let line = &mut self.lines[index];
                line.body = replace_key_value(&line.body, value)?;
                line.refresh_raw();
            }
            RenoDxManagedBaseline::Absent => {
                self.lines.remove(index);
                let adjusted_end = self
                    .lines
                    .iter()
                    .enumerate()
                    .skip(section_start + 1)
                    .find(|(_, line)| section_header(&line.body).is_some())
                    .map_or(self.lines.len(), |(line_index, _)| line_index);
                let section_body_is_empty = section_start < adjusted_end
                    && self
                        .lines
                        .get(section_start + 1..adjusted_end)
                        .is_some_and(|body| body.iter().all(|line| line.body.trim().is_empty()));
                if !section_preexisted && section_body_is_empty {
                    self.lines.drain(section_start..adjusted_end);
                }
            }
        }
        Ok(true)
    }

    /// Restores a stripped newline to the anchor line if it remained untouched.
    pub(super) fn restore_newline_anchor(&mut self, anchor: Option<&str>) -> bool {
        let Some(anchor) = anchor else {
            return false;
        };
        let Some(last) = self.lines.last_mut() else {
            return false;
        };
        if last.body != anchor || last.ending.is_empty() {
            return false;
        }
        last.ending = "";
        last.raw = last.body.clone();
        true
    }

    /// Inspects the current value of `Set_Path` in `[renodx]`.
    #[cfg(test)]
    pub(super) fn current_set_path_value(&self) -> Result<Option<String>, RenoDxConfigError> {
        let Some((section_start, section_end)) = self.target_range()? else {
            return Ok(None);
        };
        let mut value = None;
        for line in self.lines.iter().take(section_end).skip(section_start + 1) {
            if let Some((_key, rhs)) = key_assignment(&line.body, SET_PATH_KEY) {
                if value.is_some() {
                    return Err(RenoDxConfigError::Ambiguous("duplicate Set_Path keys"));
                }
                value = Some(rhs.trim().to_owned());
            }
        }
        Ok(value)
    }
}

fn key_assignment<'a>(body: &'a str, key: &str) -> Option<(&'a str, &'a str)> {
    let trimmed = body.trim_start();
    let (lhs, rhs) = trimmed.split_once('=')?;
    lhs.trim().eq_ignore_ascii_case(key).then_some((lhs, rhs))
}

fn replace_key_value(body: &str, value: &str) -> Result<String, RenoDxConfigError> {
    let equal = body.find('=').ok_or(RenoDxConfigError::Ambiguous(
        "configuration key has no assignment delimiter",
    ))?;
    let rhs = &body[equal + 1..];
    let leading = rhs.len() - rhs.trim_start().len();
    let trailing = rhs.trim_end().len();
    let lhs = &body[..=equal];
    Ok(format!(
        "{}{}{}{}",
        lhs,
        &rhs[..leading],
        value,
        &rhs[trailing..]
    ))
}

fn section_header(body: &str) -> Option<&str> {
    let trimmed = body.trim();
    trimmed.strip_circumfix('[', ']').map(str::trim)
}

fn malformed_target_header(body: &str) -> bool {
    let candidate = body.trim_start().strip_prefix('[').map(str::trim_start);
    let Some(candidate) = candidate else {
        return false;
    };
    if section_header(body).is_some() {
        return false;
    }
    let Some(rest) = candidate.get(..RENODX_SECTION.len()) else {
        return false;
    };
    rest.eq_ignore_ascii_case(RENODX_SECTION)
        && candidate
            .get(RENODX_SECTION.len()..)
            .is_some_and(|rest| rest.is_empty() || !rest.as_bytes()[0].is_ascii_alphanumeric())
}

fn malformed_section_header(body: &str) -> bool {
    body.trim_start().starts_with('[') && section_header(body).is_none()
}

fn is_target_section(body: &str) -> bool {
    section_header(body).is_some_and(|name| name.eq_ignore_ascii_case(RENODX_SECTION))
}

fn malformed_managed_key(body: &str, keys: &[&str]) -> bool {
    let trimmed = body.trim();
    if trimmed.is_empty()
        || trimmed.starts_with(';')
        || trimmed.starts_with('#')
        || trimmed.contains('=')
    {
        return false;
    }

    keys.iter().any(|key| {
        let Some(rest) = trimmed.get(..key.len()) else {
            return false;
        };
        rest.eq_ignore_ascii_case(key)
            && trimmed
                .as_bytes()
                .get(key.len())
                .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
    })
}
