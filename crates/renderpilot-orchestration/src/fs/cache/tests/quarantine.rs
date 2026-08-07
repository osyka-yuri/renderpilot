//! Corrupt-cache diagnostic and quarantine cases.

use super::*;

fn observe_invalid_with_fault(
    fault: crate::fs::atomic::NoReplaceTestFault,
    occupied_base_slot: bool,
) -> (tempfile::TempDir, PathBuf, CacheGeneration) {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "bad");
    if occupied_base_slot {
        fs::write(corrupt_sidecar(&path, None), b"existing diagnostic")
            .expect("seed occupied diagnostic slot");
    }
    let expected_generation = read_cache_file_locked(&path)
        .expect("capture invalid cache generation")
        .expect("invalid cache is present")
        .generation;
    let fault_guard = crate::fs::atomic::inject_no_replace_test_fault(fault);
    let result = observe_cache_file(&path, |_, _| Err::<(), _>(crate::failed("invalid doc")));
    drop(fault_guard);

    assert!(
        result.is_err(),
        "diagnostic publication fault {fault:?} must fail the observation"
    );
    assert_eq!(
        fs::read(&path).expect("read retained invalid cache"),
        b"bad"
    );
    assert_eq!(
        read_cache_file_locked(&path)
            .expect("re-read retained invalid cache")
            .expect("retained invalid cache is present")
            .generation,
        expected_generation,
        "a failed diagnostic publication cannot overwrite the active invalid generation"
    );
    (dir, path, expected_generation)
}

#[test]
fn invalid_cache_propagates_prepublication_faults_without_diagnostics() {
    for fault in [
        crate::fs::atomic::NoReplaceTestFault::Create,
        crate::fs::atomic::NoReplaceTestFault::Write,
        crate::fs::atomic::NoReplaceTestFault::Sync,
        crate::fs::atomic::NoReplaceTestFault::Publish,
    ] {
        let (dir, path, _) = observe_invalid_with_fault(fault, false);
        assert!(
            corrupt_diagnostic_paths(&path).is_empty(),
            "fault {fault:?} cannot leave a final partial diagnostic"
        );
        assert!(
            owned_publication_temp_paths(&path).is_empty(),
            "fault {fault:?} cleans its owned publication temporary file"
        );
        drop(dir);
    }
}

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
#[test]
fn invalid_cache_propagates_ambiguous_inspection_failure() {
    let (dir, path, _) =
        observe_invalid_with_fault(crate::fs::atomic::NoReplaceTestFault::Inspect, true);
    assert_eq!(
        fs::read(corrupt_sidecar(&path, None)).expect("read immutable occupied diagnostic"),
        b"existing diagnostic"
    );
    assert!(
        !corrupt_sidecar(&path, Some(1)).exists(),
        "an ambiguous occupied publication cannot advance to another diagnostic slot"
    );
    assert!(owned_publication_temp_paths(&path).is_empty());
    drop(dir);
}

#[cfg(not(target_os = "linux"))]
#[test]
fn invalid_cache_propagates_cleanup_failure_and_preserves_only_owned_residue() {
    let (dir, path, _) =
        observe_invalid_with_fault(crate::fs::atomic::NoReplaceTestFault::Cleanup, true);
    assert_eq!(
        fs::read(corrupt_sidecar(&path, None)).expect("read immutable occupied diagnostic"),
        b"existing diagnostic"
    );
    assert!(
        !corrupt_sidecar(&path, Some(1)).exists(),
        "cleanup failure cannot accept a later diagnostic slot"
    );
    let residue = owned_publication_temp_paths(&path);
    assert_eq!(
        residue.len(),
        1,
        "only the owned diagnostic temporary remains"
    );
    fs::remove_file(&residue[0]).expect("remove only the test-owned diagnostic residue");
    assert!(owned_publication_temp_paths(&path).is_empty());
    drop(dir);
}

