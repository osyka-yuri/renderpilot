//! Fresh, scoped authority for potentially risky game-file mutations.
//!
//! This module deliberately exposes only an opaque context token and the
//! closed anti-cheat engine list needed by the UI. Evidence paths and detector
//! diagnostics remain backend-only. Mutation callers validate a typed permit
//! while holding their final resource lock; validation always re-observes the
//! resource instead of trusting a cached assessment or a boolean confirmation.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_application::GameRepository;
use renderpilot_detection::{
    AntiCheatEngine, AntiCheatScanCompleteness, AntiCheatScanReport, scan_anticheat,
};
use renderpilot_domain::GameId;
use renderpilot_domain::mutation_features::{SafetyRequirement, safety_requirement};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::Context;
use crate::ServiceError;

const SAFETY_TOKEN_VERSION: &str = "v1";

/// Resource scope represented by a safety permit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafetyScope {
    /// One concrete game installation.
    Game(GameId),
    /// The process-wide shared ReShade Vulkan layer.
    SharedVulkan,
    /// A token carried a scope that cannot be represented as a user-facing
    /// game/resource identity. This keeps mismatch diagnostics opaque.
    Unknown,
}

impl fmt::Display for SafetyScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Game(game_id) => write!(formatter, "game {game_id}"),
            Self::SharedVulkan => formatter.write_str("shared Vulkan layer"),
            Self::Unknown => formatter.write_str("unknown safety scope"),
        }
    }
}

/// Completeness of the detector observation exposed to the UI.
pub type ScanCompleteness = AntiCheatScanCompleteness;

/// Fresh game-scoped anti-cheat assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameFileSafetyAssessment {
    /// Stable catalog identity of the game installation.
    pub game_id: GameId,
    /// Opaque versioned token for the observed game state.
    pub context_token: String,
    /// Known anti-cheat engines in deterministic display order.
    pub detected_engines: Vec<AntiCheatEngine>,
    /// Whether the bounded observation was complete.
    pub scan_completeness: ScanCompleteness,
}

/// Fresh process-wide shared Vulkan-layer assessment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SharedVulkanSafetyAssessment {
    /// Opaque versioned token for the observed shared layer state.
    pub context_token: String,
}

/// Typed permit authorizing one final game-scoped revalidation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameSafetyPermit {
    game_id: GameId,
    scope_hash: String,
    observation_hash: String,
}

impl GameSafetyPermit {
    /// Returns the game scope represented by this permit.
    #[must_use]
    pub fn game_id(&self) -> &GameId {
        &self.game_id
    }

    /// Returns the typed scope represented by this permit.
    #[must_use]
    pub fn scope(&self) -> SafetyScope {
        SafetyScope::Game(self.game_id.clone())
    }
}

/// Typed permit authorizing one final shared-Vulkan revalidation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedVulkanSafetyPermit {
    scope_hash: String,
    observation_hash: String,
}

impl SharedVulkanSafetyPermit {
    /// Returns the shared resource scope represented by this permit.
    #[must_use]
    pub const fn scope(&self) -> SafetyScope {
        SafetyScope::SharedVulkan
    }
}

/// Typed safety authority carried by a game mutation that may also touch the
/// shared Vulkan resource. The shared permit is optional until the resolved
/// operation proves that the shared resource will be mutated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameMutationSafetyPermits {
    game: GameSafetyPermit,
    shared_vulkan: Option<SharedVulkanSafetyPermit>,
}

impl GameMutationSafetyPermits {
    #[must_use]
    pub(crate) fn game(&self) -> &GameSafetyPermit {
        &self.game
    }

    #[must_use]
    pub(crate) fn shared_vulkan(&self) -> Option<&SharedVulkanSafetyPermit> {
        self.shared_vulkan.as_ref()
    }
}

/// Stateless authority for issuing and validating fresh safety contexts.
#[derive(Debug, Clone, Copy, Default)]
pub struct FileSafetyAuthority;

