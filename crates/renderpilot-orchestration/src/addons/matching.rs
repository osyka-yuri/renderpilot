//! Tool-agnostic game-matching vocabulary shared by every addon tool's manifest.
//!
//! An installed game is reduced to a set of [`MatchFacts`], which a tool's manifest
//! evaluates against ordered, tiered [`MatchRule`]s. The primitives here — the fact
//! set, the rule kinds, the per-rule matcher, and the confidence/incompatibility
//! verdicts — are identical for RenoDX and Luma; each tool keeps only its own
//! manifest-shaped selection logic (which title wins, how it is routed) on top.
//!
//! `MatchFacts` lives here (rather than with the game-analysis assembler) because it
//! is the *input the match rules evaluate against* — it belongs with the matching
//! vocabulary. [`crate::addons::game_analysis`] merely produces one from a game on
//! disk.

use renderpilot_domain::{Architecture, ExeGraphicsInfo, GraphicsApi, Launcher};
use serde::{Deserialize, Serialize};

/// Facts about an installed game that match rules are evaluated against.
#[derive(Debug, Clone)]
pub struct MatchFacts {
    /// Launcher that owns the game.
    pub launcher: Launcher,
    /// Launcher-specific id (Steam AppID, Epic catalog id, GOG product id).
    pub external_id: Option<String>,
    /// Game executable file name (for example `Cyberpunk2077.exe`).
    pub exe_file_name: Option<String>,
    /// Lowercase SHA-256 hex of the game executable, when computed.
    pub exe_sha256: Option<String>,
    /// Detected engine.
    pub engine: Option<Engine>,
    /// Graphics API and architecture detected from the executable.
    pub graphics: ExeGraphicsInfo,
}

/// A single rule used to match an installed game to a manifest title.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MatchRule {
    /// What the rule matches against.
    pub kind: MatchKind,
    /// The value to match (a Steam AppID, exe-name glob, engine id, …).
    #[serde(default)]
    pub value: String,
    /// Specificity tier; higher wins. Conventionally: id 100, fingerprint 90,
    /// exe-name 70, engine 40, generic 10.
    pub tier: u32,
}

/// Dimension a [`MatchRule`] matches against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    /// Steam application id.
    SteamAppid,
    /// Epic Games catalog id.
    EpicId,
    /// GOG product id.
    GogId,
    /// SHA-256 fingerprint of the game executable.
    ExeSha256,
    /// Case-insensitive glob over the executable file name.
    ExeName,
    /// Detected engine (for example `unreal`, `unity`).
    Engine,
    /// Lowest-priority catch-all fallback.
    Generic,
}

/// How confident we are that an install will work, from the wiki test-map status
/// and how the match was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchConfidence {
    /// Listed and verified working.
    Verified,
    /// Listed but work-in-progress / experimental.
    Experimental,
    /// Listed-but-untested, or matched only by engine (a generic guess).
    Untested,
}

/// Reason a matched game cannot be installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum IncompatibilityReason {
    /// The detected API is not one a DirectX add-on can target.
    ApiUnsupported {
        /// API detected from the executable.
        detected: GraphicsApi,
    },
    /// The detected API is supported but not allowed by this title.
    ApiNotAllowed {
        /// API detected from the executable.
        detected: GraphicsApi,
        /// APIs the title declares support for.
        required: Vec<GraphicsApi>,
    },
    /// The executable architecture could not be determined.
    ArchUnknown,
    /// The executable architecture is known but does not match what the title
    /// requires (e.g. a 32-bit game curated for a 64-bit-only add-on).
    ArchMismatch {
        /// Architecture detected from the executable.
        detected: Architecture,
        /// Architecture the title requires.
        required: Architecture,
    },
}

/// Upstream wiki test-map status of a manifest title.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Verified working.
    Working,
    /// Work in progress / experimental.
    Construction,
    /// Untested.
    #[default]
    Unknown,
}

/// Maps a wiki test-map [`Status`] to a [`MatchConfidence`].
#[must_use]
pub fn confidence_for_status(status: Status) -> MatchConfidence {
    match status {
        Status::Working => MatchConfidence::Verified,
        Status::Construction => MatchConfidence::Experimental,
        Status::Unknown => MatchConfidence::Untested,
    }
}

