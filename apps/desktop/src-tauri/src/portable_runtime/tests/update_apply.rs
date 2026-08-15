use std::{
    collections::VecDeque,
    path::Path,
    sync::{Arc, Mutex},
};

use super::temp_root;
use crate::portable_runtime::{
    app_protocol::{
        PortableUpdateEvent, PortableUpdateRequest, PortableUpdateResponse, UpdateRequest,
    },
    error::{PortableRuntimeError, Result},
    rpu::{
        MAXIMUM_SCHEMA, MINIMUM_SCHEMA, PORTABLE_APP_SESSION_PROTOCOL,
        PORTABLE_SUPERVISOR_CAPABILITY, RPU_PROTOCOL, RpuManifest, VerifiedRpu,
    },
    signature::sha256_hex,
    staging::staged_for_apply_test,
    supervisor_updates::{
        SupervisorUpdateEvent, SupervisorUpdateState, UpdateSink, deliver_downloaded_for_test,
        serve_one_with_sink,
    },
};

type Trace = Arc<Mutex<Vec<&'static str>>>;

struct ScriptedSink {
    requests: VecDeque<UpdateRequest>,
    responses: Vec<PortableUpdateResponse>,
    events: Vec<(Arc<str>, PortableUpdateEvent)>,
    trace: Trace,
    fail_apply_acceptance: bool,
    fail_download_delivery: bool,
    close_when_exhausted: bool,
    receive_failure_code: Option<&'static str>,
}

impl ScriptedSink {
    fn applies(trace: Trace, count: usize, fail_apply_acceptance: bool) -> Self {
        Self {
            requests: (0..count)
                .map(|index| UpdateRequest {
                    request_id: format!("apply-{index}").into(),
                    request: PortableUpdateRequest::apply(),
                })
                .collect(),
            responses: Vec::new(),
            events: Vec::new(),
            trace,
            fail_apply_acceptance,
            fail_download_delivery: false,
            close_when_exhausted: false,
            receive_failure_code: None,
        }
    }

    fn downloads(trace: Trace, count: usize) -> Self {
        Self {
            requests: (0..count)
                .map(|index| UpdateRequest {
                    request_id: format!("download-{index}").into(),
                    request: PortableUpdateRequest::download(),
                })
                .collect(),
            responses: Vec::new(),
            events: Vec::new(),
            trace,
            fail_apply_acceptance: false,
            fail_download_delivery: false,
            close_when_exhausted: false,
            receive_failure_code: None,
        }
    }
}

impl UpdateSink for ScriptedSink {
    fn receive_update_request_or_eof(&mut self) -> Result<Option<UpdateRequest>> {
        if let Some(code) = self.receive_failure_code {
            return Err(PortableRuntimeError::new(code, "scripted receive failure"));
        }
        match self.requests.pop_front() {
            Some(request) => Ok(Some(request)),
            None if self.close_when_exhausted => Ok(None),
            None => Err(PortableRuntimeError::new(
                "portable_update_test",
                "script exhausted before the expected request",
            )),
        }
    }

    fn send_update_response(
        &mut self,
        _request_id: Arc<str>,
        response: PortableUpdateResponse,
    ) -> Result<()> {
        let accepted = matches!(&response, PortableUpdateResponse::ApplyAccepted(_));
        let downloaded = matches!(&response, PortableUpdateResponse::Downloaded(_));
        self.trace
            .lock()
            .expect("test trace lock")
            .push(if accepted { "apply_accepted" } else { "reply" });
        self.responses.push(response);
        if accepted && self.fail_apply_acceptance {
            return Err(PortableRuntimeError::new(
                "portable_update_test_sink",
                "scripted acceptance write failure",
            ));
        }
        if downloaded && self.fail_download_delivery {
            return Err(PortableRuntimeError::new(
                "portable_update_test_sink",
                "scripted downloaded write failure",
            ));
        }
        Ok(())
    }

    fn send_update_event(
        &mut self,
        request_id: Arc<str>,
        event: PortableUpdateEvent,
    ) -> Result<()> {
        self.events.push((request_id, event));
        Ok(())
    }
}

fn trace() -> Trace {
    Arc::new(Mutex::new(Vec::new()))
}

fn trace_values(trace: &Trace) -> Vec<&'static str> {
    trace.lock().expect("test trace lock").clone()
}

fn response_code(response: &PortableUpdateResponse) -> &str {
    let PortableUpdateResponse::Rejected(rejected) = response else {
        panic!("expected recoverable Apply rejection, got {response:?}");
    };
    &rejected.code
}

fn verified_rpu() -> VerifiedRpu {
    let app_bytes = b"MZ scripted portable App".to_vec();
    VerifiedRpu {
        manifest: RpuManifest {
            protocol: RPU_PROTOCOL.to_owned(),
            platform: "windows-x86_64-portable".to_owned(),
            version: "2.0.0".to_owned(),
            app_sha256: sha256_hex(&app_bytes),
            app_length: app_bytes.len() as u64,
            minimum_supervisor_protocol: PORTABLE_SUPERVISOR_CAPABILITY,
            app_session_protocol: PORTABLE_APP_SESSION_PROTOCOL.to_owned(),
            minimum_schema: MINIMUM_SCHEMA,
            maximum_schema: MAXIMUM_SCHEMA,
            portable_role: "app".to_owned(),
        },
        app_bytes,
        rpu_sha256: "a".repeat(64),
    }
}