#[test]
fn occupied_diagnostics_remain_invalid_and_allow_bounded_candidate_cas() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "bad");
    let immutable = (0..MAX_CORRUPT_DIAGNOSTICS)
        .map(|slot| {
            let bytes = format!("immutable diagnostic {slot}").into_bytes();
            let diagnostic = corrupt_sidecar(
                &path,
                (slot != 0).then_some(u32::try_from(slot).expect("small slot")),
            );
            fs::write(diagnostic, &bytes).expect("seed immutable diagnostic");
            bytes
        })
        .collect::<Vec<_>>();

    let observation = observe_cache_file(&path, |bytes, _| parse_doc(bytes))
        .expect("full immutable diagnostic set retains an invalid observation");
    let generation = match observation {
        CacheObservation::Invalid { generation, .. } => generation,
        _ => panic!("full diagnostic set must retain the invalid cache for CAS repair"),
    };
    #[cfg(target_os = "linux")]
    let occupied_metadata = fs::metadata(&path).expect("snapshot invalid active cache metadata");

    #[cfg(windows)]
    {
        let result = commit_cache_candidate(
            &path,
            &generation,
            b"fetched",
            MatchingCurrentPolicy::RefreshCandidate,
            parse_doc,
        )
        .expect("bounded CAS repairs retained invalid cache");

        assert!(matches!(result, CachePublication::Published));
        assert_eq!(fs::read(&path).expect("read fetched cache"), b"fetched");
    }
    #[cfg(all(
        not(any(windows, target_os = "linux")),
        feature = "development-host-fallback"
    ))]
    {
        let result = commit_cache_candidate(
            &path,
            &generation,
            b"fetched",
            MatchingCurrentPolicy::RefreshCandidate,
            parse_doc,
        )
        .expect("bounded CAS repairs retained invalid cache");

        assert!(matches!(result, CachePublication::Published));
        assert_eq!(fs::read(&path).expect("read fetched cache"), b"fetched");
    }
    #[cfg(target_os = "linux")]
    {
        let result = commit_cache_candidate(
            &path,
            &generation,
            b"fetched",
            MatchingCurrentPolicy::RefreshCandidate,
            parse_doc,
        )
        .expect("Linux reports a present invalid occupant as unpublished");

        assert!(matches!(result, CachePublication::PreservedUnclassified));
        assert_eq!(
            fs::read(&path).expect("read retained invalid cache"),
            b"bad",
            "the fetched candidate never overwrites the live invalid occupant"
        );
        let retained_metadata = fs::metadata(&path).expect("inspect retained invalid metadata");
        assert_eq!(retained_metadata.len(), occupied_metadata.len());
        assert_eq!(
            retained_metadata
                .modified()
                .expect("read retained invalid cache mtime"),
            occupied_metadata
                .modified()
                .expect("read invalid cache snapshot mtime"),
            "Linux does not touch the retained active occupant"
        );
    }
    for (slot, expected) in immutable.into_iter().enumerate() {
        assert_eq!(
            fs::read(corrupt_sidecar(
                &path,
                (slot != 0).then_some(u32::try_from(slot).expect("small slot")),
            ))
            .expect("read immutable diagnostic"),
            expected,
            "occupied diagnostic slot {slot} remains byte-identical"
        );
    }
}

#[test]
fn quarantine_preserves_the_bad_file_in_a_diagnostic_slot() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "bad");
    quarantine_at(&path);
    #[cfg(windows)]
    assert!(
        !path.exists(),
        "Windows exact retirement moves the corrupt cache atomically"
    );
    #[cfg(not(windows))]
    assert_eq!(
        fs::read_to_string(&path).expect("read active cache"),
        "bad",
        "non-Windows cleanup retains the active cache for refresh"
    );
    assert_eq!(
        fs::read_to_string(
            crate::fs::with_added_extension(&path, "corrupt").expect("cache has a file name")
        )
        .expect("read diagnostic"),
        "bad"
    );
}

#[test]
fn repeated_quarantine_keeps_each_diagnostic_without_replacing_an_existing_slot() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "newly-corrupt");
    let quarantined =
        crate::fs::with_added_extension(&path, "corrupt").expect("cache has a file name");
    fs::write(&quarantined, "first-corrupt").expect("write prior diagnostic");

    quarantine_at(&path);

    assert_eq!(
        fs::read_to_string(&quarantined).expect("read prior diagnostic"),
        "first-corrupt"
    );
    let repeated =
        crate::fs::with_added_extension(&quarantined, "1").expect("quarantine has a file name");
    assert_eq!(
        fs::read_to_string(&repeated).expect("read repeated diagnostic"),
        "newly-corrupt"
    );
    #[cfg(windows)]
    assert!(
        !path.exists(),
        "Windows exact retirement moves the active cache into slot 1"
    );
    #[cfg(not(windows))]
    assert_eq!(
        fs::read_to_string(&path).expect("read active cache"),
        "newly-corrupt",
        "non-Windows cleanup retains the cache for the fetch commit point"
    );
}

