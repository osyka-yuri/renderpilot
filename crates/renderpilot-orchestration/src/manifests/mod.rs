//! Coordinated refresh of all remote CDN manifests.
//!
//! Separates two policies:
//!
//! - `ManifestRefreshPolicy::Passive` — respect per-manifest TTL / cache rules
//!   (startup, post-scan capability warm). Does not touch the force gate.
//! - `ManifestRefreshPolicy::Forced` — network-fetch every known manifest,
//!   ignoring TTL, gated by a process-local cooldown + single-flight lock so a
//!   user cannot spam the CDN via shell Refresh.
//!
//! Disk auto-scan is intentionally out of scope here.

mod gate;

use serde::Serialize;

use crate::addons::renodx;
use crate::addons::reshade::manifest_store as reshade_manifest_store;
use crate::addons::reshade::types::ReshadeConfig;
use crate::libraries;

pub use gate::{
    FORCE_MANIFEST_REFRESH_COOLDOWN, ForceRefreshGate, ForceRefreshGuard, ForceRefreshPermit,
};

/// How remote manifests should be loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestRefreshPolicy {
    /// Respect TTL / present-cache rules. Never hits the force gate.
    Passive,
    /// Network-fetch every known manifest (ignore TTL), subject to cooldown.
    Forced,
}

/// Outcome of a coordinated remote-manifest refresh.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ManifestRefreshOutcome {
    /// Passive path completed (per-kind results in [`ManifestRefreshReport::kinds`]).
    PassiveCompleted,
    /// Forced path completed a network batch (per-kind results filled).
    ForcedFetched,
    /// Forced path skipped because another Forced batch is still in flight.
    SkippedInFlight,
    /// Forced path skipped because the cooldown has not elapsed.
    SkippedCooldown {
        /// Suggested wait before the next Forced attempt may run.
        retry_after_secs: u64,
    },
}

/// Per-kind status for one coordinated refresh.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ManifestKindStatus {
    /// Kind was not attempted (e.g. Forced skipped by gate).
    #[default]
    Skipped,
    /// Kind loaded/fetched successfully.
    Ok,
    /// Kind failed; message is user-safe / log-oriented.
    Error {
        /// Brief failure description.
        message: String,
    },
}

impl ManifestKindStatus {
    fn from_result<T, E: std::fmt::Display>(result: &Result<T, E>) -> Self {
        match result {
            Ok(_) => Self::Ok,
            Err(error) => Self::Error {
                message: error.to_string(),
            },
        }
    }

    fn from_soft_option<T>(value: &Option<T>) -> Self {
        match value {
            Some(_) => Self::Ok,
            None => Self::Error {
                message: "unavailable".to_owned(),
            },
        }
    }
}

/// Per-kind results for a coordinated refresh.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
pub struct ManifestKindResults {
    /// Graphics DLL library catalogue (+ preset side downloads on force).
    pub libraries: ManifestKindStatus,
    /// RenoDX tool catalogue.
    pub renodx: ManifestKindStatus,
    /// Shared ReShade host sources.
    pub reshade: ManifestKindStatus,
}

/// Report returned by [`refresh_remote_manifests`].
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ManifestRefreshReport {
    /// High-level outcome (fetched / skipped / passive).
    pub outcome: ManifestRefreshOutcome,
    /// Per-kind detail.
    pub kinds: ManifestKindResults,
}

impl ManifestRefreshReport {
    fn skipped_all(outcome: ManifestRefreshOutcome) -> Self {
        Self {
            outcome,
            kinds: ManifestKindResults::default(),
        }
    }

    fn from_kind_fetches<E: std::fmt::Display>(
        outcome: ManifestRefreshOutcome,
        mode: &str,
        libraries: &Result<impl Sized, E>,
        renodx: &Result<impl Sized, E>,
        reshade: &Option<impl Sized>,
    ) -> Self {
        log_kind_failures(mode, libraries, renodx, reshade);
        Self {
            outcome,
            kinds: ManifestKindResults {
                libraries: ManifestKindStatus::from_result(libraries),
                renodx: ManifestKindStatus::from_result(renodx),
                reshade: ManifestKindStatus::from_soft_option(reshade),
            },
        }
    }
}

/// Process-global force gate used by [`ManifestRefreshPolicy::Forced`].
fn global_force_gate() -> &'static ForceRefreshGate {
    use std::sync::OnceLock;
    static GATE: OnceLock<ForceRefreshGate> = OnceLock::new();
    GATE.get_or_init(ForceRefreshGate::new)
}

/// Refreshes all remote CDN manifests according to `policy`.
///
/// Never returns a hard error for partial CDN failures: the report carries
/// per-kind status so callers (shell Refresh) can still proceed with disk scan.
///
/// Capability snapshot rebuild is **not** performed here — callers that need
/// badges updated should run `addons::capabilities` afterwards (or rely on the
/// post-scan Passive capability path).
pub async fn refresh_remote_manifests(policy: ManifestRefreshPolicy) -> ManifestRefreshReport {
    match policy {
        ManifestRefreshPolicy::Passive => refresh_passive().await,
        ManifestRefreshPolicy::Forced => refresh_forced(global_force_gate()).await,
    }
}

