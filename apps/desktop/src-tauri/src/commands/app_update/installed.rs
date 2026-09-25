//! Installed Tauri/NSIS updater backend.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use renderpilot_orchestration::github_auth::{
    GitHubToken, authorization_header_name, is_github_auth_target_url,
};
use tauri::ipc::Channel;
use tauri_plugin_updater::UpdaterExt;

use super::{
    AppUpdateApplyResponse, AppUpdateDownloadEvent,
    dto::{AppUpdateMetadata, UpdateResult},
    session::{self, AppUpdateState, UpdateSession},
};
use crate::commands::{CommandError, error::CommandErrorKind};

#[expect(
    dead_code,
    reason = "generated contract also contains UPDATER_PUBLIC_KEY, unused by the installed updater"
)]
mod updater_contract {
    include!(concat!(env!("OUT_DIR"), "/updater_contract.rs"));
}

pub(super) struct CheckedUpdate {
    pub(super) metadata: AppUpdateMetadata,
    pub(super) update: tauri_plugin_updater::Update,
}

pub(super) async fn check(app: &tauri::AppHandle) -> UpdateResult<Option<CheckedUpdate>> {
    let endpoints = updater_contract::UPDATER_ENDPOINTS;
    let endpoints_are_github =
        !endpoints.is_empty() && endpoints.iter().all(|url| is_github_auth_target_url(url));
    let token = if endpoints_are_github {
        GitHubToken::from_local_machine_async().await
    } else {
        None
    };
    let authorization = endpoints.first().and_then(|endpoint| {
        token
            .as_ref()
            .and_then(|token| token.authorization_header_for_url_str(endpoint))
    });
    let authenticated = authorization.is_some();
    let updater_builder = app.updater_builder();
    let updater = if let Some(header) = authorization {
        updater_builder
            .header(authorization_header_name(), header)
            .and_then(|builder| builder.build())
    } else {
        updater_builder.build()
    }
    .map_err(|error| {
        CommandError::with_diagnostic(CommandErrorKind::AppUpdateCheckFailed, error)
    })?;
    let check = updater.check().await;
    drop(updater);
    drop(token);
    let checked = match check {
        Err(error) if should_retry_anonymous_check(authenticated, &error) => {
            // Reuse no credential source: the retry is explicitly anonymous.
            app.updater_builder()
                .build()
                .map_err(|error| {
                    CommandError::with_diagnostic(CommandErrorKind::AppUpdateCheckFailed, error)
                })?
                .check()
                .await
        }
        result => result,
    };
    let Some(mut update) = checked.map_err(|error| {
        CommandError::with_diagnostic(CommandErrorKind::AppUpdateCheckFailed, error)
    })?
    else {
        return Ok(None);
    };
    sanitize_download_headers(&mut update);
    let metadata = AppUpdateMetadata {
        current_version: update.current_version.clone(),
        version: update.version.clone(),
        date: update.date.map(|date| date.to_string()),
        body: update.body.clone().unwrap_or_default(),
    };
    Ok(Some(CheckedUpdate { metadata, update }))
}

pub(super) async fn download(
    state: &AppUpdateState,
    session_id: String,
    on_event: Channel<AppUpdateDownloadEvent>,
) -> UpdateResult<()> {
    let mut update = take_checked(state, &session_id)?;
    let authenticated = update.headers.contains_key(authorization_header_name());
    let mut result = download_once(&update, &on_event).await;
    if result
        .as_ref()
        .is_err_and(|error| should_retry_anonymous_download(authenticated, error))
    {
        // Clear the token before retry and before restoring the checked session
        // if the anonymous request also fails.
        update.headers.remove(authorization_header_name());
        result = download_once(&update, &on_event).await;
    }
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(error) => {
            *session::lock(state)? = UpdateSession::Checked {
                id: session_id,
                update,
            };
            return Err(CommandError::with_diagnostic(
                CommandErrorKind::AppUpdateDownloadFailed,
                error,
            ));
        }
    };
    update.headers.remove(authorization_header_name());
    *session::lock(state)? = UpdateSession::Downloaded {
        id: session_id,
        update,
        bytes,
    };
    Ok(())
}