impl FileSafetyAuthority {
    /// Creates the process-local safety authority.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Parses a transport token into game-scoped mutation authority. Parsing
    /// happens before unlocked prepare; freshness is checked again only at the
    /// final commit barrier under the matching game guard.
    pub fn game_permit(
        &self,
        game_id: GameId,
        context_token: Option<&str>,
    ) -> Result<GameSafetyPermit, ServiceError> {
        let context_token = context_token
            .filter(|token| !token.trim().is_empty())
            .ok_or_else(|| {
                ServiceError::safety_context_missing(SafetyScope::Game(game_id.clone()))
            })?;
        let parsed = parse_token(context_token)?;
        if parsed.namespace != SafetyTokenNamespace::Game {
            return Err(ServiceError::safety_context_scope_mismatch(
                SafetyScope::Game(game_id),
                parsed.namespace.scope(),
            ));
        }
        Ok(GameSafetyPermit {
            game_id,
            scope_hash: parsed.scope_hash,
            observation_hash: parsed.observation_hash,
        })
    }

    /// Parses a transport token into shared-Vulkan mutation authority.
    pub fn shared_vulkan_permit(
        &self,
        context_token: Option<&str>,
    ) -> Result<SharedVulkanSafetyPermit, ServiceError> {
        let context_token = context_token
            .filter(|token| !token.trim().is_empty())
            .ok_or_else(|| ServiceError::safety_context_missing(SafetyScope::SharedVulkan))?;
        let parsed = parse_token(context_token)?;
        if parsed.namespace != SafetyTokenNamespace::SharedVulkan {
            return Err(ServiceError::safety_context_scope_mismatch(
                SafetyScope::SharedVulkan,
                parsed.namespace.scope(),
            ));
        }
        Ok(SharedVulkanSafetyPermit {
            scope_hash: parsed.scope_hash,
            observation_hash: parsed.observation_hash,
        })
    }

    /// Parses the game permit and an optional shared permit carried by a
    /// transport request. Whether the shared permit is required is decided by
    /// the freshly resolved operation plan at the commit boundary.
    pub fn game_mutation_permits(
        &self,
        game_id: GameId,
        game_context_token: Option<&str>,
        shared_vulkan_context_token: Option<&str>,
    ) -> Result<GameMutationSafetyPermits, ServiceError> {
        Ok(GameMutationSafetyPermits {
            game: self.game_permit(game_id, game_context_token)?,
            shared_vulkan: shared_vulkan_context_token
                .map(|token| self.shared_vulkan_permit(Some(token)))
                .transpose()?,
        })
    }

    /// Issues a fresh game assessment. This path intentionally bypasses the
    /// generation-keyed Game Details cache.
    pub fn issue_game_assessment(
        &self,
        context: &Context,
        game_id: &GameId,
    ) -> Result<GameFileSafetyAssessment, ServiceError> {
        let game = context
            .storage()
            .require_game(game_id)
            .map_err(ServiceError::from)?;
        let install_root = canonical_install_root(Path::new(game.install_path().as_str()));
        let report = scan_anticheat(&install_root);
        let context_token = game_context_token(game_id, &install_root, &report);
        let mut detected_engines = report.engines.clone();
        detected_engines.sort_by_key(|engine| engine_id(*engine));

        Ok(GameFileSafetyAssessment {
            game_id: game_id.clone(),
            context_token,
            detected_engines,
            scan_completeness: report.completeness,
        })
    }

    /// Issues a fresh observation of the process-wide shared Vulkan layer.
    pub fn issue_shared_vulkan_assessment(
        &self,
    ) -> Result<SharedVulkanSafetyAssessment, ServiceError> {
        let context_token = shared_vulkan_context_token()?;
        Ok(SharedVulkanSafetyAssessment { context_token })
    }

    /// Validates a game permit under its exact guard and enters the synchronous
    /// commit closure only after the observation is proven fresh.
    pub(crate) fn authorize_game_commit<T>(
        &self,
        context: &Context,
        feature: &str,
        guard: &crate::game_mutation_lock::GameMutationGuard,
        permit: &GameSafetyPermit,
        commit: impl FnOnce() -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        require_game_feature(feature)?;
        self.validate_game_permit(context, guard, permit)?;
        commit()
    }

    /// Validates a shared permit under the concrete shared-resource guard and
    /// enters a synchronous commit closure.
    pub(crate) fn authorize_shared_vulkan_commit<T>(
        &self,
        feature: &str,
        guard: &crate::addons::vulkan_lock::SharedVulkanMutationGuard,
        permit: &SharedVulkanSafetyPermit,
        commit: impl FnOnce() -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        require_shared_vulkan_feature(feature)?;
        self.validate_shared_vulkan_permit(guard, permit)?;
        commit()
    }

