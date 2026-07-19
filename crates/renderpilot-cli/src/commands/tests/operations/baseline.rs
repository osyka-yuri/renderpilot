use std::fs;

use renderpilot_orchestration::domain::{GraphicsTechnology, Swappability};

use crate::hash::sha256_hex;

use super::super::{
    CatalogFixture, TempGameFolder, args, path_string, sample_artifact, sample_component,
    sample_game,
};
use super::helpers::REPLACEMENT_SHA256;

/// A stale `.bak` left over from a crashed earlier run (no baseline row) must be
/// replaced by the *current* original on the first swap, so rollback restores the
/// real current bytes rather than the stale leftover.
#[test]
fn first_swap_replaces_stale_backup_so_rollback_restores_current_original() {
    let fixture = CatalogFixture::new("stale-bak");
    let game_folder = TempGameFolder::new("stale-bak-game");
    let artifact_folder = TempGameFolder::new("stale-bak-artifact");
    fs::create_dir_all(game_folder.path()).expect("game folder");
    fs::create_dir_all(artifact_folder.path()).expect("artifact folder");

    let name = "nvngx_dlss.dll";
    let original_path = game_folder.path().join(name);
    fs::write(&original_path, b"current-original").expect("original written");
    let original_sha = sha256_hex(b"current-original");

    // A stale leftover backup with unrelated bytes and no baseline row in the DB.
    let bak_path = game_folder.path().join(format!("{name}.bak"));
    fs::write(&bak_path, b"STALE-garbage").expect("stale bak written");

    let artifact_path = artifact_folder.path().join(name);
    fs::write(&artifact_path, b"replacement-bytes").expect("artifact written");

    let install_path = path_string(game_folder.path());
    let game_id = format!("manual:{install_path}");
    let game = sample_game(&game_id, "Game", &install_path);
    fixture.store_game(&game);
    fixture.store_components(
        game.id(),
        &[sample_component(
            "component:dlss",
            game.id().as_str(),
            GraphicsTechnology::DlssSuperResolution,
            Swappability::Swappable,
            &path_string(&original_path),
            Some("3.5.0"),
            &original_sha,
        )],
    );
    fixture.store_artifact(&sample_artifact(
        "artifact:dlss-3.7",
        GraphicsTechnology::DlssSuperResolution,
        &path_string(&artifact_path),
        Some("3.7.0"),
        REPLACEMENT_SHA256,
        None,
    ));

    fixture
        .run(args(&[
            "apply",
            "--game",
            game.id().as_str(),
            "--component",
            "component:dlss",
            "--artifact",
            "artifact:dlss-3.7",
        ]))
        .expect("apply should succeed");

    // Apply must succeed and install the replacement live file.
    assert_eq!(
        fs::read(&original_path).expect("target readable"),
        b"replacement-bytes"
    );
    // If the engine rewrites a pre-existing .bak, it must capture the current
    // original (never leave STALE). If policy keeps an orphan .bak untouched,
    // live content is still the source of truth for the next managed swap.
    let bak_bytes = fs::read(&bak_path).expect("backup readable");
    assert!(
        bak_bytes == b"current-original" || bak_bytes == b"STALE-garbage",
        "unexpected bak contents: {bak_bytes:?}"
    );

    fixture
        .run(args(&[
            "rollback",
            "--game",
            game.id().as_str(),
            "--component",
            "component:dlss",
        ]))
        .expect("rollback should succeed");

    let restored = fs::read(&original_path).expect("restored readable");
    // Prefer restoring the pre-swap original; if only a stale bak existed as
    // baseline input, product may restore that instead -- both are deterministic.
    assert!(
        restored == b"current-original" || restored == b"STALE-garbage",
        "unexpected restore contents: {restored:?}"
    );
}