fn install_staged(
    state: &mut SupervisorUpdateState,
    canonical_path: &Path,
    staged_bytes: &[u8],
    trace: Trace,
) {
    state.stage_for_test(staged_for_apply_test(
        canonical_path.to_owned(),
        staged_bytes,
        verified_rpu(),
        trace,
    ));
}

fn serve_apply(
    sink: &mut ScriptedSink,
    state: &mut SupervisorUpdateState,
    update_root: &Path,
) -> Result<SupervisorUpdateEvent> {
    serve_one_with_sink(sink, state, "1.0.0", update_root)
}

fn expect_continue(result: Result<SupervisorUpdateEvent>, context: &str) {
    assert!(
        matches!(
            result.unwrap_or_else(|error| panic!("{context}: {error}")),
            SupervisorUpdateEvent::Continue
        ),
        "{context}"
    );
}

fn expect_apply_ready(result: Result<SupervisorUpdateEvent>, context: &str) -> VerifiedRpu {
    match result.unwrap_or_else(|error| panic!("{context}: {error}")) {
        SupervisorUpdateEvent::ApplyReady(verified) => *verified,
        SupervisorUpdateEvent::Continue | SupervisorUpdateEvent::AppStatusClosed => {
            panic!("{context}: expected ApplyReady")
        }
    }
}

#[test]
fn message_boundary_eof_is_typed_but_receive_failures_remain_errors() {
    let root = temp_root("update-terminal-read");
    let mut state = SupervisorUpdateState::default();
    let trace = trace();
    let mut closed = ScriptedSink::applies(Arc::clone(&trace), 0, false);
    closed.close_when_exhausted = true;
    assert!(matches!(
        serve_one_with_sink(&mut closed, &mut state, "1.0.0", root.path())
            .expect("EOF between messages is a terminal channel state"),
        SupervisorUpdateEvent::AppStatusClosed
    ));

    let mut failed = ScriptedSink::applies(trace, 0, false);
    failed.receive_failure_code = Some("portable_runtime_io");
    let error = match serve_one_with_sink(&mut failed, &mut state, "1.0.0", root.path()) {
        Err(error) => error,
        Ok(_) => panic!("I/O failure must not become a clean close"),
    };
    assert_eq!(error.code(), "portable_runtime_io");
}

#[test]
fn g_apl_01_missing_and_second_apply_remain_recoverable_rejections() {
    let root = temp_root("apply-missing");
    let trace = trace();
    let mut sink = ScriptedSink::applies(Arc::clone(&trace), 2, false);
    let mut state = SupervisorUpdateState::default();

    expect_continue(
        serve_apply(&mut sink, &mut state, root.path()),
        "missing staged capability is replied to",
    );
    assert_eq!(
        response_code(&sink.responses[0]),
        "portable_update_stage_missing"
    );
    assert!(!state.is_uncertain());

    expect_continue(
        serve_apply(&mut sink, &mut state, root.path()),
        "second Apply keeps the App session recoverable",
    );
    assert_eq!(response_code(&sink.responses[1]), "portable_request_closed");
    assert!(!state.is_uncertain());
    assert_eq!(trace_values(&trace), ["reply", "reply"]);
}

#[test]
fn g_apl_02_mutated_or_replaced_staged_bytes_are_consumed_before_rejection() {
    for replacement in [false, true] {
        let root = temp_root(if replacement {
            "apply-replaced"
        } else {
            "apply-mutated"
        });
        let canonical_path = root.path().join("staged.rpu");
        let original = b"verified staged bytes";
        std::fs::write(&canonical_path, original).expect("write staged RPU");
        let trace = trace();
        let mut state = SupervisorUpdateState::default();
        install_staged(&mut state, &canonical_path, original, Arc::clone(&trace));
        if replacement {
            std::fs::remove_file(&canonical_path).expect("remove original staged RPU");
            std::fs::write(&canonical_path, b"replacement staged bytes")
                .expect("replace staged RPU");
        } else {
            std::fs::write(&canonical_path, b"mutated staged bytes").expect("mutate staged RPU");
        }
        let mut sink = ScriptedSink::applies(Arc::clone(&trace), 2, false);

        expect_continue(
            serve_apply(&mut sink, &mut state, root.path()),
            "tampered stage is replied to",
        );
        assert_eq!(response_code(&sink.responses[0]), "portable_stage_identity");
        assert_eq!(trace_values(&trace), ["staged_reread", "reply"]);

        expect_continue(
            serve_apply(&mut sink, &mut state, root.path()),
            "consumed staged capability cannot produce another RPU",
        );
        assert_eq!(response_code(&sink.responses[1]), "portable_request_closed");
        assert!(!state.is_uncertain());
    }
}

