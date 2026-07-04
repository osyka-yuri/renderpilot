//! Domain types for injected game add-ons (initially RenoDX).
//!
//! Unlike a [`crate::GraphicsComponent`], which models a vendor library that
//! already exists in a game folder and is swapped between versions, an add-on is
//! a third-party runtime that RenderPilot *introduces* into a game (the RenoDX
//! ReShade add-on, plus the ReShade host when absent). These types describe what
//! a game's executable renders with and what an installed add-on left behind, so
//! an install can be fully and safely reversed.
//!
//! # Submodules
//!
//! - [`installed`] — core install record ([`InstalledAddon`])
//! - [`tracked`] — update provenance ([`TrackedSource`], [`TrackedSourceRole`], ...)
//! - [`states`] — UI-ready install state projections ([`RenoDxInstallState`], ...)
//! - [`shared_artifact`] — global shared resources ([`SharedArtifactRecord`], ...)

pub mod installed;
pub mod shared_artifact;
pub mod states;
pub mod tracked;

pub use installed::InstalledAddon;
pub use shared_artifact::{
    SharedArtifactKind, SharedArtifactOrigin, SharedArtifactRecord, SharedArtifactSource,
};
pub use states::{RenoDxHostKind, RenoDxInstallState};
pub use tracked::{InstalledAddonHostKind, TrackedSource, TrackedSourceRole};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AddonKind, GameId, PathRef};

    fn game_id() -> GameId {
        GameId::new("steam:1091500").expect("valid id")
    }

    fn addon_path() -> PathRef {
        PathRef::new(r"C:\Games\CP2077\renodx-cp2077.addon64").expect("valid path")
    }

    #[test]
    fn installed_addon_always_tracks_addon_file_as_created() {
        let installed = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path());

        assert_eq!(installed.created_files(), &[addon_path()]);
        assert!(installed.backed_up_files().is_empty());
        assert!(!installed.has_host_binary_provenance());
    }

    #[test]
    fn installed_addon_records_host_binary_artifact_and_files() {
        let proxy = PathRef::new(r"C:\Games\CP2077\dxgi.dll").expect("valid path");
        let ini_backup = PathRef::new(r"C:\Games\CP2077\reshade.ini").expect("valid path");

        let installed = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_tracked_source(TrackedSource::new(
                TrackedSourceRole::HostBinary,
                "https://nightly.link/x64.zip",
                None,
                "host-digest",
            ))
            .with_created_file(proxy.clone())
            .with_backed_up_file(ini_backup.clone());

        assert!(installed.has_host_binary_provenance());
        assert_eq!(installed.created_files(), &[addon_path(), proxy]);
        assert_eq!(installed.backed_up_files(), &[ini_backup]);
    }

    #[test]
    fn host_binary_provenance_is_derived_from_a_host_artifact() {
        let base = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_tracked_source(TrackedSource::new(
                TrackedSourceRole::AddonPayload,
                "https://example/addon",
                None,
                "addon-digest",
            ));
        assert!(!base.has_host_binary_provenance());

        let with_host = base.with_tracked_source(TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example/host",
            None,
            "host-digest",
        ));
        assert!(with_host.has_host_binary_provenance());
    }

    #[test]
    fn tracked_source_is_not_advisory_by_default() {
        let source = TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example/host",
            None,
            "digest",
        );
        assert!(!source.is_advisory());
    }

    #[test]
    fn tracked_source_with_advisory_round_trips_through_json() {
        let source = TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example/host",
            None,
            "digest",
        )
        .with_advisory();
        assert!(source.is_advisory());

        let json = serde_json::to_string(&source).expect("serializes");
        let round_tripped: TrackedSource = serde_json::from_str(&json).expect("deserializes");
        assert!(round_tripped.is_advisory());
    }

    #[test]
    fn tracked_source_json_without_advisory_field_deserializes_as_false() {
        let legacy_json = r#"{
            "role": "host",
            "url": "https://example/host",
            "etag": null,
            "digest": "digest"
        }"#;
        let source: TrackedSource = serde_json::from_str(legacy_json).expect("deserializes");
        assert!(!source.is_advisory());
    }

    #[test]
    fn from_parts_rejects_rows_that_omit_addon_file() {
        let addon = addon_path();
        let other = PathRef::new(r"C:\Games\CP2077\dxgi.dll").expect("valid path");
        assert!(
            InstalledAddon::from_parts(
                game_id(),
                AddonKind::RenoDx,
                addon.clone(),
                None,
                vec![other],
                Vec::new(),
                Vec::new(),
            )
            .is_none()
        );

        assert!(
            InstalledAddon::from_parts(
                game_id(),
                AddonKind::RenoDx,
                addon.clone(),
                None,
                vec![addon],
                Vec::new(),
                Vec::new(),
            )
            .is_some()
        );
    }

    #[test]
    fn installed_addon_fields_reflect_version_for_state() {
        let installed = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_addon_version("snapshot-2026.06");

        // The record now exposes the raw fields that per-tool state builders
        // (renodx::tracking etc.) turn into RenoDxInstallState.
        assert_eq!(installed.addon_version(), Some("snapshot-2026.06"));
        assert!(!installed.has_dlss_fix());
        assert!(!installed.has_addon_source());
    }

    #[test]
    fn record_surfaces_addon_date_and_timestamps() {
        let installed = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_tracked_source(
                TrackedSource::new(
                    TrackedSourceRole::AddonPayload,
                    "https://clshortfuse.github.io/renodx/renodx-cp2077.addon64",
                    Some("\"etag\"".to_owned()),
                    "addon-digest",
                )
                .with_last_modified(Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned())),
            )
            .with_timestamps(Some(1_700_000_000_000), Some(1_700_000_500_000));

        assert_eq!(
            installed.addon_dated(),
            Some("Wed, 18 Jun 2026 12:00:00 GMT")
        );
        assert_eq!(installed.installed_at(), Some(1_700_000_000_000));
        assert_eq!(installed.updated_at(), Some(1_700_000_500_000));
        assert!(installed.has_addon_source());
    }

    #[test]
    fn record_preserves_host_kind() {
        let proxy = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_host_kind(InstalledAddonHostKind::Proxy);
        let vulkan = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_host_kind(InstalledAddonHostKind::SharedVulkanLayer);

        assert_eq!(proxy.host_kind(), Some(InstalledAddonHostKind::Proxy));
        assert_eq!(
            vulkan.host_kind(),
            Some(InstalledAddonHostKind::SharedVulkanLayer)
        );
    }

    #[test]
    fn local_addon_date_placeholder_is_not_a_tracked_upstream_source() {
        let installed = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path())
            .with_tracked_source(
                TrackedSource::new(TrackedSourceRole::AddonPayload, "", None, "addon-digest")
                    .with_last_modified(Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned())),
            );

        assert_eq!(
            installed.addon_dated(),
            Some("Wed, 18 Jun 2026 12:00:00 GMT")
        );
        assert!(!installed.has_addon_source());
    }

    #[test]
    fn has_dlss_fix_reflects_tracked_source() {
        let base = InstalledAddon::new(game_id(), AddonKind::RenoDx, addon_path());
        assert!(!base.has_dlss_fix());

        let with_fix = base.with_tracked_source(TrackedSource::new(
            TrackedSourceRole::DlssFix,
            "https://example/renodx-dlssfix.addon64",
            None,
            "dlss-fix-digest",
        ));
        assert!(with_fix.has_dlss_fix());
    }

    #[test]
    fn install_state_serializes_with_status_tag() {
        let json = serde_json::to_string(&RenoDxInstallState::NotInstalled).expect("serialize");
        assert_eq!(json, r#"{"status":"not_installed"}"#);

        let installed = RenoDxInstallState::Installed {
            host_kind: Some(RenoDxHostKind::Proxy),
            version: None,
            addon_dated: None,
            installed_at: 1_700_000_000_000,
            updated_at: 1_700_000_000_000,
            dlss_fix_installed: false,
            addon_tracked: true,
        };
        let json = serde_json::to_string(&installed).expect("serialize");
        assert_eq!(
            json,
            r#"{"status":"installed","host_kind":"proxy","version":null,"addon_dated":null,"installed_at":1700000000000,"updated_at":1700000000000,"dlss_fix_installed":false,"addon_tracked":true}"#
        );
    }

    #[test]
    fn tracked_source_last_modified_round_trips_and_defaults() {
        let source = TrackedSource::new(
            TrackedSourceRole::AddonPayload,
            "https://example/addon",
            None,
            "digest",
        )
        .with_last_modified(Some("Wed, 18 Jun 2026 12:00:00 GMT".to_owned()));
        let json = serde_json::to_string(&source).expect("serialize");
        assert_eq!(
            serde_json::from_str::<TrackedSource>(&json).expect("round-trip"),
            source
        );

        let legacy = r#"{"role":"addon_payload","url":"https://example/addon","etag":null,"digest":"digest"}"#;
        let parsed: TrackedSource = serde_json::from_str(legacy).expect("legacy parse");
        assert_eq!(parsed.last_modified(), None);
        assert_eq!(parsed.channel(), None);
    }

    #[test]
    fn tracked_source_channel_round_trips_and_defaults() {
        let source = TrackedSource::new(
            TrackedSourceRole::HostBinary,
            "https://example/host.zip",
            None,
            "digest",
        )
        .with_channel("stable");
        let json = serde_json::to_string(&source).expect("serialize");
        assert_eq!(
            serde_json::from_str::<TrackedSource>(&json).expect("round-trip"),
            source
        );

        let legacy =
            r#"{"role":"host","url":"https://example/host.zip","etag":null,"digest":"digest"}"#;
        let parsed: TrackedSource = serde_json::from_str(legacy).expect("legacy parse");
        assert_eq!(parsed.channel(), None);
    }
}
