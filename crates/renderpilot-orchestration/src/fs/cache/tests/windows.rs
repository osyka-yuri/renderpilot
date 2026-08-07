//! Windows exact-retirement cases.

use super::*;

#[cfg(windows)]
#[test]
fn windows_writable_contention_fails_closed_without_mutation() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "original");
    let writer = fs::OpenOptions::new()
        .write(true)
        .access_mode(FILE_GENERIC_WRITE)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(&path)
        .expect("pre-open writable non-delete-sharing handle");

    let started = Instant::now();
    let error = observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes))
        .expect_err("exclusive cache proof must fail closed under writable contention");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "bounded cache contention must not wait indefinitely"
    );
    assert!(
        error.to_string().contains("changed repeatedly"),
        "contention reports bounded fail-closed churn"
    );
    drop(writer);
    assert_eq!(
        fs::read(&path).expect("read unchanged cache after contention"),
        b"original",
        "failed contention cannot mutate the active cache"
    );
}

#[cfg(windows)]
#[test]
fn windows_exclusive_snapshot_blocks_in_place_and_replacement_writes() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "pinned");
    let observation = observe_cache_file(&path, |bytes, _metadata| {
        assert_eq!(
            bytes, b"pinned",
            "validator receives the exact pinned snapshot"
        );
        assert!(
            fs::OpenOptions::new().write(true).open(&path).is_err(),
            "an exclusive final snapshot blocks an in-place writer"
        );
        assert!(
            crate::fs::write_file_atomically(&path, b"replacement").is_err(),
            "an exclusive final snapshot blocks pathname replacement"
        );
        Ok::<_, ServiceError>("pinned")
    })
    .expect("valid pinned observation");

    assert!(matches!(
        observation,
        CacheObservation::Valid {
            value: "pinned",
            ..
        }
    ));
    assert_eq!(
        fs::read(&path).expect("read cache after blocked writers"),
        b"pinned",
        "blocked writers leave the exact pinned cache untouched"
    );
}

#[cfg(windows)]
#[test]
fn windows_retirement_failure_discards_prepared_candidate_without_mutation() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "original");
    let observed = observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes))
        .expect("observe original cache");
    let hook =
        inject_cache_publication_test_hook(CachePublicationTestHook::FailBeforeExactRetirement);

    let error = commit_cache_candidate(
        &path,
        observed.generation(),
        b"candidate",
        MatchingCurrentPolicy::RefreshCandidate,
        parse_doc,
    )
    .expect_err("injected retirement failure must fail closed");
    drop(hook);

    assert!(
        error.to_string().contains("before exact retirement"),
        "the exact-retirement failure remains diagnostic"
    );
    assert_eq!(
        fs::read(&path).expect("read cache after failed retirement"),
        b"original",
        "retirement failure preserves the original exact cache"
    );
    assert!(
        windows_owned_publication_temp_paths(&path).is_empty(),
        "failed retirement discards the prepared owned candidate"
    );
}

#[cfg(windows)]
#[test]
fn windows_successor_after_exact_retirement_wins_no_replace_and_reclassification() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "observed");
    let observed = observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes))
        .expect("observe original cache");
    let winner = b"valid successor";
    let hook = inject_cache_publication_test_hook(
        CachePublicationTestHook::InstallSuccessorAfterExactRetirement(winner.to_vec()),
    );
    let mut validation_count = 0_usize;

    let result = commit_cache_candidate(
        &path,
        observed.generation(),
        b"candidate",
        MatchingCurrentPolicy::RefreshCandidate,
        |bytes| {
            validation_count += 1;
            assert_eq!(bytes, winner, "only the late successor is reclassified");
            Ok::<_, ServiceError>("valid successor")
        },
    )
    .expect("late valid successor wins no-replace publication");
    drop(hook);

    assert!(matches!(
        result,
        CachePublication::Current("valid successor")
    ));
    assert_eq!(validation_count, 1, "the winner is classified exactly once");
    assert_eq!(
        fs::read(&path).expect("read retained successor"),
        winner,
        "the prepared candidate never overwrites the late successor"
    );
    assert!(
        windows_owned_publication_temp_paths(&path).is_empty(),
        "occupied publication cleans only its prepared temporary file"
    );
}
