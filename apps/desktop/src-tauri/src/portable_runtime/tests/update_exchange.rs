use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

use renderpilot_orchestration::portable::RuntimePathsV1;

use super::{error_code, hash, temp_root};
use crate::portable_runtime::{
    activation::{
        AppSession,
        update_exchange::{
            DownloadReceiveState, PortableDownloadEvent, accept_update_exchange_message,
            exchange_update_with_session,
        },
    },
    app_protocol::{
        AppControlMessage, PortableAppSessionV2, PortableUpdateEvent, PortableUpdateRequest,
        PortableUpdateResponse, StartupMode, write_message,
    },
    rpu::{MAXIMUM_SCHEMA, MINIMUM_SCHEMA, PORTABLE_APP_SESSION_PROTOCOL},
};

fn startup() -> PortableAppSessionV2 {
    let root = temp_root("startup-paths");
    let portable_root = root.path().join("Переносимый root");
    let generation = portable_root
        .join(".renderpilot-generations/v1/objects")
        .join(hash('a'));
    let app = generation.join("renderpilot-app.exe");
    PortableAppSessionV2 {
        app_session_protocol: PORTABLE_APP_SESSION_PROTOCOL.to_owned(),
        epoch: hash('b'),
        generation_sha256: hash('c'),
        minimum_schema: MINIMUM_SCHEMA,
        maximum_schema: MAXIMUM_SCHEMA,
        transaction_id: "transaction".to_owned(),
        supervisor_session_transcript_sha256: hash('f'),
        portable_root_identity: hash('1'),
        generation_root_identity: hash('2'),
        mode: StartupMode::activation_trial(),
        runtime_paths: RuntimePathsV1::from_portable_root(portable_root, &generation, &app)
            .expect("derive typed stable paths"),
        challenge: hash('d'),
        migration_permit_nonce: hash('9'),
        commit_permit_nonce: hash('e'),
    }
}

fn update_session(root: &Path, name: &str, control_bytes: &[u8]) -> (AppSession, PathBuf) {
    let control_path = root.join(format!("{name}-control.jsonl"));
    fs::write(&control_path, control_bytes).expect("write scripted control frames");
    let status_path = root.join(format!("{name}-status.jsonl"));
    let status = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(&status_path)
        .expect("create scripted status pipe");
    (
        AppSession::new(
            File::open(control_path).expect("open scripted control pipe"),
            status,
            startup(),
        ),
        status_path,
    )
}

#[test]
fn update_exchange_requires_a_correlated_terminal_response() {
    let mut state = DownloadReceiveState::default();
    let mut discarded = |_| {};
    let response = accept_update_exchange_message(
        "request",
        &PortableUpdateRequest::apply(),
        &mut state,
        AppControlMessage::update_response("request", PortableUpdateResponse::apply_accepted()),
        &mut discarded,
    )
    .expect("matching supervisor response")
    .expect("terminal response");
    assert!(matches!(response, PortableUpdateResponse::ApplyAccepted(_)));
    assert_eq!(
        error_code(accept_update_exchange_message(
            "request",
            &PortableUpdateRequest::apply(),
            &mut state,
            AppControlMessage::update_response(
                "different",
                PortableUpdateResponse::apply_accepted()
            ),
            &mut discarded,
        )),
        "portable_update_protocol"
    );
}

