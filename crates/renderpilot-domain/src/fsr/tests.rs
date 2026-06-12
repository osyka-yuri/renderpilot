use crate::{ComponentFile, PathRef, Version};

use super::*;

fn file(path: &str) -> ComponentFile {
    ComponentFile::new(PathRef::new(path).expect("path should be valid"))
}

fn artifact_file(path: &str, install_as: Option<&str>) -> ComponentFile {
    let file_ = file(path);

    match install_as {
        Some(install_as) => file_.with_install_as(install_as),
        None => file_,
    }
}

fn versioned_file(path: &str, version: &str) -> ComponentFile {
    file(path).with_version(Version::parse(version).expect("version should be valid"))
}

fn file_names(files: &[ComponentFile]) -> Vec<&str> {
    files
        .iter()
        .filter_map(|file| file.path().file_name())
        .collect()
}

#[test]
fn recognizes_split_members_and_marker() {
    assert!(naming::is_split_member("amd_fidelityfx_loader_dx12.dll"));
    assert!(naming::is_split_member("AMD_FIDELITYFX_UPSCALER_DX12.DLL")); // case-insensitive
    assert!(naming::is_split_member(
        "amd_fidelityfx_framegeneration_dx12.dll"
    ));
    // Optional effects are package-only members too.
    assert!(naming::is_split_member("amd_fidelityfx_denoiser_dx12.dll"));
    assert!(naming::is_split_member(
        "amd_fidelityfx_radiancecache_dx12.dll"
    ));
    // The installed entry point is not itself a packaged member name.
    assert!(!naming::is_split_member("amd_fidelityfx_dx12.dll"));

    assert!(naming::is_split_marker("amd_fidelityfx_upscaler_dx12.dll"));
    assert!(!naming::is_split_marker("amd_fidelityfx_loader_dx12.dll"));
    assert!(!naming::is_split_marker("amd_fidelityfx_dx12.dll"));

    // VK variants mirror DX12.
    assert!(naming::is_split_member("amd_fidelityfx_loader_vk.dll"));
    assert!(naming::is_split_member("AMD_FIDELITYFX_UPSCALER_VK.DLL")); // case-insensitive
    assert!(naming::is_split_member(
        "amd_fidelityfx_framegeneration_vk.dll"
    ));
    assert!(naming::is_split_member("amd_fidelityfx_denoiser_vk.dll"));
    assert!(naming::is_split_member(
        "amd_fidelityfx_radiancecache_vk.dll"
    ));
    assert!(!naming::is_split_member("amd_fidelityfx_vk.dll")); // entry point is not a member

    assert!(naming::is_split_marker("amd_fidelityfx_upscaler_vk.dll"));
    assert!(!naming::is_split_marker("amd_fidelityfx_loader_vk.dll"));
    assert!(!naming::is_split_marker("amd_fidelityfx_vk.dll"));
}

#[test]
fn optional_effects_are_the_denoiser_and_radiance_cache_only() {
    assert!(naming::is_optional_effect(
        "amd_fidelityfx_denoiser_dx12.dll"
    ));
    assert!(naming::is_optional_effect(
        "AMD_FIDELITYFX_RADIANCECACHE_DX12.DLL"
    )); // case-insensitive
        // Core members and the entry point are never optional.
    assert!(!naming::is_optional_effect(
        "amd_fidelityfx_loader_dx12.dll"
    ));
    assert!(!naming::is_optional_effect(
        "amd_fidelityfx_upscaler_dx12.dll"
    ));
    assert!(!naming::is_optional_effect(
        "amd_fidelityfx_framegeneration_dx12.dll"
    ));
    assert!(!naming::is_optional_effect("amd_fidelityfx_dx12.dll"));

    // VK variants mirror DX12.
    assert!(naming::is_optional_effect("amd_fidelityfx_denoiser_vk.dll"));
    assert!(naming::is_optional_effect(
        "AMD_FIDELITYFX_RADIANCECACHE_VK.DLL"
    )); // case-insensitive
    assert!(!naming::is_optional_effect("amd_fidelityfx_loader_vk.dll"));
    assert!(!naming::is_optional_effect(
        "amd_fidelityfx_upscaler_vk.dll"
    ));
    assert!(!naming::is_optional_effect(
        "amd_fidelityfx_framegeneration_vk.dll"
    ));
    assert!(!naming::is_optional_effect("amd_fidelityfx_vk.dll"));
}

