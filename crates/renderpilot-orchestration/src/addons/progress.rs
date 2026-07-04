//! Shared download-progress helpers for addon install/update flows.

use crate::net::{DownloadProgress, ProgressObserver};

/// Fixed precision used to map each sequential download onto one equal-width
/// segment of a single operation-level progress bar.
const STAGE_UNITS: u64 = 1_000_000;

/// Wraps a download-local observer so several sequential downloads contribute
/// to one monotonic operation-level progress stream.
///
/// Each stage receives equal weight because future response sizes are not known
/// before their requests start. `stage_index` is zero-based and must be less
/// than `stage_count`.
pub(crate) fn sequential_stage_observer<'a>(
    progress: Option<&'a ProgressObserver<'a>>,
    stage_index: u64,
    stage_count: u64,
) -> Option<impl Fn(DownloadProgress<'_>) + Send + Sync + 'a> {
    debug_assert!(stage_count > 0);
    debug_assert!(stage_index < stage_count);

    progress.map(move |observe| {
        move |local: DownloadProgress<'_>| {
            if local.total_bytes == 0 {
                observe(local);
                return;
            }

            let stage_progress = ((local.downloaded_bytes.min(local.total_bytes) as u128)
                * (STAGE_UNITS as u128)
                / (local.total_bytes as u128)) as u64;

            observe(DownloadProgress {
                downloaded_bytes: stage_index
                    .saturating_mul(STAGE_UNITS)
                    .saturating_add(stage_progress),
                total_bytes: stage_count.saturating_mul(STAGE_UNITS),
                phase: local.phase,
            });
        }
    })
}

/// Emits finalizing progress using the registered tool's phase i18n key.
pub(crate) fn emit_tool_finalizing(
    progress: Option<&ProgressObserver<'_>>,
    kind: renderpilot_domain::AddonKind,
) {
    let phase = crate::addons::tool::require_tool(kind).finalizing_phase();
    emit_finalizing(progress, phase);
}

/// Emits the indeterminate "finalizing" progress event so the UI can show a
/// spinner while an install or update writes files to disk and persists its
/// record — the post-download phase that otherwise leaves a 100% bar frozen
/// until the command returns. `downloaded_bytes == total_bytes == 0` signals the
/// indeterminate phase. `phase` is the tool-owned i18n key the frontend looks up
/// (e.g. `renodx.phase.finalizing`, `luma.phase.finalizing`).
pub(crate) fn emit_finalizing(progress: Option<&ProgressObserver<'_>>, phase: &'static str) {
    if let Some(observe) = progress {
        observe(DownloadProgress {
            downloaded_bytes: 0,
            total_bytes: 0,
            phase: Some(phase),
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn sequential_stages_form_one_monotonic_progress_range() {
        let observed = Mutex::new(Vec::new());
        let outer = |progress: DownloadProgress<'_>| {
            observed.lock().expect("progress lock").push((
                progress.downloaded_bytes,
                progress.total_bytes,
                progress.phase.map(str::to_owned),
            ));
        };

        for stage in 0..3 {
            let stage_observer =
                sequential_stage_observer(Some(&outer), stage, 3).expect("stage observer");
            stage_observer(DownloadProgress {
                downloaded_bytes: 0,
                total_bytes: 100,
                phase: Some("download"),
            });
            stage_observer(DownloadProgress {
                downloaded_bytes: 100,
                total_bytes: 100,
                phase: Some("download"),
            });
        }

        let observed = observed.lock().expect("progress lock").clone();
        let downloaded: Vec<u64> = observed.iter().map(|entry| entry.0).collect();
        assert_eq!(
            downloaded,
            vec![0, 1_000_000, 1_000_000, 2_000_000, 2_000_000, 3_000_000]
        );
        assert!(downloaded.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(observed.iter().all(|entry| entry.1 == 3_000_000));
        assert!(
            observed
                .iter()
                .all(|entry| entry.2.as_deref() == Some("download"))
        );
    }

    #[test]
    fn sequential_stage_preserves_indeterminate_progress() {
        let observed = Mutex::new(Vec::new());
        let outer = |progress: DownloadProgress<'_>| {
            observed.lock().expect("progress lock").push((
                progress.downloaded_bytes,
                progress.total_bytes,
                progress.phase.map(str::to_owned),
            ));
        };
        let stage_observer = sequential_stage_observer(Some(&outer), 1, 3).expect("stage observer");

        stage_observer(DownloadProgress {
            downloaded_bytes: 0,
            total_bytes: 0,
            phase: Some("waiting"),
        });

        assert_eq!(
            *observed.lock().expect("progress lock"),
            vec![(0, 0, Some("waiting".to_owned()))]
        );
    }
}