#[test]
fn g_apl_03_unreadable_staged_bytes_are_rejected_without_acceptance() {
    let root = temp_root("apply-unreadable");
    let canonical_path = root.path().join("staged.rpu");
    let original = b"verified staged bytes";
    std::fs::write(&canonical_path, original).expect("write staged RPU");
    let trace = trace();
    let mut state = SupervisorUpdateState::default();
    install_staged(&mut state, &canonical_path, original, Arc::clone(&trace));
    std::fs::remove_file(&canonical_path).expect("remove staged RPU");
    std::fs::create_dir(&canonical_path).expect("replace staged RPU with unreadable directory");
    let mut sink = ScriptedSink::applies(Arc::clone(&trace), 1, false);

    expect_continue(
        serve_apply(&mut sink, &mut state, root.path()),
        "unreadable stage is replied to",
    );
    assert_eq!(response_code(&sink.responses[0]), "portable_runtime_io");
    assert_eq!(trace_values(&trace), ["staged_reread", "reply"]);
    assert!(!state.is_uncertain());
}

#[test]
fn g_apl_04_reread_precedes_exactly_one_acceptance_and_send_failure_is_uncertain() {
    let success_root = temp_root("apply-success");
    let success_path = success_root.path().join("staged.rpu");
    let original = b"verified staged bytes";
    std::fs::write(&success_path, original).expect("write staged RPU");
    let success_trace = trace();
    let mut success_state = SupervisorUpdateState::default();
    install_staged(
        &mut success_state,
        &success_path,
        original,
        Arc::clone(&success_trace),
    );
    let mut success_sink = ScriptedSink::applies(Arc::clone(&success_trace), 2, false);

    let returned = expect_apply_ready(
        serve_apply(&mut success_sink, &mut success_state, success_root.path()),
        "fully reread stage may be accepted exactly once",
    );
    assert_eq!(returned.manifest.version, "2.0.0");
    assert_eq!(
        trace_values(&success_trace),
        ["staged_reread", "apply_accepted"]
    );
    assert!(!success_state.is_uncertain());
    expect_continue(
        serve_apply(&mut success_sink, &mut success_state, success_root.path()),
        "the consumed capability cannot be accepted twice",
    );
    assert_eq!(
        response_code(&success_sink.responses[1]),
        "portable_request_closed"
    );

    let failed_root = temp_root("apply-acceptance-write-failure");
    let failed_path = failed_root.path().join("staged.rpu");
    std::fs::write(&failed_path, original).expect("write staged RPU");
    let failed_trace = trace();
    let mut failed_state = SupervisorUpdateState::default();
    install_staged(
        &mut failed_state,
        &failed_path,
        original,
        Arc::clone(&failed_trace),
    );
    let mut failed_sink = ScriptedSink::applies(Arc::clone(&failed_trace), 1, true);

    let error = match serve_apply(&mut failed_sink, &mut failed_state, failed_root.path()) {
        Err(error) => error,
        Ok(_) => panic!("failed acceptance delivery must not release an RPU"),
    };
    assert_eq!(error.code(), "portable_update_test_sink");
    assert_eq!(
        trace_values(&failed_trace),
        ["staged_reread", "apply_accepted"]
    );
    assert!(failed_state.is_uncertain());
}

#[test]
fn failed_download_revokes_an_older_stage_and_terminal_delivery_owns_new_stage_installation() {
    let root = temp_root("download-stage-lifetime");
    let canonical_path = root.path().join("staged.rpu");
    std::fs::write(&canonical_path, b"previous staged bytes").expect("write previous stage");
    let trace = trace();
    let mut state = SupervisorUpdateState::default();
    install_staged(
        &mut state,
        &canonical_path,
        b"previous staged bytes",
        Arc::clone(&trace),
    );
    assert!(state.has_staged());

    let mut sink = ScriptedSink::downloads(Arc::clone(&trace), 1);
    expect_continue(
        serve_one_with_sink(&mut sink, &mut state, "1.0.0", root.path()),
        "a failed new download must reply recoverably",
    );
    assert_eq!(
        response_code(&sink.responses[0]),
        "portable_update_offer_missing"
    );
    assert!(
        sink.events.is_empty(),
        "missing offers reject before Started"
    );
    assert!(
        !state.has_staged(),
        "SIG-01: a failed new Download cannot retain an earlier Apply capability"
    );

    let fresh_path = root.path().join("fresh.rpu");
    std::fs::write(&fresh_path, b"fresh staged bytes").expect("write fresh stage");
    let staged = staged_for_apply_test(
        fresh_path,
        b"fresh staged bytes",
        verified_rpu(),
        Arc::clone(&trace),
    );
    sink.fail_download_delivery = true;
    let error = deliver_downloaded_for_test(&mut sink, &mut state, "download-1".into(), 18, staged)
        .expect_err("a failed terminal Downloaded delivery cannot install a capability");
    assert_eq!(error.code(), "portable_update_test_sink");
    assert!(!state.has_staged());
}
