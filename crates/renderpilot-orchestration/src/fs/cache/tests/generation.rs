//! Generation, CAS, and transaction concurrency cases.

use super::*;

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn atomic_replacement_changes_generation_even_for_identical_bytes() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "same");
    let first =
        observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes)).expect("observe original");

    crate::fs::write_file_atomically(&path, b"same").expect("atomically replace bytes");
    let replacement = observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes))
        .expect("observe replacement");

    assert_ne!(
        first.generation(),
        replacement.generation(),
        "platform file identity distinguishes an atomic identical-byte replacement"
    );
}

#[test]
fn in_place_mutation_changes_generation_through_the_digest() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "old");
    let first =
        observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes)).expect("observe original");

    fs::write(&path, b"new").expect("mutate active cache in place");
    let mutated =
        observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes)).expect("observe mutation");

    assert_ne!(
        first.generation(),
        mutated.generation(),
        "same-length in-place writes are detected by the SHA-256 component"
    );
}

#[test]
fn changed_invalid_current_is_quarantined_and_replaced_inside_the_commit_lease() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "valid");
    let observed = observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes))
        .expect("observe valid cache");
    fs::write(&path, b"bad").expect("replace with corrupt cache");
    #[cfg(target_os = "linux")]
    let occupied_metadata = fs::metadata(&path).expect("snapshot corrupt cache metadata");
    #[cfg(target_os = "linux")]
    let mut validation_count = 0_usize;

    #[cfg(windows)]
    {
        let result = commit_cache_candidate(
            &path,
            observed.generation(),
            b"fetched",
            MatchingCurrentPolicy::RefreshCandidate,
            parse_doc,
        )
        .expect("commit fetched cache");

        assert!(matches!(result, CachePublication::Published));
        assert_eq!(fs::read(&path).expect("read published cache"), b"fetched");
    }
    #[cfg(all(
        not(any(windows, target_os = "linux")),
        feature = "development-host-fallback"
    ))]
    {
        let result = commit_cache_candidate(
            &path,
            observed.generation(),
            b"fetched",
            MatchingCurrentPolicy::RefreshCandidate,
            parse_doc,
        )
        .expect("commit fetched cache");

        assert!(matches!(result, CachePublication::Published));
        assert_eq!(fs::read(&path).expect("read published cache"), b"fetched");
    }
    #[cfg(target_os = "linux")]
    {
        let result = commit_cache_candidate(
            &path,
            observed.generation(),
            b"fetched",
            MatchingCurrentPolicy::RefreshCandidate,
            |bytes| {
                validation_count += 1;
                parse_doc(bytes)
            },
        )
        .expect("Linux reports a changed present occupant as unpublished");

        assert!(matches!(result, CachePublication::PreservedUnclassified));
        assert_eq!(
            validation_count, 0,
            "Linux must not validate a present successor before returning unpublished"
        );
        assert_eq!(
            fs::read(&path).expect("read retained corrupt cache"),
            b"bad",
            "the fetched candidate never replaces the changed occupant"
        );
        let retained_metadata = fs::metadata(&path).expect("inspect retained corrupt metadata");
        assert_eq!(retained_metadata.len(), occupied_metadata.len());
        assert_eq!(
            retained_metadata
                .modified()
                .expect("read retained corrupt cache mtime"),
            occupied_metadata
                .modified()
                .expect("read corrupt cache snapshot mtime"),
            "Linux does not rewrite the changed active occupant"
        );
        assert!(
            corrupt_diagnostic_paths(&path).is_empty(),
            "Linux does not create a diagnostic for an unvalidated present successor"
        );
    }
    #[cfg(not(target_os = "linux"))]
    assert_eq!(
        fs::read(corrupt_sidecar(&path, None)).expect("read corrupt diagnostic"),
        b"bad"
    );
}

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
#[test]
fn changed_valid_successor_during_invalid_commit_is_returned() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "observed valid cache");
    let observed = observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes))
        .expect("observe original cache");
    fs::write(&path, b"rejected cache").expect("install rejected successor");
    let winner = b"valid concurrent winner";
    let mut validation_count = 0_usize;
    let mut winner_modified = None;

    let result = commit_cache_candidate(
        &path,
        observed.generation(),
        b"fetched candidate",
        MatchingCurrentPolicy::RefreshCandidate,
        |bytes| {
            validation_count += 1;
            match bytes {
                b"rejected cache" => {
                    crate::fs::write_file_atomically(&path, winner)
                        .expect("non-compliant actor installs valid winner");
                    winner_modified = Some(
                        fs::metadata(&path)
                            .expect("inspect valid winner")
                            .modified()
                            .expect("read valid winner mtime"),
                    );
                    Err(crate::failed("rejected cache"))
                }
                bytes if bytes == winner => Ok("valid concurrent winner"),
                unexpected => panic!("unexpected cache classification {unexpected:?}"),
            }
        },
    );

    let result = result.expect("classify the valid concurrent winner");
    assert!(matches!(
        result,
        CachePublication::Current("valid concurrent winner")
    ));
    assert_eq!(validation_count, 2, "reclassify the swapped-in winner once");
    assert_eq!(fs::read(&path).expect("read valid winner"), winner);
    assert_eq!(
        fs::metadata(&path)
            .expect("inspect retained winner")
            .modified()
            .expect("read retained winner mtime"),
        winner_modified.expect("record winner mtime before classification returns"),
        "the winner is neither rewritten nor touched after it wins"
    );
    assert_eq!(
        fs::read(corrupt_sidecar(&path, None)).expect("read exact rejected diagnostic"),
        b"rejected cache"
    );
    assert!(
        corrupt_diagnostic_paths(&path).len() <= MAX_CORRUPT_DIAGNOSTICS,
        "the rejected snapshot produces at most one bounded immutable diagnostic set"
    );
    assert_ne!(
        fs::read(&path).expect("read retained winner"),
        b"fetched candidate"
    );
}

