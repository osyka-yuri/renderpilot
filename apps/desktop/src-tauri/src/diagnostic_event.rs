//! Closed backend events admitted to the portable App diagnostic transcript.
//!
//! None of these variants accepts error prose, paths, identifiers supplied by
//! a caller, or formatting arguments. Console logging remains the sole owner
//! of operational detail.

use crate::command_error_contract::CommandErrorKind;
#[cfg(any(all(windows, feature = "portable"), test))]
use crate::command_error_contract::CommandErrorSeverity;

macro_rules! closed_codes {
    ($(#[$impl_meta:meta])* $name:ident { $($variant:ident => $code:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(crate) enum $name {
            $($variant),+
        }

        $(#[$impl_meta])*
        impl $name {
            pub(crate) const fn code(self) -> &'static str {
                match self {
                    $(Self::$variant => $code),+
                }
            }
        }
    };
}

closed_codes! {
    CommandOperation {
        InspectGameInstall => "inspect_game_install",
        AddGame => "add_game",
        RemoveGameFromCatalog => "remove_game_from_catalog",
        ScanAutoLibraries => "scan_auto_libraries",
        RefreshRemoteManifests => "refresh_remote_manifests",
        QueryGameCards => "query_game_cards",
        BootstrapGamesCatalog => "bootstrap_games_catalog",
        GetGameDetails => "get_game_details",
        GetGameFileSafetyAssessment => "get_game_file_safety_assessment",
        GetSharedVulkanSafetyAssessment => "get_shared_vulkan_safety_assessment",
        FetchGameCover => "fetch_game_cover",
        ClearGameCover => "clear_game_cover",
        SetGameCover => "set_game_cover",
        SetGameFavorite => "set_game_favorite",
        SetGameHidden => "set_game_hidden",
        GetCatalogSetting => "get_catalog_setting",
        SetCatalogSetting => "set_catalog_setting",
        ApplySwap => "apply_swap",
        PlanSwap => "plan_swap",
        RollbackComponent => "rollback_component",
        PlanRollback => "plan_rollback",
        ListLibraryPackages => "list_library_packages",
        DownloadLibraryPackage => "download_library_package",
        DownloadArtifact => "download_artifact",
        DeleteLibraryPackage => "delete_library_package",
        ListNvapiSupportedSettings => "list_nvapi_supported_settings",
        ListNvapiSettingStates => "list_nvapi_setting_states",
        ListGameExecutableCandidates => "list_game_executable_candidates",
        ResolveGameExecutable => "resolve_game_executable",
        SetGameExecutableOverride => "set_game_executable_override",
        ClearGameExecutableOverride => "clear_game_executable_override",
        GetNvapiSettingState => "get_nvapi_setting_state",
        SetNvapiSettingValue => "set_nvapi_setting_value",
        RevertNvapiSetting => "revert_nvapi_setting",
        ListGlobalNvapiSettingStates => "list_global_nvapi_setting_states",
        SetGlobalNvapiSettingValue => "set_global_nvapi_setting_value",
        RevertGlobalNvapiSetting => "revert_global_nvapi_setting",
        GetDlssIndicatorState => "get_dlss_indicator_state",
        SetDlssIndicatorEnabled => "set_dlss_indicator_enabled",
        RenodxAvailability => "renodx_availability",
        RenodxInstall => "renodx_install",
        RenodxInstallFromFile => "renodx_install_from_file",
        RenodxSwitchReshadeChannel => "renodx_switch_reshade_channel",
        RenodxUninstall => "renodx_uninstall",
        RenodxVulkanLayerStatus => "renodx_vulkan_layer_status",
        RenodxVulkanLayerManagementStatus => "renodx_vulkan_layer_management_status",
        RenodxApplyVulkanLayer => "renodx_apply_vulkan_layer",
        RenodxRemoveVulkanLayer => "renodx_remove_vulkan_layer",
        RenodxCheckUpdate => "renodx_check_update",
        RenodxUpdate => "renodx_update",
        RenodxInstallDlssFix => "renodx_install_dlss_fix",
        RenodxUpdateDlssFix => "renodx_update_dlss_fix",
        RenodxRetryDlssFixRecovery => "renodx_retry_dlss_fix_recovery",
        RenodxUninstallDlssFix => "renodx_uninstall_dlss_fix",
        RenodxDlssFixAvailability => "renodx_dlss_fix_availability",
        LumaAvailability => "luma_availability",
        LumaInstall => "luma_install",
        LumaUninstall => "luma_uninstall",
        LumaCheckUpdate => "luma_check_update",
        LumaUpdate => "luma_update",
        AppUpdateCheck => "app_update_check",
        AppUpdateDownload => "app_update_download",
        AppUpdateApply => "app_update_apply",
        AppUpdateClose => "app_update_close"
    }
}

closed_codes! {
    #[cfg(any(all(windows, feature = "portable"), test))]
    CatalogRefreshPhase {
        Scan => "catalog_scan",
        RemoteCatalog => "catalog_remote_catalog",
        Capabilities => "catalog_capabilities",
        LiveValidation => "catalog_live_validation",
        Revision => "catalog_revision"
    }
}

closed_codes! {
    #[cfg(any(all(windows, feature = "portable"), test))]
    CapabilityOperation {
        RefreshCatalogCapabilities => "refresh_catalog_capabilities",
        RefreshGameCatalogAddonCapabilities => "refresh_game_catalog_addon_capabilities"
    }
}

closed_codes! {
    #[cfg(any(all(windows, feature = "portable"), test))]
    CoverGcOperation {
        StartupCoverGc => "startup_cover_gc",
        ClearGameCover => "clear_game_cover"
    }
}