#[test]
fn app_download_exchange_fsm_accepts_only_canonical_correlated_complete_streams() {
    const UNIT: u64 = 64 * 1024;
    let request = PortableUpdateRequest::download();
    let mut state = DownloadReceiveState::default();
    let mut observed = Vec::new();
    let mut on_event = |event| observed.push(event);

    for message in [
        AppControlMessage::update_event(
            "request",
            PortableUpdateEvent::download_started(Some(2 * UNIT)),
        ),
        AppControlMessage::update_event("request", PortableUpdateEvent::download_progress(UNIT)),
        AppControlMessage::update_event("request", PortableUpdateEvent::download_progress(UNIT)),
        AppControlMessage::update_event("request", PortableUpdateEvent::download_finished()),
    ] {
        assert!(
            accept_update_exchange_message("request", &request, &mut state, message, &mut on_event)
                .expect("valid stream frame")
                .is_none()
        );
    }
    let terminal = accept_update_exchange_message(
        "request",
        &request,
        &mut state,
        AppControlMessage::update_response("request", PortableUpdateResponse::downloaded(2 * UNIT)),
        &mut on_event,
    )
    .expect("matching terminal")
    .expect("terminal response");
    assert!(matches!(terminal, PortableUpdateResponse::Downloaded(_)));
    assert_eq!(
        observed,
        vec![
            PortableDownloadEvent::Started {
                content_length: Some(2 * UNIT),
            },
            PortableDownloadEvent::Progress {
                chunk_length: UNIT as usize,
            },
            PortableDownloadEvent::Progress {
                chunk_length: UNIT as usize,
            },
            PortableDownloadEvent::Finished,
        ]
    );

    let mut trailing = DownloadReceiveState::default();
    let mut discarded = |_| {};
    for message in [
        AppControlMessage::update_event(
            "trailing",
            PortableUpdateEvent::download_started(Some(UNIT + 3)),
        ),
        AppControlMessage::update_event("trailing", PortableUpdateEvent::download_progress(UNIT)),
        AppControlMessage::update_event("trailing", PortableUpdateEvent::download_progress(3)),
        AppControlMessage::update_event("trailing", PortableUpdateEvent::download_finished()),
    ] {
        assert!(
            accept_update_exchange_message(
                "trailing",
                &request,
                &mut trailing,
                message,
                &mut discarded,
            )
            .expect("valid trailing partial frame")
            .is_none()
        );
    }
    assert!(matches!(
        accept_update_exchange_message(
            "trailing",
            &request,
            &mut trailing,
            AppControlMessage::update_response(
                "trailing",
                PortableUpdateResponse::downloaded(UNIT + 3),
            ),
            &mut discarded,
        )
        .expect("terminal after a trailing partial")
        .expect("terminal response"),
        PortableUpdateResponse::Downloaded(_)
    ));
}

#[test]
fn app_download_exchange_fsm_rejects_wrong_correlation_and_premature_terminal() {
    let request = PortableUpdateRequest::download();
    let mut discarded = |_| {};

    for message in [
        AppControlMessage::update_event("other", PortableUpdateEvent::download_started(None)),
        AppControlMessage::update_response("request", PortableUpdateResponse::downloaded(0)),
    ] {
        assert_eq!(
            error_code(accept_update_exchange_message(
                "request",
                &request,
                &mut DownloadReceiveState::default(),
                message,
                &mut discarded,
            )),
            "portable_update_protocol"
        );
    }
}

#[test]
fn app_download_exchange_fsm_rejects_declared_and_logical_bounds() {
    const UNIT: u64 = 64 * 1024;
    let request = PortableUpdateRequest::download();
    let mut discarded = |_| {};
    let mut bounded = DownloadReceiveState::default();
    accept_update_exchange_message(
        "request",
        &request,
        &mut bounded,
        AppControlMessage::update_event("request", PortableUpdateEvent::download_started(Some(1))),
        &mut discarded,
    )
    .expect("start stream");
    assert_eq!(
        error_code(accept_update_exchange_message(
            "request",
            &request,
            &mut bounded,
            AppControlMessage::update_event("request", PortableUpdateEvent::download_progress(2),),
            &mut discarded,
        )),
        "portable_update_protocol"
    );

    let mut oversized = DownloadReceiveState::default();
    accept_update_exchange_message(
        "request",
        &request,
        &mut oversized,
        AppControlMessage::update_event("request", PortableUpdateEvent::download_started(None)),
        &mut discarded,
    )
    .expect("start stream before rejecting oversized frame");
    assert_eq!(
        error_code(accept_update_exchange_message(
            "request",
            &request,
            &mut oversized,
            AppControlMessage::update_event(
                "request",
                PortableUpdateEvent::download_progress(UNIT + 1),
            ),
            &mut discarded,
        )),
        "portable_update_protocol"
    );
}

