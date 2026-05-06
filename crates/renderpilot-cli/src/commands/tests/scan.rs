use std::{ffi::OsString, fs};

use crate::run;

use super::{temp_db_path, CatalogEnvironmentGuard, TempGameFolder};

#[test]
fn scan_folder_outputs_game_and_components_json() {
    let _catalog = CatalogEnvironmentGuard::new(temp_db_path("scan-output"));
    let folder = TempGameFolder::new("cli-scan-folder");
    fs::create_dir_all(folder.path()).expect("temp folder should be created");
    fs::write(folder.path().join("nvngx_dlss.dll"), b"").expect("test file should be written");

    let output = run(vec![
        OsString::from("scan-folder"),
        folder.path().as_os_str().to_owned(),
    ])
    .expect("scan should succeed");
    let json: serde_json::Value = serde_json::from_str(&output).expect("valid json");

    assert!(json.get("game").is_some());
    let components = json["components"].as_array().expect("components array");
    assert_eq!(components.len(), 1);
    assert_eq!(components[0]["file_name"], "nvngx_dlss.dll");
    assert_eq!(components[0]["technology"], "DlssSuperResolution");
    assert_eq!(components[0]["kind"], "NativeLibrary");
    assert_eq!(components[0]["detection_confidence"], "High");
    assert_eq!(components[0]["swappability"], "Swappable");
    assert_eq!(components[0]["version"], serde_json::Value::Null);
    assert_eq!(components[0]["status"], "unknown_version");
    assert_eq!(
        components[0]["sha256"],
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        components[0]["cache_key"]["sha256"],
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(components[0]["cache_key"]["size"], 0);
    assert!(components[0]["cache_key"].get("modified_at").is_some());
    assert!(components[0]["cache_key"].get("path").is_some());
}

#[test]
fn scan_folder_reports_missing_folder() {
    let folder = TempGameFolder::new("missing-cli-scan-folder");
    let error = run(vec![
        OsString::from("scan-folder"),
        folder.path().as_os_str().to_owned(),
    ])
    .expect_err("missing folder should fail");

    assert!(error.to_string().contains("game folder does not exist"));
}
