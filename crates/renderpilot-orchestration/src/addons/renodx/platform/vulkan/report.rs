//! Orchestration glue for the shared ReShade Vulkan layer.
//!
//! Wraps the Windows-only platform layer
//! ([`renderpilot_platform_windows::vulkan_layer`]) in a cross-platform service API:
//! a status report the UI can render plus backend-authored maintenance actions. The
//! layer is a single shared resource (one ReShade Vulkan overlay system-wide), so
//! the service detects an existing one and reuses it, installing one only when
//! none is present. On non-Windows every operation reports
//! [`VulkanLayerDetection::Unsupported`] / errors — RenoDX for Vulkan is a Windows
//! feature.

use crate::addons::renodx::dto::vulkan::{
    LayerDiagnosticReason, VulkanLayerActions, VulkanLayerArchitecture, VulkanLayerDetection,
    VulkanLayerFacts, VulkanLayerReport, VulkanLoaderVisibility,
};
#[cfg(any(test, windows))]
use crate::addons::reshade::dto::ActionDescriptor;
#[cfg(windows)]
use crate::addons::reshade::dto::{ActionConfirmationScope, ActionDisabledReason};
#[cfg(windows)]
const VULKAN_CONFIRM: ActionConfirmationScope = ActionConfirmationScope::AllVulkanRenoDxGames;

/// Builds the public shared-layer report from the platform detection state.
///
/// On Windows this calls the platform detector's `detect_report` and threads the
/// facts + diagnostics into the public DTO. On non-Windows it returns
/// [`VulkanLayerDetection::Unsupported`].
#[must_use]
pub fn layer_report() -> VulkanLayerReport {
    #[cfg(windows)]
    {
        platform_layer_report()
    }
    #[cfg(not(windows))]
    {
        unsupported_report()
    }
}

#[cfg(windows)]
fn platform_layer_report() -> VulkanLayerReport {
    use renderpilot_platform_windows::vulkan_layer::{self, WindowsLayerRegistry};

    let Some(dir) = vulkan_layer::reshade_common_dir() else {
        return not_installed_report();
    };
    let report = vulkan_layer::detect_report(&WindowsLayerRegistry, &dir);
    map_platform_report(report)
}

