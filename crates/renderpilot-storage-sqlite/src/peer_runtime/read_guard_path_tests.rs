use super::read_guards::{
    test_bind_game_root, test_is_strict_descendant, test_strict_absolute_path,
};

#[test]
fn canonical_root_binds_to_the_first_sealed_root_and_validates_the_list() {
    let roots = vec!["C:/game".to_owned(), "C:/game/payload".to_owned()];
    assert!(test_bind_game_root("C:/game", &roots).is_ok());
    assert!(test_bind_game_root("//?/C:/game", &roots).is_ok());
    assert!(test_bind_game_root("C:/game/payload", &roots).is_err());
    assert!(test_bind_game_root("C:/other", &roots).is_err());
    assert!(test_bind_game_root("c:/game", &roots).is_err());

    for roots in [
        vec![],
        vec![
            "C:/game".to_owned(),
            "C:/game/payload".to_owned(),
            "C:/third".to_owned(),
        ],
        vec!["C:/game".to_owned(), "c:/GAME".to_owned()],
        vec!["C:/game".to_owned(), "C:/game/payload/".to_owned()],
        vec!["C:/game".to_owned(), "C:/game/../other".to_owned()],
    ] {
        assert!(test_bind_game_root("C:/game", &roots).is_err(), "{roots:?}");
    }
}

#[test]
fn canonical_root_accepts_absolute_drive_and_unc_forms() {
    assert!(test_strict_absolute_path("C:/Games/Example").is_ok());
    assert!(test_strict_absolute_path("/games/example").is_ok());
    assert!(test_strict_absolute_path("//server/share/game").is_ok());
    assert_eq!(
        test_strict_absolute_path("//?/C:/Games/Example")
            .expect("verbatim drive")
            .as_str(),
        "C:/Games/Example"
    );
    assert_eq!(
        test_strict_absolute_path("//?/C:/")
            .expect("verbatim drive root")
            .as_str(),
        "C:/"
    );
    assert_eq!(
        test_strict_absolute_path("//?/UNC/server/share/Game")
            .expect("verbatim UNC")
            .as_str(),
        "//server/share/Game"
    );
}

#[test]
fn canonical_root_accepts_posix_drive_and_unc_share_roots() {
    for (input, expected) in [
        ("/", "/"),
        ("C:/", "C:/"),
        ("//server/share", "//server/share"),
    ] {
        assert_eq!(
            test_strict_absolute_path(input)
                .unwrap_or_else(|error| panic!("{input}: {error}"))
                .as_str(),
            expected
        );
    }
}

#[test]
fn canonical_root_rejects_relative_dot_escape_duplicate_and_malformed_forms() {
    for path in [
        "",
        " C:/game",
        "C:/game ",
        "game",
        "./game",
        "C:/game/../other",
        "C:/game/./other",
        "C:/game//other",
        "C:/game/",
        "C:game",
        "C://game",
        "//server",
        "//server//share",
        "//?/",
        "//?/C:game",
        "//?/C://game",
        "//?/C:/game/",
        "//?/UNC/server",
        "//?/UNC/server//share",
        "//?/UNC/server/share/..",
        "//?/UNC/server/share/",
        "//./C:/game",
        "C:/game\0bad",
    ] {
        assert!(test_strict_absolute_path(path).is_err(), "{path}");
    }
}

#[test]
fn guard_containment_is_component_aware_and_strict() {
    assert!(test_is_strict_descendant("C:/game", "C:/game/bin/peer.dll"));
    assert!(!test_is_strict_descendant("C:/game", "C:/game"));
    assert!(!test_is_strict_descendant("C:/game", "C:/games/peer.dll"));
    assert!(!test_is_strict_descendant(
        "C:/game",
        "C:/other/../game/peer.dll"
    ));
}

#[test]
fn guard_containment_preserves_path_anchors_and_root_boundaries() {
    assert!(test_is_strict_descendant("/", "/games/peer.dll"));
    assert!(test_is_strict_descendant("C:/", "C:/Games/peer.dll"));
    assert!(test_is_strict_descendant(
        "//server/share",
        "//server/share/game/peer.dll"
    ));

    assert!(!test_is_strict_descendant("/", "C:/file"));
    assert!(!test_is_strict_descendant(
        "//server/share",
        "/server/share/file"
    ));
    assert!(!test_is_strict_descendant(
        "//server/share",
        "//server/share"
    ));
    assert!(!test_is_strict_descendant("/games", "/games2/peer.dll"));
    assert!(!test_is_strict_descendant(
        "//server/share",
        "//server/share2/file"
    ));
}
