use super::*;

#[test]
fn detector_scans_intel_xell_runtime_files_from_disk() {
    let folder = temp_dlss_folder(b"intel-xell");
    let game = game_installation(&folder);
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");

    fs::rename(folder.join(TEMP_DLSS_NAME), folder.join("libxell.dll"))
        .expect("fixture should rename to XeLL dll");
    fs::write(folder.join("libxell_dx11.dll"), b"intel-xell-dx11")
        .expect("XeLL dx11 dll should be written");

    let libraries = detector
        .detect_library_files(&game)
        .expect("detection should succeed");

    assert_detects(&libraries, "libxell.dll", LibraryTechnology::IntelXeLl);
    assert_detects(&libraries, "libxell_dx11.dll", LibraryTechnology::IntelXeLl);
}

#[test]
fn detector_scans_amd_denoiser_loader_and_upscaler_runtime_files_from_disk() {
    let folder = temp_dlss_folder(b"amd-native-fsr4");
    let game = game_installation(&folder);
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");

    fs::remove_file(folder.join(TEMP_DLSS_NAME)).expect("temporary dlss file should be removed");
    fs::write(
        folder.join("amd_fidelityfx_denoiser_dx12.dll"),
        b"amd-denoiser",
    )
    .expect("denoiser dll should be written");
    fs::write(folder.join("amd_fidelityfx_loader_dx12.dll"), b"amd-loader")
        .expect("loader dll should be written");
    fs::write(
        folder.join("amd_fidelityfx_upscaler_dx12.dll"),
        b"amd-upscaler-dx12",
    )
    .expect("upscaler dx12 dll should be written");
    fs::write(
        folder.join("amd_fidelityfx_framegeneration_dx12.dll"),
        b"amd-framegeneration-dx12",
    )
    .expect("frame generation dx12 dll should be written");

    let libraries = detector
        .detect_library_files(&game)
        .expect("detection should succeed");

    assert_detects(
        &libraries,
        "amd_fidelityfx_denoiser_dx12.dll",
        LibraryTechnology::AmdFsrRayRegeneration,
    );
    assert_detects(
        &libraries,
        "amd_fidelityfx_loader_dx12.dll",
        LibraryTechnology::AmdFsrLoader,
    );
    assert_detects(
        &libraries,
        "amd_fidelityfx_upscaler_dx12.dll",
        LibraryTechnology::AmdFsrUpscaler,
    );
    assert_detects(
        &libraries,
        "amd_fidelityfx_framegeneration_dx12.dll",
        LibraryTechnology::AmdFsrFrameGeneration,
    );

    let components = group_into_components(&game, &libraries).expect("grouping should succeed");
    assert_eq!(
        components.len(),
        4,
        "native FSR 4 keeps one component per DLL"
    );
    for technology in [
        LibraryTechnology::AmdFsrUpscaler,
        LibraryTechnology::AmdFsrLoader,
        LibraryTechnology::AmdFsrFrameGeneration,
        LibraryTechnology::AmdFsrRayRegeneration,
    ] {
        let component = components
            .iter()
            .find(|component| component.technology() == technology)
            .expect("expected native FSR 4 component");
        assert_eq!(
            component.files().len(),
            1,
            "native FSR 4 components stay single-file"
        );
        assert_eq!(
            component.swappability(),
            Swappability::Swappable,
            "native FSR 4 per-DLL components should remain independently swappable"
        );
    }

    let artifacts = group_into_artifacts(game.id(), &libraries).expect("artifact grouping");
    assert_eq!(
        artifacts.len(),
        4,
        "native FSR 4 keeps one artifact per DLL"
    );
    assert!(artifacts.iter().all(|artifact| artifact.files().len() == 1));
}

