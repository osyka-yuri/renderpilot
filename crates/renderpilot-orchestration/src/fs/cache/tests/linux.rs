//! Linux preservation and publication cases.

use super::*;

#[cfg(target_os = "linux")]
#[test]
fn linux_absent_path_publishes_candidate() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("manifest.json");
    let observation = observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes))
        .expect("observe absent cache");

    let result = commit_cache_candidate(
        &path,
        observation.generation(),
        b"candidate-c",
        MatchingCurrentPolicy::RefreshCandidate,
        parse_doc,
    )
    .expect("publish absent cache candidate");

    assert!(matches!(result, CachePublication::Published));
    assert_eq!(
        fs::read(&path).expect("read published candidate"),
        b"candidate-c"
    );
    assert!(
        corrupt_diagnostic_paths(&path).is_empty(),
        "publication of an absent path creates no corrupt-cache diagnostics"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_occupied_path_is_preserved_without_validation_or_diagnostics() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "occupied-a");
    let observation = observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes))
        .expect("observe occupied cache A");
    let metadata = fs::metadata(&path).expect("snapshot occupied cache A metadata");
    let mut validation_count = 0_usize;

    let result = commit_cache_candidate(
        &path,
        observation.generation(),
        b"candidate-c",
        MatchingCurrentPolicy::RefreshCandidate,
        |_bytes| {
            validation_count += 1;
            Ok(())
        },
    )
    .expect("Linux retains a present cache path without classifying it");

    assert!(matches!(result, CachePublication::PreservedUnclassified));
    assert_eq!(
        validation_count, 0,
        "Linux must not classify an occupied pathname"
    );
    assert_eq!(
        fs::read(&path).expect("read retained cache A"),
        b"occupied-a"
    );
    let retained = fs::metadata(&path).expect("read retained cache A metadata");
    assert_eq!(retained.len(), metadata.len());
    assert_eq!(
        retained.modified().expect("read retained cache A mtime"),
        metadata.modified().expect("read occupied cache A mtime"),
        "an unpublished commit leaves the occupied cache metadata untouched"
    );
    assert!(
        corrupt_diagnostic_paths(&path).is_empty(),
        "an occupied cache path creates no diagnostics or quarantine artifacts"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_late_successor_is_preserved_and_byte_metadata_stable() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "snapshot-a");
    let observation = observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes))
        .expect("observe valid snapshot A");
    let generation = match observation {
        CacheObservation::Valid { generation, .. } => generation,
        _ => panic!("fixture must observe a valid current cache"),
    };
    let successor = b"successor-b-with-distinct-length";
    let candidate = b"candidate-c";
    let hook = inject_linux_cache_conflict_test_hook(
        LinuxCacheConflictTestHook::InstallSuccessorAfterSnapshotProof(successor.to_vec()),
    );

    cache_linux_conflict_test_after_snapshot_proof(&path)
        .expect("noncooperating late writer installs successor B");
    drop(hook);
    let successor_metadata = fs::metadata(&path).expect("snapshot successor B metadata");
    let mut validation_count = 0_usize;

    let result = commit_cache_candidate(
        &path,
        &generation,
        candidate,
        MatchingCurrentPolicy::RefreshCandidate,
        |_bytes| {
            validation_count += 1;
            Ok(())
        },
    )
    .expect("Linux leaves late successor B outside this transaction");

    assert!(matches!(result, CachePublication::PreservedUnclassified));
    assert_eq!(
        validation_count, 0,
        "late successor B is never classified as current"
    );
    let metadata = fs::metadata(&path).expect("inspect retained successor metadata");
    assert_eq!(
        fs::read(&path).expect("read late valid successor"),
        successor,
        "candidate C never overwrites successor B installed after snapshot proof"
    );
    assert_eq!(
        metadata.len(),
        successor.len() as u64,
        "the surviving destination metadata belongs to successor B, not candidate C"
    );
    assert_ne!(metadata.len(), candidate.len() as u64);
    assert_eq!(
        metadata.modified().expect("read retained successor mtime"),
        successor_metadata
            .modified()
            .expect("read successor B snapshot mtime"),
        "the cache commit does not rewrite late successor B"
    );
    assert!(
        corrupt_diagnostic_paths(&path).is_empty(),
        "late successor B does not trigger a quarantine or diagnostic"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_invalid_occupied_path_is_preserved_without_diagnostics() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "bad");
    let generation = read_cache_file_locked(&path)
        .expect("read exact invalid snapshot A")
        .expect("invalid snapshot remains present")
        .generation;
    let candidate = b"candidate-c";
    let metadata = fs::metadata(&path).expect("snapshot invalid occupant metadata");
    let mut validation_count = 0_usize;

    let result = commit_cache_candidate(
        &path,
        &generation,
        candidate,
        MatchingCurrentPolicy::RefreshCandidate,
        |_bytes| {
            validation_count += 1;
            Ok(())
        },
    )
    .expect("Linux retains an invalid present occupant without classifying it");

    assert!(matches!(result, CachePublication::PreservedUnclassified));
    assert_eq!(
        validation_count, 0,
        "invalid occupants are not classified during commit"
    );
    assert_eq!(
        fs::read(&path).expect("read retained invalid occupant"),
        b"bad"
    );
    let retained = fs::metadata(&path).expect("inspect retained invalid occupant");
    assert_eq!(retained.len(), metadata.len());
    assert_eq!(
        retained
            .modified()
            .expect("read retained invalid occupant mtime"),
        metadata
            .modified()
            .expect("read invalid occupant snapshot mtime"),
        "the invalid occupied pathname is neither replaced nor touched"
    );
    assert!(
        corrupt_diagnostic_paths(&path).is_empty(),
        "commit never creates diagnostics or quarantine artifacts for an occupied path"
    );
    let prohibited_dead_code_allowance = concat!("cfg_attr(not(windows), ", "allow(dead_code))");
    assert!(
        !include_str!("../../cache.rs").contains(prohibited_dead_code_allowance),
        "Linux retention stays structural rather than relying on a non-Windows dead-code allowance"
    );
}