async fn download_once(
    update: &tauri_plugin_updater::Update,
    on_event: &Channel<AppUpdateDownloadEvent>,
) -> Result<Vec<u8>, tauri_plugin_updater::Error> {
    let started = Arc::new(AtomicBool::new(false));
    let chunk_started = Arc::clone(&started);
    let finish_started = Arc::clone(&started);
    let chunk_events = on_event.clone();
    let finish_events = on_event.clone();
    update
        .download(
            move |chunk_length, content_length| {
                if !chunk_started.swap(true, Ordering::AcqRel) {
                    let _ = chunk_events.send(AppUpdateDownloadEvent::Started { content_length });
                }
                let _ = chunk_events.send(AppUpdateDownloadEvent::Progress { chunk_length });
            },
            move || {
                if !finish_started.swap(true, Ordering::AcqRel) {
                    let _ = finish_events.send(AppUpdateDownloadEvent::Started {
                        content_length: None,
                    });
                }
                let _ = finish_events.send(AppUpdateDownloadEvent::Finished);
            },
        )
        .await
}

fn sanitize_download_headers(update: &mut tauri_plugin_updater::Update) {
    if should_strip_authorization_for_download(update.download_url.as_str()) {
        update.headers.remove(authorization_header_name());
    }
}

fn should_retry_anonymous_check(authenticated: bool, error: &tauri_plugin_updater::Error) -> bool {
    authenticated && matches!(error, tauri_plugin_updater::Error::ReleaseNotFound)
}

fn should_retry_anonymous_download(
    authenticated: bool,
    error: &tauri_plugin_updater::Error,
) -> bool {
    authenticated && matches!(error, tauri_plugin_updater::Error::Network(_))
}

fn should_strip_authorization_for_download(url: &str) -> bool {
    !is_github_auth_target_url(url)
}

pub(super) fn apply(
    state: &AppUpdateState,
    session_id: String,
) -> UpdateResult<AppUpdateApplyResponse> {
    let (update, bytes) = take_downloaded(state, &session_id)?;
    if let Err(error) = update.install(&bytes) {
        *session::lock(state)? = UpdateSession::Downloaded {
            id: session_id,
            update,
            bytes,
        };
        return Err(CommandError::with_diagnostic(
            CommandErrorKind::AppUpdateInstallFailed,
            error,
        ));
    }
    Ok(AppUpdateApplyResponse::Installed)
}

fn take_checked(state: &AppUpdateState, id: &str) -> UpdateResult<tauri_plugin_updater::Update> {
    let mut session = session::lock(state)?;
    match std::mem::replace(&mut *session, UpdateSession::Idle) {
        UpdateSession::Checked { id: actual, update } if actual == id => Ok(update),
        other => {
            *session = other;
            Err(CommandError::with_diagnostic(
                CommandErrorKind::AppUpdateInvalidSession,
                "updater session was not ready to download",
            ))
        }
    }
}

fn take_downloaded(
    state: &AppUpdateState,
    id: &str,
) -> UpdateResult<(tauri_plugin_updater::Update, Vec<u8>)> {
    let mut session = session::lock(state)?;
    match std::mem::replace(&mut *session, UpdateSession::Idle) {
        UpdateSession::Downloaded {
            id: actual,
            update,
            bytes,
        } if actual == id => Ok((update, bytes)),
        other => {
            *session = other;
            Err(CommandError::with_diagnostic(
                CommandErrorKind::AppUpdateInvalidSession,
                "updater session was not ready to apply",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_retry_uses_only_the_typed_release_not_found_error() {
        assert!(should_retry_anonymous_check(
            true,
            &tauri_plugin_updater::Error::ReleaseNotFound
        ));
        assert!(!should_retry_anonymous_check(
            false,
            &tauri_plugin_updater::Error::ReleaseNotFound
        ));
        assert!(!should_retry_anonymous_check(
            true,
            &tauri_plugin_updater::Error::EmptyEndpoints
        ));
    }

    #[test]
    fn download_retry_uses_typed_network_error_without_string_parsing() {
        assert!(should_retry_anonymous_download(
            true,
            &tauri_plugin_updater::Error::Network("HTTP 403".to_owned())
        ));
        assert!(!should_retry_anonymous_download(
            false,
            &tauri_plugin_updater::Error::Network("HTTP 403".to_owned())
        ));
        assert!(!should_retry_anonymous_download(
            true,
            &tauri_plugin_updater::Error::EmptyEndpoints
        ));
    }

    #[test]
    fn returned_manifest_urls_strip_authentication_off_github() {
        assert!(!should_strip_authorization_for_download(
            "https://github.com/owner/repo/releases/download/v1/app.zip"
        ));
        for url in [
            "https://objects.githubusercontent.com/file",
            "https://github.com.evil.example/file",
            "https://github.com:8443/file",
        ] {
            assert!(should_strip_authorization_for_download(url), "{url}");
        }
    }

    #[test]
    fn configured_endpoint_contract_is_github_only() {
        let urls = updater_contract::UPDATER_ENDPOINTS;
        assert!(!urls.is_empty());
        assert!(urls.iter().all(|url| is_github_auth_target_url(url)));
    }
}