#[test]
fn detector_keeps_dx12_lineage_fsr_cohesive() {
    let folder = temp_dlss_folder(b"amd-dx12-lineage");
    let game = game_installation(&folder);
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");

    fs::remove_file(folder.join(TEMP_DLSS_NAME)).expect("temporary dlss file should be removed");
    fs::write(folder.join("amd_fidelityfx_dx12.dll"), b"amd-dx12")
        .expect("dx12 dll should be written");
    fs::write(
        folder.join("amd_fidelityfx_upscaler_dx12.dll"),
        b"amd-upscaler-dx12",
    )
    .expect("upscaler dx12 dll should be written");

    let libraries = detector
        .detect_library_files(&game)
        .expect("detection should succeed");

    assert_detects(
        &libraries,
        "amd_fidelityfx_dx12.dll",
        LibraryTechnology::AmdFsr,
    );
    assert_detects(
        &libraries,
        "amd_fidelityfx_upscaler_dx12.dll",
        LibraryTechnology::AmdFsrUpscaler,
    );

    let components = group_into_components(&game, &libraries).expect("grouping should succeed");
    let fsr = components
        .iter()
        .find(|component| component.technology() == LibraryTechnology::AmdFsr)
        .expect("expected cohesive FSR component");

    assert_eq!(components.len(), 1, "dx12-lineage FSR stays cohesive");
    assert_eq!(fsr.files().len(), 2);
    assert_eq!(fsr.swappability(), Swappability::BundleOnly);

    // Neither file carries a PE version (garbage bytes), so release cohesion
    // cannot be proven — the entry point the game loads is the representative,
    // not the possibly-leftover upscaler.
    assert_eq!(
        fsr.files()[0].path().file_name(),
        Some("amd_fidelityfx_dx12.dll"),
        "without proven cohesion the entry point is files()[0]"
    );
}

#[test]
fn detector_keeps_unified_fsr31_as_one_component() {
    let folder = temp_dlss_folder(b"amd-fsr31");
    let game = game_installation(&folder);
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");

    fs::rename(
        folder.join(TEMP_DLSS_NAME),
        folder.join("amd_fidelityfx_dx12.dll"),
    )
    .expect("temporary dlss file should rename to unified fsr dll");

    let libraries = detector
        .detect_library_files(&game)
        .expect("detection should succeed");

    assert_detects(
        &libraries,
        "amd_fidelityfx_dx12.dll",
        LibraryTechnology::AmdFsr,
    );

    let components = group_into_components(&game, &libraries).expect("grouping should succeed");
    let fsr = components
        .iter()
        .find(|component| component.technology() == LibraryTechnology::AmdFsr)
        .expect("expected unified FSR component");

    assert_eq!(components.len(), 1, "pure FSR 3.1 stays one component");
    assert_eq!(fsr.files().len(), 1);
    assert_eq!(fsr.swappability(), Swappability::Swappable);
}

#[test]
fn detector_scans_amd_radiance_cache_runtime_file_from_disk() {
    let folder = temp_dlss_folder(b"amd-radiance-cache");
    let game = game_installation(&folder);
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");

    fs::rename(
        folder.join(TEMP_DLSS_NAME),
        folder.join("amd_fidelityfx_radiancecache_dx12.dll"),
    )
    .expect("temporary dlss file should rename to radiance cache dll");

    let libraries = detector
        .detect_library_files(&game)
        .expect("detection should succeed");

    assert_detects(
        &libraries,
        "amd_fidelityfx_radiancecache_dx12.dll",
        LibraryTechnology::AmdFsrRadianceCache,
    );
}

#[test]
fn default_detector_depth_finds_deeply_nested_nvidia_runtime_dlls() {
    let root = temp_dlss_folder(b"root");
    let nested = root
        .join("Engine")
        .join("Plugins")
        .join("Runtime")
        .join("Nvidia")
        .join("DLSS")
        .join("Binaries")
        .join("ThirdParty")
        .join("Win64");
    fs::create_dir_all(&nested).expect("nested runtime path should be created");
    fs::write(nested.join("nvngx_dlss.dll"), b"deep-nvidia").expect("deep nvidia dll");

    let game = game_installation(&root);
    let detector = LibraryPatternComponentDetector::windows_default().expect("valid patterns");
    let libraries = detector
        .detect_library_files(&game)
        .expect("deep detection should succeed");

    assert!(libraries.iter().any(|library| {
        library.file_name() == "nvngx_dlss.dll"
            && library.technology() == LibraryTechnology::DlssSuperResolution
    }));
}
