//! Stable, presentation-free updater IPC contract.

use serde::Serialize;

use crate::commands::CommandError;

/// Exact response returned by `app_update_check`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateCheckResponse {
    pub(super) session_id: String,
    pub(super) metadata: AppUpdateMetadata,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AppUpdateMetadata {
    pub(super) current_version: String,
    pub(super) version: String,
    pub(super) date: Option<String>,
    pub(super) body: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AppUpdateDownloadEvent {
    Started {
        #[serde(rename = "contentLength")]
        content_length: Option<u64>,
    },
    Progress {
        #[serde(rename = "chunkLength")]
        chunk_length: usize,
    },
    Finished,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AppUpdateApplyResponse {
    #[cfg(not(all(windows, feature = "portable")))]
    Installed,
    #[cfg(all(windows, feature = "portable"))]
    NativeExit,
}

pub(super) type UpdateResult<T> = Result<T, CommandError>;

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn wire_contract_keeps_exact_tags_and_error_shape() {
        assert_eq!(
            serde_json::to_value(AppUpdateDownloadEvent::Started {
                content_length: Some(42),
            })
            .expect("serialize download event"),
            json!({ "type": "started", "contentLength": 42 })
        );
        assert_eq!(
            serde_json::to_value(AppUpdateDownloadEvent::Progress { chunk_length: 7 })
                .expect("serialize progress event"),
            json!({ "type": "progress", "chunkLength": 7 })
        );
        assert_eq!(
            serde_json::to_value(AppUpdateDownloadEvent::Finished)
                .expect("serialize finished event"),
            json!({ "type": "finished" })
        );
        #[cfg(all(windows, feature = "portable"))]
        assert_eq!(
            serde_json::to_value(AppUpdateApplyResponse::NativeExit)
                .expect("serialize portable apply response"),
            json!({ "type": "native-exit" })
        );
        #[cfg(not(all(windows, feature = "portable")))]
        assert_eq!(
            serde_json::to_value(AppUpdateApplyResponse::Installed)
                .expect("serialize installed apply response"),
            json!({ "type": "installed" })
        );
        assert_eq!(
            serde_json::to_value(AppUpdateCheckResponse {
                session_id: "session".to_owned(),
                metadata: AppUpdateMetadata {
                    current_version: "1.8.2".to_owned(),
                    version: "1.9.0".to_owned(),
                    date: None,
                    body: "notes".to_owned(),
                },
            })
            .expect("serialize check response"),
            json!({
                "sessionId": "session",
                "metadata": {
                    "currentVersion": "1.8.2",
                    "version": "1.9.0",
                    "date": null,
                    "body": "notes"
                }
            })
        );
    }
}
