//! Fixture builders shared by more than one test module under `update::`.

use std::path::Path;

use renderpilot_domain::{
    AddonKind, Architecture, GameId, InstalledAddon, PathRef, TrackedSource, TrackedSourceRole,
};

use crate::addons::luma::fetch::types::{LumaPayload, LumaPayloadFile};
use crate::addons::luma::use_cases::update_target::ResolvedUpdateTarget;

pub(super) fn payload_file(relative_path: &str, bytes: &[u8]) -> LumaPayloadFile {
    LumaPayloadFile {
        relative_path: relative_path.to_owned(),
        bytes: bytes.to_vec(),
    }
}

pub(super) fn payload(files: Vec<LumaPayloadFile>) -> LumaPayload {
    LumaPayload {
        files,
        main_addon_rel: "Luma-Game.addon".to_owned(),
        zip_digest: "digest".to_owned(),
        etag: None,
        last_modified: None,
        build_number: None,
    }
}

pub(super) fn empty_payload() -> LumaPayload {
    payload(vec![payload_file("Luma-Game.addon", b"x")])
}

pub(super) fn path_ref(path: &Path) -> PathRef {
    PathRef::new(path.to_string_lossy().into_owned()).expect("path")
}

pub(super) fn resolved_target(game_dir: &Path) -> ResolvedUpdateTarget {
    resolved_target_with_proxy(game_dir, "dxgi.dll")
}

pub(super) fn resolved_target_with_proxy(game_dir: &Path, proxy: &str) -> ResolvedUpdateTarget {
    ResolvedUpdateTarget {
        game_dir: game_dir.to_path_buf(),
        asset: "Luma-Game.zip".to_owned(),
        addon_file: "Luma-Game.addon".to_owned(),
        arch: Architecture::X64,
        proxy_dll_name: proxy.to_owned(),
        external_requirement: None,
    }
}

pub(super) fn empty_record(game_dir: &Path) -> InstalledAddon {
    InstalledAddon::new(
        GameId::new("steam:1").expect("id"),
        AddonKind::Luma,
        path_ref(&game_dir.join("Luma-Game.addon")),
    )
}

pub(super) fn record_with_addon(addon: &Path) -> InstalledAddon {
    InstalledAddon::from_parts(
        GameId::new("steam:403670").expect("id"),
        AddonKind::Luma,
        path_ref(addon),
        None,
        vec![path_ref(addon)],
        Vec::new(),
        vec![TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example/old.zip",
            None,
            "old-digest",
        )],
    )
    .expect("record")
}

pub(super) fn record_with_sources(addon: &str, sources: Vec<TrackedSource>) -> InstalledAddon {
    InstalledAddon::from_parts(
        GameId::new("steam:1").expect("id"),
        AddonKind::Luma,
        PathRef::new(addon).expect("path"),
        None,
        vec![PathRef::new(addon).expect("path")],
        Vec::new(),
        sources,
    )
    .expect("record")
}

pub(super) fn record_with_digest(addon: &str, digest: &str) -> InstalledAddon {
    record_with_sources(
        addon,
        vec![TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example/Luma.zip",
            None,
            digest,
        )],
    )
}

pub(super) fn multi_source_record(
    addon: &str,
    payload_digest: &str,
    host: &str,
    dgvoodoo: &str,
) -> InstalledAddon {
    record_with_sources(
        addon,
        vec![
            TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                "https://example/Luma.zip",
                None,
                payload_digest,
            ),
            TrackedSource::new(
                TrackedSourceRole::HostBinary,
                "https://example/reshade.zip",
                None,
                host,
            ),
            TrackedSource::new(
                TrackedSourceRole::DgVoodooWrapper,
                "https://example/dgvoodoo.zip",
                None,
                dgvoodoo,
            ),
        ],
    )
}

/// Marks `game_dir`'s proxy slot as a recognized custom build (GShade), so
/// host preparation short-circuits to `unchanged` without ever reaching the
/// network -- see `reshade::scan::is_known_custom_build`.
pub(super) fn mark_gshade_custom_build(game_dir: &Path) {
    std::fs::write(game_dir.join("dxgi.dll"), b"gshade-proxy-stub").expect("write proxy stub");
    std::fs::write(game_dir.join("GShade64.dll"), b"gshade-runtime").expect("write gshade runtime");
}
