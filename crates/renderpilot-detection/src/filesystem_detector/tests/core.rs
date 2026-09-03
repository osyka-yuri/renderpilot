use super::*;

#[test]
fn fixture_detects_known_graphics_libraries() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(&fixture_path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("fixture detection should succeed");

    assert_detects(
        &libraries,
        "nvngx_dlss.dll",
        LibraryTechnology::DlssSuperResolution,
    );
    assert_detects(
        &libraries,
        "nvngx_dlssg.dll",
        LibraryTechnology::DlssFrameGeneration,
    );
    assert_detects(
        &libraries,
        "nvngx_dlssd.dll",
        LibraryTechnology::DlssRayReconstruction,
    );
    assert_detects(
        &libraries,
        "sl.interposer.dll",
        LibraryTechnology::NvidiaStreamline,
    );
    assert_detects(&libraries, "libxess.dll", LibraryTechnology::IntelXeSs);
}

#[test]
fn fixture_does_not_detect_garbage_dlls() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(&fixture_path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("fixture detection should succeed");

    assert!(
        !libraries
            .iter()
            .any(|library| library.file_name() == "random.dll")
    );
    assert!(
        !libraries
            .iter()
            .any(|library| library.file_name() == "not_a_graphics.dll")
    );
}

#[test]
fn full_scan_does_not_hide_game_owned_system_named_directories() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(&fixture_path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("fixture detection should succeed");

    assert!(
        libraries
            .iter()
            .any(|library| library.file_path().as_str().contains("/Windows/System32/")),
        "a directory name alone is not authority to exclude part of a confirmed install tree",
    );
}

#[test]
fn bounded_full_detection_reports_incomplete_instead_of_publishing_partial_state() {
    let detector = LibraryPatternComponentDetector::windows_default()
        .expect("valid patterns")
        .with_max_depth(1);
    let game = game_installation(&fixture_path());
    let error = detector
        .detect_library_files(&game)
        .expect_err("a bounded authoritative scan must fail closed");

    assert!(
        error
            .message()
            .contains("installation scan was incomplete; catalog state was preserved"),
    );
}

#[derive(Clone)]
struct ProbeOnlyObservationSource {
    identity: StrongFileIdentity,
    full_reads: Arc<AtomicUsize>,
}

impl FileObservationSource for ProbeOnlyObservationSource {
    fn observe(&self, _path: &Path) -> AppResult<FileObservationResult> {
        self.full_reads.fetch_add(1, Ordering::SeqCst);
        Ok(FileObservationResult::Unavailable)
    }

    fn probe_identity(&self, _path: &Path) -> AppResult<FileIdentityProbeResult> {
        Ok(FileIdentityProbeResult::Available(self.identity.clone()))
    }
}

#[test]
fn exact_strong_reuse_avoids_content_observation() {
    let folder = temp_dlss_folder(b"reused bytes");
    let game = game_installation(&folder);
    let path = PathRef::new(folder.join(TEMP_DLSS_NAME).to_string_lossy().as_ref()).expect("path");
    let identity = StrongFileIdentity {
        kind: "test_identity".to_owned(),
        object_identity: "object-1".to_owned(),
        change_token: "change-1".to_owned(),
        size: 12,
    };
    let full_reads = Arc::new(AtomicUsize::new(0));
    let detector = LibraryPatternComponentDetector::windows_default()
        .expect("patterns")
        .with_file_observation_source(Arc::new(ProbeOnlyObservationSource {
            identity: identity.clone(),
            full_reads: Arc::clone(&full_reads),
        }));
    let mut reusable = HashMap::new();
    reusable.insert(
        path.as_str().to_owned(),
        ReusableFileMetadata {
            observation: FileObservation {
                path,
                identity_kind: identity.kind,
                object_identity: identity.object_identity,
                change_token: identity.change_token,
                size: identity.size,
                sha256: sha256_bytes(b"reused bytes").expect("sha"),
            },
            version: None,
            runtime_target: None,
            pe_compatibility: None,
        },
    );

    let detected = detector
        .detect_library_files_with_reuse(&game, &reusable)
        .expect("strong reuse");

    assert_eq!(detected.len(), 1);
    assert_eq!(full_reads.load(Ordering::SeqCst), 0);
}

