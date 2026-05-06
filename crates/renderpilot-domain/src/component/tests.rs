use crate::{
    ArtifactId, ArtifactTrustLevel, ComponentFile, ComponentId, ComponentKind, GameId,
    GraphicsComponent, GraphicsTechnology, LibraryArtifact, PathRef, Sha256Hash, Swappability,
    Version,
};

use super::ComponentError;

#[test]
fn sha256_hash_normalizes_to_lowercase() {
    let hash = Sha256Hash::new("ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789")
        .expect("valid hash");

    assert_eq!(
        hash.as_str(),
        "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
    );
}

#[test]
fn sha256_hash_rejects_wrong_length() {
    let error = Sha256Hash::new("abc").expect_err("short hash should fail");

    assert_eq!(error, ComponentError::InvalidSha256Hash);
}

#[test]
fn sha256_hash_rejects_non_hex_text() {
    let error =
        Sha256Hash::new("x".repeat(Sha256Hash::HEX_LENGTH)).expect_err("non-hex hash should fail");

    assert_eq!(error, ComponentError::InvalidSha256Hash);
}

#[test]
fn sha256_hash_deserialization_validates_input() {
    let error = serde_json::from_str::<Sha256Hash>(r#""abc""#).expect_err("hash should fail");

    assert!(error
        .to_string()
        .contains("sha256 must be a 64-character hexadecimal string"));
}

#[test]
fn component_file_keeps_version_and_hash() {
    let file =
        ComponentFile::new(PathRef::new(r"C:\Games\Game\nvngx_dlss.dll").expect("valid path"))
            .with_version(Version::parse("3.7.20").expect("valid version"))
            .with_sha256(
                Sha256Hash::new("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
                    .expect("valid hash"),
            );

    assert_eq!(file.path().as_str(), "C:/Games/Game/nvngx_dlss.dll");
    assert_eq!(file.version().map(Version::as_str), Some("3.7.20"));
}

#[test]
fn graphics_component_collects_component_files() {
    let component = GraphicsComponent::new(
        ComponentId::new("component:dlss").expect("valid component id"),
        GameId::new("steam:10").expect("valid game id"),
        ComponentKind::NativeLibrary,
        GraphicsTechnology::DlssSuperResolution,
        Swappability::Swappable,
    )
    .with_file(ComponentFile::new(
        PathRef::new("C:/Games/Game/nvngx_dlss.dll").expect("valid path"),
    ));

    assert_eq!(component.files().len(), 1);
}

#[test]
fn library_artifact_normalizes_source() {
    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:dlss:3.7.20").expect("valid artifact id"),
        GraphicsTechnology::DlssSuperResolution,
        " nvngx_dlss.dll ",
        ComponentFile::new(PathRef::new("data/library/nvngx_dlss.dll").expect("valid path"))
            .with_sha256(
                Sha256Hash::new("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
                    .expect("valid hash"),
            ),
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact should be valid")
    .with_source(" NVIDIA ")
    .expect("valid source");

    assert_eq!(artifact.file_name(), "nvngx_dlss.dll");
    assert_eq!(artifact.source(), Some("NVIDIA"));
    assert_eq!(artifact.trust_level(), ArtifactTrustLevel::LocalObserved);
}

#[test]
fn library_artifact_rejects_empty_source() {
    let error = LibraryArtifact::new(
        ArtifactId::new("artifact:dlss:3.7.20").expect("valid artifact id"),
        GraphicsTechnology::DlssSuperResolution,
        "nvngx_dlss.dll",
        ComponentFile::new(PathRef::new("data/library/nvngx_dlss.dll").expect("valid path"))
            .with_sha256(
                Sha256Hash::new("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
                    .expect("valid hash"),
            ),
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact should be valid")
    .with_source(" ")
    .expect_err("source should be required when present");

    assert_eq!(error, ComponentError::EmptyText("artifact_source"));
}

#[test]
fn library_artifact_tracks_source_game() {
    let source_game_id = GameId::new("manual:C:/Games/Test").expect("valid game id");
    let artifact = LibraryArtifact::new(
        ArtifactId::new("artifact:dlss:3.7.20").expect("valid artifact id"),
        GraphicsTechnology::DlssSuperResolution,
        "nvngx_dlss.dll",
        ComponentFile::new(PathRef::new("data/library/nvngx_dlss.dll").expect("valid path"))
            .with_sha256(
                Sha256Hash::new("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
                    .expect("valid hash"),
            ),
        ArtifactTrustLevel::LocalObserved,
    )
    .expect("artifact should be valid")
    .with_source_game_id(source_game_id.clone());

    assert_eq!(artifact.source_game_id(), Some(&source_game_id));
}

#[test]
fn library_artifact_requires_sha256() {
    let error = LibraryArtifact::new(
        ArtifactId::new("artifact:dlss:3.7.20").expect("valid artifact id"),
        GraphicsTechnology::DlssSuperResolution,
        "nvngx_dlss.dll",
        ComponentFile::new(PathRef::new("data/library/nvngx_dlss.dll").expect("valid path")),
        ArtifactTrustLevel::LocalObserved,
    )
    .expect_err("artifact without sha256 should fail");

    assert_eq!(error, ComponentError::MissingArtifactSha256);
}
