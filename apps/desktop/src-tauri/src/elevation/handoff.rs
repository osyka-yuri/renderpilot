//! Short-lived elevation handoff marker (file, not argv).
//!
//! Written before `ShellExecuteW(runas)` so a failed or still-unelevated child
//! does not open another UAC prompt during the TTL window. Cleared on elevated
//! startup and on cancel/policy failure.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long a pending elevation handoff is treated as "already attempted".
///
/// Dual role (single TTL, no second marker format):
/// - **Anti-loop**: after a failed or still-unelevated child, suppress another
///   *automatic* UAC prompt for a short window.
/// - **Grace**: cover elevated-process cold start (UAC consent, AV scan, disk).
///
/// Elevated startups still clear the marker immediately on success; UAC cancel
/// and policy failure clear it immediately too. 180s balances AV/disk delay
/// against re-prompt latency after a true failed elevate.
///
/// During the suppress window the in-app **Relaunch as administrator** button
/// still works: [`super::ElevationRelaunchTrigger::UserRequest`] never consults
/// this TTL for suppression.
const ELEVATION_HANDOFF_TTL: Duration = Duration::from_secs(180);

const ELEVATION_HANDOFF_FILENAME: &str = "elevation-handoff";
const ELEVATION_HANDOFF_TMP_FILENAME: &str = "elevation-handoff.tmp";

/// Clears any elevation handoff marker. Safe to call from elevated startups so
/// a subsequent NSIS restart is not treated as a failed handoff.
pub fn clear_elevation_handoff_marker() {
    let _ = std::fs::remove_file(elevation_handoff_path());
    let _ = std::fs::remove_file(elevation_handoff_tmp_path());
}

/// Returns `true` if a recent elevation handoff is still pending — meaning a
/// prior process already tried to elevate and auto-relaunch should not open
/// another UAC prompt.
///
/// Uses a short-lived file (not argv) so the Tauri NSIS updater's `/ARGS`
/// restart after install cannot permanently suppress auto-elevation.
pub(super) fn has_recent_elevation_handoff() -> bool {
    let Some(written_at) = handoff_written_at_secs() else {
        return false;
    };

    let now = unix_now_secs();
    // Future timestamps (clock skew / corrupt-but-numeric payload) must not
    // stick forever via saturating_sub → age 0.
    if written_at > now {
        clear_elevation_handoff_marker();
        return false;
    }

    let age_secs = now - written_at;
    if age_secs > ELEVATION_HANDOFF_TTL.as_secs() {
        clear_elevation_handoff_marker();
        return false;
    }

    true
}

pub(super) fn write_elevation_handoff_marker() {
    let path = elevation_handoff_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let payload = unix_now_secs().to_string();
    let tmp = elevation_handoff_tmp_path();

    // Write then rename so a crash mid-write cannot leave a truncated marker
    // that fails to parse (or worse, a partial future-looking value). Same
    // volume as the final path under the app data root.
    if let Err(error) = std::fs::write(&tmp, &payload) {
        log::warn!("Failed to write elevation handoff temp marker at {tmp:?}: {error}");
        return;
    }
    if let Err(error) = std::fs::rename(&tmp, &path) {
        let _ = std::fs::remove_file(&tmp);
        if let Err(fallback) = std::fs::write(&path, payload) {
            log::warn!(
                "Failed to publish elevation handoff marker at {path:?} (rename: {error}; write: {fallback})"
            );
        }
    }
}

fn elevation_handoff_path() -> PathBuf {
    elevation_handoff_dir().join(ELEVATION_HANDOFF_FILENAME)
}

fn elevation_handoff_tmp_path() -> PathBuf {
    elevation_handoff_dir().join(ELEVATION_HANDOFF_TMP_FILENAME)
}

/// Directory for the handoff marker (test builds may override the root).
fn elevation_handoff_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(override_dir) = test_handoff_dir_override() {
        return override_dir;
    }

    super::renderpilot_local_data_dir()
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Reads a valid handoff timestamp. Invalid or unreadable markers are deleted.
fn handoff_written_at_secs() -> Option<u64> {
    let path = elevation_handoff_path();
    let bytes = std::fs::read(&path).ok()?;
    match parse_handoff_timestamp(&bytes) {
        Some(written_at) => Some(written_at),
        None => {
            let _ = std::fs::remove_file(&path);
            None
        }
    }
}

fn parse_handoff_timestamp(bytes: &[u8]) -> Option<u64> {
    let text = std::str::from_utf8(bytes).ok()?;
    text.trim().parse::<u64>().ok()
}

#[cfg(test)]
fn test_handoff_dir_override() -> Option<PathBuf> {
    HANDOFF_DIR_OVERRIDE.lock().ok().and_then(|g| g.clone())
}