#[cfg(not(windows))]
#[test]
fn repeated_invalid_successors_fail_closed_without_publishing_candidate() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "observed valid cache");
    let observed = observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes))
        .expect("observe original cache");
    fs::write(&path, b"invalid successor 0").expect("install first invalid successor");
    #[cfg(target_os = "linux")]
    let initial_successor_metadata =
        fs::metadata(&path).expect("snapshot first invalid successor metadata");
    #[cfg(target_os = "linux")]
    let validation_count = 0_usize;
    #[cfg(not(target_os = "linux"))]
    let mut validation_count = 0_usize;
    #[cfg(not(target_os = "linux"))]
    let mut successor_modified = None;

    let result = commit_cache_candidate::<(), _>(
        &path,
        observed.generation(),
        b"fetched candidate",
        MatchingCurrentPolicy::RefreshCandidate,
        |bytes| {
            #[cfg(target_os = "linux")]
            {
                let _ = bytes;
                panic!("Linux must not validate a present invalid successor");
            }
            #[cfg(not(target_os = "linux"))]
            {
                let expected = format!("invalid successor {validation_count}");
                assert_eq!(bytes, expected.as_bytes());
                validation_count += 1;
                let next = format!("invalid successor {validation_count}");
                crate::fs::write_file_atomically(&path, next.as_bytes())
                    .expect("non-compliant actor installs another invalid successor");
                successor_modified = Some(
                    fs::metadata(&path)
                        .expect("inspect invalid successor")
                        .modified()
                        .expect("read invalid successor mtime"),
                );
                Err::<(), _>(crate::failed("invalid successor"))
            }
        },
    );

    #[cfg(not(target_os = "linux"))]
    {
        let error = result.expect_err("bounded churn fails closed");
        assert_eq!(validation_count, CACHE_CHURN_RETRIES + 1);
        assert!(
            error.to_string().contains("changed repeatedly"),
            "the bounded retry budget reports the churn error"
        );
        assert_eq!(
            fs::read(&path).expect("read final invalid successor"),
            format!("invalid successor {}", CACHE_CHURN_RETRIES + 1).into_bytes()
        );
    }
    #[cfg(target_os = "linux")]
    {
        let result = result.expect("Linux reports the first present successor as unpublished");
        assert!(matches!(result, CachePublication::PreservedUnclassified));
        assert_eq!(
            validation_count, 0,
            "Linux does not validate or retry a present invalid successor"
        );
        assert_eq!(
            fs::read(&path).expect("read retained invalid successor"),
            b"invalid successor 0"
        );
        let retained_metadata = fs::metadata(&path).expect("inspect retained invalid successor");
        assert_eq!(retained_metadata.len(), initial_successor_metadata.len());
        assert_eq!(
            retained_metadata
                .modified()
                .expect("read retained invalid successor mtime"),
            initial_successor_metadata
                .modified()
                .expect("read initial invalid successor mtime"),
            "the first successor remains byte-and-metadata-identical"
        );
        assert!(
            corrupt_diagnostic_paths(&path).is_empty(),
            "Linux creates no diagnostic for an unvalidated present successor"
        );
    }
    assert_ne!(
        fs::read(&path).expect("read final invalid successor"),
        b"fetched candidate"
    );
    #[cfg(not(target_os = "linux"))]
    for slot in 0..=CACHE_CHURN_RETRIES {
        assert_eq!(
            fs::read(corrupt_sidecar(
                &path,
                (slot != 0).then_some(u32::try_from(slot).expect("small slot")),
            ))
            .expect("read exact invalid diagnostic"),
            format!("invalid successor {slot}").into_bytes()
        );
    }
}