/// Maps a matched title's wiki status to a [`MatchConfidence`], downgraded to
/// [`MatchConfidence::Untested`] for any engine or generic-catch-all match — a
/// universal add-on's per-game compatibility is unknown regardless of how
/// confident the manifest is about the game itself.
#[must_use]
pub fn confidence_for_match(status: Status, kind: MatchKind) -> MatchConfidence {
    if matches!(kind, MatchKind::Engine | MatchKind::Generic) {
        return MatchConfidence::Untested;
    }
    confidence_for_status(status)
}

/// Tie-break rank for a title's curated status. `Construction` and `Unknown`
/// intentionally remain equivalent, matching the old derived-channel policy.
const fn status_rank(status: Status) -> u8 {
    match status {
        Status::Working => 0,
        Status::Construction | Status::Unknown => 1,
    }
}

/// Engine detected from a game or named by an engine-level manifest rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    /// Unreal Engine.
    Unreal,
    /// Unreal Engine with the extended (UE-Extended) treatment; curated, not auto-detected.
    UnrealExtended,
    /// Unity.
    Unity,
}

impl Engine {
    /// Stable manifest/local-identity string for this engine.
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Unreal => "unreal",
            Self::UnrealExtended => "unreal_extended",
            Self::Unity => "unity",
        }
    }
}

/// Parses a string value (from a manifest match rule) to an [`Engine`].
#[must_use]
pub(crate) fn parse_engine(value: &str) -> Option<Engine> {
    match value.trim().to_ascii_lowercase().as_str() {
        "unreal" => Some(Engine::Unreal),
        "unreal_extended" | "unreal-extended" => Some(Engine::UnrealExtended),
        "unity" => Some(Engine::Unity),
        _ => None,
    }
}

/// A manifest title that can compete in the shared best-match selection
/// ([`select_title`]). Implemented by each tool's title type; the selection
/// tie-break (rule tier, then curated status, then smallest id) is thereby
/// identical across tools by construction.
pub trait SelectableTitle {
    /// Ordered match rules; selection prefers the highest [`MatchRule::tier`].
    fn match_rules(&self) -> &[MatchRule];
    /// Curated test-map status, the first tie-break after rule tier.
    fn status(&self) -> Status;
    /// Stable title id, the final lexicographic tie-break (smallest wins).
    fn id(&self) -> &str;
}

/// Selects the best `(title, matching-rule)` for the facts: highest rule tier,
/// then more-mature status, then lexicographically smallest title id.
#[must_use]
pub fn select_title<'m, T: SelectableTitle>(
    titles: &'m [T],
    facts: &MatchFacts,
) -> Option<(&'m T, &'m MatchRule)> {
    titles
        .iter()
        .filter_map(|title| best_matching_rule(title, facts).map(|rule| (title, rule)))
        .max_by(|left, right| selection_key(left).cmp(&selection_key(right)))
}

fn selection_key<'a, T: SelectableTitle>(
    candidate: &(&'a T, &'a MatchRule),
) -> (u32, std::cmp::Reverse<u8>, std::cmp::Reverse<&'a str>) {
    let (title, rule) = *candidate;
    (
        rule.tier,
        std::cmp::Reverse(status_rank(title.status())),
        std::cmp::Reverse(title.id()),
    )
}

fn best_matching_rule<'m, T: SelectableTitle>(
    title: &'m T,
    facts: &MatchFacts,
) -> Option<&'m MatchRule> {
    title
        .match_rules()
        .iter()
        .filter(|rule| rule_matches(rule.kind, &rule.value, facts))
        .max_by_key(|rule| rule.tier)
}

/// Whether a single rule of `kind` with `value` matches `facts`.
#[must_use]
pub fn rule_matches(kind: MatchKind, value: &str, facts: &MatchFacts) -> bool {
    match kind {
        MatchKind::SteamAppid => facts.launcher == Launcher::Steam && external_id_eq(facts, value),
        MatchKind::EpicId => facts.launcher == Launcher::Epic && external_id_eq(facts, value),
        MatchKind::GogId => facts.launcher == Launcher::Gog && external_id_eq(facts, value),
        MatchKind::ExeSha256 => facts
            .exe_sha256
            .as_deref()
            .is_some_and(|hash| hash.eq_ignore_ascii_case(value.trim())),
        MatchKind::ExeName => facts
            .exe_file_name
            .as_deref()
            .is_some_and(|name| glob_matches_ci(value, name)),
        MatchKind::Engine => match parse_engine(value) {
            Some(engine) => facts.engine == Some(engine),
            None => false,
        },
        MatchKind::Generic => true,
    }
}