#[test]
fn quarantine_slots_are_bounded_and_immutable() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("manifest.json");

    for attempt in 0..MAX_CORRUPT_DIAGNOSTICS {
        let contents = format!("corrupt attempt {attempt}");
        fs::write(&path, &contents).expect("write corrupt cache");
        quarantine_at(&path);
        #[cfg(windows)]
        assert!(
            !path.exists(),
            "Windows exact retirement clears the active cache into an available slot"
        );
        #[cfg(not(windows))]
        assert_eq!(
            fs::read_to_string(&path).expect("read active cache"),
            contents,
            "non-Windows cleanup retains the active cache"
        );
    }

    let diagnostics = corrupt_diagnostic_paths(&path);
    assert_eq!(diagnostics.len(), MAX_CORRUPT_DIAGNOSTICS);
    for (attempt, diagnostic) in diagnostics.iter().enumerate() {
        assert_eq!(
            fs::read_to_string(diagnostic).expect("read diagnostic"),
            format!("corrupt attempt {attempt}")
        );
    }

    let before = diagnostics
        .iter()
        .map(|diagnostic| fs::read(diagnostic).expect("snapshot diagnostic"))
        .collect::<Vec<_>>();
    fs::write(&path, "latest corruption").expect("write latest corruption");
    quarantine_at(&path);

    assert_eq!(
        fs::read_to_string(&path).expect("read active corruption"),
        "latest corruption",
        "a full diagnostic set leaves the active cache for atomic refresh"
    );
    for (diagnostic, expected) in diagnostics.iter().zip(before) {
        assert_eq!(
            fs::read(diagnostic).expect("read immutable diagnostic"),
            expected,
            "an occupied diagnostic slot is never replaced or deleted"
        );
    }
}

#[test]
fn quarantine_preserves_legacy_sidecars_without_growing_them() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("manifest.json");
    for suffix in std::iter::once(None).chain((1..=6).map(Some)) {
        let diagnostic = corrupt_sidecar(&path, suffix);
        fs::write(&diagnostic, format!("legacy {suffix:?}")).expect("write legacy diagnostic");
    }
    let legacy = fs::read_dir(dir.path())
        .expect("list legacy sidecars")
        .map(|entry| {
            let path = entry.expect("read legacy entry").path();
            let bytes = fs::read(&path).expect("snapshot legacy entry");
            (path, bytes)
        })
        .collect::<Vec<_>>();

    fs::write(&path, "current corruption").expect("write current corrupt cache");
    quarantine_at(&path);

    assert_eq!(
        fs::read_to_string(&path).expect("read active cache"),
        "current corruption"
    );
    for (diagnostic, expected) in legacy {
        assert_eq!(
            fs::read(&diagnostic).expect("read preserved legacy diagnostic"),
            expected,
            "legacy diagnostics are never deleted based on a reusable pathname"
        );
    }
    assert!(!corrupt_sidecar(&path, Some(7)).exists());
}

#[test]
fn quarantine_preserves_foreign_and_unsafe_siblings() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("manifest.json");
    let base = corrupt_sidecar(&path, None);
    fs::write(&base, "prior base corruption").expect("write prior base diagnostic");

    let unsafe_directory = corrupt_sidecar(&path, Some(1));
    fs::create_dir(&unsafe_directory).expect("create unsafe directory");
    let second_unsafe_directory = corrupt_sidecar(&path, Some(2));
    fs::create_dir(&second_unsafe_directory).expect("create second unsafe directory");
    let foreign_sibling = dir.path().join("manifest.json.corrupt.notes");
    fs::write(&foreign_sibling, "foreign sibling").expect("write foreign sibling");

    fs::write(&path, "current corruption").expect("write current corrupt cache");
    quarantine_at(&path);

    assert!(unsafe_directory.is_dir(), "directories are never removed");
    assert!(
        second_unsafe_directory.is_dir(),
        "every non-file directory entry occupies its slot"
    );
    assert_eq!(
        fs::read_to_string(&foreign_sibling).expect("read foreign sibling"),
        "foreign sibling"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("read active cache"),
        "current corruption",
        "occupied unsafe slots are not bypassed or removed"
    );
}

#[test]
fn concurrent_quarantine_never_replaces_an_existing_slot() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "shared corruption");
    let barrier = Arc::new(Barrier::new(9));
    let workers = (0..8)
        .map(|_| {
            let path = path.clone();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                quarantine_at(&path);
            })
        })
        .collect::<Vec<_>>();

    barrier.wait();
    for worker in workers {
        worker.join().expect("quarantine worker");
    }

    let diagnostics = corrupt_diagnostic_paths(&path);
    #[cfg(windows)]
    assert_eq!(
        diagnostics.len(),
        1,
        "only one Windows exact retirement can claim the active cache"
    );
    #[cfg(not(windows))]
    assert_eq!(diagnostics.len(), MAX_CORRUPT_DIAGNOSTICS);
    for diagnostic in diagnostics {
        assert_eq!(
            fs::read_to_string(diagnostic).expect("read concurrent diagnostic"),
            "shared corruption"
        );
    }
    #[cfg(windows)]
    assert!(!path.exists());
    #[cfg(not(windows))]
    assert_eq!(
        fs::read_to_string(path).expect("read active cache"),
        "shared corruption",
        "non-Windows cleanup retains the active cache"
    );
}
