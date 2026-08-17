use std::{fs, fs::FileTimes, time::SystemTime};

use renderpilot_domain::Sha256Hash;

use super::{
    FileObservationResult, FileObservationSource, SystemFileObservationSource, UsnJournalBounds,
    WindowsCacheKeyMaterial, WindowsLeaseIdentity, WindowsLeaseState, WindowsObservation,
    finish_windows_observation, strong_cache_keys_for_window, usn_output_value, usn_record_value,
};

fn sha256_fixture() -> Sha256Hash {
    Sha256Hash::new("0".repeat(64)).expect("fixture hash")
}

fn cache_material(usn: i64) -> Option<WindowsCacheKeyMaterial> {
    Some(WindowsCacheKeyMaterial {
        identity: WindowsLeaseIdentity {
            volume_serial: 7,
            file_id: [3; 16],
        },
        usn,
        size: 4,
    })
}

#[test]
fn missing_reusable_identity_keeps_a_safe_held_read_available_without_a_cache_key() {
    let before = WindowsLeaseState { size: 4 };
    let after = WindowsLeaseState { size: 4 };
    let reopened = WindowsLeaseState { size: 4 };
    let result = finish_windows_observation(WindowsObservation {
        before: &before,
        after: &after,
        reopened: &reopened,
        keys: [None, None, None],
        bytes: b"data".to_vec(),
        sha256: sha256_fixture(),
    });

    assert!(matches!(
        result,
        FileObservationResult::Available(snapshot) if snapshot.cache_key.is_none()
    ));
}

#[test]
fn one_usn_window_validates_all_lease_identity_samples() {
    let before = UsnJournalBounds {
        id: 11,
        first: 10,
        next: 100,
    };
    let after = UsnJournalBounds {
        id: 11,
        first: 20,
        next: 120,
    };

    let keys = strong_cache_keys_for_window(
        before,
        after,
        [cache_material(40), cache_material(40), cache_material(40)],
    );

    assert!(keys.iter().all(Option::is_some));
    assert_eq!(keys[0], keys[1]);
    assert_eq!(keys[0], keys[2]);
}

#[test]
fn journal_discontinuity_or_out_of_window_usn_disables_reuse() {
    let before = UsnJournalBounds {
        id: 11,
        first: 10,
        next: 100,
    };
    let changed_generation = UsnJournalBounds {
        id: 12,
        first: 10,
        next: 100,
    };
    let shifted_bounds = UsnJournalBounds {
        id: 11,
        first: 50,
        next: 120,
    };

    assert_eq!(
        strong_cache_keys_for_window(
            before,
            changed_generation,
            [cache_material(40), cache_material(40), cache_material(40)],
        ),
        [None, None, None]
    );
    assert_eq!(
        strong_cache_keys_for_window(
            before,
            shifted_bounds,
            [cache_material(40), cache_material(40), cache_material(40)],
        ),
        [None, None, None]
    );
}

#[test]
fn usn_record_value_accepts_complete_v2_and_v3_records() {
    let mut v2 = vec![0_u8; 32];
    v2[0..4].copy_from_slice(&(32_u32).to_le_bytes());
    v2[4..6].copy_from_slice(&(2_u16).to_le_bytes());
    v2[24..32].copy_from_slice(&(17_i64).to_le_bytes());
    assert_eq!(usn_record_value(&v2), Some(17));

    let mut v3 = vec![0_u8; 48];
    v3[0..4].copy_from_slice(&(48_u32).to_le_bytes());
    v3[4..6].copy_from_slice(&(3_u16).to_le_bytes());
    v3[40..48].copy_from_slice(&(31_i64).to_le_bytes());
    assert_eq!(usn_record_value(&v3), Some(31));
}

#[test]
fn usn_record_value_rejects_declared_records_that_end_before_the_usn() {
    let mut truncated_v2 = vec![0_u8; 40];
    truncated_v2[0..4].copy_from_slice(&(31_u32).to_le_bytes());
    truncated_v2[4..6].copy_from_slice(&(2_u16).to_le_bytes());
    truncated_v2[24..32].copy_from_slice(&(17_i64).to_le_bytes());
    assert_eq!(usn_record_value(&truncated_v2), None);

    let mut truncated_v3 = vec![0_u8; 48];
    truncated_v3[0..4].copy_from_slice(&(32_u32).to_le_bytes());
    truncated_v3[4..6].copy_from_slice(&(3_u16).to_le_bytes());
    truncated_v3[40..48].copy_from_slice(&(31_i64).to_le_bytes());
    assert_eq!(usn_record_value(&truncated_v3), None);
}

fn valid_v2_output() -> Vec<u8> {
    let mut output = vec![0_u8; 128];
    output[0..4].copy_from_slice(&(32_u32).to_le_bytes());
    output[4..6].copy_from_slice(&(2_u16).to_le_bytes());
    output[24..32].copy_from_slice(&(17_i64).to_le_bytes());
    output
}

#[test]
fn usn_output_value_handles_zero_and_exact_return_counts_without_panicking() {
    let output = valid_v2_output();

    assert_eq!(usn_output_value(true, &output, 0), None);
    assert_eq!(
        usn_output_value(true, &output, u32::try_from(output.len()).expect("length")),
        Some(17)
    );
}

#[test]
fn usn_output_value_rejects_a_return_count_larger_than_the_driver_buffer() {
    let output = valid_v2_output();
    let overlong = u32::try_from(output.len() + 1).expect("length");

    assert_eq!(usn_output_value(true, &output, overlong), None);
}

#[test]
fn failed_usn_read_never_parses_returned_bytes() {
    let output = valid_v2_output();

    assert_eq!(usn_output_value(false, &output, 32), None);
}

#[test]
fn same_size_replacement_with_restored_mtime_never_reuses_the_old_key() {
    let directory = tempfile::tempdir().expect("temp directory");
    let path = directory.path().join("nvngx_dlss.dll");
    fs::write(&path, b"AAAA").expect("initial bytes");
    let original_mtime = fs::metadata(&path)
        .expect("metadata")
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let source = SystemFileObservationSource;
    let FileObservationResult::Available(first) = source.observe(&path).expect("first observe")
    else {
        panic!("fixture must be readable");
    };

    fs::write(&path, b"BBBB").expect("same-size replacement");
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open for timestamp restore")
        .set_times(FileTimes::new().set_modified(original_mtime))
        .expect("restore mtime");
    let FileObservationResult::Available(second) = source.observe(&path).expect("second observe")
    else {
        panic!("replacement must be readable");
    };

    assert_ne!(first.sha256, second.sha256);
    // Unsupported/remote/no-journal filesystems are intentionally
    // full-read-only and persist no reusable observation.
    if let (Some(first), Some(second)) = (first.cache_key, second.cache_key) {
        assert_ne!(first, second);
    }
}