#[test]
fn is_split_set_is_marked_by_the_upscaler() {
    assert!(lineage::is_split_set(&[
        file("C:/game/amd_fidelityfx_dx12.dll"),
        file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
    ]));
    // Unified FSR 3.x: a single dx12 backend, no upscaler member.
    assert!(!lineage::is_split_set(&[file(
        "C:/game/amd_fidelityfx_dx12.dll"
    )]));

    assert!(lineage::is_split_set(&[
        file("C:/game/amd_fidelityfx_vk.dll"),
        file("C:/game/amd_fidelityfx_upscaler_vk.dll"),
    ]));
    assert!(!lineage::is_split_set(&[file(
        "C:/game/amd_fidelityfx_vk.dll"
    )]));
}

#[test]
fn entry_point_distinguishes_dx12_lineage_from_native_fsr4() {
    // dx12-lineage: pure FSR 3.1, or one we upgraded (loader installed as dx12).
    let unified = [file("C:/game/amd_fidelityfx_dx12.dll")];
    let upgraded = [
        file("C:/game/amd_fidelityfx_dx12.dll"),
        file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
        file("C:/game/amd_fidelityfx_framegeneration_dx12.dll"),
    ];
    assert!(lineage::has_entry_point(&unified));
    assert!(lineage::has_entry_point(&upgraded));
    assert!(!lineage::is_native_fsr4(&unified));
    assert!(!lineage::is_native_fsr4(&upgraded));

    // Native FSR 4: loads its own loader, no dx12 entry point.
    let native = [
        file("C:/game/amd_fidelityfx_loader_dx12.dll"),
        file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
    ];
    assert!(!lineage::has_entry_point(&native));
    assert!(lineage::is_native_fsr4(&native));

    // VK-lineage: pure FSR 3.1 VK, or one we upgraded (loader installed as vk).
    let unified_vk = [file("C:/game/amd_fidelityfx_vk.dll")];
    let upgraded_vk = [
        file("C:/game/amd_fidelityfx_vk.dll"),
        file("C:/game/amd_fidelityfx_upscaler_vk.dll"),
        file("C:/game/amd_fidelityfx_framegeneration_vk.dll"),
    ];
    assert!(lineage::has_entry_point(&unified_vk));
    assert!(lineage::has_entry_point(&upgraded_vk));
    assert!(!lineage::is_native_fsr4(&unified_vk));
    assert!(!lineage::is_native_fsr4(&upgraded_vk));

    // Native FSR 4 VK: loads its own loader, no vk entry point.
    let native_vk = [
        file("C:/game/amd_fidelityfx_loader_vk.dll"),
        file("C:/game/amd_fidelityfx_upscaler_vk.dll"),
    ];
    assert!(!lineage::has_entry_point(&native_vk));
    assert!(lineage::is_native_fsr4(&native_vk));
}

#[test]
fn display_file_prefers_entry_point_when_present() {
    let cohesive = [
        file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
        file("C:/game/amd_fidelityfx_dx12.dll"),
        file("C:/game/amd_fidelityfx_framegeneration_dx12.dll"),
    ];
    assert_eq!(
        representative::display_component_file(&cohesive).and_then(|file| file.path().file_name()),
        Some("amd_fidelityfx_dx12.dll"),
    );

    let native = [
        file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
        file("C:/game/amd_fidelityfx_loader_dx12.dll"),
    ];
    assert_eq!(
        representative::display_component_file(&native).and_then(|file| file.path().file_name()),
        Some("amd_fidelityfx_upscaler_dx12.dll"),
    );

    let cohesive_vk = [
        file("C:/game/amd_fidelityfx_upscaler_vk.dll"),
        file("C:/game/amd_fidelityfx_vk.dll"),
        file("C:/game/amd_fidelityfx_framegeneration_vk.dll"),
    ];
    assert_eq!(
        representative::display_component_file(&cohesive_vk).and_then(|f| f.path().file_name()),
        Some("amd_fidelityfx_vk.dll"),
    );
}