#[test]
fn app_download_exchange_fsm_rejects_post_partial_and_non_download_events_but_allows_rejection() {
    let request = PortableUpdateRequest::download();
    let mut discarded = |_| {};
    let mut partial = DownloadReceiveState::default();
    accept_update_exchange_message(
        "request",
        &request,
        &mut partial,
        AppControlMessage::update_event("request", PortableUpdateEvent::download_started(None)),
        &mut discarded,
    )
    .expect("start stream before the trailing partial");
    accept_update_exchange_message(
        "request",
        &request,
        &mut partial,
        AppControlMessage::update_event("request", PortableUpdateEvent::download_progress(1)),
        &mut discarded,
    )
    .expect("accept one trailing partial");
    assert_eq!(
        error_code(accept_update_exchange_message(
            "request",
            &request,
            &mut partial,
            AppControlMessage::update_event("request", PortableUpdateEvent::download_progress(1)),
            &mut discarded,
        )),
        "portable_update_protocol"
    );

    assert!(matches!(
        accept_update_exchange_message(
            "request",
            &request,
            &mut DownloadReceiveState::default(),
            AppControlMessage::update_response(
                "request",
                PortableUpdateResponse::rejected("network"),
            ),
            &mut discarded,
        )
        .expect("rejection remains retryable"),
        Some(PortableUpdateResponse::Rejected(_))
    ));

    assert_eq!(
        error_code(accept_update_exchange_message(
            "request",
            &PortableUpdateRequest::check(),
            &mut DownloadReceiveState::default(),
            AppControlMessage::update_event("request", PortableUpdateEvent::download_started(None),),
            &mut discarded,
        )),
        "portable_update_protocol"
    );
}

#[test]
fn session_update_exchange_fences_failures_before_later_io_but_retries_rejected() {
    let root = temp_root("update-exchange-fence");
    for (name, control, expected_code) in [
        (
            "malformed",
            b"not-json\n".as_slice(),
            "portable_protocol_invalid",
        ),
        ("closed", b"".as_slice(), "portable_protocol_closed"),
    ] {
        let (session, status_path) = update_session(root.path(), name, control);
        let mut discarded = |_| {};
        assert_eq!(
            error_code(exchange_update_with_session(
                &session,
                "first",
                &PortableUpdateRequest::check(),
                &mut discarded,
            )),
            expected_code,
        );
        let written_before_fenced_retry = fs::metadata(&status_path)
            .expect("stat first request")
            .len();
        assert_eq!(
            error_code(exchange_update_with_session(
                &session,
                "second",
                &PortableUpdateRequest::check(),
                &mut discarded,
            )),
            "portable_update_fenced",
        );
        assert_eq!(
            fs::metadata(&status_path).expect("stat fenced retry").len(),
            written_before_fenced_retry,
            "the permanent fence rejects before it writes another request"
        );
    }

    let mut replies = Vec::new();
    for request_id in ["first", "second"] {
        write_message(
            &mut replies,
            &AppControlMessage::update_response(
                request_id,
                PortableUpdateResponse::rejected("network"),
            ),
        )
        .expect("encode recoverable rejection");
    }
    let (session, status_path) = update_session(root.path(), "rejected", &replies);
    let mut discarded = |_| {};
    assert!(matches!(
        exchange_update_with_session(
            &session,
            "first",
            &PortableUpdateRequest::check(),
            &mut discarded,
        )
        .expect("rejected exchanges do not fence the session"),
        PortableUpdateResponse::Rejected(_)
    ));
    let written_after_first_rejection = fs::metadata(&status_path)
        .expect("stat rejected request")
        .len();
    assert!(matches!(
        exchange_update_with_session(
            &session,
            "second",
            &PortableUpdateRequest::check(),
            &mut discarded,
        )
        .expect("the next rejected exchange still reaches the supervisor"),
        PortableUpdateResponse::Rejected(_)
    ));
    assert!(
        fs::metadata(&status_path)
            .expect("stat retried request")
            .len()
            > written_after_first_rejection,
        "a recoverable Rejected terminal permits the next request I/O"
    );
}
