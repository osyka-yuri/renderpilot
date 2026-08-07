use super::*;
use std::fs;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;

fn publication_temp_paths(directory: &Path, destination: &Path) -> Vec<PathBuf> {
    let prefix = format!(
        "{}.publish-{}-",
        destination
            .file_name()
            .expect("test destination has a file name")
            .to_string_lossy(),
        std::process::id()
    );
    fs::read_dir(directory)
        .expect("read publication directory")
        .map(|entry| entry.expect("read publication entry").path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(&prefix))
        })
        .collect()
}

#[cfg(windows)]
fn relocate_exact_candidate_and_install_successor(
    prepared: &PreparedNoReplaceWrite,
) -> (PathBuf, PathBuf) {
    let visible = prepared
        .temp_path
        .as_ref()
        .expect("Windows prepared candidate retains its visible temporary path")
        .clone();
    let relocated = visible.with_file_name(format!(
        "{}.relocated",
        visible
            .file_name()
            .expect("temporary path has a file name")
            .to_string_lossy()
    ));
    fs::rename(&visible, &relocated)
        .expect("rename the visible candidate while its exact handle remains open");
    fs::write(&visible, b"foreign successor")
        .expect("install a foreign successor at the reusable temporary path");
    (visible, relocated)
}

#[cfg(windows)]
#[test]
fn windows_exact_candidate_publication_ignores_source_path_substitution() {
    let directory = tempfile::tempdir().expect("temp dir");
    let destination = directory.path().join("候補-диагностика.bin");
    let prepared = prepare_file_atomically_no_replace(&destination, b"candidate")
        .expect("prepare unpublished candidate");
    let (successor, relocated) = relocate_exact_candidate_and_install_successor(&prepared);

    assert!(matches!(
        prepared
            .publish()
            .expect("publish the exact retained candidate"),
        NoReplaceWrite::Published
    ));
    assert_eq!(
        fs::read(&destination).expect("read published destination"),
        b"candidate",
        "the retained object, not the substituted temporary pathname, reaches destination"
    );
    assert_eq!(
        fs::read(&successor).expect("read foreign successor"),
        b"foreign successor",
        "publication must not consume a successor at the former temporary pathname"
    );
    assert!(
        !relocated.exists(),
        "publication consumes the exact relocated candidate object"
    );
}

#[cfg(windows)]
#[test]
fn windows_exact_candidate_discard_and_drop_ignore_source_path_substitution() {
    let directory = tempfile::tempdir().expect("temp dir");
    let destination = directory.path().join("候補-破棄.bin");
    let prepared = prepare_file_atomically_no_replace(&destination, b"discarded")
        .expect("prepare disposable candidate");
    let (successor, relocated) = relocate_exact_candidate_and_install_successor(&prepared);
    prepared
        .discard()
        .expect("discard the exact retained candidate");
    assert_eq!(
        fs::read(&successor).expect("read foreign discard successor"),
        b"foreign successor"
    );
    assert!(
        !destination.exists() && !relocated.exists(),
        "discard deletes only the relocated exact candidate without publishing"
    );

    let dropped_destination = directory.path().join("候補-drop.bin");
    let (successor, relocated) = {
        let prepared = prepare_file_atomically_no_replace(&dropped_destination, b"dropped")
            .expect("prepare candidate for Drop cleanup");
        let paths = relocate_exact_candidate_and_install_successor(&prepared);
        (paths.0, paths.1)
    };
    assert_eq!(
        fs::read(&successor).expect("read foreign Drop successor"),
        b"foreign successor"
    );
    assert!(
        !dropped_destination.exists() && !relocated.exists(),
        "Drop deletes the exact relocated candidate and leaves the successor untouched"
    );
}

#[cfg(windows)]
#[test]
fn windows_nested_catalog_path_publishes_exact_bytes() {
    let directory = tempfile::tempdir().expect("temp dir");
    let destination = directory
        .path()
        .join("libraries")
        .join("v1")
        .join("catalog.json");
    let candidate = b"{\"schema_version\":1}\n";

    assert!(matches!(
        write_file_atomically_no_replace(&destination, candidate)
            .expect("publish nested catalog path"),
        NoReplaceWrite::Published
    ));
    assert_eq!(
        fs::read(&destination).expect("read exact nested catalog path"),
        candidate
    );
    assert!(
        !directory.path().join("catalog.json").exists(),
        "publication must not resolve the leaf relative to an unrelated directory"
    );
}

