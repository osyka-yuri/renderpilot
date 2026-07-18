use renderpilot_domain::{
    AddonKind, GameId, InstalledAddon, InstalledAddonHostKind, PathRef, TrackedSource,
    TrackedSourceRole,
};

use crate::ServiceError;
use crate::addons::luma::fetch::types::LumaPayload;
use crate::addons::tracking;

use super::*;

fn game_id() -> GameId {
    GameId::new("steam:403640").expect("id")
}

fn path(value: &str) -> PathRef {
    PathRef::new(value).expect("path")
}

fn base_record() -> InstalledAddon {
    InstalledAddon::new(
        game_id(),
        AddonKind::Luma,
        path(r"C:\Games\Test\Luma-Test.addon"),
    )
}

fn test_payload(build_number: Option<u64>) -> LumaPayload {
    LumaPayload {
        files: Vec::new(),
        main_addon_rel: "Luma-Test.addon".to_owned(),
        zip_digest: "zip".to_owned(),
        etag: None,
        last_modified: None,
        build_number,
    }
}

#[test]
fn resolved_addon_version_prefers_payload_build_label() {
    let record = base_record().with_addon_version("Build 1");
    assert_eq!(
        resolved_addon_version(&record, &test_payload(Some(600))).as_deref(),
        Some("Build 600")
    );
}

#[test]
fn resolved_addon_version_keeps_existing_when_build_number_absent() {
    let record = base_record().with_addon_version("Build 515");
    assert_eq!(
        resolved_addon_version(&record, &test_payload(None)).as_deref(),
        Some("Build 515")
    );
}

#[test]
fn resolved_addon_version_is_none_when_both_absent() {
    let record = base_record();
    assert_eq!(resolved_addon_version(&record, &test_payload(None)), None);
}

#[test]
fn rebuild_preserves_timestamps_and_host_kind() {
    let record = base_record().with_timestamps(Some(10), Some(20));
    let updated = rebuild(
        &record,
        tracking::RebuildParts {
            addon_file: path(r"C:\Games\Test\Luma-Test.addon"),
            created_files: vec![path(r"C:\Games\Test\Luma-Test.addon")],
            backed_up_files: Vec::new(),
            tracked_sources: vec![TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                "https://example/asset.zip",
                None,
                "digest",
            )],
            addon_version: tracking::AddonVersionUpdate::Set(Some("Build 600".to_owned())),
            managed_files: tracking::ManagedFilesUpdate::Keep,
            label: "test rebuild".to_owned(),
        },
    )
    .expect("rebuild");

    assert_eq!(updated.installed_at(), Some(10));
    assert_eq!(updated.updated_at(), Some(20));
    assert_eq!(updated.host_kind(), Some(InstalledAddonHostKind::Proxy));
    assert_eq!(updated.addon_version(), Some("Build 600"));
}

#[test]
fn rebuild_carries_over_the_reshade_channel() {
    // B.6: a rebuild must not silently drop the recorded channel -- it's the
    // only place `LumaInstallState.reshade_channel` reads from.
    let record = base_record().with_reshade_channel("nightly");
    let updated = rebuild(
        &record,
        tracking::RebuildParts {
            addon_file: path(r"C:\Games\Test\Luma-Test.addon"),
            created_files: vec![path(r"C:\Games\Test\Luma-Test.addon")],
            backed_up_files: Vec::new(),
            tracked_sources: Vec::new(),
            addon_version: tracking::AddonVersionUpdate::Set(None),
            managed_files: tracking::ManagedFilesUpdate::Keep,
            label: "test rebuild".to_owned(),
        },
    )
    .expect("rebuild");

    assert_eq!(updated.reshade_channel(), Some("nightly"));
}