closed_codes! {
    #[cfg(any(all(windows, feature = "portable"), test))]
    EventPublicationOperation {
        CatalogDelta => "catalog_delta",
        CatalogSyncState => "catalog_sync_state"
    }
}

/// A type-safe event closed over the only backend failures approved for
/// persistence. Each constructor fixes every field combination admitted to
/// the portable diagnostic v1 schema.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BackendDiagnosticEvent {
    CommandFailure {
        operation: CommandOperation,
        kind: CommandErrorKind,
    },
    CatalogIssue {
        phase: CatalogRefreshPhase,
    },
    CapabilityFailure {
        operation: CapabilityOperation,
    },
    CoverGcFailure {
        operation: CoverGcOperation,
    },
    EventPublicationFailure {
        operation: EventPublicationOperation,
    },
}

impl BackendDiagnosticEvent {
    pub(crate) const fn command_failure(
        operation: CommandOperation,
        kind: CommandErrorKind,
    ) -> Self {
        Self::CommandFailure { operation, kind }
    }

    pub(crate) const fn catalog_issue(phase: CatalogRefreshPhase) -> Self {
        Self::CatalogIssue { phase }
    }

    pub(crate) const fn capability_failure(operation: CapabilityOperation) -> Self {
        Self::CapabilityFailure { operation }
    }

    pub(crate) const fn cover_gc_failure(operation: CoverGcOperation) -> Self {
        Self::CoverGcFailure { operation }
    }

    pub(crate) const fn event_publication_failure(operation: EventPublicationOperation) -> Self {
        Self::EventPublicationFailure { operation }
    }

    #[cfg(any(all(windows, feature = "portable"), test))]
    pub(crate) const fn record(self) -> BackendDiagnosticRecord {
        match self {
            Self::CommandFailure { operation, kind } => BackendDiagnosticRecord {
                level: match kind.severity() {
                    CommandErrorSeverity::Warning => BackendDiagnosticLevel::Warning,
                    CommandErrorSeverity::Error => BackendDiagnosticLevel::Error,
                },
                phase: "command",
                code: kind.code(),
                operation: Some(operation.code()),
            },
            Self::CatalogIssue { phase } => BackendDiagnosticRecord {
                level: BackendDiagnosticLevel::Warning,
                phase: phase.code(),
                code: "catalog_refresh_failed",
                operation: None,
            },
            Self::CapabilityFailure { operation } => BackendDiagnosticRecord {
                level: BackendDiagnosticLevel::Warning,
                phase: "capability_refresh",
                code: "capability_refresh_failed",
                operation: Some(operation.code()),
            },
            Self::CoverGcFailure { operation } => BackendDiagnosticRecord {
                level: BackendDiagnosticLevel::Warning,
                phase: "cover_gc",
                code: "orphan_cleanup_failed",
                operation: Some(operation.code()),
            },
            Self::EventPublicationFailure { operation } => BackendDiagnosticRecord {
                level: BackendDiagnosticLevel::Warning,
                phase: "event_publication",
                code: "event_publication_failed",
                operation: Some(operation.code()),
            },
        }
    }
}

#[cfg(any(all(windows, feature = "portable"), test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BackendDiagnosticLevel {
    Warning,
    Error,
}

/// Internal rendering data. Its fields come exclusively from closed enums.
#[cfg(any(all(windows, feature = "portable"), test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BackendDiagnosticRecord {
    level: BackendDiagnosticLevel,
    phase: &'static str,
    code: &'static str,
    operation: Option<&'static str>,
}

#[cfg(all(windows, feature = "portable"))]
impl BackendDiagnosticRecord {
    pub(crate) const fn level(self) -> BackendDiagnosticLevel {
        self.level
    }

    pub(crate) const fn phase(self) -> &'static str {
        self.phase
    }

    pub(crate) const fn code(self) -> &'static str {
        self.code
    }

    pub(crate) const fn operation(self) -> Option<&'static str> {
        self.operation
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackendDiagnosticEvent, BackendDiagnosticLevel, CapabilityOperation, CatalogRefreshPhase,
        CommandOperation, CoverGcOperation, EventPublicationOperation,
    };
    use crate::command_error_contract::CommandErrorKind;

    #[test]
    fn command_failure_is_closed_and_preserves_the_generated_severity() {
        let record = BackendDiagnosticEvent::command_failure(
            CommandOperation::ClearGameCover,
            CommandErrorKind::StorageFailed,
        )
        .record();
        assert_eq!(record.level, BackendDiagnosticLevel::Error);
        assert_eq!(record.phase, "command");
        assert_eq!(record.code, "storage_failed");
        assert_eq!(record.operation, Some("clear_game_cover"));
    }

    #[test]
    fn soft_failure_shapes_have_no_detail_slot() {
        let catalog = BackendDiagnosticEvent::catalog_issue(CatalogRefreshPhase::Scan).record();
        let capability = BackendDiagnosticEvent::capability_failure(
            CapabilityOperation::RefreshCatalogCapabilities,
        )
        .record();
        let cover =
            BackendDiagnosticEvent::cover_gc_failure(CoverGcOperation::StartupCoverGc).record();
        let event = BackendDiagnosticEvent::event_publication_failure(
            EventPublicationOperation::CatalogDelta,
        )
        .record();

        assert_eq!(catalog.operation, None);
        assert_eq!(catalog.phase, "catalog_scan");
        assert_eq!(capability.operation, Some("refresh_catalog_capabilities"));
        assert_eq!(cover.code, "orphan_cleanup_failed");
        assert_eq!(event.phase, "event_publication");
    }
}
