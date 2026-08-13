use renderpilot_orchestration::portable::RuntimePathsV1;

use super::{hash, temp_root};

#[test]
fn runtime_paths_stay_under_the_stable_portable_root_after_unicode_move() {
    let root = temp_root("unicode-move");
    let portable_root = root.path().join("РендерПилот-移動");
    let generation = portable_root
        .join(".renderpilot-generations/v1/objects")
        .join("a".repeat(64));
    let app = generation.join("renderpilot-app.exe");
    let paths = RuntimePathsV1::from_portable_root(portable_root.clone(), &generation, &app)
        .expect("derive moved portable paths");
    paths
        .validate()
        .expect("all durable paths remain contained");
    assert_eq!(paths.data_root, portable_root.join("data"));
    assert!(paths.catalog_db_path.starts_with(&portable_root));
    assert!(paths.webview2_root.starts_with(&portable_root));
    assert_eq!(paths.selected_app_executable, app);
    assert!(paths.selected_generation_root.ends_with(hash('a')));
}
