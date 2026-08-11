use std::{collections::BTreeMap, io::Read, time::Duration};

use serde::Deserialize;

use super::{
    app_process::TrialProcess,
    app_protocol::{
        AppControlMessage, AppStatusMessage, PortableUpdateRequest, PortableUpdateResponse,
    },
    error::{PortableRuntimeError, Result},
    request_gate::RequestGate,
    rpu::{VerifiedRpu, canonical_version},
    staging::stage_verified_rpu_expected,
};

const PORTABLE_PLATFORM: &str = "windows-x86_64-portable";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_RPU_BYTES: u64 = 1024 * 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const CHECK_TIMEOUT: Duration = Duration::from_secs(60);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

#[derive(Default)]
pub struct SupervisorUpdateState {
    offer: Option<UpdateOffer>,
    staged: Option<VerifiedRpu>,
    gate: RequestGate,
}

impl SupervisorUpdateState {
    pub fn is_uncertain(&self) -> bool {
        self.gate.is_uncertain()
    }
}

struct UpdateOffer {
    version: String,
    date: Option<String>,
    body: String,
    url: String,
    signature: String,
}

#[derive(Deserialize)]
struct LatestManifest {
    version: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    pub_date: Option<String>,
    platforms: BTreeMap<String, LatestPlatform>,
}

#[derive(Deserialize)]
struct LatestPlatform {
    url: String,
    signature: String,
}

/// Receives one App-owned DTO through the inherited private pipes. Every
/// network byte and durable updater write is performed synchronously by the
/// live supervisor while it retains D18 and the child job.
pub fn serve_one(
    trial: &mut TrialProcess,
    state: &mut SupervisorUpdateState,
    current_version: &str,
    update_root: &std::path::Path,
) -> Result<Option<VerifiedRpu>> {
    let AppStatusMessage::UpdateRequest {
        request_id,
        request,
    } = trial.receive()?
    else {
        return Err(PortableRuntimeError::new(
            "portable_update_protocol",
            "expected authenticated update request",
        ));
    };

    let response = match request {
        PortableUpdateRequest::Check => check_update(state, current_version),
        PortableUpdateRequest::Download => download_update(state, update_root),
        PortableUpdateRequest::Apply => accept_apply(state)?,
    };

    let accepted = matches!(response, PortableUpdateResponse::ApplyAccepted);
    if let Err(error) = trial.send(&AppControlMessage::UpdateResponse {
        request_id,
        response,
    }) {
        if accepted {
            state.gate.close_uncertain();
        }
        return Err(error);
    }
    if accepted {
        state.gate.close_recoverable();
        return Ok(state.staged.take());
    }
    Ok(None)
}

fn check_update(
    state: &mut SupervisorUpdateState,
    current_version: &str,
) -> PortableUpdateResponse {
    match fetch_offer(current_version) {
        Ok(offer) => {
            let available = offer.is_some();
            state.offer = offer;
            if !available {
                state.staged = None;
            }
            let offer = state.offer.as_ref();
            PortableUpdateResponse::Check {
                available,
                current_version: current_version.to_owned(),
                version: offer
                    .map(|offer| offer.version.clone())
                    .unwrap_or_else(|| current_version.to_owned()),
                date: offer.and_then(|offer| offer.date.clone()),
                body: offer.map(|offer| offer.body.clone()).unwrap_or_default(),
            }
        }
        Err(error) => PortableUpdateResponse::Rejected {
            code: error.code().to_owned(),
        },
    }
}

fn download_update(
    state: &mut SupervisorUpdateState,
    update_root: &std::path::Path,
) -> PortableUpdateResponse {
    let Some(offer) = state.offer.as_ref() else {
        return PortableUpdateResponse::Rejected {
            code: "portable_update_offer_missing".to_owned(),
        };
    };
    match download_and_stage(update_root, offer) {
        Ok((length, staged)) => {
            state.staged = Some(staged);
            PortableUpdateResponse::Downloaded {
                content_length: length,
            }
        }
        Err(error) => PortableUpdateResponse::Rejected {
            code: error.code().to_owned(),
        },
    }
}

