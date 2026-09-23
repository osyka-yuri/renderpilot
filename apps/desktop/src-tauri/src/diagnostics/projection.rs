//! Closed, privacy-safe event projections for the generic Rust and UI sinks.

use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticLevel {
    #[cfg(not(all(windows, feature = "portable")))]
    Info,
    Warning,
    Error,
}

impl DiagnosticLevel {
    #[cfg(not(all(windows, feature = "portable")))]
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RustLogCategory {
    MutationRecovery,
    AddonLifecycle,
    CatalogRecovery,
    CatalogHistory,
    RemoteCatalog,
    StorageFilesystem,
    ApiScanCover,
    Detection,
    WebviewRuntime,
    WorkspaceOther,
}

impl RustLogCategory {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::MutationRecovery => "mutation_recovery",
            Self::AddonLifecycle => "addon_lifecycle",
            Self::CatalogRecovery => "catalog_recovery",
            Self::CatalogHistory => "catalog_history",
            Self::RemoteCatalog => "remote_catalog",
            Self::StorageFilesystem => "storage_filesystem",
            Self::ApiScanCover => "api_scan_cover",
            Self::Detection => "detection",
            Self::WebviewRuntime => "webview_runtime",
            Self::WorkspaceOther => "workspace_other",
        }
    }

    pub(crate) fn for_module(module: &str) -> Option<Self> {
        if module_under(module, "renderpilot_orchestration::file_mutation")
            || module_under(module, "renderpilot_orchestration::peer_mutation_executor")
            || module_under(
                module,
                "renderpilot_orchestration::addons::shared_vulkan_mutation",
            )
        {
            Some(Self::MutationRecovery)
        } else if module_under(module, "renderpilot_orchestration::addons") {
            Some(Self::AddonLifecycle)
        } else if module_under(module, "renderpilot_orchestration::catalog::scan") {
            Some(Self::CatalogRecovery)
        } else if module_under(module, "renderpilot_orchestration::catalog::history") {
            Some(Self::CatalogHistory)
        } else if module_under(module, "renderpilot_orchestration::cdn")
            || module_under(module, "renderpilot_orchestration::manifests")
            || module_under(module, "renderpilot_orchestration::libraries")
        {
            Some(Self::RemoteCatalog)
        } else if module_under(module, "renderpilot_orchestration::storage")
            || module_under(module, "renderpilot_orchestration::fs")
            || module_under(module, "renderpilot_orchestration::util")
            || module_under(module, "renderpilot_orchestration::context")
            || module_under(module, "renderpilot_storage_sqlite")
        {
            Some(Self::StorageFilesystem)
        } else if module_under(module, "renderpilot_api::scan")
            || module_under(module, "renderpilot_api::covers")
        {
            Some(Self::ApiScanCover)
        } else if module_under(module, "renderpilot_detection") {
            Some(Self::Detection)
        } else if module_under(module, "renderpilot_desktop::webview_runtime") {
            Some(Self::WebviewRuntime)
        } else if module_under(module, "renderpilot_orchestration")
            || module_under(module, "renderpilot_api")
            || module_under(module, "renderpilot_desktop")
        {
            Some(Self::WorkspaceOther)
        } else {
            None
        }
    }
}

