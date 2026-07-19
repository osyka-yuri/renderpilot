//! Snapshot revalidation guards for the unlocked prepare window.

use renderpilot_domain::{
    ManagedAddonFile, ManagedFileBaseline, PathRef, Sha256Hash, TrackedSource, TrackedSourceRole,
};

use super::ensure_record_still_matches_snapshot;
use super::test_fixtures::{multi_source_record, record_with_digest, record_with_sources};

#[test]
fn snapshot_matches_identical_records() {
    let a = record_with_digest(r"C:\Games\A\Luma-Game.addon", "digest-a");
    let b = record_with_digest(r"C:\Games\A\Luma-Game.addon", "digest-a");
    assert!(ensure_record_still_matches_snapshot(&a, &b).is_ok());
}

#[test]
fn snapshot_rejects_digest_drift() {
    let a = record_with_digest(r"C:\Games\A\Luma-Game.addon", "digest-a");
    let b = record_with_digest(r"C:\Games\A\Luma-Game.addon", "digest-b");
    assert!(ensure_record_still_matches_snapshot(&a, &b).is_err());
}

#[test]
fn snapshot_accepts_advisory_payload_promoted_to_zip_digest() {
    let url = "https://example/Luma.zip";
    let snapshot = record_with_sources(
        r"C:\Games\A\Luma-Game.addon",
        vec![
            TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                url,
                None,
                "advisory-content-digest",
            )
            .with_advisory(),
        ],
    );
    let current = record_with_sources(
        r"C:\Games\A\Luma-Game.addon",
        vec![TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            url,
            Some("etag".to_owned()),
            "zip-digest",
        )],
    );
    assert!(ensure_record_still_matches_snapshot(&snapshot, &current).is_ok());
}

#[test]
fn snapshot_rejects_non_advisory_payload_digest_drift_even_with_same_url() {
    let url = "https://example/Luma.zip";
    let snapshot = record_with_sources(
        r"C:\Games\A\Luma-Game.addon",
        vec![TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            url,
            None,
            "digest-a",
        )],
    );
    let current = record_with_sources(
        r"C:\Games\A\Luma-Game.addon",
        vec![TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            url,
            None,
            "digest-b",
        )],
    );
    assert!(ensure_record_still_matches_snapshot(&snapshot, &current).is_err());
}

#[test]
fn snapshot_rejects_host_drift_even_when_payload_was_promoted() {
    let url = "https://example/Luma.zip";
    let snapshot = record_with_sources(
        r"C:\Games\A\Luma-Game.addon",
        vec![
            TrackedSource::new(TrackedSourceRole::AddonPayload, url, None, "adv").with_advisory(),
            TrackedSource::new(
                TrackedSourceRole::HostBinary,
                "https://example/reshade.zip",
                None,
                "h1",
            ),
            TrackedSource::new(
                TrackedSourceRole::DgVoodooWrapper,
                "https://example/dgvoodoo.zip",
                None,
                "d",
            ),
        ],
    );
    let current = record_with_sources(
        r"C:\Games\A\Luma-Game.addon",
        vec![
            TrackedSource::new(TrackedSourceRole::AddonPayload, url, None, "zip"),
            TrackedSource::new(
                TrackedSourceRole::HostBinary,
                "https://example/reshade.zip",
                None,
                "h2",
            ),
            TrackedSource::new(
                TrackedSourceRole::DgVoodooWrapper,
                "https://example/dgvoodoo.zip",
                None,
                "d",
            ),
        ],
    );
    assert!(ensure_record_still_matches_snapshot(&snapshot, &current).is_err());
}

#[test]
fn snapshot_rejects_addon_path_drift() {
    let a = record_with_digest(r"C:\Games\A\Luma-Game.addon", "digest-a");
    let b = record_with_digest(r"C:\Games\B\Luma-Game.addon", "digest-a");
    assert!(ensure_record_still_matches_snapshot(&a, &b).is_err());
}

#[test]
fn snapshot_matches_identical_multi_source_records() {
    let a = multi_source_record(r"C:\Games\A\Luma-Game.addon", "p", "h", "d");
    let b = multi_source_record(r"C:\Games\A\Luma-Game.addon", "p", "h", "d");
    assert!(ensure_record_still_matches_snapshot(&a, &b).is_ok());
}

#[test]
fn snapshot_rejects_host_digest_drift() {
    let a = multi_source_record(r"C:\Games\A\Luma-Game.addon", "p", "h1", "d");
    let b = multi_source_record(r"C:\Games\A\Luma-Game.addon", "p", "h2", "d");
    assert!(ensure_record_still_matches_snapshot(&a, &b).is_err());
}

#[test]
fn snapshot_rejects_dgvoodoo_digest_drift() {
    let a = multi_source_record(r"C:\Games\A\Luma-Game.addon", "p", "h", "d1");
    let b = multi_source_record(r"C:\Games\A\Luma-Game.addon", "p", "h", "d2");
    assert!(ensure_record_still_matches_snapshot(&a, &b).is_err());
}

#[test]
fn snapshot_rejects_managed_files_drift() {
    let hash = Sha256Hash::new("a".repeat(64)).expect("hash");
    let binding = ManagedAddonFile::owned(
        PathRef::new(r"C:\Games\A\nvngx_dlss.dll").expect("path"),
        ManagedFileBaseline::Absent,
        hash,
    );
    let a = record_with_digest(r"C:\Games\A\Luma-Game.addon", "digest-a");
    let b = a
        .clone()
        .try_with_managed_files(vec![binding])
        .expect("managed");
    assert!(ensure_record_still_matches_snapshot(&a, &b).is_err());
}