#[test]
fn artifact_install_target_uses_install_as_and_component_lineage() {
    let loader = artifact_file(
        "C:/lib/amd_fidelityfx_loader_dx12.dll",
        Some("amd_fidelityfx_dx12.dll"),
    );
    let upscaler = artifact_file("C:/lib/amd_fidelityfx_upscaler_dx12.dll", None);

    let cohesive_component = [file("C:/game/amd_fidelityfx_dx12.dll")];
    assert_eq!(
        install_targets::resolve_artifact_install_target(&loader, &cohesive_component),
        "amd_fidelityfx_dx12.dll",
    );

    let native_component = [
        file("C:/game/amd_fidelityfx_loader_dx12.dll"),
        file("C:/game/amd_fidelityfx_upscaler_dx12.dll"),
    ];
    assert_eq!(
        install_targets::resolve_artifact_install_target(&loader, &native_component),
        "amd_fidelityfx_loader_dx12.dll",
    );
    assert_eq!(
        install_targets::resolve_artifact_install_target(&upscaler, &native_component),
        "amd_fidelityfx_upscaler_dx12.dll",
    );

    // VK variants mirror DX12.
    let loader_vk = artifact_file(
        "C:/lib/amd_fidelityfx_loader_vk.dll",
        Some("amd_fidelityfx_vk.dll"),
    );
    let upscaler_vk = artifact_file("C:/lib/amd_fidelityfx_upscaler_vk.dll", None);

    let cohesive_vk = [file("C:/game/amd_fidelityfx_vk.dll")];
    assert_eq!(
        install_targets::resolve_artifact_install_target(&loader_vk, &cohesive_vk),
        "amd_fidelityfx_vk.dll",
    );

    let native_vk = [
        file("C:/game/amd_fidelityfx_loader_vk.dll"),
        file("C:/game/amd_fidelityfx_upscaler_vk.dll"),
    ];
    assert_eq!(
        install_targets::resolve_artifact_install_target(&loader_vk, &native_vk),
        "amd_fidelityfx_loader_vk.dll",
    );
    assert_eq!(
        install_targets::resolve_artifact_install_target(&upscaler_vk, &native_vk),
        "amd_fidelityfx_upscaler_vk.dll",
    );
}

#[test]
fn entry_point_wins_over_a_separate_loader_stack() {
    // Mixed lineage — a real unified FSR 3.1 entry point next to a
    // loader+denoiser Ray Regeneration stack. The package loader (and the
    // unified backend alike) must replace the entry point the game loads for
    // upscaling, never the RR stack's loader.
    let mixed = [
        file("C:/game/amd_fidelityfx_dx12.dll"),
        file("C:/game/amd_fidelityfx_loader_dx12.dll"),
        file("C:/game/amd_fidelityfx_denoiser_dx12.dll"),
    ];

    let package_loader = artifact_file(
        "C:/lib/amd_fidelityfx_loader_dx12.dll",
        Some("amd_fidelityfx_dx12.dll"),
    );
    assert_eq!(
        install_targets::resolve_artifact_install_target(&package_loader, &mixed),
        "amd_fidelityfx_dx12.dll",
    );

    let unified_backend = artifact_file("C:/lib/amd_fidelityfx_dx12.dll", None);
    assert_eq!(
        install_targets::resolve_artifact_install_target(&unified_backend, &mixed),
        "amd_fidelityfx_dx12.dll",
    );

    // VK mirror.
    let mixed_vk = [
        file("C:/game/amd_fidelityfx_vk.dll"),
        file("C:/game/amd_fidelityfx_loader_vk.dll"),
        file("C:/game/amd_fidelityfx_denoiser_vk.dll"),
    ];
    let package_loader_vk = artifact_file(
        "C:/lib/amd_fidelityfx_loader_vk.dll",
        Some("amd_fidelityfx_vk.dll"),
    );
    assert_eq!(
        install_targets::resolve_artifact_install_target(&package_loader_vk, &mixed_vk),
        "amd_fidelityfx_vk.dll",
    );
    let unified_backend_vk = artifact_file("C:/lib/amd_fidelityfx_vk.dll", None);
    assert_eq!(
        install_targets::resolve_artifact_install_target(&unified_backend_vk, &mixed_vk),
        "amd_fidelityfx_vk.dll",
    );
}