fn module_under(module: &str, prefix: &str) -> bool {
    module == prefix
        || module
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with("::"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RustLogEvent {
    pub(crate) level: DiagnosticLevel,
    pub(crate) category: RustLogCategory,
    pub(crate) site_id: [u8; 16],
    pub(crate) source_module: &'static str,
    pub(crate) source_line: u32,
    pub(crate) path: Option<String>,
}

impl RustLogEvent {
    pub(crate) fn from_site(
        level: tracing::Level,
        module: &'static str,
        line: u32,
    ) -> Option<Self> {
        let level = match level {
            tracing::Level::WARN => DiagnosticLevel::Warning,
            tracing::Level::ERROR => DiagnosticLevel::Error,
            _ => return None,
        };
        let category = RustLogCategory::for_module(module)?;
        let input = format!("{module}\0{line}\0{}", env!("CARGO_PKG_VERSION"));
        let digest = Sha256::digest(input.as_bytes());
        let mut site_id = [0; 16];
        site_id.copy_from_slice(&digest[..16]);
        Some(Self {
            level,
            category,
            site_id,
            source_module: module,
            source_line: line,
            path: None,
        })
    }

    pub(crate) fn site_id_hex(&self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(32);
        for byte in self.site_id {
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        output
    }
}

pub(crate) fn is_valid_diagnostic_path(path: &str) -> bool {
    !path.is_empty() && path.len() <= 1024 && !path.chars().any(char::is_control)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FrontendDiagnosticEvent {
    I18n {
        operation: I18nOperation,
        locale: LocaleCode,
        mode: LocaleMode,
    },
    ClientBoundary {
        level: DiagnosticLevel,
        code: BoundaryCode,
    },
    IpcTransport,
}

impl FrontendDiagnosticEvent {
    pub(crate) const fn level(self) -> DiagnosticLevel {
        match self {
            Self::I18n { .. }
            | Self::ClientBoundary {
                level: DiagnosticLevel::Warning,
                ..
            } => DiagnosticLevel::Warning,
            Self::ClientBoundary { level, .. } => level,
            Self::IpcTransport => DiagnosticLevel::Error,
        }
    }

    pub(crate) const fn phase_code(self) -> &'static str {
        match self {
            Self::I18n { .. } => "frontend_i18n",
            Self::ClientBoundary { .. } => "frontend_client_boundary",
            Self::IpcTransport => "frontend_ipc_transport",
        }
    }

    pub(crate) const fn event_code(self) -> &'static str {
        match self {
            Self::I18n { .. } => "locale_load_failed",
            Self::ClientBoundary {
                code: BoundaryCode::UnknownContract,
                ..
            } => "unknown_contract",
            Self::ClientBoundary {
                code: BoundaryCode::KnownLocal,
                ..
            } => "known_local_error",
            Self::ClientBoundary {
                code: BoundaryCode::MalformedContract,
                ..
            } => "malformed_contract",
            Self::IpcTransport => "transport_failed",
        }
    }

    pub(crate) const fn operation(self) -> Option<&'static str> {
        match self {
            Self::I18n { operation, .. } => Some(operation.code()),
            Self::ClientBoundary { .. } | Self::IpcTransport => None,
        }
    }

    pub(crate) const fn locale(self) -> Option<&'static str> {
        match self {
            Self::I18n { locale, .. } => Some(locale.code()),
            _ => None,
        }
    }

    pub(crate) const fn mode(self) -> Option<&'static str> {
        match self {
            Self::I18n { mode, .. } => Some(mode.code()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum I18nOperation {
    Initialize,
    Switch,
    SystemLanguageChange,
}

impl I18nOperation {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "initialize_locale" => Some(Self::Initialize),
            "switch_locale" => Some(Self::Switch),
            "system_language_change" => Some(Self::SystemLanguageChange),
            _ => None,
        }
    }

    const fn code(self) -> &'static str {
        match self {
            Self::Initialize => "initialize_locale",
            Self::Switch => "switch_locale",
            Self::SystemLanguageChange => "system_language_change",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocaleCode {
    En,
    De,
    Es,
    Fr,
    Ja,
    PtBr,
    Ru,
    ZhHans,
    ZhHant,
}

impl LocaleCode {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "en" => Some(Self::En),
            "de" => Some(Self::De),
            "es" => Some(Self::Es),
            "fr" => Some(Self::Fr),
            "ja" => Some(Self::Ja),
            "pt-BR" => Some(Self::PtBr),
            "ru" => Some(Self::Ru),
            "zh-Hans" => Some(Self::ZhHans),
            "zh-Hant" => Some(Self::ZhHant),
            _ => None,
        }
    }

    const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::De => "de",
            Self::Es => "es",
            Self::Fr => "fr",
            Self::Ja => "ja",
            Self::PtBr => "pt-BR",
            Self::Ru => "ru",
            Self::ZhHans => "zh-Hans",
            Self::ZhHant => "zh-Hant",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocaleMode {
    System,
    Locale(LocaleCode),
}

impl LocaleMode {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        if value == "system" {
            Some(Self::System)
        } else {
            LocaleCode::parse(value).map(Self::Locale)
        }
    }

    const fn code(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Locale(locale) => locale.code(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BoundaryCode {
    KnownLocal,
    UnknownContract,
    MalformedContract,
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticLevel, RustLogCategory, RustLogEvent, is_valid_diagnostic_path};

    #[test]
    fn rust_projection_includes_static_source_identity_and_site_hash() {
        let event = RustLogEvent::from_site(
            tracing::Level::WARN,
            "renderpilot_orchestration::catalog::scan",
            81,
        )
        .expect("workspace warning event");
        assert_eq!(event.level, DiagnosticLevel::Warning);
        assert_eq!(event.category, RustLogCategory::CatalogRecovery);
        assert_eq!(
            event.source_module,
            "renderpilot_orchestration::catalog::scan"
        );
        assert_eq!(event.source_line, 81);
        assert!(event.path.is_none());
        assert_eq!(event.site_id_hex().len(), 32);
        assert!(
            event
                .site_id_hex()
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        );
        assert_eq!(
            RustLogEvent::from_site(
                tracing::Level::WARN,
                "renderpilot_orchestration::catalog::scan",
                81,
            ),
            Some(event.clone()),
            "a callsite identity is deterministic within one app version"
        );
        assert_ne!(
            RustLogEvent::from_site(
                tracing::Level::WARN,
                "renderpilot_orchestration::catalog::scan",
                82,
            ),
            Some(event),
            "a source-line change produces a different callsite identity"
        );
    }

    #[test]
    fn rust_projection_requires_a_workspace_site_and_warn_or_error_level() {
        assert!(RustLogEvent::from_site(tracing::Level::INFO, "renderpilot_desktop", 9).is_none());
        assert!(RustLogEvent::from_site(tracing::Level::WARN, "reqwest::client", 9).is_none());
        assert_eq!(
            RustLogCategory::for_module("renderpilot_api::scanner"),
            Some(RustLogCategory::WorkspaceOther),
            "classification respects full module boundaries"
        );
        assert_eq!(
            RustLogCategory::for_module("renderpilot_storage_sqlite::mutation_journal"),
            Some(RustLogCategory::StorageFilesystem)
        );
        assert_eq!(
            RustLogCategory::for_module("renderpilot_desktop::commands::error"),
            Some(RustLogCategory::WorkspaceOther)
        );
    }

    #[test]
    fn diagnostic_paths_are_bounded_nonempty_and_free_of_control_characters() {
        assert!(is_valid_diagnostic_path("D:/Games/Example"));
        assert!(!is_valid_diagnostic_path(""));
        assert!(!is_valid_diagnostic_path(&"x".repeat(1025)));
        assert!(!is_valid_diagnostic_path("D:/Games\n/Example"));
    }
}
