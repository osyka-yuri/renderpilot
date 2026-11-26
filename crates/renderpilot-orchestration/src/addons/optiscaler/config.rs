//! Comment/order-preserving semantic three-way merge for OptiScaler INI files.

use super::types::{ConfigKeyMigration, ManagedIniValue};
use std::collections::HashMap;

pub(super) fn append_reshade_chain_invariant(values: &mut Vec<ManagedIniValue>, enabled: bool) {
    values.push(ManagedIniValue {
        section: "Plugins".to_owned(),
        key: "LoadReshade".to_owned(),
        value: if enabled { "true" } else { "false" }.to_owned(),
    });
}

/// Changes only the peer-owned ReShade chain switch while preserving every
/// unrelated OptiScaler INI line, comment, spelling, and newline convention.
/// This is deliberately a thin typed use of the established semantic merger;
/// RenoDX never parses or writes OptiScaler configuration itself.
pub(crate) fn set_reshade_chain_enabled(bytes: &[u8], enabled: bool) -> Vec<u8> {
    let mut invariants = Vec::with_capacity(1);
    append_reshade_chain_invariant(&mut invariants, enabled);
    three_way_merge(bytes, bytes, bytes, &[], &invariants).bytes
}

#[derive(Debug, Clone)]
pub(crate) struct ConfigMergeResult {
    pub(crate) bytes: Vec<u8>,
    pub(crate) conflicts: Vec<String>,
}

#[derive(Debug, Clone)]
struct IniLine {
    raw: String,
    section: String,
    key: Option<String>,
    value: Option<String>,
}