#[test]
fn rebuild_supports_a_renamed_main_addon() {
    let record = base_record();
    let renamed = path(r"C:\Games\Test\Luma-Test-Renamed.addon");
    let updated = rebuild(
        &record,
        tracking::RebuildParts {
            addon_file: renamed.clone(),
            created_files: vec![renamed.clone()],
            backed_up_files: Vec::new(),
            tracked_sources: Vec::new(),
            addon_version: tracking::AddonVersionUpdate::Set(None),
            managed_files: tracking::ManagedFilesUpdate::Keep,
            label: "test rebuild".to_owned(),
        },
    )
    .expect("rebuild");

    assert_eq!(updated.addon_file(), &renamed);
}

#[test]
fn rebuild_names_the_missing_addon_file_in_its_error() {
    let record = base_record();
    let error = rebuild(
        &record,
        tracking::RebuildParts {
            addon_file: path(r"C:\Games\Test\Luma-Test.addon"),
            created_files: vec![path(r"C:\Games\Test\dxgi.dll")],
            backed_up_files: Vec::new(),
            tracked_sources: Vec::new(),
            addon_version: tracking::AddonVersionUpdate::Set(None),
            managed_files: tracking::ManagedFilesUpdate::Keep,
            label: "test rebuild".to_owned(),
        },
    )
    .expect_err("missing addon_file from created_files must fail");

    match error {
        ServiceError::CommandFailed(message) => {
            assert!(message.contains("Luma-Test.addon"), "{message}");
        }
        other => panic!("expected CommandFailed, got {other:?}"),
    }
}

#[test]
fn owns_path_matches_via_same_path_not_raw_string_equality() {
    use tempfile::tempdir;

    let dir = tempdir().expect("tempdir");
    let host = dir.path().join("dxgi.dll");
    std::fs::write(&host, b"host").expect("write");
    let recorded = path(&host.to_string_lossy());
    let record = InstalledAddon::from_parts(
        game_id(),
        AddonKind::Luma,
        path(&dir.path().join("Luma-Test.addon").to_string_lossy()),
        None,
        vec![
            path(&dir.path().join("Luma-Test.addon").to_string_lossy()),
            recorded,
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("record");

    // Same filesystem path via a re-joined PathBuf (not necessarily the same
    // OsString representation as was stored) must still count as ownership.
    assert!(owns_path(&record, &dir.path().join("dxgi.dll")));
    assert!(!owns_path(&record, &dir.path().join("d3d11.dll")));
}

#[test]
fn payload_owned_paths_excludes_host_proxy_and_reshade_ini() {
    let record = InstalledAddon::from_parts(
        game_id(),
        AddonKind::Luma,
        path(r"C:\Games\Test\Luma-Test.addon"),
        None,
        vec![
            path(r"C:\Games\Test\Luma-Test.addon"),
            path(r"C:\Games\Test\dxgi.dll"),
            path(r"C:\Games\Test\ReShade.ini"),
            path(r"C:\Games\Test\Luma\Global\A.hlsl"),
        ],
        Vec::new(),
        Vec::new(),
    )
    .expect("record");

    let payload = payload_owned_paths(&record, &[]);
    assert_eq!(payload.len(), 2);
    assert!(payload.iter().any(|p| p.ends_with("Luma-Test.addon")));
    assert!(payload.iter().any(|p| p.ends_with("A.hlsl")));
    assert!(!payload.iter().any(|p| p.ends_with("dxgi.dll")));
    assert!(!payload.iter().any(|p| p.ends_with("ReShade.ini")));

    let adjacent = owned_host_adjacent_paths(&record);
    assert_eq!(adjacent.len(), 1);
    assert!(adjacent[0].ends_with("ReShade.ini"));
}

fn record_with_sources(sources: Vec<TrackedSource>) -> InstalledAddon {
    InstalledAddon::from_parts(
        game_id(),
        AddonKind::Luma,
        path(r"C:\Games\Test\Luma-Test.addon"),
        None,
        vec![path(r"C:\Games\Test\Luma-Test.addon")],
        Vec::new(),
        sources,
    )
    .expect("record")
}

fn advisory_payload(etag: Option<String>, last_modified: Option<String>) -> TrackedSource {
    TrackedSource::new(
        TrackedSourceRole::AddonPayload,
        "https://example/Luma.zip",
        etag,
        "content-digest",
    )
    .with_last_modified(last_modified)
    .with_advisory()
}

#[test]
fn payload_needs_provenance_bind_only_for_unbound_advisory_payload() {
    assert!(payload_needs_provenance_bind(&record_with_sources(vec![
        advisory_payload(None, None)
    ])));

    assert!(
        !payload_needs_provenance_bind(&record_with_sources(vec![advisory_payload(
            None,
            Some(ADVISORY_PAYLOAD_CHECKED_MARK.to_owned()),
        )])),
        "sentinel bind mark must stop auto ZIP bind"
    );

    assert!(
        !payload_needs_provenance_bind(&record_with_sources(vec![advisory_payload(
            Some("\"etag\"".to_owned()),
            None,
        )])),
        "real ETag bind mark must stop auto ZIP bind"
    );

    assert!(
        !payload_needs_provenance_bind(&record_with_sources(vec![TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example/Luma.zip",
            Some("\"etag\"".to_owned()),
            "zip-digest",
        )])),
        "promoted ZIP provenance must keep passive probes on HEAD/ETag only"
    );

    assert!(
        !payload_needs_provenance_bind(&record_with_sources(vec![
            TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                "https://example/Luma.zip",
                Some("\"etag\"".to_owned()),
                "zip-digest",
            ),
            TrackedSource::new(
                TrackedSourceRole::HostBinary,
                "https://example/host.zip",
                None,
                "host-digest",
            )
            .with_advisory()
            .with_channel("nightly"),
        ])),
        "advisory host alone must not force a release-ZIP download on passive"
    );
}