#[derive(Clone)]
struct FallbackObservationSource {
    probe_identity: StrongFileIdentity,
    snapshot: StableFileSnapshot,
    full_reads: Arc<AtomicUsize>,
}

impl FileObservationSource for FallbackObservationSource {
    fn observe(&self, _path: &Path) -> AppResult<FileObservationResult> {
        self.full_reads.fetch_add(1, Ordering::SeqCst);
        Ok(FileObservationResult::Available(self.snapshot.clone()))
    }

    fn probe_identity(&self, _path: &Path) -> AppResult<FileIdentityProbeResult> {
        Ok(FileIdentityProbeResult::Available(
            self.probe_identity.clone(),
        ))
    }
}

#[test]
fn identity_mismatch_performs_exactly_one_full_observation() {
    let bytes = b"replacement bytes";
    let folder = temp_dlss_folder(bytes);
    let game = game_installation(&folder);
    let path = PathRef::new(folder.join(TEMP_DLSS_NAME).to_string_lossy().as_ref()).expect("path");
    let reusable_identity = StrongFileIdentity {
        kind: "test_identity".to_owned(),
        object_identity: "old-object".to_owned(),
        change_token: "old-change".to_owned(),
        size: 12,
    };
    let current_identity = StrongFileIdentity {
        kind: "test_identity".to_owned(),
        object_identity: "new-object".to_owned(),
        change_token: "new-change".to_owned(),
        size: u64::try_from(bytes.len()).expect("fixture size"),
    };
    let full_reads = Arc::new(AtomicUsize::new(0));
    let detector = LibraryPatternComponentDetector::windows_default()
        .expect("patterns")
        .with_file_observation_source(Arc::new(FallbackObservationSource {
            probe_identity: current_identity.clone(),
            snapshot: StableFileSnapshot {
                cache_key: Some(current_identity),
                sha256: sha256_bytes(bytes).expect("sha"),
                bytes: bytes.to_vec(),
            },
            full_reads: Arc::clone(&full_reads),
        }));
    let reusable = HashMap::from([(
        path.as_str().to_owned(),
        ReusableFileMetadata {
            observation: FileObservation {
                path,
                identity_kind: reusable_identity.kind,
                object_identity: reusable_identity.object_identity,
                change_token: reusable_identity.change_token,
                size: reusable_identity.size,
                sha256: sha256_bytes(b"old bytes").expect("sha"),
            },
            version: None,
            runtime_target: None,
            pe_compatibility: None,
        },
    )]);

    let detected = detector
        .detect_library_files_with_reuse(&game, &reusable)
        .expect("mismatch falls back to a full observation");

    assert_eq!(detected.len(), 1);
    assert_eq!(full_reads.load(Ordering::SeqCst), 1);
    assert_eq!(
        detected[0]
            .observation()
            .expect("a reusable fake snapshot has an observation")
            .object_identity,
        "new-object",
        "the fresh stable object, not the stale cache identity, must be persisted"
    );
}

#[derive(Clone)]
struct UncacheableObservationSource {
    bytes: Vec<u8>,
    full_reads: Arc<AtomicUsize>,
}

impl FileObservationSource for UncacheableObservationSource {
    fn observe(&self, _path: &Path) -> AppResult<FileObservationResult> {
        self.full_reads.fetch_add(1, Ordering::SeqCst);
        Ok(FileObservationResult::Available(StableFileSnapshot {
            cache_key: None,
            sha256: sha256_bytes(&self.bytes).expect("hash"),
            bytes: self.bytes.clone(),
        }))
    }

    fn probe_identity(&self, _path: &Path) -> AppResult<FileIdentityProbeResult> {
        Ok(FileIdentityProbeResult::Uncacheable)
    }
}

