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

use std::future::Future;

use serde::Serialize;

use crate::addons::luma;
use crate::addons::renodx;
use crate::addons::reshade::manifest_store as reshade_manifest_store;
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
    /// Kind failed. Technical detail is retained in backend logs.
    Error,
}

impl ManifestKindStatus {
    fn from_result<T, E: std::fmt::Display>(result: &Result<T, E>) -> Self {
        match result {
            Ok(_) => Self::Ok,
            Err(_) => Self::Error,
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
    /// Luma tool catalogue.
    pub luma: ManifestKindStatus,
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
    /// Whether the authoritative replacement-library catalog file changed.
    /// This coordinator-only fact is intentionally not part of the UI JSON.
    #[serde(skip)]
    pub libraries_changed: bool,
}

impl ManifestRefreshReport {
    fn skipped_all(outcome: ManifestRefreshOutcome) -> Self {
        Self {
            outcome,
            kinds: ManifestKindResults::default(),
            libraries_changed: false,
        }
    }

    fn from_kind_fetches<E: std::fmt::Display>(
        outcome: ManifestRefreshOutcome,
        libraries_changed: bool,
        mode: &str,
        libraries: &Result<impl Sized, E>,
        renodx: &Result<impl Sized, E>,
        luma: &Result<impl Sized, E>,
        reshade: &Result<impl Sized, E>,
    ) -> Self {
        log_kind_failures(mode, libraries, renodx, luma, reshade);
        Self {
            outcome,
            kinds: ManifestKindResults {
                libraries: ManifestKindStatus::from_result(libraries),
                renodx: ManifestKindStatus::from_result(renodx),
                luma: ManifestKindStatus::from_result(luma),
                reshade: ManifestKindStatus::from_result(reshade),
            },
            libraries_changed,
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
    let (libraries, renodx, luma, reshade) = join_catalog_fetches(
        libraries::get_or_fetch_catalog(),
        renodx::manifest_store::get_or_fetch_manifest(),
        luma::manifest_store::get_or_fetch_manifest(),
        reshade_manifest_store::get_or_fetch_catalog(),
    )
    .await;

    ManifestRefreshReport::from_kind_fetches(
        ManifestRefreshOutcome::PassiveCompleted,
        false,
        "passive",
        &libraries,
        &renodx,
        &luma,
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
    let (libraries, renodx, luma, reshade) = join_catalog_fetches(
        libraries::fetch_catalog(),
        renodx::manifest_store::fetch_manifest(),
        luma::manifest_store::fetch_manifest(),
        reshade_manifest_store::fetch_catalog(),
    )
    .await;

    ManifestRefreshReport::from_kind_fetches(
        ManifestRefreshOutcome::ForcedFetched,
        libraries.as_ref().copied().unwrap_or(false),
        "forced",
        &libraries,
        &renodx,
        &luma,
        &reshade,
    )
}

async fn join_catalog_fetches<L, R, U, S, E>(
    libraries: impl Future<Output = Result<L, E>>,
    renodx: impl Future<Output = Result<R, E>>,
    luma: impl Future<Output = Result<U, E>>,
    reshade: impl Future<Output = Result<S, E>>,
) -> (Result<L, E>, Result<R, E>, Result<U, E>, Result<S, E>) {
    tokio::join!(libraries, renodx, luma, reshade)
}

fn log_kind_failures<E: std::fmt::Display>(
    mode: &str,
    libraries: &Result<impl Sized, E>,
    renodx: &Result<impl Sized, E>,
    luma: &Result<impl Sized, E>,
    reshade: &Result<impl Sized, E>,
) {
    if let Err(error) = libraries {
        log::warn!("remote manifests ({mode}): libraries failed: {error}");
    }
    if let Err(error) = renodx {
        log::warn!("remote manifests ({mode}): renodx failed: {error}");
    }
    if let Err(error) = luma {
        log::warn!("remote manifests ({mode}): luma failed: {error}");
    }
    if let Err(error) = reshade {
        log::warn!("remote manifests ({mode}): reshade failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use std::time::Duration;

    #[test]
    fn skipped_report_defaults_kinds_to_skipped() {
        let report = ManifestRefreshReport::skipped_all(ManifestRefreshOutcome::SkippedInFlight);
        assert_eq!(report.kinds.libraries, ManifestKindStatus::Skipped);
        assert_eq!(report.kinds.renodx, ManifestKindStatus::Skipped);
        assert_eq!(report.kinds.luma, ManifestKindStatus::Skipped);
        assert_eq!(report.kinds.reshade, ManifestKindStatus::Skipped);
    }

    #[test]
    fn kind_status_from_result_maps_ok_and_err() {
        let ok: Result<(), &str> = Ok(());
        let err: Result<(), &str> = Err("boom");
        assert_eq!(ManifestKindStatus::from_result(&ok), ManifestKindStatus::Ok);
        assert_eq!(
            ManifestKindStatus::from_result(&err),
            ManifestKindStatus::Error
        );
    }

    #[test]
    fn error_status_serializes_without_backend_prose() {
        let value = serde_json::to_value(ManifestKindStatus::Error)
            .expect("serialize manifest error status");

        assert_eq!(value, serde_json::json!({ "status": "error" }));
        assert!(value.get("message").is_none());
    }

    #[test]
    fn from_kind_fetches_builds_report_and_maps_reshade_error() {
        let libraries: Result<(), &str> = Ok(());
        let renodx: Result<(), &str> = Err("renodx down");
        let luma: Result<(), &str> = Ok(());
        let reshade: Result<(), &str> = Err("reshade down");

        let report = ManifestRefreshReport::from_kind_fetches(
            ManifestRefreshOutcome::ForcedFetched,
            true,
            "test",
            &libraries,
            &renodx,
            &luma,
            &reshade,
        );

        assert_eq!(report.outcome, ManifestRefreshOutcome::ForcedFetched);
        assert!(report.libraries_changed);
        assert_eq!(report.kinds.libraries, ManifestKindStatus::Ok);
        assert_eq!(report.kinds.renodx, ManifestKindStatus::Error);
        assert_eq!(report.kinds.luma, ManifestKindStatus::Ok);
        assert_eq!(report.kinds.reshade, ManifestKindStatus::Error);
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

    #[tokio::test]
    async fn coordinated_batch_resolves_the_shared_reshade_catalog_once() {
        let reshade_calls = AtomicUsize::new(0);

        let (_, _, _, reshade) = join_catalog_fetches(
            async { Ok::<_, &'static str>(()) },
            async { Ok::<_, &'static str>(()) },
            async { Ok::<_, &'static str>(()) },
            async {
                reshade_calls.fetch_add(1, Ordering::Relaxed);
                Ok::<_, &'static str>(())
            },
        )
        .await;

        assert!(reshade.is_ok());
        assert_eq!(reshade_calls.load(Ordering::Relaxed), 1);
    }
}
