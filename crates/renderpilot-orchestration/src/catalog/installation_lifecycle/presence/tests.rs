use super::*;

#[test]
fn readable_temporary_installation_is_present() {
    let tree = tempfile::tempdir().expect("temporary presence-test directory");
    let root = tree.path().join("game");
    fs::create_dir(&root).expect("create installation directory");

    assert_eq!(
        probe_installation_root(&root),
        InstallationPresence::Present
    );
}

#[test]
fn missing_child_below_readable_parent_has_root_bound_evidence() {
    let tree = tempfile::tempdir().expect("temporary presence-test directory");
    let root = tree.path().join("game");

    let evidence = match probe_installation_root(&root) {
        InstallationPresence::ConfirmedAbsent { evidence } => evidence,
        other => panic!("expected confirmed absence, got {other:?}"),
    };
    let (expected_anchor, _) = absolute_path_parts(&root).expect("temporary path is absolute");
    assert_eq!(evidence.readable_anchor(), expected_anchor.as_path());
    assert_eq!(evidence.missing_segment(), root.as_path());
    assert_eq!(evidence.probed_root(), root.as_path());
}

#[test]
fn non_directory_replacement_is_indeterminate() {
    let tree = tempfile::tempdir().expect("temporary presence-test directory");
    let root = tree.path().join("game");
    fs::write(&root, b"replacement").expect("create non-directory replacement");

    assert!(matches!(
        probe_installation_root(&root),
        InstallationPresence::Indeterminate { ref reason }
            if reason.contains("non-directory")
    ));
}

#[test]
fn dangling_link_is_indeterminate() {
    let tree = tempfile::tempdir().expect("temporary presence-test directory");
    let link = tree.path().join("game");
    let target = tree.path().join("missing-target");
    if let Err(error) = create_directory_symlink(&target, &link) {
        // Symlink creation may require an elevated test account on Windows.
        if cfg!(windows) && error.kind() == io::ErrorKind::PermissionDenied {
            eprintln!("skipping dangling-link filesystem case: {error}");
            return;
        }
        panic!("create dangling directory link: {error}");
    }

    assert!(matches!(
        probe_installation_root(&link),
        InstallationPresence::Indeterminate { ref reason }
            if reason.contains("symbolic link or reparse point")
    ));
}

#[cfg(unix)]
fn create_directory_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn create_directory_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

#[cfg(not(any(unix, windows)))]
fn create_directory_symlink(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "directory symlinks are unavailable on this platform",
    ))
}

#[test]
fn relative_path_is_indeterminate() {
    assert!(matches!(
        probe_installation_root(Path::new("relative\\game")),
        InstallationPresence::Indeterminate { ref reason }
            if reason.contains("must be absolute")
    ));
}

#[test]
fn access_errors_are_reported_as_indeterminate() {
    let offline_anchor = Path::new("Z:\\");
    assert!(matches!(
        io_indeterminate(
            "inspect volume/share anchor",
            offline_anchor,
            &io::Error::new(io::ErrorKind::PermissionDenied, "injected offline volume")
        ),
        InstallationPresence::Indeterminate { ref reason }
            if reason.contains("offline volume")
    ));
}

#[cfg(windows)]
#[test]
fn unused_drive_root_is_indeterminate_not_absent() {
    let unused_root = ('A'..='Z')
        .map(|letter| PathBuf::from(format!("{letter}:\\")))
        .find(|path| {
            fs::symlink_metadata(path).is_err_and(|error| error.kind() == io::ErrorKind::NotFound)
        });

    let Some(unused_root) = unused_root else {
        eprintln!("skipping unavailable-drive case: no drive root returned NotFound");
        return;
    };

    assert!(matches!(
        probe_installation_root(&unused_root),
        InstallationPresence::Indeterminate { .. }
    ));
}