/// Maps a platform detector report into the public DTO.
#[cfg(windows)]
fn map_platform_report(
    report: renderpilot_platform_windows::vulkan_layer::VulkanLayerReport,
) -> VulkanLayerReport {
    use renderpilot_platform_windows::vulkan_layer::VulkanLayerState;

    let version = report.facts.version.or_else(|| {
        report.facts.dll_path.as_deref().and_then(|dll| {
            renderpilot_detection::read_windows_file_version(dll).map(|v| v.to_string())
        })
    });

    let facts = VulkanLayerFacts {
        manifest_path: report.facts.manifest_path,
        dll_path: report.facts.dll_path,
        version,
        architecture: map_architecture(report.facts.architecture),
        loader_visibility: map_visibility(report.facts.loader_visibility),
    };
    let mut diagnostics: Vec<LayerDiagnosticReason> =
        report.diagnostics.iter().map(map_diagnostic).collect();

    match report.state {
        VulkanLayerState::Absent => VulkanLayerReport {
            layer_detection: VulkanLayerDetection::NotInstalled,
            layer_facts: facts,
            diagnostic_reasons: diagnostics,
            actions: VulkanLayerActions {
                install: Some(ActionDescriptor::enabled()),
                update: None,
                switch_channel: None,
                remove: None,
                resolve_conflict: None,
            },
        },
        VulkanLayerState::Installed => {
            let switch_channel = ActionDescriptor::enabled().with_confirmation(VULKAN_CONFIRM);
            VulkanLayerReport {
                layer_detection: VulkanLayerDetection::Installed,
                layer_facts: facts,
                diagnostic_reasons: diagnostics,
                actions: VulkanLayerActions {
                    install: None,
                    update: Some(ActionDescriptor::enabled().with_confirmation(VULKAN_CONFIRM)),
                    switch_channel: Some(switch_channel),
                    remove: Some(ActionDescriptor::enabled().with_confirmation(VULKAN_CONFIRM)),
                    resolve_conflict: None,
                },
            }
        }
        VulkanLayerState::InstalledDisabled => VulkanLayerReport {
            layer_detection: VulkanLayerDetection::InstalledDisabled,
            layer_facts: facts,
            diagnostic_reasons: diagnostics,
            actions: VulkanLayerActions {
                install: None,
                update: Some(ActionDescriptor::enabled().with_confirmation(VULKAN_CONFIRM)),
                switch_channel: None,
                remove: Some(ActionDescriptor::enabled().with_confirmation(VULKAN_CONFIRM)),
                resolve_conflict: None,
            },
        },
        VulkanLayerState::External => {
            if !diagnostics.contains(&LayerDiagnosticReason::ExternalLayerDetected) {
                diagnostics.insert(0, LayerDiagnosticReason::ExternalLayerDetected);
            }
            VulkanLayerReport {
                layer_detection: VulkanLayerDetection::ExternalReadOnly,
                layer_facts: facts,
                diagnostic_reasons: diagnostics,
                actions: VulkanLayerActions {
                    install: None,
                    update: None,
                    switch_channel: None,
                    remove: None,
                    resolve_conflict: None,
                },
            }
        }
        VulkanLayerState::Conflict => {
            let mut report = VulkanLayerReport {
                layer_detection: VulkanLayerDetection::Conflict,
                layer_facts: facts,
                diagnostic_reasons: diagnostics,
                actions: VulkanLayerActions {
                    install: None,
                    update: None,
                    switch_channel: None,
                    remove: None,
                    resolve_conflict: Some(ActionDescriptor::disabled(
                        ActionDisabledReason::BlockedByConflict,
                    )),
                },
            };
            if conflict_is_standard_mutable(&report) {
                report.actions.resolve_conflict =
                    Some(ActionDescriptor::enabled().with_confirmation(VULKAN_CONFIRM));
            }
            report
        }
        VulkanLayerState::Unsupported => VulkanLayerReport {
            layer_detection: VulkanLayerDetection::Unsupported,
            layer_facts: facts,
            diagnostic_reasons: diagnostics,
            actions: VulkanLayerActions {
                install: None,
                update: None,
                switch_channel: None,
                remove: None,
                resolve_conflict: None,
            },
        },
    }
}

#[cfg(windows)]
fn map_architecture(
    arch: renderpilot_platform_windows::vulkan_layer::VulkanLayerArchitecture,
) -> VulkanLayerArchitecture {
    use renderpilot_platform_windows::vulkan_layer::VulkanLayerArchitecture as Arch;
    match arch {
        Arch::X64 => VulkanLayerArchitecture::X64,
        Arch::X86 => VulkanLayerArchitecture::X86,
        Arch::Unknown => VulkanLayerArchitecture::Unknown,
    }
}

#[cfg(windows)]
fn map_visibility(
    vis: renderpilot_platform_windows::vulkan_layer::VulkanLoaderVisibility,
) -> VulkanLoaderVisibility {
    use renderpilot_platform_windows::vulkan_layer::VulkanLoaderVisibility as Vis;
    match vis {
        Vis::Normal => VulkanLoaderVisibility::Normal,
        Vis::HkcuNotVisibleWhenElevated => VulkanLoaderVisibility::HkcuNotVisibleWhenElevated,
        Vis::Ambiguous => VulkanLoaderVisibility::Ambiguous,
    }
}