#[test]
fn same_release_build_compares_the_last_segment() {
    let loader = Version::parse("2.1.0.604").expect("version");
    let upscaler = Version::parse("4.0.3.604").expect("version");
    let unified = Version::parse("1.0.1.41314").expect("version");

    assert!(lineage::same_release_build(&loader, &upscaler));
    assert!(!lineage::same_release_build(&unified, &upscaler));
    // The 3.1.4-era split release shares the 44888 build the same way.
    let loader_44888 = Version::parse("1.0.2.44888").expect("version");
    let upscaler_44888 = Version::parse("4.0.2.44888").expect("version");
    assert!(lineage::same_release_build(&loader_44888, &upscaler_44888));
}

#[test]
fn upscaler_represents_a_cohesive_set() {
    // Our 3.1 → 4 upgrade: the entry point IS the loader; builds match.
    let loader = Version::parse("2.1.0.604").expect("version");
    let upscaler = Version::parse("4.0.3.604").expect("version");
    let framegen = Version::parse("4.0.0.604").expect("version");
    assert!(lineage::upscaler_represents_set([
        ("amd_fidelityfx_dx12.dll", Some(&loader)),
        ("amd_fidelityfx_upscaler_dx12.dll", Some(&upscaler)),
        ("amd_fidelityfx_framegeneration_dx12.dll", Some(&framegen)),
    ]));

    // Native FSR 4: no entry point — the upscaler always represents.
    assert!(lineage::upscaler_represents_set([
        ("amd_fidelityfx_loader_dx12.dll", Some(&loader)),
        ("amd_fidelityfx_upscaler_dx12.dll", Some(&upscaler)),
    ]));

    // VK mirror of the upgraded set.
    assert!(lineage::upscaler_represents_set([
        ("amd_fidelityfx_vk.dll", Some(&loader)),
        ("amd_fidelityfx_upscaler_vk.dll", Some(&upscaler)),
    ]));
}

#[test]
fn entry_point_represents_when_the_set_is_not_cohesive() {
    let unified = Version::parse("1.0.1.41314").expect("version");
    let upscaler = Version::parse("4.0.3.604").expect("version");

    // A real unified FSR 3.1 next to a leftover upscaler: builds differ.
    assert!(!lineage::upscaler_represents_set([
        ("amd_fidelityfx_dx12.dll", Some(&unified)),
        ("amd_fidelityfx_upscaler_dx12.dll", Some(&upscaler)),
    ]));

    // Unknown versions cannot prove cohesion — the entry point wins.
    assert!(!lineage::upscaler_represents_set([
        ("amd_fidelityfx_dx12.dll", None),
        ("amd_fidelityfx_upscaler_dx12.dll", Some(&upscaler)),
    ]));
    assert!(!lineage::upscaler_represents_set([
        ("amd_fidelityfx_dx12.dll", Some(&unified)),
        ("amd_fidelityfx_upscaler_dx12.dll", None),
    ]));

    // No upscaler at all (unified FSR 3.1, or an entry point + RR stack).
    assert!(!lineage::upscaler_represents_set([(
        "amd_fidelityfx_dx12.dll",
        Some(&unified)
    ),]));
    let loader = Version::parse("2.1.0.604").expect("version");
    let denoiser = Version::parse("1.0.0.604").expect("version");
    assert!(!lineage::upscaler_represents_set([
        ("amd_fidelityfx_dx12.dll", Some(&unified)),
        ("amd_fidelityfx_loader_dx12.dll", Some(&loader)),
        ("amd_fidelityfx_denoiser_dx12.dll", Some(&denoiser)),
    ]));

    // A cross-API upscaler cannot vouch for this entry point.
    assert!(!lineage::upscaler_represents_set([
        ("amd_fidelityfx_dx12.dll", Some(&unified)),
        ("amd_fidelityfx_upscaler_vk.dll", Some(&upscaler)),
    ]));
}