#[cfg(windows)]
#[test]
fn windows_live_candidate_prevents_parent_rename_then_publishes_nested_leaf() {
    let directory = tempfile::tempdir().expect("temp dir");
    let requested_parent = directory.path().join("libraries").join("v1");
    let destination = requested_parent.join("catalog.json");
    let candidate = b"candidate catalog";
    let prepared = prepare_file_atomically_no_replace(&destination, candidate)
        .expect("prepare exact candidate under original parent");
    let relocation = directory.path().join("retained-v1");

    let error = fs::rename(&requested_parent, &relocation)
        .expect_err("an open live candidate prevents parent rename");
    assert_eq!(
        error.raw_os_error(),
        Some(windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED as i32)
    );
    assert!(
        requested_parent.is_dir(),
        "the original parent remains authoritative"
    );
    assert!(
        !relocation.exists(),
        "a denied rename must not create the relocation path"
    );
    assert!(
        !destination.exists(),
        "preparation alone must not create the final destination"
    );

    assert!(matches!(
        prepared
            .publish()
            .expect("publish through the still-live original parent"),
        NoReplaceWrite::Published
    ));
    assert_eq!(
        fs::read(&destination).expect("read exact nested destination"),
        candidate,
        "the live candidate publishes exact bytes at the original nested leaf"
    );
}

#[cfg(windows)]
#[test]
fn windows_live_candidate_prevents_parent_rename_and_preserves_winner() {
    let directory = tempfile::tempdir().expect("temp dir");
    let requested_parent = directory.path().join("libraries").join("v1");
    let destination = requested_parent.join("catalog.json");
    let prepared = prepare_file_atomically_no_replace(&destination, b"candidate catalog")
        .expect("prepare exact candidate under original parent");
    let relocation = directory.path().join("retained-v1");

    let error = fs::rename(&requested_parent, &relocation)
        .expect_err("an open live candidate prevents parent rename");
    assert_eq!(
        error.raw_os_error(),
        Some(windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED as i32)
    );
    assert!(
        requested_parent.is_dir(),
        "the original parent remains authoritative"
    );
    assert!(
        !relocation.exists(),
        "a denied rename must not create the relocation path"
    );
    assert!(
        !destination.exists(),
        "preparation alone must not create the final destination"
    );
    fs::write(&destination, b"winner").expect("install winner in original parent");

    assert!(matches!(
        prepared
            .publish()
            .expect("winner in original parent is an expected occupancy result"),
        NoReplaceWrite::Occupied
    ));
    assert_eq!(
        fs::read(&destination).expect("read original-parent winner"),
        b"winner",
        "no-replace publication must preserve the winner at the original nested leaf"
    );
    assert!(
        !relocation.exists(),
        "occupied publication must not create the denied relocation path"
    );
    assert!(
        publication_temp_paths(&requested_parent, &destination).is_empty(),
        "occupied publication must clean only its exact candidate"
    );
}

