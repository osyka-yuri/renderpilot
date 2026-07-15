use super::*;
use std::path::Path;

#[test]
fn normalized_key_is_case_insensitive_and_forward_slash() {
    assert_eq!(
        normalized_key(Path::new(r"C:\Games\DLSS.dll")),
        "c:/games/dlss.dll"
    );
    assert_eq!(
        normalized_key(Path::new("C:/Games/DLSS.dll")),
        normalized_key(Path::new(r"c:\games\dlss.dll"))
    );
}

#[test]
fn is_within_accepts_self_and_descendants_only() {
    let root = Path::new(r"C:\Games");
    assert!(is_within(Path::new(r"C:\Games"), root));
    assert!(is_within(Path::new(r"C:\Games\sub\file.dll"), root));
    assert!(!is_within(Path::new(r"C:\GamesOther\file.dll"), root));
    assert!(!is_within(Path::new(r"D:\Games\file.dll"), root));
}

#[test]
fn is_within_handles_drive_root_scope() {
    assert!(is_within(Path::new("D:/foo"), Path::new("D:/")));
    assert!(is_within(Path::new("D:/"), Path::new("D:/")));
    assert!(!is_within(Path::new("E:/foo"), Path::new("D:/")));
}

#[test]
fn canonical_candidate_walks_up_to_existing_ancestor() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("missing_dir").join("nvngx_dlss.dll");
    assert!(!target.exists());

    let resolved = canonical_candidate(&target).expect("walks up");
    let canonical_root = std::fs::canonicalize(dir.path()).expect("canonicalize tempdir");
    assert!(is_within(&resolved, &canonical_root));
    assert_eq!(resolved.file_name().unwrap(), "nvngx_dlss.dll");
}

#[test]
fn same_path_is_case_insensitive_when_targets_are_missing() {
    assert!(same_path(
        Path::new(r"C:\Games\Missing\nvngx_dlss.dll"),
        Path::new("c:/games/missing/NVNGX_DLSS.DLL"),
    ));
    assert!(!same_path(
        Path::new(r"C:\Games\Missing\a.dll"),
        Path::new(r"C:\Games\Missing\b.dll"),
    ));
}
