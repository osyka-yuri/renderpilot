//! Minimal INI model backing [`super::engine::MergeStrategy::IniSetKeys`].
//!
//! Preserves input verbatim except for keys it is asked to set, so foreign config
//! content round-trips unchanged. The model is deliberately tiny — it handles
//! only the section/key/value shape the install engine needs for additive config
//! merges (e.g. `ReShade.ini` keys RenoDX requires).
//!
//! The structure makes the "there is always somewhere to put a line" invariant
//! explicit: the lines before the first `[section]` header live in their own
//! [`Ini::preamble`] field, and named [`IniSection`]s always carry a real header.
//! So a body line is never dropped (it joins the current section or the preamble),
//! and there is no "hope the vector is non-empty" access — no runtime `expect`.

/// Preserves input verbatim except for keys it is asked to set, so foreign config
/// content round-trips unchanged.
pub(crate) struct Ini {
    /// Lines before the first `[section]` header (comments, blanks, stray keys).
    /// Always present, so a pre-header body line always has a home.
    preamble: Vec<String>,
    /// Named sections, in document order; each carries a real header.
    sections: Vec<IniSection>,
}

struct IniSection {
    /// Section name without brackets.
    header: String,
    /// Raw lines belonging to the section (excluding its own header line).
    lines: Vec<String>,
}

impl Ini {
    /// Parses `text` into a preamble + named sections, preserving line content
    /// verbatim.
    pub(crate) fn parse(text: &str) -> Self {
        let mut preamble = Vec::new();
        let mut sections: Vec<IniSection> = Vec::new();

        for raw in text.lines() {
            let line = raw.trim_end_matches('\r');
            let trimmed = line.trim();
            if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() >= 2 {
                sections.push(IniSection {
                    header: trimmed[1..trimmed.len() - 1].to_owned(),
                    lines: Vec::new(),
                });
            } else {
                // A body line joins the current (last) section, or the preamble
                // when no `[section]` header has appeared yet — never dropped.
                match sections.last_mut() {
                    Some(section) => section.lines.push(line.to_owned()),
                    None => preamble.push(line.to_owned()),
                }
            }
        }

        Self { preamble, sections }
    }

    /// Sets `key=value` in `section`, replacing an existing key (case-insensitive)
    /// or appending it, creating the section if needed.
    pub(crate) fn set(&mut self, section: &str, key: &str, value: &str) {
        let entry = format!("{key}={value}");
        match self
            .sections
            .iter_mut()
            .find(|s| s.header.eq_ignore_ascii_case(section))
        {
            Some(target) => {
                if let Some(line) = target.lines.iter_mut().find(|line| line_key_eq(line, key)) {
                    *line = replace_line_value(line, key, value);
                } else {
                    target.lines.push(entry);
                }
            }
            // A brand-new section is created already holding its single entry, so
            // there is no "push then re-borrow the last element" dance.
            None => self.sections.push(IniSection {
                header: section.to_owned(),
                lines: vec![entry],
            }),
        }
    }

    /// Removes `key` from `section` (case-insensitive on both). The section is
    /// left in place even if empty — use [`remove_section`](Self::remove_section)
    /// to remove an entire section.
    pub(crate) fn remove_key(&mut self, section: &str, key: &str) {
        if let Some(target) = self
            .sections
            .iter_mut()
            .find(|s| s.header.eq_ignore_ascii_case(section))
        {
            target.lines.retain(|line| !line_key_eq(line, key));
        }
    }

    /// Removes the entire `section` (case-insensitive), including its header and
    /// all its lines. A missing section is a no-op. The preamble is unaffected.
    pub(crate) fn remove_section(&mut self, section: &str) {
        self.sections
            .retain(|s| !s.header.eq_ignore_ascii_case(section));
    }