#[cfg(windows)]
#[test]
fn windows_occupied_and_ambiguous_failures_never_reclassify_destination_paths() {
    let directory = tempfile::tempdir().expect("temp dir");
    let destination = directory.path().join("occupied.bin");
    fs::write(&destination, b"winner").expect("seed immutable destination winner");
    let prepared = prepare_file_atomically_no_replace(&destination, b"candidate")
        .expect("prepare occupied candidate");
    let (successor, relocated) = relocate_exact_candidate_and_install_successor(&prepared);
    assert!(matches!(
        prepared
            .publish()
            .expect("occupied destination is an expected result"),
        NoReplaceWrite::Occupied
    ));
    assert_eq!(fs::read(&destination).expect("read winner"), b"winner");
    assert_eq!(
        fs::read(&successor).expect("read successor"),
        b"foreign successor"
    );
    assert!(
        !relocated.exists(),
        "occupied cleanup removes the exact candidate"
    );

    let ambiguous = directory.path().join("ambiguous.bin");
    fs::write(&ambiguous, b"winner").expect("seed ambiguous destination winner");
    let prepared = prepare_file_atomically_no_replace(&ambiguous, b"candidate")
        .expect("prepare ambiguous candidate");
    let (successor, relocated) = relocate_exact_candidate_and_install_successor(&prepared);
    let fault = inject_no_replace_test_fault(NoReplaceTestFault::Publish);
    let error = match prepared.publish() {
        Err(error) => error,
        Ok(NoReplaceWrite::Published | NoReplaceWrite::Occupied) => {
            panic!("an ambiguous publication error must remain an error")
        }
    };
    drop(fault);
    assert!(
        error
            .to_string()
            .contains("injected no-replace publication fault"),
        "the publication error is not reclassified from the occupied pathname"
    );
    assert_eq!(
        fs::read(&ambiguous).expect("read ambiguous winner"),
        b"winner"
    );
    assert_eq!(
        fs::read(&successor).expect("read successor"),
        b"foreign successor"
    );
    assert!(
        !relocated.exists(),
        "error cleanup deletes the exact candidate"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_unnamed_candidates_preserve_lookalikes_across_all_cleanup_paths() {
    let directory = tempfile::tempdir().expect("temp dir");
    let destination = directory.path().join("candidate.bin");
    let lookalike = directory.path().join("candidate.bin.publish-lookalike");
    fs::write(&lookalike, b"foreign lookalike").expect("write lookalike sibling");
    let prepared = prepare_file_atomically_no_replace(&destination, b"candidate")
        .expect("prepare unnamed Linux candidate");
    assert!(
        publication_temp_paths(directory.path(), &destination).is_empty(),
        "Linux preparation exposes no temporary filename"
    );
    assert!(matches!(
        prepared.publish().expect("link exact unnamed candidate"),
        NoReplaceWrite::Published
    ));
    assert_eq!(
        fs::read(&destination).expect("read destination"),
        b"candidate"
    );
    assert_eq!(
        fs::read(&lookalike).expect("read lookalike"),
        b"foreign lookalike"
    );

    let discard_destination = directory.path().join("discard.bin");
    let discard_lookalike = directory.path().join("discard.bin.publish-lookalike");
    fs::write(&discard_lookalike, b"foreign discard lookalike").expect("write discard lookalike");
    prepare_file_atomically_no_replace(&discard_destination, b"discard")
        .expect("prepare discard candidate")
        .discard()
        .expect("close exact unnamed discard candidate");
    assert!(!discard_destination.exists());
    assert_eq!(
        fs::read(&discard_lookalike).expect("read discard lookalike"),
        b"foreign discard lookalike"
    );

    let drop_destination = directory.path().join("drop.bin");
    let drop_lookalike = directory.path().join("drop.bin.publish-lookalike");
    fs::write(&drop_lookalike, b"foreign Drop lookalike").expect("write Drop lookalike");
    drop(
        prepare_file_atomically_no_replace(&drop_destination, b"drop")
            .expect("prepare Drop candidate"),
    );
    assert!(!drop_destination.exists());
    assert_eq!(
        fs::read(&drop_lookalike).expect("read Drop lookalike"),
        b"foreign Drop lookalike"
    );

    let occupied = directory.path().join("occupied.bin");
    fs::write(&occupied, b"winner").expect("seed occupied destination");
    let prepared = prepare_file_atomically_no_replace(&occupied, b"candidate")
        .expect("prepare occupied unnamed candidate");
    assert!(matches!(
        prepared
            .publish()
            .expect("exact link reports occupied destination"),
        NoReplaceWrite::Occupied
    ));
    assert_eq!(fs::read(&occupied).expect("read winner"), b"winner");

    let failed = directory.path().join("failed.bin");
    let prepared = prepare_file_atomically_no_replace(&failed, b"candidate")
        .expect("prepare fail-closed unnamed candidate");
    let fault = inject_no_replace_test_fault(NoReplaceTestFault::Publish);
    assert!(
        prepared.publish().is_err(),
        "a failed exact-link attempt is never reported as publication or occupancy"
    );
    drop(fault);
    assert!(
        !failed.exists(),
        "failed exact link never creates a destination"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_no_replace_keeps_the_candidate_unnamed_and_preserves_an_occupied_winner() {
    let directory = tempfile::tempdir().expect("temp dir");
    let destination = directory.path().join("candidate.bin");
    let prepared = prepare_file_atomically_no_replace(&destination, b"candidate")
        .expect("prepare exact unnamed Linux candidate");
    assert!(
        fs::read_dir(directory.path())
            .expect("inspect candidate directory before publication")
            .next()
            .is_none(),
        "O_TMPFILE preparation exposes no named source before the exact no-replace link"
    );
    assert!(matches!(
        prepared.publish().expect("link exact unnamed candidate"),
        NoReplaceWrite::Published
    ));
    assert_eq!(
        fs::read(&destination).expect("read published destination"),
        b"candidate"
    );

    let winner = directory.path().join("winner.bin");
    fs::write(&winner, b"winner").expect("seed occupied winner");
    let prepared = prepare_file_atomically_no_replace(&winner, b"candidate")
        .expect("prepare second exact unnamed candidate");
    assert!(matches!(
        prepared
            .publish()
            .expect("occupied exact link is a normal no-replace result"),
        NoReplaceWrite::Occupied
    ));
    assert_eq!(
        fs::read(&winner).expect("read occupied winner"),
        b"winner",
        "an occupied Linux destination keeps its winner while the unnamed candidate closes"
    );
}

#[test]
fn no_replace_faults_leave_no_destination_or_owned_temp() {
    for fault in [
        NoReplaceTestFault::Create,
        NoReplaceTestFault::Write,
        NoReplaceTestFault::Sync,
        NoReplaceTestFault::Publish,
    ] {
        let directory = tempfile::tempdir().expect("temp dir");
        let destination = directory.path().join(format!("fault-{fault:?}.bin"));
        let _fault = inject_no_replace_test_fault(fault);

        let error = match write_file_atomically_no_replace(&destination, b"candidate") {
            Err(error) => error,
            Ok(NoReplaceWrite::Published | NoReplaceWrite::Occupied) => {
                panic!("injected no-replace publication fault must propagate")
            }
        };

        assert!(
            error
                .to_string()
                .contains("injected no-replace publication fault"),
            "fault {fault:?} must remain diagnostic"
        );
        assert!(
            !destination.exists(),
            "fault {fault:?} must not publish a final destination"
        );
        assert!(
            publication_temp_paths(directory.path(), &destination).is_empty(),
            "fault {fault:?} must clean the temporary file it owns"
        );
    }
}

#[cfg(all(
    not(any(windows, target_os = "linux")),
    feature = "development-host-fallback"
))]
#[test]
fn no_replace_inspection_failure_is_never_reported_as_occupied() {
    let directory = tempfile::tempdir().expect("temp dir");
    let destination = directory.path().join("occupied.bin");
    fs::write(&destination, b"immutable winner").expect("seed occupied destination");
    let _fault = inject_no_replace_test_fault(NoReplaceTestFault::Inspect);

    let result = write_file_atomically_no_replace(&destination, b"candidate");

    assert!(
        result.is_err(),
        "ambiguous publication failure must propagate"
    );
    assert_eq!(
        fs::read(&destination).expect("read immutable winner"),
        b"immutable winner"
    );
    assert!(
        publication_temp_paths(directory.path(), &destination).is_empty(),
        "inspection failure still cleans the owned temporary file"
    );
}

#[cfg(not(target_os = "linux"))]
#[test]
fn no_replace_cleanup_failure_preserves_winner_and_only_its_temp_remains() {
    let directory = tempfile::tempdir().expect("temp dir");
    let destination = directory.path().join("occupied.bin");
    fs::write(&destination, b"immutable winner").expect("seed occupied destination");
    let _fault = inject_no_replace_test_fault(NoReplaceTestFault::Cleanup);

    let result = write_file_atomically_no_replace(&destination, b"candidate");

    assert!(result.is_err(), "injected cleanup failure must propagate");
    assert_eq!(
        fs::read(&destination).expect("read immutable winner"),
        b"immutable winner"
    );
    let owned_residue = publication_temp_paths(directory.path(), &destination);
    assert_eq!(
        owned_residue.len(),
        1,
        "only the failed call's known temporary file may remain"
    );
    fs::remove_file(&owned_residue[0]).expect("remove only the test-owned temporary residue");
    assert!(
        publication_temp_paths(directory.path(), &destination).is_empty(),
        "test cleanup removes no winner or foreign path"
    );
}

#[test]
fn atomic_write_replaces_existing_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("payload.bin");
    fs::write(&path, b"old").expect("seed file");

    write_file_atomically(&path, b"new").expect("replace file");

    assert_eq!(fs::read(&path).expect("read replaced file"), b"new");
}