/// Whether the game's launcher-specific id equals `value` (trimmed).
#[must_use]
pub fn external_id_eq(facts: &MatchFacts, value: &str) -> bool {
    facts
        .external_id
        .as_deref()
        .is_some_and(|id| id.trim() == value.trim())
}

/// Case-insensitive glob match supporting `*` (any run) and `?` (any one char).
#[must_use]
pub fn glob_matches_ci(pattern: &str, file_name: &str) -> bool {
    let pattern: Vec<u8> = pattern.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let name: Vec<u8> = file_name.bytes().map(|b| b.to_ascii_lowercase()).collect();
    let (mut p, mut f) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut star_f = 0usize;

    while f < name.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == name[f]) {
            p += 1;
            f += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            star_f = f;
            p += 1;
        } else if let Some(star_p) = star {
            p = star_p + 1;
            star_f += 1;
            f = star_f;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matches_star_and_question_mark() {
        assert!(glob_matches_ci("game*.exe", "GameLauncher.exe"));
        assert!(glob_matches_ci("game?.exe", "game1.exe"));
        assert!(!glob_matches_ci("game?.exe", "game12.exe"));
        assert!(glob_matches_ci("*.exe", "anything.exe"));
        assert!(!glob_matches_ci("game.exe", "other.exe"));
    }

    #[test]
    fn confidence_maps_status() {
        assert_eq!(
            confidence_for_status(Status::Working),
            MatchConfidence::Verified
        );
        assert_eq!(
            confidence_for_status(Status::Construction),
            MatchConfidence::Experimental
        );
        assert_eq!(
            confidence_for_status(Status::Unknown),
            MatchConfidence::Untested
        );
    }

    struct TestTitle {
        id: &'static str,
        status: Status,
        rules: Vec<MatchRule>,
    }

    impl SelectableTitle for TestTitle {
        fn match_rules(&self) -> &[MatchRule] {
            &self.rules
        }

        fn status(&self) -> Status {
            self.status
        }

        fn id(&self) -> &str {
            self.id
        }
    }

    fn generic_rule(tier: u32) -> MatchRule {
        MatchRule {
            kind: MatchKind::Generic,
            value: String::new(),
            tier,
        }
    }

    fn any_facts() -> MatchFacts {
        MatchFacts {
            launcher: Launcher::Manual,
            external_id: None,
            exe_file_name: None,
            exe_sha256: None,
            engine: None,
            graphics: ExeGraphicsInfo::new(Vec::new(), None),
        }
    }

    #[test]
    fn select_title_prefers_the_highest_rule_tier() {
        let low = TestTitle {
            id: "aaa",
            status: Status::Working,
            rules: vec![generic_rule(70)],
        };
        let high = TestTitle {
            id: "bbb",
            status: Status::Working,
            rules: vec![generic_rule(100)],
        };
        let titles = [low, high];
        let (winner, rule) = select_title(&titles, &any_facts()).expect("a match");
        assert_eq!(winner.id, "bbb");
        assert_eq!(rule.tier, 100);
    }

    #[test]
    fn select_title_breaks_a_tier_tie_by_the_more_mature_status() {
        let beta = TestTitle {
            id: "aaa",
            status: Status::Construction,
            rules: vec![generic_rule(100)],
        };
        let stable = TestTitle {
            id: "bbb",
            status: Status::Working,
            rules: vec![generic_rule(100)],
        };
        let titles = [beta, stable];
        let (winner, _) = select_title(&titles, &any_facts()).expect("a match");
        assert_eq!(winner.id, "bbb");
    }

    #[test]
    fn select_title_breaks_a_tier_and_status_tie_by_the_smallest_id() {
        let bbb = TestTitle {
            id: "bbb",
            status: Status::Unknown,
            rules: vec![generic_rule(100)],
        };
        let aaa = TestTitle {
            id: "aaa",
            status: Status::Construction,
            rules: vec![generic_rule(100)],
        };
        let titles = [bbb, aaa];
        let (winner, _) = select_title(&titles, &any_facts()).expect("a match");
        assert_eq!(winner.id, "aaa");
    }
}