#[test]
fn stable_authority_survives_sidecar_replacement() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "cache");
    let legacy_sidecar = crate::fs::with_added_extension(&path, "renderpilot-cache-v1.lock")
        .expect("cache has a file name");
    let (held_tx, held_rx) = mpsc::sync_channel(0);
    let (release_tx, release_rx) = mpsc::sync_channel(0);
    let (attempt_tx, attempt_rx) = mpsc::sync_channel(0);
    let (acquired_tx, acquired_rx) = mpsc::sync_channel(0);

    let authority_path = path.clone();
    let authority_sidecar = legacy_sidecar.clone();
    let authority = std::thread::spawn(move || {
        with_cache_file_transaction(&authority_path, || {
            fs::write(&authority_sidecar, b"legacy authority").expect("create legacy sidecar");
            fs::remove_file(&authority_sidecar).expect("remove legacy sidecar");
            let replacement = authority_sidecar.with_extension("replacement");
            fs::write(&replacement, b"replacement sidecar").expect("write sidecar replacement");
            fs::rename(&replacement, &authority_sidecar).expect("replace legacy sidecar");
            held_tx.send(()).expect("report held authority");
            release_rx.recv().expect("release authority");
            Ok(())
        })
        .expect("complete authority transaction");
    });

    held_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("transaction A holds authority");
    let competing_path = path;
    let competitor = std::thread::spawn(move || {
        attempt_tx.send(()).expect("report transaction B attempt");
        with_cache_file_transaction(&competing_path, || {
            acquired_tx
                .send(())
                .expect("report transaction B authority");
            Ok(())
        })
        .expect("complete competing transaction");
    });

    attempt_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("transaction B starts");
    assert!(
        acquired_rx
            .recv_timeout(Duration::from_millis(150))
            .is_err(),
        "transaction B remains blocked by the stable kernel authority, not the replaceable sidecar"
    );
    release_tx.send(()).expect("release transaction A");
    acquired_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("transaction B acquires after A releases");
    authority.join().expect("authority thread");
    competitor.join().expect("competing thread");
    assert_eq!(
        fs::read(&legacy_sidecar).expect("read replacement legacy sidecar"),
        b"replacement sidecar"
    );
}

#[cfg(not(windows))]
#[test]
fn swapped_cache_quarantines_exact_observation_without_touching_winner() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("manifest.json");
    let rejected = vec![b'x'; MAX_CORRUPT_DIAGNOSTIC_BYTES + 17];
    let winner = b"valid replacement winner".to_vec();
    fs::write(&path, &rejected).expect("write rejected cache");
    let reader_path = path.clone();
    let expected_rejected = rejected.clone();
    let (observed_tx, observed_rx) = mpsc::sync_channel(0);
    let (resume_tx, resume_rx) = mpsc::sync_channel(0);

    let reader = std::thread::spawn(move || {
        observe_cache_file(&reader_path, |bytes, _metadata| {
            assert_eq!(bytes, expected_rejected);
            observed_tx
                .send(())
                .expect("report exact rejected snapshot");
            resume_rx.recv().expect("resume rejected validation");
            Err::<(), _>(crate::failed("rejected snapshot"))
        })
        .expect("invalid observations are returned rather than propagated")
    });
    observed_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("validator observes rejected bytes");
    crate::fs::write_file_atomically(&path, &winner).expect("non-compliant actor installs winner");
    resume_tx.send(()).expect("finish validation");

    let observation = reader.join().expect("reader thread");
    assert!(matches!(observation, CacheObservation::Invalid { .. }));
    assert_eq!(fs::read(&path).expect("read winner"), winner);
    assert_eq!(
        fs::read(corrupt_sidecar(&path, None)).expect("read bounded rejected diagnostic"),
        rejected[..MAX_CORRUPT_DIAGNOSTIC_BYTES],
        "the diagnostic is the exact bounded rejected snapshot, never the active winner"
    );
}