#[test]
fn copy_file_atomically_replaces_existing_dest() {
    let dir = tempfile::tempdir().expect("temp dir");
    let source = dir.path().join("src.bin");
    let dest = dir.path().join("dest.bin");
    fs::write(&source, b"new-content").expect("seed source");
    fs::write(&dest, b"old").expect("seed dest");

    copy_file_atomically(&source, &dest).expect("atomic copy");

    assert_eq!(fs::read(&dest).expect("read dest"), b"new-content");
}

#[test]
fn copy_file_atomically_failure_leaves_dest_unchanged() {
    let dir = tempfile::tempdir().expect("temp dir");
    let dest = dir.path().join("dest.bin");
    fs::write(&dest, b"original").expect("seed dest");
    let missing = dir.path().join("does-not-exist.bin");

    let result = copy_file_atomically(&missing, &dest);

    assert!(result.is_err(), "copying a missing source must fail");
    assert_eq!(
        fs::read(&dest).expect("read dest"),
        b"original",
        "a failed copy must leave the existing destination untouched"
    );
}

#[test]
fn copy_file_atomically_is_a_noop_for_the_same_file() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("f.bin");
    fs::write(&path, b"data").expect("seed file");

    copy_file_atomically(&path, &path).expect("same-file no-op");

    assert_eq!(fs::read(&path).expect("read file"), b"data");
}

