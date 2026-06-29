//! Shared download-progress helpers for the RenoDX install/update flows.

use crate::net::{DownloadProgress, ProgressObserver};

/// Progress phase emitted after every download has finished, while the flow
/// finalizes on disk (laying down files, fsyncing, persisting/refreshing the
/// record). `downloaded_bytes == total_bytes == 0` signals an indeterminate phase
/// to the UI, so the bar shows a spinner + this label instead of a stuck 100% bar.
const FINALIZING_PHASE: &str = "renodx.phase.finalizing";

/// Emits the indeterminate "finalizing" progress event so the UI can show a
/// spinner while an install or update writes files to disk and persists its
/// record — the post-download phase that otherwise leaves a 100% bar frozen until
/// the command returns.
pub(super) fn emit_finalizing(progress: Option<&ProgressObserver<'_>>) {
    if let Some(observe) = progress {
        observe(DownloadProgress {
            downloaded_bytes: 0,
            total_bytes: 0,
            phase: Some(FINALIZING_PHASE),
        });
    }
}
