use std::collections::BTreeSet;
#[cfg(any(windows, test))]
use std::path::Path;

use renderpilot_domain::{
    ArtifactId, ComponentId, GameId, GraphicsComponent, GraphicsTechnology, OperationId,
    Swappability,
};
use serde::Serialize;
use serde_json::Value;

use crate::CliError;

pub(crate) type JsonResult = Result<Value, CliError>;

pub(crate) fn to_json<T: Serialize>(value: T) -> JsonResult {
    serde_json::to_value(value).map_err(Into::into)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DashboardRiskLevel {
    Unknown,
    Low,
    Medium,
    High,
}

impl DashboardRiskLevel {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

pub(crate) fn available_update_count<'a>(
    groups: impl IntoIterator<Item = &'a renderpilot_application::ComponentFileReplacementCandidates>,
) -> usize {
    groups
        .into_iter()
        .filter(|group| {
            group.candidates().iter().any(|candidate| {
                candidate.comparison() == renderpilot_application::CandidateComparison::NewerVersion
            })
        })
        .count()
}

pub(crate) fn library_tags(components: &[GraphicsComponent]) -> Vec<String> {
    components
        .iter()
        .filter(|component| is_visible_graphics_technology(component.technology()))
        .map(|component| component.technology().as_slug().to_owned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

pub(crate) fn is_visible_graphics_technology(technology: GraphicsTechnology) -> bool {
    technology != GraphicsTechnology::Unknown
}

pub(crate) fn dashboard_risk_level(components: &[GraphicsComponent]) -> DashboardRiskLevel {
    components
        .iter()
        .map(component_risk_level)
        .max()
        .unwrap_or(DashboardRiskLevel::Unknown)
}

pub(crate) fn component_risk_level(component: &GraphicsComponent) -> DashboardRiskLevel {
    if is_high_risk_component(component) {
        DashboardRiskLevel::High
    } else if is_medium_risk_component(component) {
        DashboardRiskLevel::Medium
    } else if is_low_risk_component(component) {
        DashboardRiskLevel::Low
    } else {
        DashboardRiskLevel::Unknown
    }
}

pub(crate) fn is_high_risk_component(component: &GraphicsComponent) -> bool {
    matches!(
        component.swappability(),
        Swappability::Unsafe | Swappability::IntegratedIntoEngine
    )
}

pub(crate) fn is_medium_risk_component(component: &GraphicsComponent) -> bool {
    matches!(
        component.swappability(),
        Swappability::BundleOnly | Swappability::ReadOnly
    )
}

pub(crate) fn is_low_risk_component(component: &GraphicsComponent) -> bool {
    component.swappability() == Swappability::Swappable
}

#[cfg(any(windows, test))]
pub(crate) fn normalized_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(crate) fn parse_game_id(value: impl Into<String>) -> Result<GameId, CliError> {
    parse_identifier(value, CliError::InvalidGameId)
}

pub(crate) fn parse_component_id(value: impl Into<String>) -> Result<ComponentId, CliError> {
    parse_identifier(value, CliError::InvalidComponentId)
}

pub(crate) fn parse_artifact_id(value: impl Into<String>) -> Result<ArtifactId, CliError> {
    parse_identifier(value, CliError::InvalidArtifactId)
}

pub(crate) fn parse_operation_id(value: impl Into<String>) -> Result<OperationId, CliError> {
    parse_identifier(value, CliError::InvalidOperationId)
}

pub(crate) fn parse_identifier<T>(
    value: impl Into<String>,
    invalid: fn(String) -> CliError,
) -> Result<T, CliError>
where
    T: TryFrom<String>,
{
    let value = value.into();

    T::try_from(value.clone()).map_err(|_| invalid(value))
}
