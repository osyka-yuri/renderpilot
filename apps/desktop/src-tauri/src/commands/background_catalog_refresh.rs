//! Testable startup catalog coordination and Tauri event publication.

use std::future::Future;
use std::sync::Arc;
use std::{collections::BTreeSet, fmt};

use crate::{
    backend_diagnostics,
    diagnostic_event::{
        BackendDiagnosticEvent, CatalogRefreshPhase as DiagnosticCatalogRefreshPhase,
        CoverGcOperation, EventPublicationOperation,
    },
};
use futures_util::future::join;
use renderpilot_api::{self as desktop, AutoScanOutput, ValidatedCatalogRefreshOutput};
use renderpilot_orchestration::Context;
use serde::Serialize;
use tauri::Emitter;

use super::addon_catalog;

type PhaseResult<T> = Result<T, String>;

#[derive(Debug, Clone, Copy, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
enum CatalogDeltaReason {
    Scan,
    RemoteCatalog,
    Capabilities,
    LiveFacts,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogDelta {
    revision: u64,
    reasons: Vec<CatalogDeltaReason>,
    changed_game_ids: Vec<String>,
    removed_game_ids: Vec<String>,
}

#[derive(Default)]
struct CatalogDeltaBuilder {
    reasons: BTreeSet<CatalogDeltaReason>,
    changed_game_ids: BTreeSet<String>,
    removed_game_ids: BTreeSet<String>,
}

impl CatalogDeltaBuilder {
    fn record_scan(&mut self, scan: AutoScanOutput) {
        if !scan.changed_game_ids.is_empty() || !scan.removed_game_ids.is_empty() {
            self.reasons.insert(CatalogDeltaReason::Scan);
        }
        self.changed_game_ids.extend(scan.changed_game_ids);
        self.removed_game_ids.extend(scan.removed_game_ids);
    }

    fn record_reason(&mut self, reason: CatalogDeltaReason) {
        self.reasons.insert(reason);
    }

    fn record_live_changes(&mut self, game_ids: Vec<String>) {
        if game_ids.is_empty() {
            return;
        }
        self.reasons.insert(CatalogDeltaReason::LiveFacts);
        self.changed_game_ids.extend(game_ids);
    }

    fn has_changes(&self) -> bool {
        !self.reasons.is_empty()
    }