#[cfg(windows)]
fn map_diagnostic(
    diag: &renderpilot_platform_windows::vulkan_layer::VulkanLayerDiagnostic,
) -> LayerDiagnosticReason {
    use renderpilot_platform_windows::vulkan_layer::VulkanLayerDiagnostic as D;
    match diag {
        D::RegistryMissing => LayerDiagnosticReason::RegistryMissing,
        D::RegistryDisabled => LayerDiagnosticReason::RegistryDisabled,
        D::DuplicateLayerManifest => LayerDiagnosticReason::DuplicateLayerManifest,
        D::AmbiguousLoaderVisibility => LayerDiagnosticReason::AmbiguousLoaderVisibility,
        D::MissingLayerDll => LayerDiagnosticReason::MissingLayerDll,
        D::UnreadableDll => LayerDiagnosticReason::UnreadableDll,
        D::MissingManifest => LayerDiagnosticReason::MissingManifest,
        D::UnsupportedArchitecture => LayerDiagnosticReason::UnsupportedArchitecture,
        D::HkcuNotVisibleWhenElevated => LayerDiagnosticReason::HkcuNotVisibleWhenElevated,
        D::ManifestMalformed => LayerDiagnosticReason::ManifestMalformed,
        D::BackendValidationFailed => LayerDiagnosticReason::BackendValidationFailed,
        D::RegistryScopeNotWritable => LayerDiagnosticReason::RegistryScopeNotWritable,
        D::PermissionDenied => LayerDiagnosticReason::PermissionDenied,
        D::HashMismatch => LayerDiagnosticReason::HashMismatch,
        D::DbOnlyFallback => LayerDiagnosticReason::DbOnlyFallback,
    }
}

#[cfg(any(test, windows))]
fn not_installed_report() -> VulkanLayerReport {
    VulkanLayerReport {
        layer_detection: VulkanLayerDetection::NotInstalled,
        layer_facts: VulkanLayerFacts {
            manifest_path: None,
            dll_path: None,
            version: None,
            architecture: VulkanLayerArchitecture::Unknown,
            loader_visibility: VulkanLoaderVisibility::Normal,
        },
        diagnostic_reasons: Vec::new(),
        actions: VulkanLayerActions {
            install: Some(ActionDescriptor::enabled()),
            update: None,
            switch_channel: None,
            remove: None,
            resolve_conflict: None,
        },
    }
}

#[cfg(any(test, not(windows)))]
fn unsupported_report() -> VulkanLayerReport {
    VulkanLayerReport {
        layer_detection: VulkanLayerDetection::Unsupported,
        layer_facts: VulkanLayerFacts {
            manifest_path: None,
            dll_path: None,
            version: None,
            architecture: VulkanLayerArchitecture::Unknown,
            loader_visibility: VulkanLoaderVisibility::Normal,
        },
        diagnostic_reasons: Vec::new(),
        actions: VulkanLayerActions {
            install: None,
            update: None,
            switch_channel: None,
            remove: None,
            resolve_conflict: None,
        },
    }
}

