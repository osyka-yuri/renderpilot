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
fn windows_verbatim_drive_prefix_maps_to_normal_dos_spelling() {
    assert_eq!(
        strip_windows_verbatim_prefix_lexically(Path::new(r"\\?\D:\Games\Example\Game.exe"))
            .expect("drive prefix is supported"),
        Path::new(r"D:\Games\Example\Game.exe")
    );
}

#[test]
fn persisted_forward_slash_drive_prefix_maps_to_normal_dos_spelling() {
    assert_eq!(
        strip_windows_verbatim_prefix_lexically(Path::new("//?/D:/Games/Example/Game.exe"))
            .expect("persisted drive prefix is supported"),
        Path::new(r"D:\Games\Example\Game.exe")
    );
}

#[test]
fn windows_verbatim_unc_prefix_maps_to_normal_unc_spelling() {
    assert_eq!(
        strip_windows_verbatim_prefix_lexically(Path::new(r"\\?\UNC\server\share\Games\Game.exe"))
            .expect("UNC prefix is supported"),
        Path::new(r"\\server\share\Games\Game.exe")
    );
}

#[test]
fn persisted_forward_slash_unc_prefix_maps_to_normal_unc_spelling() {
    assert_eq!(
        strip_windows_verbatim_prefix_lexically(Path::new("//?/UNC/server/share/Games/Game.exe"))
            .expect("persisted UNC prefix is supported"),
        Path::new(r"\\server\share\Games\Game.exe")
    );
}

#[test]
fn windows_verbatim_device_namespace_fails_closed() {
    assert!(
        strip_windows_verbatim_prefix_lexically(Path::new(r"\\?\Volume{abc}\Games\Game.exe"))
            .is_err()
    );
    assert!(
        strip_windows_verbatim_prefix_lexically(Path::new(
            r"\\?\GLOBALROOT\Device\HarddiskVolume1\Game.exe"
        ))
        .is_err()
    );
    assert!(
        strip_windows_verbatim_prefix_lexically(Path::new("//?/Volume{abc}/Games/Game.exe"))
            .is_err()
    );
    assert!(
        strip_windows_verbatim_prefix_lexically(Path::new(
            "//?/GLOBALROOT/Device/HarddiskVolume1/Game.exe"
        ))
        .is_err()
    );
}

#[cfg(windows)]
#[test]
fn canonical_verbatim_drive_path_keeps_the_same_target_after_normalization() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("probe.exe");
    std::fs::write(&file, b"probe").expect("write probe file");
    let canonical = canonicalize_existing(&file).expect("canonical path");
    let normalized = strip_windows_verbatim_prefix(&canonical).expect("safe normal path");
    let normalized_canonical = std::fs::canonicalize(&normalized).expect("same file exists");
    assert_eq!(
        normalized_key(&canonical),
        normalized_key(&normalized_canonical)
    );
    assert!(!normalized.to_string_lossy().starts_with(r"\\?\"));
}

#[cfg(windows)]
#[test]
fn persisted_forward_slash_verbatim_path_keeps_the_same_target_after_normalization() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("probe.exe");
    std::fs::write(&file, b"probe").expect("write probe file");
    let canonical = canonicalize_existing(&file).expect("canonical path");
    let wire_path = canonical.to_string_lossy().replace('\\', "/");
    assert!(wire_path.starts_with("//?/"));

    let normalized = strip_windows_verbatim_prefix(Path::new(&wire_path))
        .expect("persisted spelling resolves to same file");
    let normalized_canonical = std::fs::canonicalize(&normalized).expect("same file exists");
    assert_eq!(
        normalized_key(&canonical),
        normalized_key(&normalized_canonical)
    );
    assert!(!normalized.to_string_lossy().starts_with(r"\\?\"));
}

#[cfg(windows)]
#[test]
#[ignore = "manual NVIDIA probe: creates no persisted profile and never calls SaveSettings"]
fn brothers_shipping_executable_creates_with_verified_normal_path_without_saving() {
    use renderpilot_nvapi::{Nvapi, NvapiError};
    use sha2::{Digest, Sha256};

    const GAME_ID: &str = "game:01M37SY9DSPA7ZN4XG7FC81XYQ";
    const TITLE: &str = "Brothers: A Tale of Two Sons Remake";
    let exe = Path::new(
        r"D:\SteamLibrary\steamapps\common\Brothers - A Tale of Two Sons - Remake\Brothers\Binaries\Win64\Brothers-Win64-Shipping.exe",
    );
    if !exe.is_file() {
        eprintln!("Brothers Remake is not installed at the probed location; skipped");
        return;
    }

    let canonical = canonicalize_existing(exe).expect("canonicalize installed executable");
    let normalized = strip_windows_verbatim_prefix(&canonical)
        .expect("ordinary spelling resolves to the same installed executable");
    assert!(!normalized.to_string_lossy().starts_with(r"\\?\"));
    assert_eq!(
        normalized_key(&canonical),
        normalized_key(&std::fs::canonicalize(&normalized).expect("normalized path resolves"))
    );
    let path = normalized.to_string_lossy().replace('\\', "/");
    let digest = Sha256::digest(GAME_ID.as_bytes());
    let profile_name = format!("{TITLE} [{}]", hex::encode(&digest[..6]));
    let nvapi = Nvapi::get().expect("local NVAPI available");
    nvapi.initialize().expect("initialize NVAPI");

    let session = nvapi.create_session().expect("open DRS session");
    assert!(matches!(
        session.find_profile_by_name(&profile_name),
        Err(NvapiError::ProfileNotFound)
    ));
    let profile = session
        .create_profile(&profile_name)
        .expect("create transient probe profile");
    profile
        .create_application(&path)
        .expect("CreateApplication accepts verified ordinary DOS path");
    drop(session); // Intentionally no SaveSettings call.

    let fresh = nvapi.create_session().expect("open fresh DRS session");
    assert!(matches!(
        fresh.find_profile_by_name(&profile_name),
        Err(NvapiError::ProfileNotFound)
    ));
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
    let canonical_root = canonicalize_existing(dir.path()).expect("canonicalize tempdir");
    assert!(is_within(&resolved, &canonical_root));
    assert_eq!(resolved.file_name().unwrap(), "nvngx_dlss.dll");
}

#[cfg(windows)]
#[test]
fn canonicalize_existing_expands_short_name_components() {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

    #[expect(
        unsafe_code,
        reason = "GetShortPathNameW creates the Windows alias needed by this regression test"
    )]
    fn short_path(path: &Path) -> std::io::Result<std::path::PathBuf> {
        let input = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        // SAFETY: `input` is NUL-terminated and the null output performs only
        // the documented size query.
        let required = unsafe { GetShortPathNameW(input.as_ptr(), std::ptr::null_mut(), 0) };
        if required == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut output = vec![0_u16; required as usize];
        // SAFETY: `output` owns `required` writable UTF-16 code units.
        let written = unsafe { GetShortPathNameW(input.as_ptr(), output.as_mut_ptr(), required) };
        if written == 0 {
            return Err(std::io::Error::last_os_error());
        }
        output.truncate(written as usize);
        Ok(std::path::PathBuf::from(OsString::from_wide(&output)))
    }

    let dir = tempfile::tempdir().expect("tempdir");
    let short = short_path(dir.path()).expect("short path");
    if normalized_key(&short) == normalized_key(dir.path()) {
        // 8.3 name creation can be disabled per volume.
        return;
    }

    let actual = canonicalize_existing(&short).expect("canonical short path");
    let expected = canonicalize_existing(dir.path()).expect("canonical long path");
    assert_eq!(normalized_key(&actual), normalized_key(&expected));
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