#[test]
fn atomic_no_replace_write_preserves_an_existing_destination() {
    let directory = tempfile::tempdir().expect("temp dir");
    let destination = directory.path().join("существующий-目的.bin");
    fs::write(&destination, b"existing bytes").expect("seed existing destination");

    let result = write_file_atomically_no_replace(&destination, b"candidate bytes")
        .expect("occupied destination is a normal outcome");

    assert!(matches!(result, NoReplaceWrite::Occupied));
    assert_eq!(
        fs::read(&destination).expect("read existing destination"),
        b"existing bytes"
    );
    let entries = fs::read_dir(directory.path())
        .expect("read publication directory")
        .map(|entry| entry.expect("read publication entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(
        entries,
        vec![
            destination
                .file_name()
                .expect("destination has file name")
                .to_os_string()
        ],
        "the occupied publication cleans its owned temporary file"
    );
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn no_replace_publication_accepts_an_absent_unicode_destination() {
    let directory = tempfile::tempdir().expect("temp dir");
    let destination = directory.path().join("диагностика.bin");
    let outcome = write_file_atomically_no_replace(&destination, b"source")
        .expect("publish without replacement");

    assert!(matches!(outcome, NoReplaceWrite::Published));
    assert_eq!(fs::read(destination).expect("read destination"), b"source");
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn no_replace_never_replaces_an_existing_destination() {
    let directory = tempfile::tempdir().expect("temp dir");
    let destination = directory.path().join("destination.bin");
    fs::write(&destination, b"destination").expect("write destination");

    let outcome = write_file_atomically_no_replace(&destination, b"source")
        .expect("occupied destination is expected");

    assert!(matches!(outcome, NoReplaceWrite::Occupied));
    assert_eq!(
        fs::read(destination).expect("read destination"),
        b"destination"
    );
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn no_replace_never_replaces_an_existing_directory() {
    let directory = tempfile::tempdir().expect("temp dir");
    let destination = directory.path().join("occupied");
    fs::create_dir(&destination).expect("create occupied directory");

    let outcome = write_file_atomically_no_replace(&destination, b"source")
        .expect("occupied directory is expected");

    assert!(matches!(outcome, NoReplaceWrite::Occupied));
    assert!(destination.is_dir());
}

#[cfg(windows)]
#[test]
fn windows_nested_unicode_path_exceeds_legacy_max_path() {
    let directory = tempfile::tempdir().expect("temp dir");
    let mut deep = directory.path().join("libraries").join("v1");
    for segment in [
        "候補-диагностика-".repeat(12),
        "каталог-検証-".repeat(12),
        "公開-данные-".repeat(12),
    ] {
        deep.push(segment);
    }
    fs::create_dir_all(&deep).expect("create long directory");
    let destination = deep.join("каталог-候補.json");
    assert!(
        destination.as_os_str().encode_wide().count() > 260,
        "test path must exceed legacy MAX_PATH"
    );
    assert!(matches!(
        write_file_atomically_no_replace(&destination, b"long unicode catalog path")
            .expect("publish long path"),
        NoReplaceWrite::Published
    ));

    assert_eq!(
        fs::read(destination).expect("read long-path destination"),
        b"long unicode catalog path"
    );
}