    /// Validates both scopes under the canonical combined lock set before one
    /// synchronous combined commit begins.
    pub(crate) fn authorize_game_shared_commit<T>(
        &self,
        context: &Context,
        feature: &str,
        guards: &crate::game_mutation_lock::GameSharedMutationGuards,
        permits: &GameMutationSafetyPermits,
        commit: impl FnOnce() -> Result<T, ServiceError>,
    ) -> Result<T, ServiceError> {
        require_game_feature(feature)?;
        require_shared_vulkan_feature(feature)?;
        self.validate_game_permit(context, guards.game(), permits.game())?;
        let shared_permit = permits
            .shared_vulkan()
            .ok_or_else(|| ServiceError::safety_context_missing(SafetyScope::SharedVulkan))?;
        self.validate_shared_vulkan_permit(guards.shared_vulkan(), shared_permit)?;
        commit()
    }

    fn validate_game_permit(
        &self,
        context: &Context,
        guard: &crate::game_mutation_lock::GameMutationGuard,
        permit: &GameSafetyPermit,
    ) -> Result<(), ServiceError> {
        let game_id = guard.game_id();
        if permit.game_id != *game_id {
            return Err(ServiceError::safety_context_scope_mismatch(
                SafetyScope::Game(game_id.clone()),
                permit.scope(),
            ));
        }

        let current = self.issue_game_assessment(context, game_id)?;
        let current_token = parse_token(&current.context_token)
            .map_err(|_| ServiceError::safety_context_stale(SafetyScope::Game(game_id.clone())))?;
        if permit.scope_hash != current_token.scope_hash {
            return Err(ServiceError::safety_context_scope_mismatch(
                SafetyScope::Game(game_id.clone()),
                SafetyScope::Unknown,
            ));
        }
        if permit.observation_hash != current_token.observation_hash {
            return Err(ServiceError::safety_context_stale(SafetyScope::Game(
                game_id.clone(),
            )));
        }
        Ok(())
    }

    fn validate_shared_vulkan_permit(
        &self,
        _guard: &crate::addons::vulkan_lock::SharedVulkanMutationGuard,
        permit: &SharedVulkanSafetyPermit,
    ) -> Result<(), ServiceError> {
        let current = self.issue_shared_vulkan_assessment()?;
        let current_token = parse_token(&current.context_token)
            .map_err(|_| ServiceError::safety_context_stale(SafetyScope::SharedVulkan))?;
        if permit.scope_hash != current_token.scope_hash {
            return Err(ServiceError::safety_context_scope_mismatch(
                SafetyScope::SharedVulkan,
                SafetyScope::Unknown,
            ));
        }
        if permit.observation_hash != current_token.observation_hash {
            return Err(ServiceError::safety_context_stale(
                SafetyScope::SharedVulkan,
            ));
        }
        Ok(())
    }
}

/// Issues a fresh assessment for one catalog game.
fn require_game_feature(feature: &str) -> Result<(), ServiceError> {
    match safety_requirement(feature) {
        Some(SafetyRequirement::Game | SafetyRequirement::GameWithOptionalSharedVulkan) => Ok(()),
        Some(requirement) => Err(ServiceError::InvalidInput(format!(
            "feature {feature} cannot use the game safety gate ({requirement:?})"
        ))),
        None => Err(ServiceError::InvalidInput(format!(
            "feature {feature} has no registered game safety policy"
        ))),
    }
}

fn require_shared_vulkan_feature(feature: &str) -> Result<(), ServiceError> {
    match safety_requirement(feature) {
        Some(SafetyRequirement::SharedVulkan | SafetyRequirement::GameWithOptionalSharedVulkan) => {
            Ok(())
        }
        Some(requirement) => Err(ServiceError::InvalidInput(format!(
            "feature {feature} cannot use the shared Vulkan safety gate ({requirement:?})"
        ))),
        None => Err(ServiceError::InvalidInput(format!(
            "feature {feature} has no registered shared Vulkan safety policy"
        ))),
    }
}