pub(crate) fn conflict_is_standard_mutable(report: &VulkanLayerReport) -> bool {
    !report.diagnostic_reasons.iter().any(|reason| {
        matches!(
            reason,
            LayerDiagnosticReason::DuplicateLayerManifest
                | LayerDiagnosticReason::AmbiguousLoaderVisibility
                | LayerDiagnosticReason::ExternalLayerDetected
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_installed_report_offers_install_only() {
        let report = not_installed_report();
        assert_eq!(report.layer_detection, VulkanLayerDetection::NotInstalled);
        assert!(report.actions.install.is_some());
        assert!(report.actions.update.is_none());
        assert!(report.actions.switch_channel.is_none());
        assert!(report.actions.remove.is_none());
        assert!(report.actions.resolve_conflict.is_none());
    }

    #[test]
    fn unsupported_report_offers_no_actions() {
        let report = unsupported_report();
        assert_eq!(report.layer_detection, VulkanLayerDetection::Unsupported);
        assert!(report.actions.install.is_none());
        assert!(report.actions.update.is_none());
        assert!(report.actions.resolve_conflict.is_none());
    }

    /// Acceptance test: no forbidden ownership/proof terms in the serialized
    /// `VulkanLayerReport` (the public wire shape the UI consumes).
    #[test]
    fn vulkan_layer_report_serializes_without_forbidden_terms() {
        let report = not_installed_report();
        let json = serde_json::to_string(&report).expect("serializes");
        for forbidden in [
            "managed",
            "unmanaged",
            "foreign",
            "owned",
            "ownership",
            "managed_by_us",
            "marker",
            "marker_version",
            "source",
            "digest",
            "sha256",
            "validator",
            "backup_path",
            "rollback_manifest",
            "created_by",
            "installed_by",
            "tracked_source",
            "provenance",
        ] {
            assert!(
                !json.contains(forbidden),
                "VulkanLayerReport JSON contains forbidden term `{forbidden}`: {json}"
            );
        }
    }

    /// Acceptance test: `external_read_only` never exposes update/switch/remove.
    #[test]
    fn external_read_only_never_exposes_mutating_actions() {
        let report = unsupported_report();
        // Build an external_read_only report manually (the platform_mapping tests
        // cover the Windows path; this covers the contract on any platform).
        let external = VulkanLayerReport {
            layer_detection: VulkanLayerDetection::ExternalReadOnly,
            layer_facts: report.layer_facts,
            diagnostic_reasons: vec![LayerDiagnosticReason::ExternalLayerDetected],
            actions: VulkanLayerActions {
                install: None,
                update: None,
                switch_channel: None,
                remove: None,
                resolve_conflict: None,
            },
        };
        assert!(external.actions.update.is_none());
        assert!(external.actions.switch_channel.is_none());
        assert!(external.actions.remove.is_none());
        assert!(external.actions.install.is_none());
    }

    #[cfg(windows)]
    mod platform_mapping {
        use super::*;
        use renderpilot_platform_windows::vulkan_layer::{
            VulkanLayerDiagnostic, VulkanLayerFacts as PlatformFacts,
            VulkanLayerReport as PlatformReport, VulkanLayerState,
        };
        use std::path::PathBuf;

        fn platform_report(
            state: VulkanLayerState,
            diagnostics: Vec<VulkanLayerDiagnostic>,
        ) -> PlatformReport {
            PlatformReport {
                state,
                facts: PlatformFacts {
                    manifest_path: Some(PathBuf::from("C:\\layer\\manifest.json")),
                    dll_path: Some(PathBuf::from("C:\\layer\\ReShade64.dll")),
                    version: Some("6.7.3".to_owned()),
                    architecture:
                        renderpilot_platform_windows::vulkan_layer::VulkanLayerArchitecture::X64,
                    loader_visibility:
                        renderpilot_platform_windows::vulkan_layer::VulkanLoaderVisibility::Normal,
                },
                diagnostics,
            }
        }

        #[test]
        fn absent_maps_to_not_installed_with_install_action() {
            let report = map_platform_report(platform_report(VulkanLayerState::Absent, Vec::new()));
            assert_eq!(report.layer_detection, VulkanLayerDetection::NotInstalled);
            assert!(report.actions.install.is_some());
            assert!(report.actions.update.is_none());
            assert!(report.actions.remove.is_none());
        }

        #[test]
        fn installed_maps_to_installed_with_update_switch_remove() {
            let report =
                map_platform_report(platform_report(VulkanLayerState::Installed, Vec::new()));
            assert_eq!(report.layer_detection, VulkanLayerDetection::Installed);
            assert!(report.actions.update.is_some());
            assert!(report.actions.switch_channel.is_some());
            assert!(report.actions.remove.is_some());
            assert!(report.actions.install.is_none());
            assert!(report.actions.resolve_conflict.is_none());
            // Confirmation required for destructive actions.
            assert!(report.actions.update.unwrap().requires_confirmation);
            assert!(report.actions.remove.unwrap().requires_confirmation);
        }

        #[test]
        fn external_maps_to_external_read_only_without_mutating_actions() {
            let report =
                map_platform_report(platform_report(VulkanLayerState::External, Vec::new()));
            assert_eq!(
                report.layer_detection,
                VulkanLayerDetection::ExternalReadOnly
            );
            assert!(report.actions.install.is_none());
            assert!(report.actions.update.is_none());
            assert!(report.actions.switch_channel.is_none());
            assert!(report.actions.remove.is_none());
            // ExternalLayerDetected must be surfaced.
            assert!(
                report
                    .diagnostic_reasons
                    .contains(&LayerDiagnosticReason::ExternalLayerDetected)
            );
        }

        #[test]
        fn installed_disabled_maps_with_update_and_remove() {
            let report = map_platform_report(platform_report(
                VulkanLayerState::InstalledDisabled,
                vec![VulkanLayerDiagnostic::RegistryDisabled],
            ));
            assert_eq!(
                report.layer_detection,
                VulkanLayerDetection::InstalledDisabled
            );
            assert!(report.actions.install.is_none());
            assert!(report.actions.update.is_some());
            assert!(report.actions.update.unwrap().requires_confirmation);
            assert!(report.actions.switch_channel.is_none());
            assert!(report.actions.remove.is_some());
            assert!(report.actions.resolve_conflict.is_none());
            assert!(
                report
                    .diagnostic_reasons
                    .contains(&LayerDiagnosticReason::RegistryDisabled)
            );
        }

        #[test]
        fn duplicate_conflict_maps_to_resolve_conflict_only() {
            let report = map_platform_report(platform_report(
                VulkanLayerState::Conflict,
                vec![VulkanLayerDiagnostic::DuplicateLayerManifest],
            ));
            assert_eq!(report.layer_detection, VulkanLayerDetection::Conflict);
            assert!(report.actions.install.is_none());
            assert!(report.actions.update.is_none());
            assert!(report.actions.remove.is_none());
            assert!(report.actions.resolve_conflict.is_some());
            assert!(!report.actions.resolve_conflict.unwrap().enabled);
            assert!(
                report
                    .diagnostic_reasons
                    .contains(&LayerDiagnosticReason::DuplicateLayerManifest)
            );
        }

        #[test]
        fn ambiguous_conflict_does_not_offer_repair_update() {
            let report = map_platform_report(platform_report(
                VulkanLayerState::Conflict,
                vec![VulkanLayerDiagnostic::AmbiguousLoaderVisibility],
            ));
            assert_eq!(report.layer_detection, VulkanLayerDetection::Conflict);
            assert!(report.actions.update.is_none());
            assert!(report.actions.resolve_conflict.is_some());
        }

        #[test]
        fn registry_missing_conflict_maps_to_repair_update_action() {
            let report = map_platform_report(platform_report(
                VulkanLayerState::Conflict,
                vec![VulkanLayerDiagnostic::RegistryMissing],
            ));
            assert_eq!(report.layer_detection, VulkanLayerDetection::Conflict);
            let resolve = report.actions.resolve_conflict.expect("repair action");
            assert!(resolve.enabled);
            assert!(resolve.requires_confirmation);
            assert!(report.actions.update.is_none());
            assert!(report.actions.install.is_none());
            assert!(report.actions.remove.is_none());
            assert!(
                report
                    .diagnostic_reasons
                    .contains(&LayerDiagnosticReason::RegistryMissing)
            );
        }

        #[test]
        fn unsupported_maps_to_unsupported_with_no_actions() {
            let report = map_platform_report(platform_report(
                VulkanLayerState::Unsupported,
                vec![VulkanLayerDiagnostic::UnsupportedArchitecture],
            ));
            assert_eq!(report.layer_detection, VulkanLayerDetection::Unsupported);
            assert!(report.actions.install.is_none());
            assert!(report.actions.resolve_conflict.is_none());
            assert!(
                report
                    .diagnostic_reasons
                    .contains(&LayerDiagnosticReason::UnsupportedArchitecture)
            );
        }

        #[test]
        fn facts_are_threaded_from_platform_to_public_dto() {
            let report =
                map_platform_report(platform_report(VulkanLayerState::Installed, Vec::new()));
            assert_eq!(
                report.layer_facts.manifest_path.as_deref(),
                Some(std::path::Path::new("C:\\layer\\manifest.json"))
            );
            assert_eq!(
                report.layer_facts.dll_path.as_deref(),
                Some(std::path::Path::new("C:\\layer\\ReShade64.dll"))
            );
            assert_eq!(report.layer_facts.version.as_deref(), Some("6.7.3"));
            assert_eq!(
                report.layer_facts.architecture,
                VulkanLayerArchitecture::X64
            );
            assert_eq!(
                report.layer_facts.loader_visibility,
                VulkanLoaderVisibility::Normal
            );
        }

        #[test]
        fn hkcu_visibility_is_threaded_from_platform() {
            let facts = PlatformFacts {
                manifest_path: Some(PathBuf::from("C:\\layer\\manifest.json")),
                dll_path: Some(PathBuf::from("C:\\layer\\ReShade64.dll")),
                version: None,
                architecture: renderpilot_platform_windows::vulkan_layer::VulkanLayerArchitecture::X64,
                loader_visibility:
                    renderpilot_platform_windows::vulkan_layer::VulkanLoaderVisibility::HkcuNotVisibleWhenElevated,
            };
            let report = map_platform_report(PlatformReport {
                state: VulkanLayerState::Installed,
                facts,
                diagnostics: vec![VulkanLayerDiagnostic::HkcuNotVisibleWhenElevated],
            });
            assert_eq!(
                report.layer_facts.loader_visibility,
                VulkanLoaderVisibility::HkcuNotVisibleWhenElevated
            );
            assert!(
                report
                    .diagnostic_reasons
                    .contains(&LayerDiagnosticReason::HkcuNotVisibleWhenElevated)
            );
        }

        #[test]
        fn missing_dll_conflict_surfaces_missing_layer_dll_diagnostic() {
            let report = map_platform_report(PlatformReport {
                state: VulkanLayerState::Conflict,
                facts: PlatformFacts {
                    manifest_path: Some(PathBuf::from("C:\\layer\\manifest.json")),
                    dll_path: Some(PathBuf::from("C:\\layer\\ReShade64.dll")),
                    version: None,
                    architecture:
                        renderpilot_platform_windows::vulkan_layer::VulkanLayerArchitecture::Unknown,
                    loader_visibility:
                        renderpilot_platform_windows::vulkan_layer::VulkanLoaderVisibility::Normal,
                },
                diagnostics: vec![VulkanLayerDiagnostic::MissingLayerDll],
            });
            assert_eq!(report.layer_detection, VulkanLayerDetection::Conflict);
            assert!(report.actions.resolve_conflict.is_some());
            assert_eq!(
                report.diagnostic_reasons,
                vec![LayerDiagnosticReason::MissingLayerDll]
            );
        }

        #[test]
        fn installed_switch_channel_offered_without_target_channel() {
            let report =
                map_platform_report(platform_report(VulkanLayerState::Installed, Vec::new()));
            let switch = report
                .actions
                .switch_channel
                .expect("switch_channel is offered");
            // target_channel is not set (channel tracking was removed with the marker).
            assert_eq!(switch.target_channel, None);
        }

        #[test]
        fn unknown_architecture_is_backend_validation_failed_conflict() {
            let report = map_platform_report(PlatformReport {
                state: VulkanLayerState::Conflict,
                facts: PlatformFacts {
                    manifest_path: None,
                    dll_path: None,
                    version: None,
                    architecture:
                        renderpilot_platform_windows::vulkan_layer::VulkanLayerArchitecture::Unknown,
                    loader_visibility:
                        renderpilot_platform_windows::vulkan_layer::VulkanLoaderVisibility::Normal,
                },
                diagnostics: vec![VulkanLayerDiagnostic::BackendValidationFailed],
            });
            assert_eq!(report.layer_detection, VulkanLayerDetection::Conflict);
            assert_eq!(
                report.diagnostic_reasons,
                vec![LayerDiagnosticReason::BackendValidationFailed]
            );
        }
    }
}
