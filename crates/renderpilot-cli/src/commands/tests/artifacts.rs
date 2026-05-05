use std::{ffi::OsString, fs};

use crate::run;

use super::{CatalogEnvironmentGuard, TempGameFolder, args, temp_db_path};

#[test]
fn list_artifacts_groups_artifacts_from_multiple_scans() {
    let _catalog = CatalogEnvironmentGuard::new(temp_db_path("list-artifacts-multi"));
    let dlss_folder = TempGameFolder::new("cli-artifacts-dlss");
    let xess_folder = TempGameFolder::new("cli-artifacts-xess");

    fs::create_dir_all(dlss_folder.path()).expect("temp folder should be created");
    fs::create_dir_all(xess_folder.path()).expect("temp folder should be created");
    fs::write(dlss_folder.path().join("nvngx_dlss.dll"), b"dlss-a")
        .expect("dlss file should be written");
    fs::write(xess_folder.path().join("libxess.dll"), b"xess-b")
        .expect("xess file should be written");

    run(vec![
        OsString::from("scan-folder"),
        dlss_folder.path().as_os_str().to_owned(),
    ])
    .expect("first scan should succeed");
    run(vec![
        OsString::from("scan-folder"),
        xess_folder.path().as_os_str().to_owned(),
    ])
    .expect("second scan should succeed");

    let output = run(args(&["list-artifacts"])).expect("artifact list should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");
    let groups = json["groups"].as_array().expect("groups array");

    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["technology"], "dlss_super_resolution");
    assert_eq!(groups[0]["artifacts"][0]["file_name"], "nvngx_dlss.dll");
    assert_eq!(groups[1]["technology"], "intel_xess");
    assert_eq!(groups[1]["artifacts"][0]["file_name"], "libxess.dll");
}

#[test]
fn list_artifacts_filters_by_technology() {
    let _catalog = CatalogEnvironmentGuard::new(temp_db_path("list-artifacts-filter"));
    let dlss_folder = TempGameFolder::new("cli-filter-dlss");
    let fg_folder = TempGameFolder::new("cli-filter-fg");

    fs::create_dir_all(dlss_folder.path()).expect("temp folder should be created");
    fs::create_dir_all(fg_folder.path()).expect("temp folder should be created");
    fs::write(dlss_folder.path().join("nvngx_dlss.dll"), b"dlss-a")
        .expect("dlss file should be written");
    fs::write(fg_folder.path().join("nvngx_dlssg.dll"), b"fg-a").expect("fg file should be written");

    run(vec![
        OsString::from("scan-folder"),
        dlss_folder.path().as_os_str().to_owned(),
    ])
    .expect("first scan should succeed");
    run(vec![
        OsString::from("scan-folder"),
        fg_folder.path().as_os_str().to_owned(),
    ])
    .expect("second scan should succeed");

    let output = run(args(&[
        "list-artifacts",
        "--technology",
        "dlss_super_resolution",
    ]))
    .expect("filtered artifact list should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");
    let groups = json["groups"].as_array().expect("groups array");

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["technology"], "dlss_super_resolution");
    assert_eq!(groups[0]["artifacts"].as_array().expect("artifact array").len(), 1);
    assert_eq!(groups[0]["artifacts"][0]["file_name"], "nvngx_dlss.dll");
}

#[test]
fn scan_folder_deduplicates_identical_sha256_across_games() {
    let _catalog = CatalogEnvironmentGuard::new(temp_db_path("scan-dedup"));
    let first_folder = TempGameFolder::new("cli-dedup-first");
    let second_folder = TempGameFolder::new("cli-dedup-second");

    fs::create_dir_all(first_folder.path()).expect("temp folder should be created");
    fs::create_dir_all(second_folder.path()).expect("temp folder should be created");
    fs::write(first_folder.path().join("nvngx_dlss.dll"), b"same-bytes")
        .expect("first file should be written");
    fs::write(second_folder.path().join("nvngx_dlss.dll"), b"same-bytes")
        .expect("second file should be written");

    run(vec![
        OsString::from("scan-folder"),
        first_folder.path().as_os_str().to_owned(),
    ])
    .expect("first scan should succeed");
    run(vec![
        OsString::from("scan-folder"),
        second_folder.path().as_os_str().to_owned(),
    ])
    .expect("second scan should succeed");

    let output = run(args(&[
        "list-artifacts",
        "--technology",
        "dlss_super_resolution",
    ]))
    .expect("artifact list should render");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");
    let artifacts = json["groups"][0]["artifacts"]
        .as_array()
        .expect("artifact array");

    assert_eq!(artifacts.len(), 1);
}