#[test]
fn uncacheable_snapshot_is_detected_with_one_full_read_and_no_reusable_observation() {
    let bytes = b"uncacheable replacement";
    let folder = temp_dlss_folder(bytes);
    let game = game_installation(&folder);
    let full_reads = Arc::new(AtomicUsize::new(0));
    let detector = LibraryPatternComponentDetector::windows_default()
        .expect("patterns")
        .with_file_observation_source(Arc::new(UncacheableObservationSource {
            bytes: bytes.to_vec(),
            full_reads: Arc::clone(&full_reads),
        }));

    let detected = detector
        .detect_library_files_with_reuse(&game, &HashMap::new())
        .expect("a full uncacheable snapshot remains a successful detection");

    assert_eq!(detected.len(), 1);
    assert_eq!(full_reads.load(Ordering::SeqCst), 1);
    assert!(detected[0].observation().is_none());
    assert_eq!(detected[0].sha256(), &sha256_bytes(bytes).expect("hash"));
}

#[test]
fn streamline_is_bundle_only() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(&fixture_path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("fixture detection should succeed");
    let streamline = libraries
        .iter()
        .find(|library| library.file_name() == "sl.interposer.dll")
        .expect("streamline should be detected");

    assert_eq!(streamline.swappability(), Swappability::BundleOnly);
}

#[test]
fn detected_files_include_hash_unknown_version_status_and_strong_observation() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(&fixture_path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("fixture detection should succeed");
    let dlss = libraries
        .iter()
        .find(|library| library.file_name() == "nvngx_dlss.dll")
        .expect("DLSS should be detected");

    assert_eq!(dlss.sha256().as_str(), FIXTURE_NEWLINE_FILE_SHA256);
    assert_eq!(dlss.version(), None);
    assert_eq!(dlss.status(), VersionDetectionStatus::UnknownVersion);
    if let Some(observation) = dlss.observation() {
        assert_eq!(observation.path, *dlss.file_path());
        assert_eq!(observation.size, 1);
        assert_eq!(observation.sha256.as_str(), FIXTURE_NEWLINE_FILE_SHA256);
    }
}

#[test]
fn component_detector_trait_maps_detected_files_to_domain_components() {
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let game = game_installation(&fixture_path());
    let components = detector
        .detect_components(&game)
        .expect("component detection should succeed");

    assert!(
        components
            .iter()
            .any(|component| component.technology() == LibraryTechnology::DlssSuperResolution)
    );
    assert!(
        components
            .iter()
            .any(|component| component.files().iter().any(|file| file.sha256().is_some()))
    );
}

