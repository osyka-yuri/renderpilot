//! Error type for the GUI API facade.

use renderpilot_orchestration::application::AppError;
use renderpilot_orchestration::ServiceError;

/// Errors produced by the GUI API facade functions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiError {
    /// A game identifier could not be parsed.
    InvalidGameId(String),
    /// A component identifier could not be parsed.
    InvalidComponentId(String),
    /// An artifact identifier could not be parsed.
    InvalidArtifactId(String),
    /// An operation identifier could not be parsed.
    InvalidOperationId(String),
    /// API output could not be serialized to JSON.
    OutputSerializationFailed(String),
    /// A service-layer error from orchestration.
    Service(ServiceError),
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGameId(id) => write!(f, "invalid game id: {id}"),
            Self::InvalidComponentId(id) => write!(f, "invalid component id: {id}"),
            Self::InvalidArtifactId(id) => write!(f, "invalid artifact id: {id}"),
            Self::InvalidOperationId(id) => write!(f, "invalid operation id: {id}"),
            Self::OutputSerializationFailed(msg) => {
                write!(f, "could not serialize output: {msg}")
            }
            Self::Service(e) => std::fmt::Display::fmt(e, f),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<ServiceError> for ApiError {
    fn from(error: ServiceError) -> Self {
        Self::Service(error)
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        Self::Service(ServiceError::from(error))
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self::OutputSerializationFailed(error.to_string())
    }
}
