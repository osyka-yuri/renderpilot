//! Closed backend events admitted to installed and portable App diagnostics.
//!
//! Command failures retain only allowlisted reason codes and a selected game
//! root for stale inspection. Arbitrary error prose and formatting arguments
//! remain in console logging.

use crate::command_error_contract::{CommandErrorKind, CommandErrorSeverity};

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
        GetNvapiProfileStatus => "get_nvapi_profile_status",
        CreateNvapiProfile => "create_nvapi_profile",
        DeleteNvapiProfile => "delete_nvapi_profile",
        MoveNvapiProfile => "move_nvapi_profile",
        ListGlobalNvapiSettingStates => "list_global_nvapi_setting_states",
        SetGlobalNvapiSettingValue => "set_global_nvapi_setting_value",
        RevertGlobalNvapiSetting => "revert_global_nvapi_setting",
        GetDlssIndicatorState => "get_dlss_indicator_state",
        SetDlssIndicatorEnabled => "set_dlss_indicator_enabled",
        RenodxAvailability => "renodx_availability",
        RenodxApplyEngineConfig => "renodx_apply_engine_config",
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
        LumaApplyEngineConfig => "luma_apply_engine_config",
        LumaInstall => "luma_install",
        LumaUninstall => "luma_uninstall",
        LumaCheckUpdate => "luma_check_update",
        LumaUpdate => "luma_update",
        OptiScalerAvailability => "get_optiscaler_availability",
        OptiScalerInstall => "install_optiscaler",
        OptiScalerCheckUpdate => "check_optiscaler_update",
        OptiScalerUpdate => "update_optiscaler",
        OptiScalerRepair => "repair_optiscaler",
        OptiScalerSetModules => "set_optiscaler_modules",
        OptiScalerRelocate => "relocate_optiscaler",
        OptiScalerUninstall => "uninstall_optiscaler",
        AppUpdateCheck => "app_update_check",
        AppUpdateDownload => "app_update_download",
        AppUpdateApply => "app_update_apply",
        AppUpdateClose => "app_update_close"
    }
}

closed_codes! {
    CatalogRefreshPhase {
        Scan => "catalog_scan",
        RemoteCatalog => "catalog_remote_catalog",
        Capabilities => "catalog_capabilities",
        LiveValidation => "catalog_live_validation",
        Revision => "catalog_revision"
    }
}

closed_codes! {
    CapabilityOperation {
        RefreshCatalogCapabilities => "refresh_catalog_capabilities",
        RefreshGameCatalogAddonCapabilities => "refresh_game_catalog_addon_capabilities"
    }
}

closed_codes! {
    CoverGcOperation {
        StartupCoverGc => "startup_cover_gc",
        ClearGameCover => "clear_game_cover"
    }
}

closed_codes! {
    EventPublicationOperation {
        CatalogDelta => "catalog_delta",
        CatalogSyncState => "catalog_sync_state"
    }
}

