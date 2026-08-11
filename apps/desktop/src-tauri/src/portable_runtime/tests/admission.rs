use std::fs;

use super::{error_code, hash, temp_root};
use crate::portable_runtime::{epoch_namespace::establish_epoch, process_admission::AdmissionLock};

#[test]
fn admission_is_single_owner_and_uses_the_fixed_lock_leaf() {
    let root = temp_root("admission-owner");
    let authority = root.path().join("authority");
    establish_epoch(&authority, &hash('a')).expect("create a valid epoch namespace");

    let first = AdmissionLock::acquire(&authority).expect("first supervisor owns admission");
    assert_eq!(
        error_code(AdmissionLock::acquire(&authority)),
        "portable_runtime_io",
        "a second supervisor must not acquire the retained share-zero lock"
    );
    drop(first);
    AdmissionLock::acquire(&authority).expect("authority is released only at owner teardown");

    let source = include_str!("../process_admission.rs");
    assert!(source.contains("authority_root.join(\"admission.lock\")"));
    assert!(!source.contains("pub fn release"));
}

#[test]
fn namespace_rejects_invalid_epochs_collisions_and_unknown_preserved_leaves() {
    let root = temp_root("namespace");
    let authority = root.path().join("authority");
    let provenance = authority.join("provenance");
    fs::create_dir_all(&provenance).expect("create portable provenance namespace");

    assert_eq!(
        error_code(establish_epoch(&authority, "not-an-epoch")),
        "portable_epoch_invalid"
    );

    let epoch = hash('b');
    let first = establish_epoch(&authority, &epoch).expect("publish the exact epoch once");
    assert!(first.is_dir());
    assert!(provenance.is_dir(), "known portable provenance is retained");
    assert_eq!(
        error_code(establish_epoch(&authority, &epoch)),
        "portable_epoch_collision"
    );

    let foreign = authority.join("foreign-state");
    fs::write(&foreign, b"do not repair or delete foreign state").expect("write foreign leaf");
    assert_eq!(
        error_code(establish_epoch(&authority, &hash('c'))),
        "portable_namespace_unknown"
    );
    assert_eq!(
        fs::read(&foreign).expect("foreign leaf is retained"),
        b"do not repair or delete foreign state"
    );
}

#[test]
fn namespace_directory_leaves_and_scan_publication_contract_fail_closed() {
    let root = temp_root("namespace-directory");
    let authority = root.path().join("authority");
    fs::create_dir_all(authority.join("epochs")).expect("create authority namespace");
    let foreign = authority.join("foreign-directory");
    fs::create_dir(&foreign).expect("create foreign directory");

    assert_eq!(
        error_code(establish_epoch(&authority, &hash('d'))),
        "portable_namespace_unknown"
    );
    assert!(
        foreign.is_dir(),
        "unknown directory is preserved for recovery"
    );

    let source = include_str!("../epoch_namespace.rs");
    let before = source
        .find("let before = stable_scan(&epochs)?;")
        .expect("Scan A");
    let publish = source
        .find("std::fs::create_dir(&epoch_root)?;")
        .expect("publication");
    let after = source
        .find("let after = stable_scan(&epochs)?;")
        .expect("Scan B");
    let delta = source
        .find("expect_exact_addition(&before, &after")
        .expect("exact delta");
    assert!(before < publish && publish < after && after < delta);
}

#[test]
fn namespace_win32_classifier_pins_untrusted_leaf_shapes_without_repair() {
    let source = include_str!("../win32/directory.rs");
    for required in [
        "FILE_FLAG_OPEN_REPARSE_POINT",
        "FILE_ATTRIBUTE_REPARSE_POINT",
        "String::from_utf16(raw_name)",
        "nNumberOfLinks != 1",
        "portable_namespace_unopenable",
        "raw namespace scan A/B differed",
    ] {
        assert!(
            source.contains(required),
            "missing fail-closed classifier contract: {required}"
        );
    }
    assert!(!source.contains("remove_file"));
    assert!(!source.contains("remove_dir"));
}