fn canonical_install_root(path: &Path) -> PathBuf {
    // A catalog row may outlive a temporarily unavailable installation. Use
    // the canonical existing ancestor in that case so the detector can still
    // return a limited assessment and the UI can keep the generic advisory
    // visible. The lexical fallback is included in the token and therefore
    // cannot be mistaken for a complete canonical observation.
    fs::canonicalize(path)
        .or_else(|_| crate::paths::canonical_candidate(path))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn game_context_token(
    game_id: &GameId,
    install_root: &Path,
    report: &AntiCheatScanReport,
) -> String {
    let mut evidence = report
        .evidence
        .iter()
        .map(|entry| {
            format!(
                "{}|{}|{}|{}",
                engine_id(entry.engine),
                entry.matched_marker,
                evidence_kind_id(entry.kind),
                canonical_identity(&entry.path),
            )
        })
        .collect::<Vec<_>>();
    evidence.sort();

    let mut engines = report
        .engines
        .iter()
        .copied()
        .map(engine_id)
        .collect::<Vec<_>>();
    engines.sort_unstable();

    let mut unreadable_directories = report
        .unreadable_directories
        .iter()
        .map(|path| canonical_identity(path))
        .collect::<Vec<_>>();
    unreadable_directories.sort();

    let mut unreadable_entries = report
        .unreadable_entries
        .iter()
        .map(|path| canonical_identity(path))
        .collect::<Vec<_>>();
    unreadable_entries.sort();

    let scope_hash = game_scope_hash(game_id, install_root);
    let observation_payload = format!(
        "observation=game\ncompleteness={}\ntruncated={}\nscanned_entries={}\nunreadable_entry_count={}\nengines={}\nevidence={}\nunreadable_directories={}\nunreadable_entries={}",
        completeness_id(report.completeness),
        report.truncated,
        report.scanned_entry_count,
        report.unreadable_entry_count,
        engines.join(";"),
        evidence.join(";"),
        unreadable_directories.join(";"),
        unreadable_entries.join(";"),
    );
    framed_token(
        SafetyTokenNamespace::Game,
        &scope_hash,
        &opaque_token(observation_payload.as_bytes()),
    )
}

fn shared_vulkan_context_token() -> Result<String, ServiceError> {
    let report = crate::addons::renodx::vulkan::layer_report();
    let mut payload = format!(
        "observation=shared_vulkan\ndetection={:?}\narchitecture={:?}\nvisibility={:?}\nversion={}",
        report.layer_detection,
        report.layer_facts.architecture,
        report.layer_facts.loader_visibility,
        report.layer_facts.version.as_deref().unwrap_or(""),
    );

    for (label, path) in [
        ("manifest", report.layer_facts.manifest_path.as_deref()),
        ("dll", report.layer_facts.dll_path.as_deref()),
    ] {
        payload.push_str(&format!(
            "\n{label}_path={}",
            path.map(canonical_identity).unwrap_or_default()
        ));
        payload.push_str(&format!(
            "\n{label}_observation={}",
            path.map(observe_shared_file)
                .unwrap_or_else(|| "absent".to_owned())
        ));
    }

    // The public Vulkan report intentionally omits the app-registration file,
    // but shared-layer mutations also rewrite it when a game is registered or
    // removed. Include its content in the opaque observation without exposing
    // the path or app list on the wire.
    if let Some(common_dir) = renderpilot_platform_windows::vulkan_layer::reshade_common_dir() {
        let apps_ini = common_dir.join("ReShadeApps.ini");
        payload.push_str(&format!(
            "\napps_ini_observation={}",
            observe_shared_file(&apps_ini)
        ));
    }

    let mut diagnostics = report
        .diagnostic_reasons
        .iter()
        .map(|diagnostic| format!("{diagnostic:?}"))
        .collect::<Vec<_>>();
    diagnostics.sort_unstable();
    payload.push_str(&format!("\ndiagnostics={}", diagnostics.join(";")));

    let scope_hash = shared_vulkan_scope_hash();
    Ok(framed_token(
        SafetyTokenNamespace::SharedVulkan,
        &scope_hash,
        &opaque_token(payload.as_bytes()),
    ))
}

fn game_scope_hash(game_id: &GameId, install_root: &Path) -> String {
    opaque_token(
        format!(
            "{SAFETY_TOKEN_VERSION}\nscope=game\ngame_id={}\nroot={}",
            game_id.as_str(),
            canonical_identity(install_root),
        )
        .as_bytes(),
    )
}

fn shared_vulkan_scope_hash() -> String {
    let resource_root = renderpilot_platform_windows::vulkan_layer::reshade_common_dir()
        .map(|path| canonical_identity(&path))
        .unwrap_or_else(|| "unsupported".to_owned());
    opaque_token(
        format!("{SAFETY_TOKEN_VERSION}\nscope=shared_vulkan\nroot={resource_root}").as_bytes(),
    )
}

fn observe_shared_file(path: &Path) -> String {
    match renderpilot_detection::sha256_file(path) {
        Ok(hash) => format!("present:{hash}"),
        Err(_) if path.exists() => "unreadable".to_owned(),
        Err(_) => "absent".to_owned(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SafetyTokenNamespace {
    Game,
    SharedVulkan,
}

impl SafetyTokenNamespace {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Game => "game",
            Self::SharedVulkan => "shared_vulkan",
        }
    }

    const fn scope(self) -> SafetyScope {
        match self {
            Self::Game => SafetyScope::Unknown,
            Self::SharedVulkan => SafetyScope::SharedVulkan,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedSafetyToken {
    namespace: SafetyTokenNamespace,
    scope_hash: String,
    observation_hash: String,
}

fn framed_token(
    namespace: SafetyTokenNamespace,
    scope_hash: &str,
    observation_hash: &str,
) -> String {
    format!(
        "{SAFETY_TOKEN_VERSION}.{}.{}.{}",
        namespace.as_str(),
        scope_hash,
        observation_hash,
    )
}

/// Parses and validates the opaque token framing. Malformed, wrong-version,
/// and non-hex tokens fail closed as missing context; no token fragment is
/// included in the resulting error.
fn parse_token(token: &str) -> Result<ParsedSafetyToken, ServiceError> {
    let parts = token.split('.').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != SAFETY_TOKEN_VERSION {
        return Err(ServiceError::safety_context_missing(SafetyScope::Unknown));
    }
    let namespace = match parts[1] {
        "game" => SafetyTokenNamespace::Game,
        "shared_vulkan" => SafetyTokenNamespace::SharedVulkan,
        _ => return Err(ServiceError::safety_context_missing(SafetyScope::Unknown)),
    };
    if !is_token_hash(parts[2]) || !is_token_hash(parts[3]) {
        return Err(ServiceError::safety_context_missing(SafetyScope::Unknown));
    }
    Ok(ParsedSafetyToken {
        namespace,
        scope_hash: parts[2].to_owned(),
        observation_hash: parts[3].to_owned(),
    })
}

fn is_token_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn opaque_token(payload: &[u8]) -> String {
    hex::encode(Sha256::digest(payload))
}

fn canonical_identity(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

const fn engine_id(engine: AntiCheatEngine) -> &'static str {
    match engine {
        AntiCheatEngine::EasyAntiCheat => "easy_anti_cheat",
        AntiCheatEngine::BattlEye => "battleye",
    }
}

const fn evidence_kind_id(kind: renderpilot_detection::AntiCheatEvidenceKind) -> &'static str {
    match kind {
        renderpilot_detection::AntiCheatEvidenceKind::File => "file",
        renderpilot_detection::AntiCheatEvidenceKind::Directory => "directory",
        renderpilot_detection::AntiCheatEvidenceKind::Other => "other",
    }
}

const fn completeness_id(completeness: ScanCompleteness) -> &'static str {
    match completeness {
        ScanCompleteness::Complete => "complete",
        ScanCompleteness::Limited => "limited",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderpilot_application::GameRepository;
    use renderpilot_detection::{AntiCheatScanOptions, scan_anticheat_with_options};
    use renderpilot_domain::{
        GameIdentity, GameInstallation, GameRuntime, Launcher, PathRef, Platform,
    };
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn game_token_is_deterministic_and_does_not_expose_evidence() {
        let root = tempdir().expect("root");
        fs::write(root.path().join("EasyAntiCheat_x64.dll"), b"stub").expect("marker");
        let report = scan_anticheat(root.path());
        let game = GameId::new("manual:test").expect("id");

        let first = game_context_token(&game, root.path(), &report);
        let second = game_context_token(&game, root.path(), &report);

        assert_eq!(first, second);
        assert!(first.starts_with("v1.game."));
        assert_eq!(first.split('.').count(), 4);
        assert!(!first.contains("EasyAntiCheat"));
        assert!(!first.contains("dll"));
        assert!(!first.contains(game.as_str()));
        assert!(!first.contains(canonical_identity(root.path()).as_str()));
    }

    #[test]
    fn game_token_changes_for_limited_scan_observation() {
        let root = tempdir().expect("root");
        fs::write(root.path().join("game.exe"), b"stub").expect("game");
        let complete = scan_anticheat(root.path());
        let limited =
            scan_anticheat_with_options(root.path(), AntiCheatScanOptions { max_entries: 0 });
        let game = GameId::new("manual:test").expect("id");

        assert_ne!(
            game_context_token(&game, root.path(), &complete),
            game_context_token(&game, root.path(), &limited)
        );
    }

    #[test]
    fn assessment_wire_contains_only_display_and_authority_fields() {
        let assessment = GameFileSafetyAssessment {
            game_id: GameId::new("manual:wire").expect("id"),
            context_token: "a".repeat(64),
            detected_engines: vec![AntiCheatEngine::EasyAntiCheat],
            scan_completeness: ScanCompleteness::Limited,
        };
        let wire = serde_json::to_value(assessment).expect("wire");

        assert_eq!(wire["game_id"], "manual:wire");
        assert_eq!(wire["context_token"], "a".repeat(64));
        assert_eq!(
            wire["detected_engines"],
            serde_json::json!(["easy_anti_cheat"])
        );
        assert_eq!(wire["scan_completeness"], "limited");
        for forbidden in ["evidence", "path", "safe", "severity", "message"] {
            assert!(
                wire.get(forbidden).is_none(),
                "forbidden field: {forbidden}"
            );
        }
    }

    #[test]
    fn scope_mismatch_is_typed_and_does_not_compare_raw_tokens() {
        let permit = GameSafetyPermit {
            game_id: GameId::new("manual:other").expect("id"),
            scope_hash: "a".repeat(64),
            observation_hash: "b".repeat(64),
        };
        let expected = SafetyScope::Game(GameId::new("manual:expected").expect("id"));
        let error = ServiceError::safety_context_scope_mismatch(expected.clone(), permit.scope());

        assert_eq!(
            error,
            ServiceError::SafetyContextScopeMismatch {
                expected,
                actual: SafetyScope::Game(GameId::new("manual:other").expect("id")),
            }
        );
        assert!(!error.to_string().contains("token"));
    }

    #[test]
    fn game_to_game_scope_mismatch_is_not_reported_as_stale() {
        let database = tempdir().expect("database root");
        let first_root = tempdir().expect("first game root");
        let second_root = tempdir().expect("second game root");
        let context = Context::open_at(database.path().join("catalog.sqlite")).expect("context");
        let first_id = GameId::new("manual:first-scope").expect("first id");
        let second_id = GameId::new("manual:second-scope").expect("second id");
        seed_game(&context, &first_id, first_root.path());
        seed_game(&context, &second_id, second_root.path());

        let authority = FileSafetyAuthority::new();
        let first = authority
            .issue_game_assessment(&context, &first_id)
            .expect("first assessment");
        let permit = authority
            .game_permit(second_id.clone(), Some(&first.context_token))
            .expect("well-formed game token");
        let guard = crate::game_mutation_lock::blocking_lock(&second_id);
        let error = authority
            .authorize_game_commit(
                &context,
                renderpilot_domain::mutation_features::CATALOG_SWAP,
                &guard,
                &permit,
                || Ok(()),
            )
            .expect_err("wrong game scope must be rejected");

        assert!(matches!(
            error,
            ServiceError::SafetyContextScopeMismatch { .. }
        ));
    }

    #[test]
    fn game_to_global_scope_mismatch_is_rejected_during_permit_construction() {
        let game_id = GameId::new("manual:game-global").expect("id");
        let authority = FileSafetyAuthority::new();
        let shared = authority
            .issue_shared_vulkan_assessment()
            .expect("shared assessment");

        let error = authority
            .game_permit(game_id.clone(), Some(&shared.context_token))
            .expect_err("shared token cannot construct a game permit");

        assert_eq!(
            error,
            ServiceError::SafetyContextScopeMismatch {
                expected: SafetyScope::Game(game_id),
                actual: SafetyScope::SharedVulkan,
            }
        );
    }

    #[test]
    fn global_to_game_scope_mismatch_is_rejected_during_permit_construction() {
        let root = tempdir().expect("game root");
        let report = scan_anticheat(root.path());
        let game_token = game_context_token(
            &GameId::new("manual:global-game").expect("id"),
            root.path(),
            &report,
        );

        let error = FileSafetyAuthority::new()
            .shared_vulkan_permit(Some(&game_token))
            .expect_err("game token cannot construct a shared permit");

        assert!(matches!(
            error,
            ServiceError::SafetyContextScopeMismatch {
                expected: SafetyScope::SharedVulkan,
                actual: SafetyScope::Unknown,
            }
        ));
    }

    #[test]
    fn malformed_or_wrong_version_tokens_fail_closed_as_missing() {
        let game_id = GameId::new("manual:malformed").expect("id");
        for token in [
            "not-a-token",
            "v0.game.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "v1.game.not-a-hash.aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ] {
            let error = FileSafetyAuthority::new()
                .game_permit(game_id.clone(), Some(token))
                .expect_err("malformed token must fail closed");
            assert!(matches!(error, ServiceError::SafetyContextMissing { .. }));
        }
    }

    #[test]
    fn safety_gate_policy_is_closed_and_table_driven() {
        use renderpilot_domain::mutation_features::{
            CATALOG_ROLLBACK, CATALOG_SWAP, LUMA_INSTALL, LUMA_UNINSTALL, LUMA_UPDATE,
            RENODX_DLSS_FIX_INSTALL, RENODX_DLSS_FIX_UNINSTALL, RENODX_DLSS_FIX_UPDATE,
            RENODX_INSTALL, RENODX_INSTALL_FROM_FILE, RENODX_SWITCH_RESHADE_CHANNEL,
            RENODX_UNINSTALL, RENODX_UPDATE, SHARED_VULKAN_APPLY,
        };

        let game_features = [
            CATALOG_SWAP,
            LUMA_INSTALL,
            LUMA_UPDATE,
            RENODX_DLSS_FIX_INSTALL,
            RENODX_DLSS_FIX_UPDATE,
        ];
        for feature in game_features {
            assert!(
                require_game_feature(feature).is_ok(),
                "game gate must accept {feature}"
            );
            assert!(
                require_shared_vulkan_feature(feature).is_err(),
                "shared gate must reject game-only feature {feature}"
            );
        }

        let optional_shared_features = [
            RENODX_INSTALL,
            RENODX_INSTALL_FROM_FILE,
            RENODX_UPDATE,
            RENODX_SWITCH_RESHADE_CHANNEL,
        ];
        for feature in optional_shared_features {
            assert!(
                require_game_feature(feature).is_ok(),
                "game gate must accept {feature}"
            );
            assert!(
                require_shared_vulkan_feature(feature).is_ok(),
                "shared gate must accept optional-shared feature {feature}"
            );
        }

        assert!(require_shared_vulkan_feature(SHARED_VULKAN_APPLY).is_ok());
        assert!(require_game_feature(SHARED_VULKAN_APPLY).is_err());

        for feature in [
            CATALOG_ROLLBACK,
            LUMA_UNINSTALL,
            RENODX_UNINSTALL,
            RENODX_DLSS_FIX_UNINSTALL,
        ] {
            assert!(require_game_feature(feature).is_err());
            assert!(require_shared_vulkan_feature(feature).is_err());
        }
        for feature in ["future_feature", "", "shared_vulkan_apply_typo"] {
            assert!(require_game_feature(feature).is_err());
            assert!(require_shared_vulkan_feature(feature).is_err());
        }
    }

    #[test]
    fn validation_rejects_a_detector_observable_change_as_stale() {
        let database = tempdir().expect("database root");
        let game_root = tempdir().expect("game root");
        let context = Context::open_at(database.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:safety-stale").expect("game id");
        let game = GameInstallation::new(
            GameIdentity::new(game_id.clone(), "Safety Test", Launcher::Manual).expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(game_root.path().to_string_lossy().into_owned()).expect("root"),
        );
        context.storage().upsert_game(&game).expect("game");

        let authority = FileSafetyAuthority::new();
        let assessment = authority
            .issue_game_assessment(&context, &game_id)
            .expect("assessment");
        let permit = authority
            .game_permit(game_id.clone(), Some(&assessment.context_token))
            .expect("permit");
        fs::write(game_root.path().join("BattlEye"), b"marker").expect("marker");

        let guard = crate::game_mutation_lock::blocking_lock(&game_id);
        let error = authority
            .authorize_game_commit(
                &context,
                renderpilot_domain::mutation_features::CATALOG_SWAP,
                &guard,
                &permit,
                || Ok(()),
            )
            .expect_err("detector change must stale the permit");
        assert_eq!(
            error,
            ServiceError::SafetyContextStale {
                scope: SafetyScope::Game(game_id),
            }
        );
    }

    #[tokio::test]
    async fn combined_commit_rejects_missing_shared_permit_before_entering_commit() {
        let database = tempdir().expect("database root");
        let game_root = tempdir().expect("game root");
        let context = Context::open_at(database.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:combined-missing-shared").expect("game id");
        seed_game(&context, &game_id, game_root.path());

        let authority = FileSafetyAuthority::new();
        let assessment = authority
            .issue_game_assessment(&context, &game_id)
            .expect("game assessment");
        let permits = GameMutationSafetyPermits {
            game: authority
                .game_permit(game_id.clone(), Some(&assessment.context_token))
                .expect("game permit"),
            shared_vulkan: None,
        };
        let guards = crate::game_mutation_lock::enter_game_shared_mutation_boundary_async(
            &context, &game_id,
        )
        .await
        .expect("combined guards");

        let error = authority
            .authorize_game_shared_commit(
                &context,
                renderpilot_domain::mutation_features::RENODX_UPDATE,
                &guards,
                &permits,
                || -> Result<(), ServiceError> { panic!("missing shared permit entered commit") },
            )
            .expect_err("missing shared permit must reject the combined commit");

        assert_eq!(
            error,
            ServiceError::SafetyContextMissing {
                scope: SafetyScope::SharedVulkan,
            }
        );
    }

    #[tokio::test]
    async fn combined_commit_rejects_stale_shared_permit_before_entering_commit() {
        let database = tempdir().expect("database root");
        let game_root = tempdir().expect("game root");
        let context = Context::open_at(database.path().join("catalog.sqlite")).expect("context");
        let game_id = GameId::new("manual:combined-stale-shared").expect("game id");
        seed_game(&context, &game_id, game_root.path());

        let authority = FileSafetyAuthority::new();
        let game_assessment = authority
            .issue_game_assessment(&context, &game_id)
            .expect("game assessment");
        let shared_assessment = authority
            .issue_shared_vulkan_assessment()
            .expect("shared assessment");
        let mut shared_permit = authority
            .shared_vulkan_permit(Some(&shared_assessment.context_token))
            .expect("shared permit");
        shared_permit.observation_hash = "0".repeat(64);
        let permits = GameMutationSafetyPermits {
            game: authority
                .game_permit(game_id.clone(), Some(&game_assessment.context_token))
                .expect("game permit"),
            shared_vulkan: Some(shared_permit),
        };
        let guards = crate::game_mutation_lock::enter_game_shared_mutation_boundary_async(
            &context, &game_id,
        )
        .await
        .expect("combined guards");

        let error = authority
            .authorize_game_shared_commit(
                &context,
                renderpilot_domain::mutation_features::RENODX_UPDATE,
                &guards,
                &permits,
                || -> Result<(), ServiceError> { panic!("stale shared permit entered commit") },
            )
            .expect_err("stale shared permit must reject the combined commit");

        assert_eq!(
            error,
            ServiceError::SafetyContextStale {
                scope: SafetyScope::SharedVulkan,
            }
        );
    }

    fn seed_game(context: &Context, game_id: &GameId, root: &Path) {
        let game = GameInstallation::new(
            GameIdentity::new(game_id.clone(), "Safety Test", Launcher::Manual).expect("identity"),
            Platform::Windows,
            GameRuntime::NativeWindows,
            PathRef::new(root.to_string_lossy().into_owned()).expect("root"),
        );
        context.storage().upsert_game(&game).expect("game");
    }
}
