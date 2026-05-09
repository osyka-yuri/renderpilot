//! Match a folder under `steamapps/common/<installdir>` to Steam `appmanifest_*.acf`.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Steam App ID (digits) and optional display name from the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteamInstallDetails {
    /// Numeric Steam application ID from `appmanifest_<id>.acf`.
    pub app_id: String,
    /// `name` field from the manifest when present.
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SteamInstallPath {
    steamapps_dir: PathBuf,
    installdir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppManifest {
    installdir: String,
    display_name: Option<String>,
}

/// If `game_install_root` looks like `.../steamapps/common/<folder>`, finds the manifest whose
/// `installdir` matches that folder name.
pub fn steam_install_details(game_install_root: &Path) -> Option<SteamInstallDetails> {
    let install_path = steam_install_path(game_install_root)?;

    fs::read_dir(&install_path.steamapps_dir)
        .ok()?
        .filter_map(Result::ok)
        .find_map(|entry| details_from_manifest_entry(&entry, &install_path.installdir))
}

/// Returns the lowercased set of game `installdir` values declared by
/// `appmanifest_*.acf` files in `steamapps_dir`.
///
/// Intended as a filter for sub-directories of `steamapps/common/`: only
/// folders whose name appears in this set are real installed Steam games.
///
/// Steam-internal apps that are listed as Tools / runtimes / SDKs (rather
/// than playable games) are excluded by app-id blacklist and installdir
/// prefix matching:
///
/// * `Steamworks Common Redistributables` (app id `228980`) and the
///   `Steamworks Shared` companion folder.
/// * Proton variants (`Proton 7.0`, `Proton Experimental`, ...) and
///   their anti-cheat runtimes.
/// * Steam Linux Runtime layers (`scout`, `soldier`, `sniper`).
/// * `Steam Audio`, `Steam Audio Tools`, `Steam Controller Configs`,
///   `SteamVR`.
///
/// Returns an empty set when `steamapps_dir` cannot be enumerated, when
/// no manifest is found, or when manifests cannot be parsed.
pub fn steam_install_dirs_in_steamapps(steamapps_dir: &Path) -> HashSet<String> {
    let Ok(entries) = fs::read_dir(steamapps_dir) else {
        return HashSet::new();
    };

    entries
        .filter_map(Result::ok)
        .filter_map(|entry| game_installdir_from_manifest_entry(&entry))
        .map(|installdir| installdir.to_ascii_lowercase())
        .collect()
}

fn game_installdir_from_manifest_entry(entry: &fs::DirEntry) -> Option<String> {
    let app_id = app_id_from_manifest_file_name(&entry.file_name())?;

    let content = fs::read_to_string(entry.path()).ok()?;
    let manifest = parse_app_manifest(&content)?;

    if is_steam_tool_app(&app_id, &manifest.installdir) {
        return None;
    }

    Some(manifest.installdir)
}

/// Well-known Steam app IDs that are not playable games.
///
/// Each entry is a numeric `appmanifest_<id>.acf` published by Valve for
/// runtimes, redistributables, SDKs, or platform tools. The list is
/// hand-curated; adding more is a one-line change.
const STEAM_TOOL_APP_IDS: &[&str] = &[
    "228980",  // Steamworks Common Redistributables
    "1054830", // Proton EasyAntiCheat Runtime
    "1161040", // Proton BattlEye Runtime
    "1070560", // Steam Linux Runtime 1.0 (scout)
    "1391110", // Steam Linux Runtime 2.0 (soldier)
    "1628350", // Steam Linux Runtime 3.0 (sniper)
    "1493710", // Proton Experimental
    "1887720", // Proton 7.0
    "1420170", // Proton 6.3
    "1245040", // Proton 5.13
    "1826330", // Proton 8.0
    "2348590", // Proton 9.0
    "1180440", // Steam Audio
    "1313800", // Steam Audio Tools
    "250820",  // SteamVR
];

/// Lower-case prefixes of `installdir` values that identify Steam-internal
/// runtime / tool sub-folders living under `steamapps/common/`.
///
/// These catch installs that share an appmanifest with a tool app but were
/// not added to [`STEAM_TOOL_APP_IDS`], plus shared / companion folders such
/// as `Steamworks Shared` whose underlying app id varies between Steam
/// versions.
const STEAM_TOOL_INSTALLDIR_PREFIXES: &[&str] = &[
    "steamworks ",         // "Steamworks Common Redistributables", "Steamworks Shared"
    "steam linux runtime", // "Steam Linux Runtime - Soldier", "... 3.0 (sniper)"
    "steam audio",         // "Steam Audio", "Steam Audio Tools"
    "steam controller",    // "Steam Controller Configs"
    "steamvr",
    "proton ", // "Proton 7.0", "Proton Experimental"
    "proton-",
];

fn is_steam_tool_app(app_id: &str, installdir: &str) -> bool {
    if STEAM_TOOL_APP_IDS.contains(&app_id) {
        return true;
    }

    let lowered = installdir.to_ascii_lowercase();
    STEAM_TOOL_INSTALLDIR_PREFIXES
        .iter()
        .any(|prefix| lowered.starts_with(prefix))
}

fn steam_install_path(game_install_root: &Path) -> Option<SteamInstallPath> {
    let lexical_path = without_cur_dir_components(game_install_root);

    steam_install_path_from(&lexical_path).or_else(|| {
        fs::canonicalize(game_install_root)
            .ok()
            .and_then(|path| steam_install_path_from(&path))
    })
}

fn steam_install_path_from(path: &Path) -> Option<SteamInstallPath> {
    let installdir = path.file_name()?.to_str()?.to_owned();

    let common_dir = path.parent()?;
    if !path_file_name_eq_ignore_ascii_case(common_dir, "common") {
        return None;
    }

    let steamapps_dir = common_dir.parent()?;
    if !path_file_name_eq_ignore_ascii_case(steamapps_dir, "steamapps") {
        return None;
    }

    Some(SteamInstallPath {
        steamapps_dir: steamapps_dir.to_path_buf(),
        installdir,
    })
}

fn details_from_manifest_entry(
    entry: &fs::DirEntry,
    expected_installdir: &str,
) -> Option<SteamInstallDetails> {
    let app_id = app_id_from_manifest_file_name(&entry.file_name())?;
    let content = fs::read_to_string(entry.path()).ok()?;
    let manifest = parse_app_manifest(&content)?;

    if !manifest
        .installdir
        .eq_ignore_ascii_case(expected_installdir)
    {
        return None;
    }

    Some(SteamInstallDetails {
        app_id,
        display_name: manifest.display_name,
    })
}

fn app_id_from_manifest_file_name(file_name: &OsStr) -> Option<String> {
    let file_name = file_name.to_str()?;
    let app_id = file_name
        .strip_prefix("appmanifest_")?
        .strip_suffix(".acf")?;

    if app_id.is_empty() || !app_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    Some(app_id.to_owned())
}

fn parse_app_manifest(content: &str) -> Option<AppManifest> {
    Some(AppManifest {
        installdir: parse_acf_quoted_value(content, "installdir")?,
        display_name: parse_acf_quoted_value(content, "name"),
    })
}

fn path_file_name_eq_ignore_ascii_case(path: &Path, expected: &str) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

fn without_cur_dir_components(path: &Path) -> PathBuf {
    let mut clean = PathBuf::new();

    for component in path.components() {
        if !matches!(component, Component::CurDir) {
            clean.push(component.as_os_str());
        }
    }

    clean
}

fn parse_acf_quoted_value(content: &str, key: &str) -> Option<String> {
    let mut tokens = AcfTokenizer::new(content);

    while let Some(token) = tokens.next_token() {
        let AcfToken::Quoted(candidate_key) = token else {
            continue;
        };

        match tokens.next_token() {
            Some(AcfToken::Quoted(value)) if candidate_key.eq_ignore_ascii_case(key) => {
                return Some(value);
            }
            Some(_) => continue,
            None => break,
        }
    }

    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AcfToken {
    Quoted(String),
    OpenBrace,
    CloseBrace,
}

struct AcfTokenizer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> AcfTokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn next_token(&mut self) -> Option<AcfToken> {
        loop {
            self.skip_ignored();

            let byte = *self.input.as_bytes().get(self.pos)?;

            match byte {
                b'"' => return self.read_quoted_string().map(AcfToken::Quoted),
                b'{' => {
                    self.pos += 1;
                    return Some(AcfToken::OpenBrace);
                }
                b'}' => {
                    self.pos += 1;
                    return Some(AcfToken::CloseBrace);
                }
                _ => self.advance_char()?,
            }
        }
    }

    fn skip_ignored(&mut self) {
        loop {
            self.skip_whitespace();

            let rest = &self.input[self.pos..];

            if rest.starts_with("//") {
                self.skip_line_comment();
                continue;
            }

            if rest.starts_with("/*") {
                self.skip_block_comment();
                continue;
            }

            break;
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.input[self.pos..].chars().next() {
            if !ch.is_whitespace() {
                break;
            }

            self.pos += ch.len_utf8();
        }
    }

    fn skip_line_comment(&mut self) {
        let rest = &self.input[self.pos..];

        match rest.find('\n') {
            Some(newline) => self.pos += newline + 1,
            None => self.pos = self.input.len(),
        }
    }

    fn skip_block_comment(&mut self) {
        let rest_after_open = &self.input[self.pos + 2..];

        match rest_after_open.find("*/") {
            Some(end) => self.pos += 2 + end + 2,
            None => self.pos = self.input.len(),
        }
    }

    fn read_quoted_string(&mut self) -> Option<String> {
        debug_assert_eq!(self.input.as_bytes().get(self.pos), Some(&b'"'));

        self.pos += 1;

        let mut value = String::new();

        while self.pos < self.input.len() {
            let ch = self.take_char()?;

            match ch {
                '"' => return Some(value),
                '\\' => self.push_escaped_char(&mut value)?,
                _ => value.push(ch),
            }
        }

        None
    }

    fn push_escaped_char(&mut self, value: &mut String) -> Option<()> {
        let escaped = self.take_char()?;

        match escaped {
            '"' | '\\' => value.push(escaped),
            'n' => value.push('\n'),
            'r' => value.push('\r'),
            't' => value.push('\t'),
            other => {
                value.push('\\');
                value.push(other);
            }
        }

        Some(())
    }

    fn take_char(&mut self) -> Option<char> {
        let ch = self.input[self.pos..].chars().next()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn advance_char(&mut self) -> Option<()> {
        self.take_char().map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn resolves_app_id_from_manifest_installdir_match() {
        let root = temp_dir("steam-manifest");
        let steamapps = root.join("steamapps");
        let common = steamapps.join("common").join("TestGameDir");
        fs::create_dir_all(&common).expect("dirs");

        fs::write(
            steamapps.join("appmanifest_1234567.acf"),
            r#""AppState"
{
    "appid"        "1234567"
    "installdir"  "TestGameDir"
    "name"        "My Test Game"
}
"#,
        )
        .expect("acf");

        let details = steam_install_details(&common).expect("steam details");

        assert_eq!(details.app_id, "1234567");
        assert_eq!(details.display_name.as_deref(), Some("My Test Game"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn installdir_match_is_case_insensitive() {
        let root = temp_dir("steam-manifest-ci");
        let steamapps = root.join("steamapps");
        let common = steamapps.join("common").join("MixedCaseDir");
        fs::create_dir_all(&common).expect("dirs");

        fs::write(
            steamapps.join("appmanifest_42.acf"),
            r#""AppState"
{
    "installdir" "mixedcasedir"
}
"#,
        )
        .expect("acf");

        let details = steam_install_details(&common).expect("details");
        assert_eq!(details.app_id, "42");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn skips_malformed_or_incomplete_manifests() {
        let root = temp_dir("steam-manifest-skip-bad");
        let steamapps = root.join("steamapps");
        let common = steamapps.join("common").join("TestGameDir");
        fs::create_dir_all(&common).expect("dirs");

        fs::write(steamapps.join("appmanifest_1.acf"), r#""name" "Broken""#).expect("acf");

        fs::write(
            steamapps.join("appmanifest_2.acf"),
            r#""installdir" "TestGameDir"
"name" "Good"
"#,
        )
        .expect("acf");

        let details = steam_install_details(&common).expect("details");

        assert_eq!(details.app_id, "2");
        assert_eq!(details.display_name.as_deref(), Some("Good"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ignores_non_manifest_files_and_non_numeric_manifest_ids() {
        let root = temp_dir("steam-manifest-ignore");
        let steamapps = root.join("steamapps");
        let common = steamapps.join("common").join("TestGameDir");
        fs::create_dir_all(&common).expect("dirs");

        fs::write(
            steamapps.join("appmanifest_abc.acf"),
            r#""installdir" "TestGameDir""#,
        )
        .expect("acf");

        fs::write(
            steamapps.join("not-a-manifest.acf"),
            r#""installdir" "TestGameDir""#,
        )
        .expect("acf");

        fs::write(
            steamapps.join("appmanifest_123.acf"),
            r#""installdir" "TestGameDir""#,
        )
        .expect("acf");

        let details = steam_install_details(&common).expect("details");
        assert_eq!(details.app_id, "123");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_dirs_in_steamapps_collects_installdir_values_from_acf_files() {
        let root = temp_dir("steam-install-dirs");
        let steamapps = root.join("steamapps");
        fs::create_dir_all(&steamapps).expect("steamapps dir");

        fs::write(
            steamapps.join("appmanifest_111.acf"),
            r#""AppState"
{
    "installdir" "GameAlpha"
    "name" "Alpha"
}
"#,
        )
        .expect("alpha manifest");

        fs::write(
            steamapps.join("appmanifest_222.acf"),
            r#""AppState"
{
    "installdir" "GameBeta"
}
"#,
        )
        .expect("beta manifest");

        // Non-manifest files in steamapps should be ignored.
        fs::write(steamapps.join("libraryfolders.vdf"), "// not a manifest").expect("vdf");
        fs::write(
            steamapps.join("appmanifest_abc.acf"),
            r#""installdir" "Garbage""#,
        )
        .expect("non-numeric id");

        let installdirs = steam_install_dirs_in_steamapps(&steamapps);

        let mut sorted: Vec<String> = installdirs.into_iter().collect();
        sorted.sort();

        assert_eq!(
            sorted,
            vec!["gamealpha".to_owned(), "gamebeta".to_owned()],
            "only valid installdirs from numeric appmanifest_*.acf should be collected, lowercased",
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_dirs_in_steamapps_returns_empty_for_missing_directory() {
        let missing = temp_dir("steam-install-dirs-missing");
        // Do NOT create the directory.

        let installdirs = steam_install_dirs_in_steamapps(&missing);

        assert!(
            installdirs.is_empty(),
            "missing steamapps directory should yield empty set",
        );
    }

    #[test]
    fn install_dirs_in_steamapps_excludes_tool_app_ids() {
        let root = temp_dir("steam-install-dirs-tool-app-ids");
        let steamapps = root.join("steamapps");
        fs::create_dir_all(&steamapps).expect("steamapps dir");

        // Steamworks Common Redistributables - real Steam tool app id.
        fs::write(
            steamapps.join("appmanifest_228980.acf"),
            r#""AppState"
{
    "appid" "228980"
    "installdir" "Steamworks Common Redistributables"
    "name" "Steamworks Common Redistributables"
}
"#,
        )
        .expect("redist manifest");

        // Proton Experimental.
        fs::write(
            steamapps.join("appmanifest_1493710.acf"),
            r#""AppState"
{
    "installdir" "Proton Experimental"
    "name" "Proton Experimental"
}
"#,
        )
        .expect("proton manifest");

        fs::write(
            steamapps.join("appmanifest_400.acf"),
            r#""AppState"
{
    "installdir" "Portal"
}
"#,
        )
        .expect("portal manifest");

        let installdirs = steam_install_dirs_in_steamapps(&steamapps);

        assert_eq!(
            installdirs.len(),
            1,
            "only the real game installdir should remain",
        );
        assert!(installdirs.contains("portal"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_dirs_in_steamapps_excludes_steamworks_shared_installdir_prefix() {
        let root = temp_dir("steam-install-dirs-steamworks-shared");
        let steamapps = root.join("steamapps");
        fs::create_dir_all(&steamapps).expect("steamapps dir");

        // "Steamworks Shared" appears with various app ids depending on
        // the Steam version, so the prefix filter is what catches it.
        fs::write(
            steamapps.join("appmanifest_999999.acf"),
            r#""AppState"
{
    "installdir" "Steamworks Shared"
}
"#,
        )
        .expect("shared manifest");

        fs::write(
            steamapps.join("appmanifest_400.acf"),
            r#""AppState"
{
    "installdir" "Portal"
}
"#,
        )
        .expect("portal manifest");

        let installdirs = steam_install_dirs_in_steamapps(&steamapps);

        assert!(
            !installdirs.contains("steamworks shared"),
            "Steamworks Shared must be filtered out by installdir prefix"
        );
        assert!(installdirs.contains("portal"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn install_dirs_in_steamapps_excludes_steam_runtime_and_audio_and_controller_prefixes() {
        let root = temp_dir("steam-install-dirs-runtime-prefixes");
        let steamapps = root.join("steamapps");
        fs::create_dir_all(&steamapps).expect("steamapps dir");

        for (manifest_id, installdir) in [
            ("888001", "Steam Linux Runtime - Soldier"),
            ("888002", "Steam Audio Tools"),
            ("888003", "Steam Controller Configs"),
            ("888004", "SteamVR"),
            ("888005", "Proton 9.0"),
        ] {
            fs::write(
                steamapps.join(format!("appmanifest_{manifest_id}.acf")),
                format!("\"AppState\" {{ \"installdir\" \"{installdir}\" }}"),
            )
            .expect("tool manifest");
        }

        fs::write(
            steamapps.join("appmanifest_400.acf"),
            r#""AppState"
{
    "installdir" "Portal"
}
"#,
        )
        .expect("portal manifest");

        let installdirs = steam_install_dirs_in_steamapps(&steamapps);

        assert_eq!(
            installdirs,
            std::iter::once("portal".to_owned()).collect(),
            "only the real game installdir should survive prefix-based filtering",
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn is_steam_tool_app_by_id_includes_redistributables() {
        assert!(super::is_steam_tool_app(
            "228980",
            "Steamworks Common Redistributables",
        ));
    }

    #[test]
    fn is_steam_tool_app_by_prefix_matches_steamworks_shared() {
        assert!(super::is_steam_tool_app("999999", "Steamworks Shared"));
    }

    #[test]
    fn is_steam_tool_app_rejects_normal_game() {
        assert!(!super::is_steam_tool_app("1234567", "TestGameDir"));
    }

    #[test]
    fn parser_handles_comments_and_escaped_quotes() {
        let content = r#"
            // leading comment
            "AppState"
            {
                "installdir" "TestGameDir" /* inline block comment */
                "name" "Game with \"quotes\""
            }
        "#;

        let manifest = parse_app_manifest(content).expect("manifest");

        assert_eq!(manifest.installdir, "TestGameDir");
        assert_eq!(
            manifest.display_name.as_deref(),
            Some("Game with \"quotes\"")
        );
    }

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();

        std::env::temp_dir().join(format!("renderpilot-{label}-{nanos}"))
    }
}