#[test]
fn source_has_bind_mark_detects_validators_and_sentinel() {
    assert!(!source_has_bind_mark(&advisory_payload(None, None)));
    assert!(source_has_bind_mark(&advisory_payload(
        Some("\"e\"".to_owned()),
        None
    )));
    assert!(source_has_bind_mark(&advisory_payload(
        None,
        Some(ADVISORY_PAYLOAD_CHECKED_MARK.to_owned())
    )));
}

#[test]
fn promote_advisory_payload_source_clears_advisory_and_stores_zip_digest() {
    use crate::addons::luma::fetch::types::{LumaPayload, LumaPayloadFile};

    let advisory = advisory_payload(None, None);
    let payload = LumaPayload {
        files: vec![LumaPayloadFile {
            relative_path: "Luma-Game.addon".to_owned(),
            bytes: b"addon".to_vec(),
        }],
        main_addon_rel: "Luma-Game.addon".to_owned(),
        zip_digest: "zip-digest".to_owned(),
        etag: Some("etag-1".to_owned()),
        last_modified: Some("Wed, 01 Jan 2025 00:00:00 GMT".to_owned()),
        build_number: Some(600),
    };
    let mut sources = vec![
        advisory.clone(),
        TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example/host.zip",
            None,
            "host",
        ),
    ];

    promote_advisory_payload_source(&mut sources, &advisory, &payload);

    let payload_source = sources
        .iter()
        .find(|source| source.role() == TrackedSourceRole::AddonPayload)
        .expect("payload source");
    assert!(!payload_source.is_advisory());
    assert_eq!(payload_source.digest(), "zip-digest");
    assert_eq!(payload_source.etag(), Some("etag-1"));
    assert!(
        sources
            .iter()
            .any(|source| source.role() == TrackedSourceRole::HostBinary),
        "other roles must be preserved"
    );
}