    /// Returns the trimmed value of `key` in `section` (both case-insensitive), or
    /// `None` when the section or key is absent. Comment and blank lines carry no
    /// `=` and are skipped, so a `;commented=out` key never matches.
    #[must_use]
    pub(crate) fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|s| s.header.eq_ignore_ascii_case(section))?
            .lines
            .iter()
            .find_map(|line| {
                let (line_key, value) = line.split_once('=')?;
                line_key
                    .trim()
                    .eq_ignore_ascii_case(key)
                    .then(|| value.trim())
            })
    }

    /// Whether a `[section]` header is present (case-insensitive).
    #[must_use]
    pub(crate) fn has_section(&self, section: &str) -> bool {
        self.sections
            .iter()
            .any(|s| s.header.eq_ignore_ascii_case(section))
    }

    /// Renders the INI back to text, dropping trailing empty lines and terminating
    /// with a single CRLF. A blank line separates consecutive sections, so a
    /// freshly merged or created config is readable; an existing blank line
    /// before a header is reused rather than duplicated.
    pub(crate) fn render(&self) -> String {
        let mut out: Vec<String> = self.preamble.clone();
        for section in &self.sections {
            // Insert a blank-line separator before a section header when the output
            // so far is non-empty and does not already end with a blank line. The
            // first section after an empty preamble gets no leading blank line, and
            // a foreign config that already separates its sections keeps exactly one.
            if !out.is_empty() && !out.last().is_some_and(|line| line.trim().is_empty()) {
                out.push(String::new());
            }
            out.push(format!("[{}]", section.header));
            out.extend(section.lines.iter().cloned());
        }
        while out.last().is_some_and(|line| line.trim().is_empty()) {
            out.pop();
        }
        let mut text = out.join("\r\n");
        if !text.is_empty() {
            text.push_str("\r\n");
        }
        text
    }
}

/// Returns whether an INI line assigns `key` (case-insensitive on the key, which is
/// the text left of the first `=`).
fn line_key_eq(line: &str, key: &str) -> bool {
    line.split_once('=')
        .is_some_and(|(line_key, _)| line_key.trim().eq_ignore_ascii_case(key))
}

fn replace_line_value(line: &str, fallback_key: &str, value: &str) -> String {
    let Some((left, right)) = line.split_once('=') else {
        return format!("{fallback_key}={value}");
    };
    let key = left.trim_end();
    let left_padding = trailing_whitespace(left);
    let right_padding: String = right.chars().take_while(|ch| ch.is_whitespace()).collect();
    format!("{key}{left_padding}={right_padding}{value}")
}

