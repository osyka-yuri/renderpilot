use std::fs;

use super::{error_code, hash, temp_root};
use crate::portable_runtime::{
    epoch_namespace::establish_epoch,
    process_admission::AdmissionLock,
    root_authority::{PortableRootAuthority, SupervisorRootBinding},
    supervisor::authority::SupervisorSessionAuthority,
};

#[test]
fn admission_requires_a_supervisor_root_binding_without_a_joined_lock_path() {
    let source = include_str!("../process_admission.rs");
    assert!(source.contains("pub fn acquire(binding: &SupervisorRootBinding) -> Result<Self>"));
    assert!(source.contains("acquire_supervisor_admission(binding)?"));
    assert!(!source.contains("join(\"admission.lock\")"));
    assert!(!source.contains("pub fn release"));

    let object_source = include_str!("../win32/object/admission.rs");
    assert!(object_source.contains("RelativeFileOpen::ExclusiveOpenOrCreateReadDataAndAttributes"));
    assert!(!object_source.contains("GENERIC_READ"));
    assert!(!object_source.contains("GENERIC_WRITE"));
}

#[test]
fn retained_binding_excludes_a_second_supervisor_and_releases_on_drop() {
    let root = temp_root("admission-owner");
    let root_authority = PortableRootAuthority::open(root.path()).expect("retained root");
    let binding = SupervisorRootBinding::bind(
        SupervisorSessionAuthority::for_root_test(root_authority.identity()),
        root_authority.clone(),
    )
    .expect("matching root binding");

    let first = AdmissionLock::acquire(&binding).expect("first supervisor owns admission");
    assert!(
        AdmissionLock::acquire(&binding).is_err(),
        "a second supervisor cannot acquire the retained share-zero lock"
    );
    drop(first);
    AdmissionLock::acquire(&binding).expect("admission becomes available only after drop");

    assert!(
        SupervisorRootBinding::bind(SupervisorSessionAuthority::for_test('c'), root_authority)
            .is_err(),
        "a mismatched protocol root identity cannot bind an App-capable root"
    );
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