#[test]
fn primary_rank_swaps_upscaler_and_entry_point_by_representation() {
    // Cohesive set: upscaler first, entry point second, members last.
    assert_eq!(
        representative::primary_rank("amd_fidelityfx_upscaler_dx12.dll", true),
        0
    );
    assert_eq!(
        representative::primary_rank("amd_fidelityfx_dx12.dll", true),
        1
    );
    assert_eq!(
        representative::primary_rank("amd_fidelityfx_loader_dx12.dll", true),
        2
    );

    // Non-cohesive set: the entry point the game loads comes first.
    assert_eq!(
        representative::primary_rank("amd_fidelityfx_dx12.dll", false),
        0
    );
    assert_eq!(
        representative::primary_rank("amd_fidelityfx_upscaler_dx12.dll", false),
        1
    );
    assert_eq!(
        representative::primary_rank("amd_fidelityfx_denoiser_dx12.dll", false),
        2
    );
}

#[test]
fn version_representative_follows_cohesion() {
    // Cohesive upgraded set → the upscaler carries the FSR version.
    let cohesive = [
        versioned_file("C:/game/amd_fidelityfx_dx12.dll", "2.1.0.604"),
        versioned_file("C:/game/amd_fidelityfx_upscaler_dx12.dll", "4.0.3.604"),
        versioned_file(
            "C:/game/amd_fidelityfx_framegeneration_dx12.dll",
            "4.0.0.604",
        ),
    ];
    assert_eq!(
        representative::version_representative(&cohesive).and_then(|f| f.path().file_name()),
        Some("amd_fidelityfx_upscaler_dx12.dll"),
    );

    // Real FSR 3.1 + leftover upscaler → the entry point represents.
    let mixed = [
        versioned_file("C:/game/amd_fidelityfx_upscaler_dx12.dll", "4.0.3.604"),
        versioned_file("C:/game/amd_fidelityfx_dx12.dll", "1.0.1.41314"),
    ];
    assert_eq!(
        representative::version_representative(&mixed).and_then(|f| f.path().file_name()),
        Some("amd_fidelityfx_dx12.dll"),
    );

    // Entry point + RR stack, regardless of stored file order → the entry point.
    let with_rr_stack = [
        versioned_file("C:/game/amd_fidelityfx_loader_dx12.dll", "2.1.0.604"),
        versioned_file("C:/game/amd_fidelityfx_denoiser_dx12.dll", "1.0.0.604"),
        versioned_file("C:/game/amd_fidelityfx_dx12.dll", "1.0.1.41314"),
    ];
    assert_eq!(
        representative::version_representative(&with_rr_stack).and_then(|f| f.path().file_name()),
        Some("amd_fidelityfx_dx12.dll"),
    );

    // Native FSR 4 single-file components keep their own file.
    let native_upscaler = [versioned_file(
        "C:/game/amd_fidelityfx_upscaler_dx12.dll",
        "4.0.3.604",
    )];
    assert_eq!(
        representative::version_representative(&native_upscaler).and_then(|f| f.path().file_name()),
        Some("amd_fidelityfx_upscaler_dx12.dll"),
    );

    // Non-FSR sets fall through to the first file.
    let dlss = [versioned_file("C:/game/nvngx_dlss.dll", "3.7.0")];
    assert_eq!(
        representative::version_representative(&dlss).and_then(|f| f.path().file_name()),
        Some("nvngx_dlss.dll"),
    );
}