#[test]
fn mark_advisory_payload_source_keeps_advisory_digest_and_attaches_bind_mark() {
    use crate::addons::luma::fetch::types::{LumaPayload, LumaPayloadFile};

    let advisory = advisory_payload(None, None);
    let with_validators = LumaPayload {
        files: vec![LumaPayloadFile {
            relative_path: "Luma-Game.addon".to_owned(),
            bytes: b"addon".to_vec(),
        }],
        main_addon_rel: "Luma-Game.addon".to_owned(),
        zip_digest: "zip-digest".to_owned(),
        etag: Some("etag-1".to_owned()),
        last_modified: Some("Wed, 01 Jan 2025 00:00:00 GMT".to_owned()),
        build_number: None,
    };
    let mut sources = vec![advisory.clone()];
    mark_advisory_payload_source(&mut sources, &advisory, &with_validators);
    let marked = sources
        .iter()
        .find(|source| source.role() == TrackedSourceRole::AddonPayload)
        .expect("payload source");
    assert!(marked.is_advisory());
    assert_eq!(marked.digest(), "content-digest");
    assert_eq!(marked.etag(), Some("etag-1"));
    assert!(source_has_bind_mark(marked));
    assert!(!payload_needs_provenance_bind(&record_with_sources(
        sources
    )));

    let no_validators = LumaPayload {
        files: Vec::new(),
        main_addon_rel: "Luma-Game.addon".to_owned(),
        zip_digest: "zip-digest".to_owned(),
        etag: None,
        last_modified: None,
        build_number: None,
    };
    let mut sources = vec![advisory.clone()];
    mark_advisory_payload_source(&mut sources, &advisory, &no_validators);
    let marked = sources
        .iter()
        .find(|source| source.role() == TrackedSourceRole::AddonPayload)
        .expect("payload source");
    assert_eq!(marked.last_modified(), Some(ADVISORY_PAYLOAD_CHECKED_MARK));
    assert!(source_has_bind_mark(marked));
}

#[test]
fn payload_disk_intact_requires_main_addon_and_tracked_tree_files() {
    use std::fs;

    use tempfile::tempdir;

    let dir = tempdir().expect("tempdir");
    let addon = dir.path().join("Luma-Game.addon");
    let shader = dir.path().join("Luma/Global/Copy_PS.hlsl");
    let host = dir.path().join("dxgi.dll");
    fs::create_dir_all(shader.parent().expect("parent")).expect("mkdir");
    fs::write(&addon, b"addon").expect("write addon");
    fs::write(&shader, b"shader").expect("write shader");
    fs::write(&host, b"host").expect("write host");

    let record = InstalledAddon::new(
        GameId::new("steam:1").expect("id"),
        AddonKind::Luma,
        path(&addon.to_string_lossy()),
    )
    .with_created_file(path(&shader.to_string_lossy()))
    .with_created_file(path(&host.to_string_lossy()));

    assert!(payload_disk_intact(&record));

    fs::remove_file(&shader).expect("remove shader");
    assert!(
        !payload_disk_intact(&record),
        "missing tracked payload under Luma/** must force reconverge"
    );

    fs::write(&shader, b"shader").expect("restore shader");
    fs::remove_file(&host).expect("remove host");
    assert!(
        payload_disk_intact(&record),
        "missing host proxy is not a payload integrity failure"
    );

    let d3d9 = dir.path().join("D3D9.dll");
    fs::write(&d3d9, b"wrapper").expect("write d3d9");
    let record = record.with_created_file(path(&d3d9.to_string_lossy()));
    fs::remove_file(&d3d9).expect("remove d3d9");
    assert!(
        payload_disk_intact(&record),
        "missing managed dgVoodoo wrapper is not a payload integrity failure"
    );

    fs::remove_file(&addon).expect("remove addon");
    assert!(!payload_disk_intact(&record));
}

#[test]
fn payload_disk_intact_rejects_directory_at_tracked_path() {
    use std::fs;

    use tempfile::tempdir;

    let dir = tempdir().expect("tempdir");
    let addon = dir.path().join("Luma-Game.addon");
    let shader = dir.path().join("Luma/Global/Copy_PS.hlsl");
    fs::create_dir_all(shader.parent().expect("parent")).expect("mkdir");
    fs::write(&addon, b"addon").expect("write addon");
    // Directory where a file should be -> not intact (is_file() is false).
    fs::create_dir(&shader).expect("dir as shader");

    let record = InstalledAddon::new(
        GameId::new("steam:1").expect("id"),
        AddonKind::Luma,
        path(&addon.to_string_lossy()),
    )
    .with_created_file(path(&shader.to_string_lossy()));

    assert!(!payload_disk_intact(&record));
}