async fn refresh_passive() -> ManifestRefreshReport {
    let (libraries, renodx, reshade) = tokio::join!(
        libraries::get_or_fetch_manifest(),
        renodx::manifest_store::get_or_fetch_manifest(),
        async { reshade_manifest_store::shared_config().await },
    );

    ManifestRefreshReport::from_kind_fetches(
        ManifestRefreshOutcome::PassiveCompleted,
        "passive",
        &libraries,
        &renodx,
        &reshade,
    )
}

async fn refresh_forced(gate: &ForceRefreshGate) -> ManifestRefreshReport {
    let _guard = match gate.try_begin(FORCE_MANIFEST_REFRESH_COOLDOWN) {
        ForceRefreshPermit::Granted(guard) => guard,
        ForceRefreshPermit::SkippedInFlight => {
            log::info!("remote manifest force-refresh skipped: already in flight");
            return ManifestRefreshReport::skipped_all(ManifestRefreshOutcome::SkippedInFlight);
        }
        ForceRefreshPermit::SkippedCooldown { retry_after } => {
            let retry_after_secs = retry_after.as_secs().max(1);
            log::info!(
                "remote manifest force-refresh skipped: cooldown ({retry_after_secs}s remaining)"
            );
            return ManifestRefreshReport::skipped_all(ManifestRefreshOutcome::SkippedCooldown {
                retry_after_secs,
            });
        }
    };

    force_fetch_all_kinds().await
}

async fn force_fetch_all_kinds() -> ManifestRefreshReport {
    // ReShade first so tool force overlays can reuse the resolved config
    // without a second CDN round-trip.
    let reshade = reshade_manifest_store::fetch_shared_config().await;
    // On force-fetch failure, overlay tools from disk cache only — do not
    // re-enter get_or_fetch (which may retry the network).
    let shared_for_tools: Option<ReshadeConfig> = reshade
        .clone()
        .or_else(reshade_manifest_store::cached_shared_config);

    let (libraries, renodx) = tokio::join!(
        libraries::fetch_manifest(),
        renodx::manifest_store::fetch_manifest(shared_for_tools),
    );

    ManifestRefreshReport::from_kind_fetches(
        ManifestRefreshOutcome::ForcedFetched,
        "forced",
        &libraries,
        &renodx,
        &reshade,
    )
}

fn log_kind_failures<E: std::fmt::Display>(
    mode: &str,
    libraries: &Result<impl Sized, E>,
    renodx: &Result<impl Sized, E>,
    reshade: &Option<impl Sized>,
) {
    if let Err(error) = libraries {
        log::warn!("remote manifests ({mode}): libraries failed: {error}");
    }
    if let Err(error) = renodx {
        log::warn!("remote manifests ({mode}): renodx failed: {error}");
    }
    if reshade.is_none() {
        log::warn!("remote manifests ({mode}): reshade unavailable");
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use super::*;
    use std::time::Duration;

    #[test]
    fn skipped_report_defaults_kinds_to_skipped() {
        let report = ManifestRefreshReport::skipped_all(ManifestRefreshOutcome::SkippedInFlight);
        assert_eq!(report.kinds.libraries, ManifestKindStatus::Skipped);
        assert_eq!(report.kinds.renodx, ManifestKindStatus::Skipped);
        assert_eq!(report.kinds.reshade, ManifestKindStatus::Skipped);
    }

    #[test]
    fn kind_status_from_result_maps_ok_and_err() {
        let ok: Result<(), &str> = Ok(());
        let err: Result<(), &str> = Err("boom");
        assert_eq!(ManifestKindStatus::from_result(&ok), ManifestKindStatus::Ok);
        assert_matches!(
            ManifestKindStatus::from_result(&err),
            ManifestKindStatus::Error { message } if message.contains("boom")
        );
    }

    #[test]
    fn from_kind_fetches_builds_report_and_maps_soft_reshade() {
        let libraries: Result<(), &str> = Ok(());
        let renodx: Result<(), &str> = Err("renodx down");
        let reshade: Option<()> = None;

        let report = ManifestRefreshReport::from_kind_fetches(
            ManifestRefreshOutcome::ForcedFetched,
            "test",
            &libraries,
            &renodx,
            &reshade,
        );

        assert_eq!(report.outcome, ManifestRefreshOutcome::ForcedFetched);
        assert_eq!(report.kinds.libraries, ManifestKindStatus::Ok);
        assert_matches!(
            report.kinds.renodx,
            ManifestKindStatus::Error { message } if message.contains("renodx down")
        );
        assert_matches!(
            report.kinds.reshade,
            ManifestKindStatus::Error { message } if message == "unavailable"
        );
    }

    #[test]
    fn force_gate_cooldown_and_inflight_are_independent_of_passive() {
        let gate = ForceRefreshGate::new();
        let guard = match gate.try_begin(Duration::from_secs(90)) {
            ForceRefreshPermit::Granted(guard) => guard,
            other => panic!("expected grant, got {other:?}"),
        };
        assert_matches!(
            gate.try_begin(Duration::from_secs(90)),
            ForceRefreshPermit::SkippedInFlight
        );
        drop(guard);
        assert_matches!(
            gate.try_begin(Duration::from_secs(90)),
            ForceRefreshPermit::SkippedCooldown { .. }
        );
    }
}