#[test]
fn sort_representative_first_puts_the_cohesive_upscaler_first() {
    let mut files = vec![
        versioned_file(
            "C:/game/amd_fidelityfx_framegeneration_dx12.dll",
            "4.0.0.604",
        ),
        versioned_file("C:/game/amd_fidelityfx_dx12.dll", "2.1.0.604"),
        versioned_file("C:/game/amd_fidelityfx_upscaler_dx12.dll", "4.0.3.604"),
    ];

    representative::sort_representative_first(&mut files);

    assert_eq!(
        file_names(&files),
        vec![
            "amd_fidelityfx_upscaler_dx12.dll",
            "amd_fidelityfx_dx12.dll",
            "amd_fidelityfx_framegeneration_dx12.dll",
        ],
    );
}

#[test]
fn sort_representative_first_puts_the_entry_point_before_an_rr_stack() {
    // The order additive_active_files-style rebuilds produce: a kept RR stack
    // in front of the entry point — the sort must put the entry point first.
    let mut files = vec![
        versioned_file("C:/game/amd_fidelityfx_denoiser_dx12.dll", "1.0.0.604"),
        versioned_file("C:/game/amd_fidelityfx_loader_dx12.dll", "2.1.0.604"),
        versioned_file("C:/game/amd_fidelityfx_dx12.dll", "1.0.1.41314"),
    ];

    representative::sort_representative_first(&mut files);

    assert_eq!(
        file_names(&files),
        vec![
            "amd_fidelityfx_dx12.dll",
            "amd_fidelityfx_denoiser_dx12.dll",
            "amd_fidelityfx_loader_dx12.dll",
        ],
    );
}

#[test]
fn sort_representative_first_leaves_non_fsr_sets_untouched() {
    let mut files = vec![
        versioned_file("C:/game/nvngx_dlssg.dll", "3.7.0"),
        versioned_file("C:/game/nvngx_dlss.dll", "3.7.0"),
    ];

    representative::sort_representative_first(&mut files);

    assert_eq!(
        file_names(&files),
        vec!["nvngx_dlssg.dll", "nvngx_dlss.dll"],
        "non-FSR sets keep their given order",
    );
}

#[test]
fn api_predicates_match_suffix_not_infix() {
    assert_eq!(
        naming::fsr_graphics_api("amd_fidelityfx_vk.dll"),
        Some(naming::FsrApi::Vulkan)
    );
    assert_eq!(
        naming::fsr_graphics_api("amd_fidelityfx_loader_vk.dll"),
        Some(naming::FsrApi::Vulkan)
    );
    assert_eq!(
        naming::fsr_graphics_api("AMD_FIDELITYFX_UPSCALER_VK.DLL"),
        Some(naming::FsrApi::Vulkan)
    ); // case-insensitive
    assert_ne!(
        naming::fsr_graphics_api("amd_fidelityfx_dx12.dll"),
        Some(naming::FsrApi::Vulkan)
    );
    assert_eq!(naming::fsr_graphics_api("nvidia_vk_compat.dll"), None); // infix must NOT match

    assert_eq!(
        naming::fsr_graphics_api("amd_fidelityfx_dx12.dll"),
        Some(naming::FsrApi::Dx12)
    );
    assert_eq!(
        naming::fsr_graphics_api("amd_fidelityfx_loader_dx12.dll"),
        Some(naming::FsrApi::Dx12)
    );
    assert_eq!(
        naming::fsr_graphics_api("AMD_FIDELITYFX_UPSCALER_DX12.DLL"),
        Some(naming::FsrApi::Dx12)
    ); // case-insensitive
    assert_ne!(
        naming::fsr_graphics_api("amd_fidelityfx_vk.dll"),
        Some(naming::FsrApi::Dx12)
    );
    assert_eq!(naming::fsr_graphics_api("some_dx12_shim_extra.dll"), None); // infix must NOT match
}
