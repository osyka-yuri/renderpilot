use std::fmt;

use renderpilot_domain::APP_NAME;

/// Static application metadata shared by entry points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AppInfo {
    name: &'static str,
    version: &'static str,
}

impl AppInfo {
    /// Creates static application metadata.
    #[must_use]
    pub const fn new(name: &'static str, version: &'static str) -> Self {
        Self { name, version }
    }

    /// Creates metadata for RenderPilot.
    #[must_use]
    pub const fn renderpilot(version: &'static str) -> Self {
        Self::new(APP_NAME, version)
    }

    /// Returns the user-facing application name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the semantic application version.
    #[must_use]
    pub const fn version(&self) -> &'static str {
        self.version
    }

    /// Formats the application name and version for CLI output.
    #[must_use]
    pub fn version_line(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for AppInfo {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.name, self.version)
    }
}

/// Builds RenderPilot application metadata for the given Cargo package version.
#[must_use]
pub const fn app_info(version: &'static str) -> AppInfo {
    AppInfo::renderpilot(version)
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::APP_NAME;

    use super::{app_info, AppInfo};

    #[test]
    fn app_info_uses_renderpilot_name() {
        let info = app_info("1.2.3");

        assert_eq!(info.name(), APP_NAME);
        assert_eq!(info.version(), "1.2.3");
    }

    #[test]
    fn display_formats_name_and_version() {
        let info = AppInfo::new("TestApp", "0.1.0");

        assert_eq!(info.to_string(), "TestApp 0.1.0");
    }

    #[test]
    fn version_line_matches_display_output() {
        let info = app_info("1.2.3");

        assert_eq!(info.version_line(), info.to_string());
        assert_eq!(info.version_line(), format!("{APP_NAME} 1.2.3"));
    }
}