/// A type-safe event closed over the backend failures approved for persistence.
/// Reason codes and paths are validated before the event enters a profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BackendDiagnosticEvent {
    CommandFailure {
        operation: CommandOperation,
        kind: CommandErrorKind,
        reason_code: Option<&'static str>,
        path: Option<String>,
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
        reason_code: Option<&'static str>,
        path: Option<String>,
    ) -> Self {
        Self::CommandFailure {
            operation,
            kind,
            reason_code,
            path,
        }
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

    pub(crate) fn record(&self) -> BackendDiagnosticRecord<'_> {
        match self {
            Self::CommandFailure {
                operation,
                kind,
                reason_code,
                path,
            } => BackendDiagnosticRecord {
                level: match kind.severity() {
                    CommandErrorSeverity::Warning => BackendDiagnosticLevel::Warning,
                    CommandErrorSeverity::Error => BackendDiagnosticLevel::Error,
                },
                phase: "command",
                code: kind.code(),
                operation: Some(operation.code()),
                reason_code: reason_code.filter(|code| kind.allows_reason_code(code)),
                path: path.as_deref().filter(|path| {
                    *kind == CommandErrorKind::StaleInstallInspection
                        && crate::diagnostics::is_valid_diagnostic_path(path)
                }),
            },
            Self::CatalogIssue { phase } => BackendDiagnosticRecord {
                level: BackendDiagnosticLevel::Warning,
                phase: phase.code(),
                code: "catalog_refresh_failed",
                operation: None,
                reason_code: None,
                path: None,
            },
            Self::CapabilityFailure { operation } => BackendDiagnosticRecord {
                level: BackendDiagnosticLevel::Warning,
                phase: "capability_refresh",
                code: "capability_refresh_failed",
                operation: Some(operation.code()),
                reason_code: None,
                path: None,
            },
            Self::CoverGcFailure { operation } => BackendDiagnosticRecord {
                level: BackendDiagnosticLevel::Warning,
                phase: "cover_gc",
                code: "orphan_cleanup_failed",
                operation: Some(operation.code()),
                reason_code: None,
                path: None,
            },
            Self::EventPublicationFailure { operation } => BackendDiagnosticRecord {
                level: BackendDiagnosticLevel::Warning,
                phase: "event_publication",
                code: "event_publication_failed",
                operation: Some(operation.code()),
                reason_code: None,
                path: None,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BackendDiagnosticLevel {
    Warning,
    Error,
}

/// Internal rendering data. Its fields come exclusively from closed enums.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BackendDiagnosticRecord<'a> {
    level: BackendDiagnosticLevel,
    phase: &'static str,
    code: &'static str,
    operation: Option<&'static str>,
    reason_code: Option<&'static str>,
    path: Option<&'a str>,
}

impl<'a> BackendDiagnosticRecord<'a> {
    pub(crate) const fn level(&self) -> BackendDiagnosticLevel {
        self.level
    }

    pub(crate) const fn phase(&self) -> &'static str {
        self.phase
    }

    pub(crate) const fn code(&self) -> &'static str {
        self.code
    }

    pub(crate) const fn operation(&self) -> Option<&'static str> {
        self.operation
    }

    pub(crate) const fn reason_code(&self) -> Option<&'static str> {
        self.reason_code
    }

    pub(crate) const fn path(&self) -> Option<&'a str> {
        self.path
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
        let event = BackendDiagnosticEvent::command_failure(
            CommandOperation::InspectGameInstall,
            CommandErrorKind::InvalidInstallRoot,
            Some("contains_proven_install"),
            Some("D:/Games/Example".to_owned()),
        );
        let record = event.record();
        assert_eq!(record.level, BackendDiagnosticLevel::Warning);
        assert_eq!(record.phase, "command");
        assert_eq!(record.code, "invalid_install_root");
        assert_eq!(record.operation, Some("inspect_game_install"));
        assert_eq!(record.reason_code(), Some("contains_proven_install"));
        assert_eq!(record.path, None);
    }

    #[test]
    fn command_failure_keeps_only_the_stale_inspection_path() {
        let allowed_event = BackendDiagnosticEvent::command_failure(
            CommandOperation::InspectGameInstall,
            CommandErrorKind::StaleInstallInspection,
            None,
            Some("D:/Games/Example".to_owned()),
        );
        let allowed = allowed_event.record();
        assert_eq!(allowed.path, Some("D:/Games/Example"));

        let disallowed_event = BackendDiagnosticEvent::CommandFailure {
            operation: CommandOperation::InspectGameInstall,
            kind: CommandErrorKind::StorageFailed,
            reason_code: Some("contains_proven_install"),
            path: Some("D:/Games/Example".to_owned()),
        };
        let disallowed = disallowed_event.record();
        assert_eq!(disallowed.path, None);
        assert_eq!(disallowed.reason_code(), None);

        let malformed_event = BackendDiagnosticEvent::CommandFailure {
            operation: CommandOperation::InspectGameInstall,
            kind: CommandErrorKind::StaleInstallInspection,
            reason_code: None,
            path: Some("D:/Games\n/Example".to_owned()),
        };
        let malformed = malformed_event.record();
        assert_eq!(malformed.path, None);
    }

    #[test]
    fn soft_failure_shapes_have_no_detail_slot() {
        let catalog_event = BackendDiagnosticEvent::catalog_issue(CatalogRefreshPhase::Scan);
        let catalog = catalog_event.record();
        let capability_event = BackendDiagnosticEvent::capability_failure(
            CapabilityOperation::RefreshCatalogCapabilities,
        );
        let capability = capability_event.record();
        let cover_event =
            BackendDiagnosticEvent::cover_gc_failure(CoverGcOperation::StartupCoverGc);
        let cover = cover_event.record();
        let publication_event = BackendDiagnosticEvent::event_publication_failure(
            EventPublicationOperation::CatalogDelta,
        );
        let event = publication_event.record();

        assert_eq!(catalog.operation, None);
        assert_eq!(catalog.phase, "catalog_scan");
        assert_eq!(capability.operation, Some("refresh_catalog_capabilities"));
        assert_eq!(cover.code, "orphan_cleanup_failed");
        assert_eq!(event.phase, "event_publication");
    }

    #[test]
    fn command_failure_drops_unapproved_reason_codes_and_invalid_paths() {
        let event = BackendDiagnosticEvent::command_failure(
            CommandOperation::InspectGameInstall,
            CommandErrorKind::StorageFailed,
            Some("contains_proven_install"),
            Some("D:/Games\n/Example".to_owned()),
        );
        let record = event.record();
        assert!(record.reason_code().is_none());
        assert!(record.path.is_none());
        assert_eq!(record.code(), "storage_failed");
    }
}