#[test]
fn unexpected_cache_directory_fails_closed_without_quarantine() {
    let dir = tempdir().expect("temp dir");
    let path = dir.path().join("manifest.json");
    fs::create_dir(&path).expect("create unexpected cache directory");

    assert!(
        observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes)).is_err(),
        "a cache pathname that resolves to a directory is rejected"
    );
    assert!(
        path.is_dir(),
        "the unexpected cache directory remains untouched"
    );
    assert!(
        !corrupt_sidecar(&path, None).exists(),
        "no diagnostic is created from an unsafe cache entry"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn symlink_cache_and_diagnostic_slot_fail_closed_without_following() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().expect("temp dir");
    let target = dir.path().join("foreign-target");
    let path = dir.path().join("manifest.json");
    fs::write(&target, b"foreign bytes").expect("write symlink target");
    symlink(&target, &path).expect("create cache symlink");

    assert!(
        observe_cache_file(&path, |bytes, _metadata| parse_doc(bytes)).is_err(),
        "O_NOFOLLOW rejects the active cache symlink"
    );
    assert_eq!(
        fs::read(&target).expect("read foreign target"),
        b"foreign bytes"
    );
    assert!(
        fs::symlink_metadata(&path)
            .expect("inspect cache link")
            .file_type()
            .is_symlink()
    );

    fs::remove_file(&path).expect("remove cache symlink");
    fs::write(&path, b"bad").expect("write rejected cache");
    let diagnostic = corrupt_sidecar(&path, None);
    symlink(&target, &diagnostic).expect("occupy diagnostic slot with foreign link");
    quarantine_at(&path);

    assert!(
        fs::symlink_metadata(&diagnostic)
            .expect("inspect foreign diagnostic link")
            .file_type()
            .is_symlink(),
        "a symlink occupies its diagnostic slot without being opened or replaced"
    );
    assert_eq!(
        fs::read(&target).expect("read foreign target"),
        b"foreign bytes"
    );
    assert_eq!(
        fs::read(corrupt_sidecar(&path, Some(1))).expect("read next diagnostic slot"),
        b"bad"
    );
}

#[test]
fn cache_transaction_prevents_delayed_quarantine_of_new_valid_bytes() {
    let dir = tempdir().expect("temp dir");
    let path = write_cache(dir.path(), "bad");
    let reader_path = path.clone();
    let publisher_path = path.clone();
    let (observed_tx, observed_rx) = mpsc::sync_channel(0);
    let (resume_tx, resume_rx) = mpsc::sync_channel(0);
    let (publishing_tx, publishing_rx) = mpsc::sync_channel(0);
    let (published_tx, published_rx) = mpsc::sync_channel(0);

    let reader = std::thread::spawn(move || {
        let result = read_with_corrupt_quarantine(&reader_path, || {
            let bytes = fs::read(&reader_path).expect("read rejected bytes");
            observed_tx.send(()).expect("signal observation");
            resume_rx.recv().expect("resume reader");
            parse_doc(&bytes)
        });
        assert!(result.is_err(), "the original bytes are corrupt");
    });
    observed_rx.recv().expect("reader observed corrupt bytes");

    let publisher = std::thread::spawn(move || {
        publishing_tx.send(()).expect("signal publisher attempt");
        with_cache_file_transaction(&publisher_path, || {
            crate::fs::write_file_atomically(&publisher_path, b"valid")
        })
        .expect("publish valid cache");
        published_tx.send(()).expect("signal publication");
    });
    publishing_rx.recv().expect("publisher started");
    assert!(
        published_rx
            .recv_timeout(Duration::from_millis(100))
            .is_err(),
        "the publisher must wait while validation and quarantine share the lease"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("read locked active cache"),
        "bad"
    );

    resume_tx.send(()).expect("release reader");
    reader.join().expect("reader thread");
    published_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("publisher completes after quarantine");
    publisher.join().expect("publisher thread");

    assert_eq!(
        fs::read_to_string(&path).expect("read published active cache"),
        "valid",
        "the delayed quarantine cannot move newly published valid bytes"
    );
    assert_eq!(
        fs::read_to_string(corrupt_sidecar(&path, None)).expect("read rejected diagnostic"),
        "bad"
    );
}