fn trailing_whitespace(value: &str) -> String {
    let mut chars: Vec<char> = value
        .chars()
        .rev()
        .take_while(|ch| ch.is_whitespace())
        .collect();
    chars.reverse();
    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_foreign_content_and_sets_keys() {
        let mut ini = Ini::parse("[GENERAL]\r\nEffectSearchPaths=old\r\n[OTHER]\r\nfoo=1\r\n");
        ini.set("GENERAL", "EffectSearchPaths", "new");
        ini.set("GENERAL", "KeyNew", "val");
        let rendered = ini.render();
        assert!(rendered.contains("EffectSearchPaths=new"));
        assert!(rendered.contains("KeyNew=val"));
        assert!(rendered.contains("[OTHER]"));
        assert!(rendered.contains("foo=1"));
    }

    #[test]
    fn creates_section_when_missing() {
        let mut ini = Ini::parse("");
        ini.set("NEW", "Key", "Value");
        let rendered = ini.render();
        assert!(rendered.contains("[NEW]"));
        assert!(rendered.contains("Key=Value"));
    }

    #[test]
    fn set_on_a_fresh_document_creates_then_reuses_the_section() {
        // A new section is created with its first entry, and a second `set` to the
        // same section reuses it rather than creating a duplicate.
        let mut ini = Ini::parse("");
        ini.set("ADDON", "AddonPath", ".");
        ini.set("ADDON", "DisabledAddons", "x");
        assert_eq!(
            ini.render(),
            "[ADDON]\r\nAddonPath=.\r\nDisabledAddons=x\r\n"
        );
    }

    #[test]
    fn body_line_before_any_header_is_kept_in_the_preamble() {
        // A stray body line before the first `[section]` must be preserved (not
        // silently dropped) and rendered ahead of the first header.
        let ini = Ini::parse("stray=line\r\n[S]\r\nA=1\r\n");
        assert_eq!(ini.render(), "stray=line\r\n\r\n[S]\r\nA=1\r\n");
    }

    #[test]
    fn replaces_key_case_insensitively() {
        let mut ini = Ini::parse("[S]\r\nMyKey=old\r\n");
        ini.set("S", "mykey", "new");
        assert_eq!(ini.render(), "[S]\r\nMyKey=new\r\n");
    }

    #[test]
    fn replacing_key_preserves_spaces_around_equals() {
        let mut ini = Ini::parse("[S]\r\nMyKey  = old\r\nOther\t=\told\r\n");
        ini.set("S", "mykey", "new");
        ini.set("S", "other", "new");
        assert_eq!(ini.render(), "[S]\r\nMyKey  = new\r\nOther\t=\tnew\r\n");
    }

    #[test]
    fn remove_key_strips_a_key_case_insensitively() {
        let mut ini = Ini::parse("[ADDON]\r\nAddonPath=.\r\nLoadFromDllMain=x\r\n");
        ini.remove_key("addon", "LOADFROMDLLMAIN");
        assert_eq!(ini.render(), "[ADDON]\r\nAddonPath=.\r\n");
    }

    #[test]
    fn remove_key_leaves_an_empty_section_in_place() {
        let mut ini = Ini::parse("[S]\r\nOnly=1\r\n");
        ini.remove_key("S", "only");
        assert_eq!(ini.render(), "[S]\r\n");
    }

    #[test]
    fn remove_key_is_a_noop_for_a_missing_section_or_key() {
        let mut ini = Ini::parse("[S]\r\nA=1\r\n");
        ini.remove_key("MISSING", "A");
        ini.remove_key("S", "absent");
        assert_eq!(ini.render(), "[S]\r\nA=1\r\n");
    }

    #[test]
    fn remove_section_deletes_header_and_lines() {
        let mut ini =
            Ini::parse("[ADDON]\r\nA=1\r\n[RENODX-DLSSFIX]\r\nDLSSPath=x\r\n[OTHER]\r\nB=2\r\n");
        ini.remove_section("renodx-dlssfix");
        // A blank line now separates the surviving sections.
        assert_eq!(ini.render(), "[ADDON]\r\nA=1\r\n\r\n[OTHER]\r\nB=2\r\n");
    }

    #[test]
    fn remove_section_is_a_noop_when_missing() {
        let mut ini = Ini::parse("[S]\r\nA=1\r\n");
        ini.remove_section("NOPE");
        assert_eq!(ini.render(), "[S]\r\nA=1\r\n");
    }

    #[test]
    fn render_separates_adjacent_sections_with_one_blank_line() {
        // The "слипшиеся" case: parsed input has no blank line between sections,
        // and render inserts exactly one.
        let mut ini = Ini::parse("[ADDON]\r\nAddonPath=.\r\n[RENODX-DLSSFIX]\r\nDLSSPath=x\r\n");
        ini.set("RENODX-DLSSFIX", "StreamlinePath", "y");
        assert_eq!(
            ini.render(),
            "[ADDON]\r\nAddonPath=.\r\n\r\n[RENODX-DLSSFIX]\r\nDLSSPath=x\r\nStreamlinePath=y\r\n"
        );
    }

    #[test]
    fn render_preserves_an_existing_blank_line_between_sections() {
        // A foreign config that already separates sections keeps exactly one
        // blank line — render must not duplicate it.
        let ini = Ini::parse("[GENERAL]\r\nPreset=mine.ini\r\n\r\n[OTHER]\r\nfoo=1\r\n");
        assert_eq!(
            ini.render(),
            "[GENERAL]\r\nPreset=mine.ini\r\n\r\n[OTHER]\r\nfoo=1\r\n"
        );
    }

    #[test]
    fn render_adds_no_leading_blank_line_for_first_section() {
        // An empty preamble followed by the first section header: no blank line
        // is inserted at the top of the file.
        let mut ini = Ini::parse("");
        ini.set("ADDON", "AddonPath", ".");
        assert_eq!(ini.render(), "[ADDON]\r\nAddonPath=.\r\n");
    }

    #[test]
    fn get_reads_values_case_insensitively_and_skips_comments() {
        let ini = Ini::parse(
            "; head\r\n[INSTALL]\r\nBasePath = C:\\Base \r\n[ADDON]\r\n;DisabledAddons=ignored\r\nAddonPath=addons\r\n",
        );
        assert_eq!(ini.get("install", "basepath"), Some("C:\\Base"));
        assert_eq!(ini.get("ADDON", "AddonPath"), Some("addons"));
        // A commented key is not a value.
        assert_eq!(ini.get("ADDON", "DisabledAddons"), None);
        // Missing section / key.
        assert_eq!(ini.get("MISSING", "x"), None);
        assert_eq!(ini.get("ADDON", "absent"), None);
    }

    #[test]
    fn has_section_is_case_insensitive() {
        let ini = Ini::parse("[ADDON]\r\nAddonPath=.\r\n");
        assert!(ini.has_section("addon"));
        assert!(!ini.has_section("install"));
    }

    #[test]
    fn render_separates_preamble_content_from_first_section() {
        // A comment (or any line) before the first section is followed by a
        // blank line before the header.
        let ini = Ini::parse("; remark\r\n[ADDON]\r\nAddonPath=.\r\n");
        assert_eq!(ini.render(), "; remark\r\n\r\n[ADDON]\r\nAddonPath=.\r\n");
    }
}