#[test]
fn openvr_dlls_in_different_subfolders_stay_independent_and_fail_closed() {
    let root = tempfile::tempdir().expect("root");
    let x86 = root.path().join("bin").join("win32");
    let x64 = root.path().join("bin").join("win64");
    fs::create_dir_all(&x86).expect("x86 dir");
    fs::create_dir_all(&x64).expect("x64 dir");
    fs::write(x86.join("openvr_api.dll"), b"malformed-x86").expect("x86 DLL");
    fs::write(x64.join("openvr_api.dll"), b"malformed-x64").expect("x64 DLL");

    let detector = LibraryPatternComponentDetector::windows_default().expect("patterns");
    let game = game_installation(root.path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("OpenVR detection");
    let components = group_into_components(&game, &libraries).expect("grouping");
    let openvr = components
        .iter()
        .filter(|component| component.technology() == LibraryTechnology::OpenVr)
        .collect::<Vec<_>>();

    assert_eq!(openvr.len(), 2);
    assert!(openvr.iter().all(|component| component.files().len() == 1));
    assert!(
        openvr
            .iter()
            .all(|component| { component.files()[0].pe_compatibility().is_none() })
    );
}

#[test]
fn malformed_xiph_files_are_not_grouped_by_naming_convention_alone() {
    let root = tempfile::tempdir().expect("root");
    for name in ["vorbis.dll", "ogg.dll", "libvorbis.dll", "libogg.dll"] {
        fs::write(root.path().join(name), format!("malformed-{name}")).expect("fixture DLL");
    }

    let detector = LibraryPatternComponentDetector::windows_default().expect("patterns");
    let game = game_installation(root.path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("Xiph detection");
    let components = group_into_components(&game, &libraries).expect("grouping");
    let xiph = components
        .iter()
        .filter(|component| component.technology() == LibraryTechnology::XiphVorbis)
        .collect::<Vec<_>>();

    assert_eq!(xiph.len(), 4);
    assert!(xiph.iter().all(|component| {
        component.files().len() == 1 && component.swappability() == Swappability::ReadOnly
    }));
    let ids = xiph
        .iter()
        .map(|component| component.id().as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), 4);
}

#[test]
fn vendor_xiph_runtime_names_are_detected_but_non_pe_closures_stay_read_only() {
    let root = tempfile::tempdir().expect("root");
    for name in [
        "vorbisfile_vs2010_x64_rwdi.dll",
        "vorbis_vs2010_x64_rwdi.dll",
        "ogg_vs2010_x64_rwdi.dll",
    ] {
        fs::write(root.path().join(name), b"not-a-pe").expect("vendor fixture");
    }

    let detector = LibraryPatternComponentDetector::windows_default().expect("patterns");
    let game = game_installation(root.path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("vendor Xiph name detection");
    let components = group_into_components(&game, &libraries).expect("grouping");
    let xiph = components
        .iter()
        .filter(|component| component.technology() == LibraryTechnology::XiphVorbis)
        .collect::<Vec<_>>();

    assert_eq!(libraries.len(), 3);
    assert!(
        libraries
            .iter()
            .all(|library| library.technology() == LibraryTechnology::XiphVorbis)
    );
    assert_eq!(xiph.len(), 1);
    assert_eq!(xiph[0].swappability(), Swappability::ReadOnly);
}

#[test]
fn malformed_vendor_xiph_runtime_names_are_not_detected() {
    let root = tempfile::tempdir().expect("root");
    for name in [
        "vorbis__x64.dll",
        "vorbis_vs2010_x64_rwdi..dll",
        "ogg_vs2010_x64_rwdi-.dll",
    ] {
        fs::write(root.path().join(name), b"not-a-pe").expect("malformed vendor fixture");
    }

    let detector = LibraryPatternComponentDetector::windows_default().expect("patterns");
    let game = game_installation(root.path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("malformed names do not abort scan");

    assert!(libraries.is_empty());
}

#[test]
fn xiph_singletons_fail_closed_without_cross_family_grouping() {
    let root = tempfile::tempdir().expect("root");
    fs::write(root.path().join("ogg.dll"), b"malformed-ogg").expect("Ogg fixture");
    fs::write(root.path().join("libvorbis.dll"), b"malformed-vorbis").expect("Vorbis fixture");

    let detector = LibraryPatternComponentDetector::windows_default().expect("patterns");
    let game = game_installation(root.path());
    let libraries = detector
        .detect_library_files(&game)
        .expect("Xiph detection");
    let components = group_into_components(&game, &libraries).expect("grouping");
    let xiph = components
        .iter()
        .filter(|component| component.technology() == LibraryTechnology::XiphVorbis)
        .collect::<Vec<_>>();

    assert_eq!(xiph.len(), 2);
    let ogg = xiph
        .iter()
        .find(|component| component.files()[0].path().file_name() == Some("ogg.dll"))
        .expect("plain Ogg singleton");
    let vorbis = xiph
        .iter()
        .find(|component| component.files()[0].path().file_name() == Some("libvorbis.dll"))
        .expect("lib Vorbis singleton");
    assert_eq!(ogg.swappability(), Swappability::ReadOnly);
    assert_eq!(vorbis.swappability(), Swappability::ReadOnly);
}