    fn finish(mut self, revision: u64) -> Option<CatalogDelta> {
        if self.reasons.is_empty() {
            return None;
        }
        self.changed_game_ids
            .retain(|game_id| !self.removed_game_ids.contains(game_id));

        Some(CatalogDelta {
            revision,
            reasons: self.reasons.into_iter().collect(),
            changed_game_ids: self.changed_game_ids.into_iter().collect(),
            removed_game_ids: self.removed_game_ids.into_iter().collect(),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CatalogRefreshPhase {
    Scan,
    RemoteCatalog,
    Capabilities,
    LiveValidation,
    Revision,
}

impl fmt::Display for CatalogRefreshPhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Scan => "auto-scan",
            Self::RemoteCatalog => "library catalog refresh",
            Self::Capabilities => "catalog capability refresh",
            Self::LiveValidation => "live catalog validation",
            Self::Revision => "durable catalog rebuild",
        })
    }
}

impl CatalogRefreshPhase {
    const fn diagnostic_phase(self) -> DiagnosticCatalogRefreshPhase {
        match self {
            Self::Scan => DiagnosticCatalogRefreshPhase::Scan,
            Self::RemoteCatalog => DiagnosticCatalogRefreshPhase::RemoteCatalog,
            Self::Capabilities => DiagnosticCatalogRefreshPhase::Capabilities,
            Self::LiveValidation => DiagnosticCatalogRefreshPhase::LiveValidation,
            Self::Revision => DiagnosticCatalogRefreshPhase::Revision,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct CatalogRefreshIssue {
    phase: CatalogRefreshPhase,
    message: String,
}

impl CatalogRefreshIssue {
    fn new(phase: CatalogRefreshPhase, message: impl Into<String>) -> Self {
        Self {
            phase,
            message: message.into(),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct CatalogRefreshOutcome {
    delta: Option<CatalogDelta>,
    issues: Vec<CatalogRefreshIssue>,
    partial_failure_count: usize,
}

trait CatalogRefreshPhases {
    async fn scan(&self) -> PhaseResult<AutoScanOutput>;
    async fn refresh_remote_catalog(&self) -> PhaseResult<bool>;
    async fn refresh_capabilities(&self) -> PhaseResult<bool>;
    fn invalidate_snapshot(&self);
    async fn validate_live_catalog(&self) -> PhaseResult<ValidatedCatalogRefreshOutput>;
    async fn refresh_catalog_revision(&self) -> PhaseResult<u64>;
}

struct DesktopCatalogRefreshPhases {
    context: Arc<Context>,
}

impl CatalogRefreshPhases for DesktopCatalogRefreshPhases {
    async fn scan(&self) -> PhaseResult<AutoScanOutput> {
        let context = Arc::clone(&self.context);
        tauri::async_runtime::spawn_blocking(move || {
            desktop::scan_auto_libraries_background_output(&context)
        })
        .await
        .map_err(|error| format!("worker failed: {error}"))?
        .map_err(|error| error.to_string())
    }

    async fn refresh_remote_catalog(&self) -> PhaseResult<bool> {
        desktop::fetch_libraries_catalog_output()
            .await
            .map_err(|error| error.to_string())
    }

    async fn refresh_capabilities(&self) -> PhaseResult<bool> {
        addon_catalog::refresh_catalog_addon_capabilities(Arc::clone(&self.context)).await
    }

    fn invalidate_snapshot(&self) {
        self.context.invalidate_catalog_snapshot();
    }

    async fn validate_live_catalog(&self) -> PhaseResult<ValidatedCatalogRefreshOutput> {
        let context = Arc::clone(&self.context);
        tauri::async_runtime::spawn_blocking(move || {
            desktop::refresh_validated_catalog_snapshot(&context)
        })
        .await
        .map_err(|error| format!("worker failed: {error}"))?
        .map_err(|error| error.to_string())
    }

    async fn refresh_catalog_revision(&self) -> PhaseResult<u64> {
        let context = Arc::clone(&self.context);
        tauri::async_runtime::spawn_blocking(move || {
            desktop::refresh_catalog_snapshot_revision(&context)
        })
        .await
        .map_err(|error| format!("worker failed: {error}"))?
        .map_err(|error| error.to_string())
    }
}

async fn coordinate_catalog_refresh(phases: &impl CatalogRefreshPhases) -> CatalogRefreshOutcome {
    let mut delta = CatalogDeltaBuilder::default();
    let mut issues = Vec::new();
    let mut partial_failure_count = 0;

    let (scan, remote_catalog) = join(phases.scan(), phases.refresh_remote_catalog()).await;
    match scan {
        Ok(scan) => {
            partial_failure_count = scan.partial_failure_count;
            delta.record_scan(scan);
        }
        Err(error) => issues.push(CatalogRefreshIssue::new(CatalogRefreshPhase::Scan, error)),
    }
    match remote_catalog {
        Ok(true) => delta.record_reason(CatalogDeltaReason::RemoteCatalog),
        Ok(false) => {}
        Err(error) => issues.push(CatalogRefreshIssue::new(
            CatalogRefreshPhase::RemoteCatalog,
            error,
        )),
    }

    match phases.refresh_capabilities().await {
        Ok(true) => delta.record_reason(CatalogDeltaReason::Capabilities),
        Ok(false) => {}
        Err(error) => issues.push(CatalogRefreshIssue::new(
            CatalogRefreshPhase::Capabilities,
            error,
        )),
    }
    if delta.has_changes() {
        phases.invalidate_snapshot();
    }

    // Live facts are mandatory even when durable sources report no changes:
    // DLL/EXE/.bak files can be replaced outside RenderPilot.
    let revision = match phases.validate_live_catalog().await {
        Ok(validation) => {
            delta.record_live_changes(validation.changed_game_ids);
            Some(validation.catalog_revision)
        }
        Err(error) => {
            issues.push(CatalogRefreshIssue::new(
                CatalogRefreshPhase::LiveValidation,
                error,
            ));
            None
        }
    };

    if !delta.has_changes() {
        return CatalogRefreshOutcome {
            delta: None,
            issues,
            partial_failure_count,
        };
    }

    let revision = match revision {
        Some(revision) => revision,
        None => match phases.refresh_catalog_revision().await {
            Ok(revision) => revision,
            Err(error) => {
                issues.push(CatalogRefreshIssue::new(
                    CatalogRefreshPhase::Revision,
                    error,
                ));
                return CatalogRefreshOutcome {
                    delta: None,
                    issues,
                    partial_failure_count,
                };
            }
        },
    };

    CatalogRefreshOutcome {
        delta: delta.finish(revision),
        issues,
        partial_failure_count,
    }
}

trait CatalogRefreshEventSink {
    fn emit_delta(&self, delta: CatalogDelta) -> Result<(), String>;
    fn emit_ready(&self) -> Result<(), String>;
}

struct TauriCatalogRefreshEventSink<'app> {
    app: &'app tauri::AppHandle,
}

impl CatalogRefreshEventSink for TauriCatalogRefreshEventSink<'_> {
    fn emit_delta(&self, delta: CatalogDelta) -> Result<(), String> {
        self.app
            .emit("catalog://delta", delta)
            .map_err(|error| error.to_string())
    }

    fn emit_ready(&self) -> Result<(), String> {
        self.app
            .emit("catalog://sync-state", "ready")
            .map_err(|error| error.to_string())
    }
}

fn publish_catalog_refresh(outcome: CatalogRefreshOutcome, sink: &impl CatalogRefreshEventSink) {
    for issue in outcome.issues {
        log::warn!("background {} issue: {}", issue.phase, issue.message);
        backend_diagnostics::record(BackendDiagnosticEvent::catalog_issue(
            issue.phase.diagnostic_phase(),
        ));
    }
    if let Some(delta) = outcome.delta
        && let Err(error) = sink.emit_delta(delta)
    {
        log::warn!("failed to publish catalog delta: {error}");
        backend_diagnostics::record(BackendDiagnosticEvent::event_publication_failure(
            EventPublicationOperation::CatalogDelta,
        ));
    }
    if let Err(error) = sink.emit_ready() {
        log::warn!("failed to publish catalog sync state: {error}");
        backend_diagnostics::record(BackendDiagnosticEvent::event_publication_failure(
            EventPublicationOperation::CatalogSyncState,
        ));
    }
}

async fn run(context: Arc<Context>, app: &tauri::AppHandle) -> usize {
    let phases = DesktopCatalogRefreshPhases { context };
    let outcome = coordinate_catalog_refresh(&phases).await;
    let partial_failure_count = outcome.partial_failure_count;
    publish_catalog_refresh(outcome, &TauriCatalogRefreshEventSink { app });
    partial_failure_count
}

#[derive(Debug, Eq, PartialEq)]
struct StartupExecution {
    started: bool,
    partial_failure_count: usize,
    cover_gc_error: Option<String>,
}

pub(super) struct BackgroundRefreshStart {
    pub(super) started: bool,
    pub(super) partial_failure_count: usize,
}

async fn execute_startup<Claim, CoverGc, CoverGcFuture, Refresh, RefreshFuture>(
    claim: Claim,
    cover_gc: CoverGc,
    refresh: Refresh,
) -> StartupExecution
where
    Claim: FnOnce() -> bool,
    CoverGc: FnOnce() -> CoverGcFuture,
    CoverGcFuture: Future<Output = Result<(), String>>,
    Refresh: FnOnce() -> RefreshFuture,
    RefreshFuture: Future<Output = usize>,
{
    if !claim() {
        return StartupExecution {
            started: false,
            partial_failure_count: 0,
            cover_gc_error: None,
        };
    }

    let cover_gc_error = cover_gc().await.err();
    let partial_failure_count = refresh().await;
    StartupExecution {
        started: true,
        partial_failure_count,
        cover_gc_error,
    }
}

pub(super) async fn start(context: Arc<Context>, app: tauri::AppHandle) -> BackgroundRefreshStart {
    let claim_context = Arc::clone(&context);
    let cover_context = Arc::clone(&context);
    let refresh_context = Arc::clone(&context);
    let execution = execute_startup(
        move || claim_context.begin_background_refresh(),
        move || async move {
            tauri::async_runtime::spawn_blocking(move || {
                desktop::try_gc_cover_orphans_on_startup(&cover_context)
            })
            .await
            .map_err(|error| format!("worker failed: {error}"))?
            .map_err(|error| error.to_string())
        },
        move || async move { run(refresh_context, &app).await },
    )
    .await;

    if let Some(error) = execution.cover_gc_error {
        log::warn!("background cover GC failed: {error}");
        backend_diagnostics::record(BackendDiagnosticEvent::cover_gc_failure(
            CoverGcOperation::StartupCoverGc,
        ));
    }
    BackgroundRefreshStart {
        started: execution.started,
        partial_failure_count: execution.partial_failure_count,
    }
}

#[cfg(test)]
mod tests {
    use std::future::poll_fn;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, PoisonError};
    use std::task::Poll;

    use super::{
        CatalogDelta, CatalogDeltaBuilder, CatalogDeltaReason, CatalogRefreshEventSink,
        CatalogRefreshOutcome, CatalogRefreshPhases, StartupExecution, coordinate_catalog_refresh,
        execute_startup, publish_catalog_refresh,
    };
    use renderpilot_api::{AutoScanOutput, ValidatedCatalogRefreshOutput};

    fn scan(changed: &[&str], removed: &[&str]) -> AutoScanOutput {
        AutoScanOutput {
            added_game_ids: Vec::new(),
            updated_game_ids: Vec::new(),
            changed_game_ids: changed.iter().map(|id| (*id).to_owned()).collect(),
            removed_game_ids: removed.iter().map(|id| (*id).to_owned()).collect(),
            partial_failure_count: 0,
        }
    }

    #[test]
    fn empty_builder_produces_no_delta() {
        assert_eq!(CatalogDeltaBuilder::default().finish(1), None);
    }

    #[test]
    fn delta_is_sorted_deduplicated_and_removed_wins() {
        let mut builder = CatalogDeltaBuilder::default();
        builder.record_scan(scan(&["changed", "removed", "changed"], &["removed"]));
        builder.record_reason(CatalogDeltaReason::Capabilities);
        builder.record_reason(CatalogDeltaReason::Capabilities);
        builder.record_live_changes(vec!["live".to_owned(), "changed".to_owned()]);

        let delta = builder.finish(7).expect("delta");

        assert_eq!(delta.revision, 7);
        assert_eq!(
            delta.reasons,
            vec![
                CatalogDeltaReason::Scan,
                CatalogDeltaReason::Capabilities,
                CatalogDeltaReason::LiveFacts,
            ]
        );
        assert_eq!(delta.changed_game_ids, vec!["changed", "live"]);
        assert_eq!(delta.removed_game_ids, vec!["removed"]);
    }

    struct MockPhases {
        failure_mask: u8,
        reports_changes: bool,
        prove_parallel_start: bool,
        remote_started: AtomicBool,
        invalidations: AtomicUsize,
        events: Mutex<Vec<&'static str>>,
        partial_failure_count: usize,
    }

    impl MockPhases {
        fn new(failure_mask: u8, reports_changes: bool, prove_parallel_start: bool) -> Self {
            Self {
                failure_mask,
                reports_changes,
                prove_parallel_start,
                remote_started: AtomicBool::new(false),
                invalidations: AtomicUsize::new(0),
                events: Mutex::new(Vec::new()),
                partial_failure_count: 0,
            }
        }

        fn with_partial_failures(mut self, count: usize) -> Self {
            self.partial_failure_count = count;
            self
        }

        fn fails(&self, bit: u8) -> bool {
            self.failure_mask & (1 << bit) != 0
        }

        fn record(&self, event: &'static str) {
            self.events
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(event);
        }

        fn recorded_events(&self) -> Vec<&'static str> {
            self.events
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    impl CatalogRefreshPhases for MockPhases {
        async fn scan(&self) -> Result<AutoScanOutput, String> {
            self.record("scan:start");
            if self.prove_parallel_start {
                poll_fn(|context| {
                    if self.remote_started.load(Ordering::Acquire) {
                        Poll::Ready(())
                    } else {
                        context.waker().wake_by_ref();
                        Poll::Pending
                    }
                })
                .await;
            }
            self.record("scan:end");
            if self.fails(0) {
                Err("scan failed".to_owned())
            } else if self.reports_changes {
                let mut output = scan(&["scan"], &[]);
                output.partial_failure_count = self.partial_failure_count;
                Ok(output)
            } else {
                let mut output = scan(&[], &[]);
                output.partial_failure_count = self.partial_failure_count;
                Ok(output)
            }
        }

        async fn refresh_remote_catalog(&self) -> Result<bool, String> {
            self.record("remote:start");
            self.remote_started.store(true, Ordering::Release);
            self.record("remote:end");
            if self.fails(1) {
                Err("remote failed".to_owned())
            } else {
                Ok(self.reports_changes)
            }
        }

        async fn refresh_capabilities(&self) -> Result<bool, String> {
            self.record("capabilities");
            if self.fails(2) {
                Err("capabilities failed".to_owned())
            } else {
                Ok(self.reports_changes)
            }
        }

        fn invalidate_snapshot(&self) {
            self.invalidations.fetch_add(1, Ordering::Relaxed);
            self.record("invalidate");
        }

        async fn validate_live_catalog(&self) -> Result<ValidatedCatalogRefreshOutput, String> {
            self.record("validation");
            if self.fails(3) {
                Err("validation failed".to_owned())
            } else {
                Ok(ValidatedCatalogRefreshOutput {
                    catalog_revision: 7,
                    changed_game_ids: if self.reports_changes {
                        vec!["live".to_owned()]
                    } else {
                        Vec::new()
                    },
                })
            }
        }

        async fn refresh_catalog_revision(&self) -> Result<u64, String> {
            self.record("revision");
            if self.fails(4) {
                Err("revision failed".to_owned())
            } else {
                Ok(8)
            }
        }
    }

    fn index_of(events: &[&str], expected: &str) -> usize {
        events
            .iter()
            .position(|event| *event == expected)
            .unwrap_or_else(|| panic!("missing event {expected}: {events:?}"))
    }

    #[test]
    fn scan_and_remote_start_together_before_capabilities_and_validation() {
        let phases = MockPhases::new(0, true, true);
        let outcome = tauri::async_runtime::block_on(coordinate_catalog_refresh(&phases));
        let events = phases.recorded_events();

        assert!(outcome.delta.is_some());
        assert!(index_of(&events, "scan:start") < index_of(&events, "remote:start"));
        assert!(index_of(&events, "remote:start") < index_of(&events, "scan:end"));
        assert!(index_of(&events, "scan:end") < index_of(&events, "capabilities"));
        assert!(index_of(&events, "remote:end") < index_of(&events, "capabilities"));
        assert!(index_of(&events, "capabilities") < index_of(&events, "validation"));
    }

    #[test]
    fn unchanged_phases_publish_no_delta_and_still_validate() {
        let phases = MockPhases::new(0, false, false);
        let outcome = tauri::async_runtime::block_on(coordinate_catalog_refresh(&phases));

        assert_eq!(
            outcome,
            CatalogRefreshOutcome {
                delta: None,
                issues: Vec::new(),
                partial_failure_count: 0,
            }
        );
        assert_eq!(phases.invalidations.load(Ordering::Relaxed), 0);
        assert!(phases.recorded_events().contains(&"validation"));
    }

    #[test]
    fn per_root_scan_failures_are_not_logged_again_by_the_coordinator() {
        let phases = MockPhases::new(0, false, false).with_partial_failures(2);
        let outcome = tauri::async_runtime::block_on(coordinate_catalog_refresh(&phases));

        assert!(outcome.issues.is_empty());
        assert!(outcome.delta.is_none());
        assert_eq!(outcome.partial_failure_count, 2);
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum PublishedEvent {
        Delta(CatalogDelta),
        Ready,
    }

    #[derive(Default)]
    struct RecordingSink {
        events: Mutex<Vec<PublishedEvent>>,
    }

    impl CatalogRefreshEventSink for RecordingSink {
        fn emit_delta(&self, delta: CatalogDelta) -> Result<(), String> {
            self.events
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(PublishedEvent::Delta(delta));
            Ok(())
        }

        fn emit_ready(&self) -> Result<(), String> {
            self.events
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(PublishedEvent::Ready);
            Ok(())
        }
    }

    #[test]
    fn ready_is_published_for_every_phase_failure_combination() {
        for failure_mask in 0..32 {
            let phases = MockPhases::new(failure_mask, true, false);
            let outcome = tauri::async_runtime::block_on(coordinate_catalog_refresh(&phases));
            let sink = RecordingSink::default();
            publish_catalog_refresh(outcome, &sink);

            let events = sink.events.lock().unwrap_or_else(PoisonError::into_inner);
            assert_eq!(
                events.last(),
                Some(&PublishedEvent::Ready),
                "failure mask {failure_mask:05b}"
            );
            assert!(
                events
                    .iter()
                    .filter(|event| matches!(event, PublishedEvent::Delta(_)))
                    .count()
                    <= 1
            );
        }
    }

    #[test]
    fn one_shot_and_cover_gc_order_are_explicit() {
        let skipped = tauri::async_runtime::block_on(execute_startup(
            || false,
            || async { panic!("cover GC must not run") },
            || async { panic!("refresh must not run") },
        ));
        assert_eq!(
            skipped,
            StartupExecution {
                started: false,
                partial_failure_count: 0,
                cover_gc_error: None,
            }
        );

        let events = Arc::new(Mutex::new(Vec::new()));
        let gc_events = Arc::clone(&events);
        let refresh_events = Arc::clone(&events);
        let started = tauri::async_runtime::block_on(execute_startup(
            || true,
            move || async move {
                gc_events
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push("cover-gc");
                Err("gc failed".to_owned())
            },
            move || async move {
                refresh_events
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push("refresh");
                3
            },
        ));

        assert_eq!(
            started,
            StartupExecution {
                started: true,
                partial_failure_count: 3,
                cover_gc_error: Some("gc failed".to_owned()),
            }
        );
        assert_eq!(
            *events.lock().unwrap_or_else(PoisonError::into_inner),
            vec!["cover-gc", "refresh"]
        );
    }
}
