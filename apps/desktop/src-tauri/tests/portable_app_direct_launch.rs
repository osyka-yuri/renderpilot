//! Process-level regression tests for the private managed portable App entry.
#![cfg(all(windows, feature = "portable"))]

use std::{
    ffi::OsString,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};

const RUNTIME_ROOTS: [&str; 4] = [
    "data",
    ".renderpilot-runtime-authority",
    ".renderpilot-generations",
    ".renderpilot-update",
];

struct TestRoot(PathBuf);

impl TestRoot {
    fn create(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "renderpilot-portable-app-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("create isolated portable App test root");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copied_app(root: &Path) -> PathBuf {
    let app = root.join("renderpilot-desktop.exe");
    fs::copy(env!("CARGO_BIN_EXE_renderpilot-desktop"), &app)
        .expect("copy managed portable App image");
    app
}

fn append_valid_rpsx1_tail(app: &Path) {
    let rpu = b"copied structurally valid public RPU";
    let signature = b"untrusted comment: copied test signature\ntrusted comment: copied test\n";
    let rpu_offset = fs::metadata(app).expect("App metadata").len();
    let signature_offset = rpu_offset + rpu.len() as u64;
    let mut footer = [0_u8; 102];
    footer[..5].copy_from_slice(b"RPSX1");
    footer[5] = 1;
    footer[6..14].copy_from_slice(&rpu_offset.to_le_bytes());
    footer[14..22].copy_from_slice(&(rpu.len() as u64).to_le_bytes());
    footer[22..30].copy_from_slice(&signature_offset.to_le_bytes());
    footer[30..38].copy_from_slice(&(signature.len() as u64).to_le_bytes());
    footer[38..70].copy_from_slice(&Sha256::digest(rpu));
    footer[70..102].copy_from_slice(&Sha256::digest(signature));

    let mut output = OpenOptions::new()
        .append(true)
        .open(app)
        .expect("open copied App for RPSX1 tail");
    output.write_all(rpu).expect("append copied RPU");
    output
        .write_all(signature)
        .expect("append copied RPU signature");
    output
        .write_all(&footer)
        .expect("append valid RPSX1 footer");
}

fn assert_no_portable_effects(root: &Path, app: &Path) {
    for leaf in RUNTIME_ROOTS {
        assert!(!root.join(leaf).exists(), "direct launch created {leaf}");
    }
    let entries = fs::read_dir(root)
        .expect("read isolated root")
        .map(|entry| entry.expect("read root entry").file_name())
        .collect::<Vec<OsString>>();
    assert_eq!(
        entries,
        vec![app.file_name().expect("App file name").to_os_string()]
    );
}

#[test]
fn no_argument_managed_app_exits_without_portable_roots() {
    let root = TestRoot::create("no-args");
    let app = copied_app(root.path());

    let status = Command::new(&app)
        .current_dir(root.path())
        .status()
        .expect("launch managed App without supervisor arguments");

    assert!(status.success());
    assert_no_portable_effects(root.path(), &app);
}

#[test]
fn copied_rpsx1_tail_cannot_turn_the_managed_app_into_a_supervisor() {
    let root = TestRoot::create("copied-rpsx1");
    let app = copied_app(root.path());
    append_valid_rpsx1_tail(&app);

    let status = Command::new(&app)
        .current_dir(root.path())
        .status()
        .expect("launch managed App with copied RPSX1 tail");

    assert!(status.success());
    assert_no_portable_effects(root.path(), &app);
}

#[test]
fn forged_private_handle_values_exit_before_portable_roots() {
    let root = TestRoot::create("forged-handles");
    let app = copied_app(root.path());

    let status = Command::new(&app)
        .current_dir(root.path())
        .args([
            "--renderpilot-portable-app",
            "--renderpilot-control-handle=18446744073709551614",
            "--renderpilot-status-handle=18446744073709551613",
        ])
        .status()
        .expect("launch managed App with forged private handles");

    assert_eq!(status.code(), Some(1));
    assert_no_portable_effects(root.path(), &app);
}
