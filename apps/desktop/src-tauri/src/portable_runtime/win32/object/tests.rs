use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use windows_sys::Win32::Storage::FileSystem::{
    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, SYNCHRONIZE,
};

use super::handle::{RelativeFileOpen, open_relative_file, open_root};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

struct TestRoot(PathBuf);

impl TestRoot {
    fn create() -> Self {
        let root = std::env::temp_dir().join(format!(
            "renderpilot-native-open-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("create isolated native-open test root");
        Self(root)
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn object_boundary_exposes_only_purpose_typed_operations() {
    let source = include_str!("mod.rs");
    for required in [
        "acquire_supervisor_admission",
        "DiagnosticsRoleDirectory",
        "RawSupervisorImageObject",
        "SelectedGenerationObject",
        "PortableRoot",
    ] {
        assert!(
            source.contains(required),
            "missing purpose type: {required}"
        );
    }
    assert!(!source.contains("VerifiedDirectory"));
    assert!(!source.contains("open_relative"));
}

#[test]
fn relative_native_file_open_uses_preexpanded_access_rights() {
    let test_root = TestRoot::create();
    fs::write(test_root.0.join("image.exe"), b"portable image").expect("write native-open fixture");
    let root = open_root(
        &test_root.0,
        FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    )
    .expect("retain fixture root");

    let mut image = open_relative_file(&root, "image.exe", RelativeFileOpen::SharedRead)
        .expect("open image through the production relative NtCreateFile path");

    assert_eq!(
        image.read_all().expect("read retained image"),
        b"portable image"
    );
}