pub(crate) fn three_way_merge(
    old_base: &[u8],
    user_current: &[u8],
    new_base: &[u8],
    migrations: &[ConfigKeyMigration],
    invariants: &[ManagedIniValue],
) -> ConfigMergeResult {
    let old_text = String::from_utf8_lossy(old_base);
    let user_text = String::from_utf8_lossy(user_current);
    let new_text = String::from_utf8_lossy(new_base);
    let newline = if user_text.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let final_newline = user_text.ends_with('\n');
    let mut lines = parse(&user_text);
    let old = values(&parse(&old_text));
    let new_lines = parse(&new_text);
    let new = values(&new_lines);
    let mut conflicts = Vec::new();

    for migration in migrations {
        let from = key(&migration.from_section, &migration.from_key);
        let to = key(&migration.to_section, &migration.to_key);
        let Some(index) = index_of(&lines, &from) else {
            continue;
        };
        if index_of(&lines, &to).is_some() {
            conflicts.push(format!(
                "{}.{} -> {}.{}",
                migration.from_section, migration.from_key, migration.to_section, migration.to_key
            ));
            continue;
        }
        let source_value = lines[index].value.clone().unwrap_or_default();
        let value = if migration.value_map.is_empty() {
            source_value
        } else if let Some(mapped) = migration
            .value_map
            .iter()
            .find(|(source, _)| source.eq_ignore_ascii_case(&source_value))
            .map(|(_, target)| target.clone())
        {
            mapped
        } else {
            conflicts.push(format!(
                "{}.{} -> {}.{} (unmapped value)",
                migration.from_section, migration.from_key, migration.to_section, migration.to_key
            ));
            continue;
        };
        if migration
            .from_section
            .eq_ignore_ascii_case(&migration.to_section)
        {
            lines[index].key = Some(migration.to_key.clone());
            lines[index].raw = format!("{}={value}", migration.to_key);
        } else {
            // A line's semantic section is determined by the preceding header;
            // changing only the cached field would serialize a key under the
            // old section. Move the setting through the normal insertion path.
            lines.remove(index);
            insert_value(&mut lines, &migration.to_section, &migration.to_key, &value);
        }
    }

    // Defaults removed by the new schema should disappear only when the user
    // left them untouched. User-edited and unknown values remain deliberately
    // preserved.
    for (semantic_key, old_value) in &old {
        if new.contains_key(semantic_key) {
            continue;
        }
        let Some(index) = index_of(&lines, semantic_key) else {
            continue;
        };
        if lines[index].value.as_deref() == Some(old_value.as_str()) {
            lines.remove(index);
        }
    }

    for (semantic_key, new_value) in &new {
        let Some((section, setting)) = split_key(semantic_key) else {
            continue;
        };
        if let Some(index) = index_of(&lines, semantic_key) {
            let user_value = lines[index].value.as_deref().unwrap_or_default();
            if let Some(old_value) = old.get(semantic_key) {
                if old_value == user_value {
                    set_value(&mut lines[index], new_value);
                } else if old_value != new_value && user_value != new_value {
                    conflicts.push(format!("{section}.{setting}"));
                }
            }
        } else {
            // Semantic keys are normalized for matching. Preserve the
            // spelling supplied by the new release defaults when adding a
            // setting so a migration never lowercases user-facing INI keys.
            let (display_section, display_setting) = new_lines
                .iter()
                .find(|line| {
                    line.key.as_deref().is_some_and(|setting_name| {
                        key(&line.section, setting_name) == *semantic_key
                    })
                })
                .and_then(|line| Some((line.section.as_str(), line.key.as_deref()?)))
                .unwrap_or((section, setting));
            insert_value(&mut lines, display_section, display_setting, new_value);
        }
    }

    for invariant in invariants {
        let semantic_key = key(&invariant.section, &invariant.key);
        if let Some(index) = index_of(&lines, &semantic_key) {
            if lines[index].value.as_deref() != Some(invariant.value.as_str()) {
                conflicts.push(format!("managed:{}.{}", invariant.section, invariant.key));
                set_value(&mut lines[index], &invariant.value);
            }
        } else {
            insert_value(
                &mut lines,
                &invariant.section,
                &invariant.key,
                &invariant.value,
            );
        }
    }

    conflicts.sort();
    conflicts.dedup();
    let mut output = lines
        .into_iter()
        .map(|line| line.raw)
        .collect::<Vec<_>>()
        .join(newline);
    if final_newline {
        output.push_str(newline);
    }
    ConfigMergeResult {
        bytes: output.into_bytes(),
        conflicts,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedIniValues(HashMap<String, String>);

impl ParsedIniValues {
    /// Checks one semantic INI value against the parsed configuration.
    pub(crate) fn value_matches(&self, section: &str, setting: &str, expected: &str) -> bool {
        self.0
            .get(&key(section, setting))
            .is_some_and(|value| value.eq_ignore_ascii_case(expected))
    }
}

/// Parses an OptiScaler INI file into a queryable semantic key-value map once.
pub(crate) fn parse_values(bytes: &[u8]) -> ParsedIniValues {
    ParsedIniValues(values(&parse(&String::from_utf8_lossy(bytes))))
}

/// Checks one semantic INI value without exposing parser representation.
#[cfg(test)]
pub(crate) fn value_matches(bytes: &[u8], section: &str, setting: &str, expected: &str) -> bool {
    parse_values(bytes).value_matches(section, setting, expected)
}

/// Removes only infrastructure values that still equal a value previously
/// imposed by RenderPilot. User-edited values are retained as unknown/user
/// configuration during module removal.
pub(crate) fn remove_managed_values(bytes: &[u8], removals: &[ManagedIniValue]) -> Vec<u8> {
    if removals.is_empty() {
        return bytes.to_vec();
    }
    let text = String::from_utf8_lossy(bytes);
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let final_newline = text.ends_with('\n');
    let lines = parse(&text)
        .into_iter()
        .filter(|line| {
            !removals.iter().any(|removal| {
                line.key.as_deref().is_some_and(|key_name| {
                    key(&line.section, key_name) == key(&removal.section, &removal.key)
                        && line.value.as_deref() == Some(removal.value.as_str())
                })
            })
        })
        .map(|line| line.raw)
        .collect::<Vec<_>>();
    let mut output = lines.join(newline);
    if final_newline {
        output.push_str(newline);
    }
    output.into_bytes()
}

fn parse(text: &str) -> Vec<IniLine> {
    let mut section = String::new();
    text.lines()
        .map(|raw| {
            let trimmed = raw.trim();
            if let Some(name) = trimmed.strip_circumfix('[', ']') {
                name.trim().clone_into(&mut section);
                return IniLine {
                    raw: raw.to_owned(),
                    section: section.clone(),
                    key: None,
                    value: None,
                };
            }
            let parsed = (!trimmed.starts_with(';') && !trimmed.starts_with('#'))
                .then(|| raw.split_once('='))
                .flatten();
            IniLine {
                raw: raw.to_owned(),
                section: section.clone(),
                key: parsed.map(|(key, _)| key.trim().to_owned()),
                value: parsed.map(|(_, value)| value.trim().to_owned()),
            }
        })
        .collect()
}

fn values(lines: &[IniLine]) -> HashMap<String, String> {
    lines
        .iter()
        .filter_map(|line| {
            Some((
                key(&line.section, line.key.as_deref()?),
                line.value.clone().unwrap_or_default(),
            ))
        })
        .collect()
}

fn key(section: &str, setting: &str) -> String {
    format!(
        "{}\0{}",
        section.trim().to_ascii_lowercase(),
        setting.trim().to_ascii_lowercase()
    )
}

fn split_key(value: &str) -> Option<(&str, &str)> {
    value.split_once('\0')
}

fn index_of(lines: &[IniLine], semantic_key: &str) -> Option<usize> {
    let (expected_section, expected_setting) = split_key(semantic_key)?;
    lines.iter().rposition(|line| {
        line.key.as_deref().is_some_and(|setting| {
            line.section.trim().eq_ignore_ascii_case(expected_section)
                && setting.trim().eq_ignore_ascii_case(expected_setting)
        })
    })
}

fn set_value(line: &mut IniLine, value: &str) {
    let setting = line.key.as_deref().unwrap_or_default();
    line.value = Some(value.to_owned());
    line.raw = format!("{setting}={value}");
}

fn insert_value(lines: &mut Vec<IniLine>, section: &str, setting: &str, value: &str) {
    let header = format!("[{section}]");
    let section_index = lines
        .iter()
        .position(|line| line.key.is_none() && line.raw.trim().eq_ignore_ascii_case(&header));
    if section_index.is_none() {
        if lines.last().is_some_and(|line| !line.raw.is_empty()) {
            lines.push(IniLine {
                raw: String::new(),
                section: String::new(),
                key: None,
                value: None,
            });
        }
        lines.push(IniLine {
            raw: header,
            section: section.to_owned(),
            key: None,
            value: None,
        });
    }
    let start = section_index.unwrap_or(lines.len().saturating_sub(1));
    let insert_at = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find(|(_, line)| line.key.is_none() && line.raw.trim().starts_with('['))
        .map_or(lines.len(), |(index, _)| index);
    lines.insert(
        insert_at,
        IniLine {
            raw: format!("{setting}={value}"),
            section: section.to_owned(),
            key: Some(setting.to_owned()),
            value: Some(value.to_owned()),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_user_unknowns_and_updates_untouched_defaults() {
        let result = three_way_merge(
            b"[A]\r\nKnown=old\r\n",
            b"; note\r\n[A]\r\nKnown=old\r\nCustom=yes\r\n",
            b"[A]\nKnown=new\nAdded=value\n",
            &[],
            &[],
        );
        let text = String::from_utf8(result.bytes).expect("utf8");
        assert!(text.contains("; note\r\n"));
        assert!(text.contains("Known=new\r\n"));
        assert!(text.contains("Custom=yes\r\n"));
        assert!(text.contains("Added=value\r\n"), "{text:?}");
    }

    #[test]
    fn moves_fakenvapi_and_nukem_keys_across_release_schema_sections() {
        let migrations = vec![
            ConfigKeyMigration {
                from_schema: 1,
                to_schema: 2,
                from_section: "Fakenvapi".to_owned(),
                from_key: "Enabled".to_owned(),
                to_section: "OptiScaler".to_owned(),
                to_key: "Fakenvapi".to_owned(),
                value_map: std::collections::BTreeMap::default(),
            },
            ConfigKeyMigration {
                from_schema: 1,
                to_schema: 2,
                from_section: "Nukem".to_owned(),
                from_key: "Enabled".to_owned(),
                to_section: "NvngxFG".to_owned(),
                to_key: "Enabled".to_owned(),
                value_map: std::collections::BTreeMap::default(),
            },
        ];
        let current = b"; user note\r\n[Fakenvapi]\r\nEnabled=false\r\nCustom=yes\r\n[Nukem]\r\nEnabled=true\r\n";
        let result = three_way_merge(current, current, current, &migrations, &[]);
        let text = String::from_utf8(result.bytes).expect("utf8");
        assert!(text.contains("; user note\r\n"));
        assert!(text.contains("Custom=yes\r\n"));
        assert!(text.contains("[OptiScaler]\r\nFakenvapi=false\r\n"));
        assert!(text.contains("[NvngxFG]\r\nEnabled=true\r\n"));
        assert!(value_matches(
            text.as_bytes(),
            "OptiScaler",
            "Fakenvapi",
            "false"
        ));
        assert!(value_matches(text.as_bytes(), "NvngxFG", "Enabled", "true"));
    }

    #[test]
    fn transforms_known_values_and_preserves_unmapped_values_as_conflicts() {
        let migration = ConfigKeyMigration {
            from_schema: 2,
            to_schema: 3,
            from_section: "FSR".to_owned(),
            from_key: "Fsr4ForceModel".to_owned(),
            to_section: "FSR".to_owned(),
            to_key: "Fsr4ForceEnableInt8".to_owned(),
            value_map: [("0".to_owned(), "false".to_owned())].into_iter().collect(),
        };
        let mapped = three_way_merge(
            b"[FSR]\nFsr4ForceModel=auto\n",
            b"[FSR]\nFsr4ForceModel=0\n",
            b"[FSR]\nFsr4ForceEnableInt8=auto\n",
            std::slice::from_ref(&migration),
            &[],
        );
        assert!(value_matches(
            &mapped.bytes,
            "FSR",
            "Fsr4ForceEnableInt8",
            "false"
        ));

        let unmapped = three_way_merge(
            b"[FSR]\nFsr4ForceModel=auto\n",
            b"[FSR]\nFsr4ForceModel=1\n",
            b"[FSR]\nFsr4ForceEnableInt8=auto\n",
            &[migration],
            &[],
        );
        assert!(value_matches(&unmapped.bytes, "FSR", "Fsr4ForceModel", "1"));
        assert!(value_matches(
            &unmapped.bytes,
            "FSR",
            "Fsr4ForceEnableInt8",
            "auto"
        ));
        assert_eq!(unmapped.conflicts.len(), 1);
    }

    #[test]
    fn removes_only_unchanged_managed_values() {
        let removals = [ManagedIniValue {
            section: "Libraries".to_owned(),
            key: "NvngxDlssPath".to_owned(),
            value: ".\\OptiScaler\\nvngx_dlss.dll".to_owned(),
        }];
        let unchanged = remove_managed_values(
            b"[Libraries]\r\nNvngxDlssPath=.\\OptiScaler\\nvngx_dlss.dll\r\n",
            &removals,
        );
        assert_eq!(unchanged, b"[Libraries]\r\n");
        let user_edited = remove_managed_values(
            b"[Libraries]\r\nNvngxDlssPath=C:\\Custom\\nvngx.dll\r\n",
            &removals,
        );
        assert_eq!(
            user_edited,
            b"[Libraries]\r\nNvngxDlssPath=C:\\Custom\\nvngx.dll\r\n"
        );
    }

    #[test]
    fn changes_only_the_reshade_chain_invariant() {
        let before = b"; keep\r\n[Plugins]\r\nOther=unchanged\r\nLoadReshade=false\r\n";
        let enabled = set_reshade_chain_enabled(before, true);
        assert_eq!(
            enabled,
            b"; keep\r\n[Plugins]\r\nOther=unchanged\r\nLoadReshade=true\r\n"
        );
        let disabled = set_reshade_chain_enabled(&enabled, false);
        assert_eq!(disabled, before);
    }

    #[test]
    fn removes_obsolete_untouched_defaults_but_preserves_user_edits() {
        let old = b"[A]\r\nRemoved=old\r\n";
        let new = b"[A]\nAdded=new\n";
        let untouched = three_way_merge(old, old, new, &[], &[]);
        let untouched = String::from_utf8(untouched.bytes).expect("utf8");
        assert!(!untouched.contains("Removed="));
        assert!(untouched.contains("Added=new\r\n"));

        let edited = three_way_merge(old, b"[A]\r\nRemoved=custom\r\n", new, &[], &[]);
        let edited = String::from_utf8(edited.bytes).expect("utf8");
        assert!(edited.contains("Removed=custom\r\n"));
    }
}