#[cfg(test)]
static HANDOFF_DIR_OVERRIDE: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that share the process-wide handoff override / file.
    static HANDOFF_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_handoff_tests() -> std::sync::MutexGuard<'static, ()> {
        HANDOFF_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    struct HandoffTestEnv {
        dir: PathBuf,
    }

    impl HandoffTestEnv {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "renderpilot-elevation-handoff-tests-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("test handoff dir");
            *HANDOFF_DIR_OVERRIDE.lock().expect("override lock") = Some(dir.clone());
            Self { dir }
        }

        fn path(&self) -> PathBuf {
            self.dir.join(ELEVATION_HANDOFF_FILENAME)
        }

        fn write_contents(&self, contents: &str) {
            std::fs::write(self.path(), contents).expect("write handoff contents");
        }
    }

    impl Drop for HandoffTestEnv {
        fn drop(&mut self) {
            *HANDOFF_DIR_OVERRIDE.lock().expect("override lock") = None;
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn handoff_path_uses_override_dir_in_tests() {
        let _guard = lock_handoff_tests();
        let env = HandoffTestEnv::new();
        assert_eq!(elevation_handoff_path(), env.path());
    }

    #[test]
    fn stale_handoff_marker_is_not_treated_as_attempted() {
        let _guard = lock_handoff_tests();
        let env = HandoffTestEnv::new();

        let stale_secs = unix_now_secs().saturating_sub(ELEVATION_HANDOFF_TTL.as_secs() + 30);
        env.write_contents(&stale_secs.to_string());

        assert!(
            !has_recent_elevation_handoff(),
            "TTL-expired handoff must not suppress auto-elevation"
        );
        assert!(
            !env.path().exists(),
            "stale marker should be cleaned up on inspection"
        );
    }

    #[test]
    fn recent_handoff_marker_is_detected() {
        let _guard = lock_handoff_tests();
        let _env = HandoffTestEnv::new();

        write_elevation_handoff_marker();
        assert!(has_recent_elevation_handoff());
        clear_elevation_handoff_marker();
        assert!(!has_recent_elevation_handoff());
    }

    #[test]
    fn corrupt_handoff_marker_is_ignored() {
        let _guard = lock_handoff_tests();
        let env = HandoffTestEnv::new();

        env.write_contents("not-a-timestamp");
        assert!(!has_recent_elevation_handoff());
        assert!(!env.path().exists());
    }

    #[test]
    fn future_handoff_timestamp_is_cleared() {
        let _guard = lock_handoff_tests();
        let env = HandoffTestEnv::new();

        let future_secs = unix_now_secs().saturating_add(3600);
        env.write_contents(&future_secs.to_string());

        assert!(
            !has_recent_elevation_handoff(),
            "future handoff timestamp must not suppress auto-elevation"
        );
        assert!(
            !env.path().exists(),
            "future marker should be cleaned up on inspection"
        );
    }

    #[test]
    fn parse_handoff_timestamp_accepts_trimmed_digits() {
        assert_eq!(parse_handoff_timestamp(b"  42\n"), Some(42));
        assert_eq!(parse_handoff_timestamp(b"not-a-timestamp"), None);
        assert_eq!(parse_handoff_timestamp(&[0xff, 0xfe]), None);
    }

    #[test]
    fn startup_auto_skips_when_recent_handoff_is_live() {
        let _guard = lock_handoff_tests();
        let _env = HandoffTestEnv::new();

        write_elevation_handoff_marker();
        assert_eq!(
            super::super::attempt_self_relaunch_elevated(
                super::super::ElevationRelaunchTrigger::StartupAuto
            ),
            super::super::ElevationStartupDecision::SkippedRecentHandoff
        );
        // Marker must remain so a concurrent unelevated instance still sees the handoff.
        assert!(has_recent_elevation_handoff());
        clear_elevation_handoff_marker();
    }

    #[test]
    fn atomic_write_publishes_final_marker_without_leaving_tmp() {
        let _guard = lock_handoff_tests();
        let env = HandoffTestEnv::new();

        write_elevation_handoff_marker();
        assert!(env.path().exists());
        assert!(
            !env.dir.join(ELEVATION_HANDOFF_TMP_FILENAME).exists(),
            "temp handoff file must be renamed away"
        );
        assert!(has_recent_elevation_handoff());
        clear_elevation_handoff_marker();
    }

    #[test]
    fn user_request_is_not_suppressed_by_live_handoff() {
        let _guard = lock_handoff_tests();
        let _env = HandoffTestEnv::new();

        write_elevation_handoff_marker();
        // Policy only — do not call ShellExecute in unit tests.
        assert!(!super::super::relaunch::should_suppress_auto_elevation(
            super::super::ElevationRelaunchTrigger::UserRequest,
            true
        ));
        clear_elevation_handoff_marker();
    }
}