fn accept_apply(state: &mut SupervisorUpdateState) -> Result<PortableUpdateResponse> {
    state.gate.begin()?;
    if state.staged.is_some() {
        return Ok(PortableUpdateResponse::ApplyAccepted);
    }
    state.gate.close_recoverable();
    Ok(PortableUpdateResponse::Rejected {
        code: "portable_update_stage_missing".to_owned(),
    })
}

fn fetch_offer(current_version: &str) -> Result<Option<UpdateOffer>> {
    let current = canonical_version(current_version)?;
    let client = http_client(CHECK_TIMEOUT)?;
    let endpoint = crate::updater_contract::UPDATER_ENDPOINTS
        .first()
        .ok_or_else(|| PortableRuntimeError::new("portable_update_endpoint", "no endpoint"))?;
    let response = client
        .get(*endpoint)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| PortableRuntimeError::new("portable_update_check", error.to_string()))?;
    let bytes = read_limited_response(
        response,
        MAX_MANIFEST_BYTES,
        "portable_update_manifest",
        "latest manifest exceeded maximum size",
    )?;
    let manifest: LatestManifest = serde_json::from_slice(&bytes).map_err(|error| {
        PortableRuntimeError::new("portable_update_manifest", error.to_string())
    })?;
    let LatestManifest {
        version,
        notes,
        pub_date,
        mut platforms,
    } = manifest;
    let parsed_version = canonical_version(&version).map_err(|error| {
        PortableRuntimeError::new("portable_update_manifest", error.to_string())
    })?;
    if parsed_version <= current {
        return Ok(None);
    }
    let platform = platforms.remove(PORTABLE_PLATFORM).ok_or_else(|| {
        PortableRuntimeError::new(
            "portable_update_manifest",
            "portable platform entry was absent",
        )
    })?;
    if !platform.url.starts_with("https://") || platform.signature.trim().is_empty() {
        return Err(PortableRuntimeError::new(
            "portable_update_manifest",
            "portable release entry was not an HTTPS signed RPU",
        ));
    }
    Ok(Some(UpdateOffer {
        version,
        date: pub_date,
        body: notes,
        url: platform.url,
        signature: platform.signature,
    }))
}

fn download_and_stage(
    update_root: &std::path::Path,
    offer: &UpdateOffer,
) -> Result<(u64, VerifiedRpu)> {
    let response = http_client(DOWNLOAD_TIMEOUT)?
        .get(&offer.url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| {
            PortableRuntimeError::new("portable_update_download", error.to_string())
        })?;
    let bytes = read_limited_response(
        response,
        MAX_RPU_BYTES,
        "portable_update_download",
        "portable RPU exceeded maximum size",
    )?;
    let (_, verified) =
        stage_verified_rpu_expected(update_root, &bytes, &offer.signature, &offer.version)?;
    Ok((bytes.len() as u64, verified))
}

fn http_client(timeout: Duration) -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .https_only(true)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(timeout)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|error| PortableRuntimeError::new("portable_update_client", error.to_string()))
}

fn read_limited_response(
    response: reqwest::blocking::Response,
    maximum: u64,
    code: &'static str,
    too_large: &'static str,
) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > maximum)
    {
        return Err(PortableRuntimeError::new(code, too_large));
    }
    read_limited_body(response, maximum, code, too_large)
}

fn read_limited_body(
    body: impl Read,
    maximum: u64,
    code: &'static str,
    too_large: &'static str,
) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    body.take(maximum.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| PortableRuntimeError::new(code, error.to_string()))?;
    if bytes.len() as u64 > maximum {
        return Err(PortableRuntimeError::new(code, too_large));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn response_reader_stops_after_the_bounded_overflow_probe() {
        let bytes = read_limited_body(
            Cursor::new(b"exact"),
            5,
            "portable_update_test",
            "too large",
        )
        .expect("exact body fits");
        assert_eq!(bytes, b"exact");

        let error = read_limited_body(
            Cursor::new(b"overflow"),
            5,
            "portable_update_test",
            "too large",
        )
        .expect_err("overflow body is rejected");
        assert_eq!(error.code(), "portable_update_test");
    }
}
