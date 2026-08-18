use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static FAIL_AFTER_ABSENT_RESERVATION: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
static FAIL_RESERVATION_FLUSH: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
static DRIFT_AFTER_ABSENT_RESERVATION: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
static CORRUPT_NEXT_PREIMAGE_SNAPSHOT: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
static FAIL_RESTORE_SNAPSHOT: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();

pub(crate) fn fail_next_absent_publish_for_test(path: &Path) {
    set_test_hook(&FAIL_AFTER_ABSENT_RESERVATION, path);
}

pub(crate) fn fail_next_reservation_flush_for_test(path: &Path) {
    set_test_hook(&FAIL_RESERVATION_FLUSH, path);
}

pub(crate) fn drift_next_absent_reservation_for_test(path: &Path) {
    set_test_hook(&DRIFT_AFTER_ABSENT_RESERVATION, path);
}

pub(crate) fn corrupt_next_preimage_snapshot_for_test(path: &Path) {
    set_test_hook(&CORRUPT_NEXT_PREIMAGE_SNAPSHOT, path);
}

pub(crate) fn fail_next_restore_snapshot_for_test(path: &Path) {
    set_test_hook(&FAIL_RESTORE_SNAPSHOT, path);
}

pub(super) fn take_fail_after_absent_reservation(path: &Path) -> bool {
    take_test_hook(&FAIL_AFTER_ABSENT_RESERVATION, path)
}

pub(super) fn take_fail_reservation_flush(path: &Path) -> bool {
    take_test_hook(&FAIL_RESERVATION_FLUSH, path)
}

pub(super) fn take_drift_after_absent_reservation(path: &Path) -> bool {
    take_test_hook(&DRIFT_AFTER_ABSENT_RESERVATION, path)
}

pub(super) fn take_corrupt_next_preimage_snapshot(path: &Path) -> bool {
    take_test_hook(&CORRUPT_NEXT_PREIMAGE_SNAPSHOT, path)
}

pub(super) fn take_fail_restore_snapshot(path: &Path) -> bool {
    take_test_hook(&FAIL_RESTORE_SNAPSHOT, path)
}

fn set_test_hook(hook: &OnceLock<Mutex<HashSet<PathBuf>>>, path: &Path) {
    hook.get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .expect("v2 test hook lock poisoned")
        .insert(path.to_path_buf());
}

fn take_test_hook(hook: &OnceLock<Mutex<HashSet<PathBuf>>>, path: &Path) -> bool {
    hook.get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .expect("v2 test hook lock poisoned")
        .remove(path)
}
