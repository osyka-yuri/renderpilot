use std::io::Cursor;

use renderpilot_orchestration::portable::RuntimePathsV1;

use super::{error_code, hash, temp_root};
use crate::portable_runtime::{
    app_process::schema_observation_supported,
    app_protocol::{
        AppControlMessage, PortableStartupV3, PortableUpdateRequest, StartupMode,
        committed_sequence_for_selection, read_message, write_message,
    },
    request_gate::RequestGate,
};

fn startup() -> PortableStartupV3 {
    let root = temp_root("startup-paths");
    let portable_root = root.path().join("Переносимый root");
    let generation = portable_root
        .join(".renderpilot-generations/v1/objects")
        .join(hash('a'));
    let app = generation.join("renderpilot-app.exe");
    PortableStartupV3 {
        protocol: 3,
        epoch: hash('b'),
        generation_sha256: hash('c'),
        minimum_schema: 4,
        maximum_schema: 16,
        transaction_id: "transaction".to_owned(),
        supervisor_session_transcript_sha256: hash('f'),
        portable_root_identity: hash('1'),
        generation_root_identity: hash('2'),
        mode: StartupMode::ActivationTrial,
        runtime_paths: RuntimePathsV1::from_portable_root(portable_root, &generation, &app)
            .expect("derive typed stable paths"),
        challenge: hash('d'),
        commit_permit_nonce: hash('e'),
    }
}

#[test]
fn trial_schema_observation_is_bound_to_the_signed_startup_contract() {
    let startup = startup();
    assert!(schema_observation_supported(&startup, 0));
    assert!(schema_observation_supported(&startup, 16));
    for schema in [1, 3, 4, 8, 15, 17, u32::MAX] {
        assert!(!schema_observation_supported(&startup, schema));
    }
}

#[test]
fn startup_protocol_is_typed_and_rejects_malformed_authority() {
    let valid = startup();
    valid.validate().expect("authenticated startup is valid");
    let transcript = valid.transcript_sha256().expect("hash startup transcript");
    assert_eq!(transcript.len(), 64);

    let mut malformed = valid.clone();
    malformed.generation_sha256 = "not-a-hash".to_owned();
    assert_eq!(error_code(malformed.validate()), "portable_startup_invalid");

    let mut forged_session = valid.clone();
    forged_session.supervisor_session_transcript_sha256 = "not-a-session-transcript".to_owned();
    assert_eq!(
        error_code(forged_session.validate()),
        "portable_startup_invalid"
    );

    let mut wrong_schema_contract = valid.clone();
    wrong_schema_contract.minimum_schema = 3;
    assert_eq!(
        error_code(wrong_schema_contract.validate()),
        "portable_startup_invalid"
    );

    let message = AppControlMessage::Startup(Box::new(valid));
    let mut wire = Vec::new();
    write_message(&mut wire, &message).expect("serialize the exact startup DTO");
    let decoded: AppControlMessage = read_message(&mut Cursor::new(wire)).expect("decode DTO");
    assert!(matches!(decoded, AppControlMessage::Startup(_)));
}

#[test]
fn request_gate_closes_on_recoverable_and_uncertain_transitions() {
    let gate = RequestGate::default();
    gate.begin().expect("first request owns the open gate");
    assert_eq!(error_code(gate.begin()), "portable_request_closed");
    gate.close_recoverable();
    assert!(!gate.is_uncertain());
    assert_eq!(error_code(gate.begin()), "portable_request_closed");

    let uncertain = RequestGate::default();
    uncertain.close_uncertain();
    assert!(uncertain.is_uncertain());
    assert_eq!(error_code(uncertain.begin()), "portable_request_closed");
}

#[test]
fn activation_contract_orders_read_only_ack_commit_and_commit_permit() {
    let source = include_str!("../supervisor_activation.rs");
    let ordered = [
        "JournalPhase::TrialReady",
        "JournalPhase::SelectionCommitted",
        "AppControlMessage::ActivationPermit",
        "JournalPhase::ActivationAcknowledged",
        "JournalPhase::Committed",
        "AppControlMessage::CommitPermit",
        "JournalPhase::CommitObserved",
        "write_terminal_receipt(&journal, supervisor_session)?;",
    ];
    let mut prior = 0;
    for marker in ordered {
        let current = source.find(marker).expect("activation state marker");
        assert!(
            prior < current,
            "activation marker {marker} was out of order"
        );
        prior = current;
    }
    assert!(
        !source.contains("write_terminal_receipt(&journal, &selection_hash, supervisor_session)?;")
    );
    assert!(source.contains("match trial.receive()?"));
    assert!(source.contains("App did not acknowledge the exact CommitPermit"));

    let app_source = include_str!("../activation.rs");
    let request = app_source
        .find("pub fn request_update")
        .expect("request proxy");
    let committed = app_source[request..]
        .find("require_committed()?;")
        .expect("request gate before pipe write");
    let pipe_write = app_source[request..]
        .find("AppStatusMessage::UpdateRequest")
        .expect("request DTO write");
    assert!(committed < pipe_write);
    assert!(app_source.contains("CommitPermit did not match the authenticated activation"));
    assert!(app_source.contains("committed_sequence_for_selection(journal_sequence)?"));
    let durable_initialization = app_source
        .find("let committed = commit()?;")
        .expect("permitted durable App initialization");
    let commit_ack = app_source
        .find("AppStatusMessage::CommitAck")
        .expect("commit acknowledgement");
    let committed_gate = app_source
        .find("COMMITTED.store(true, Ordering::Release)")
        .expect("ordinary command gate opens");
    assert!(durable_initialization < commit_ack);
    assert!(commit_ack < committed_gate);

    let desktop = include_str!("../../lib.rs");
    let activation = desktop
        .find("prove_visible_and_commit(app, ||")
        .expect("portable activation closure");
    let context = desktop[activation..]
        .find("Context::open()")
        .expect("durable Context initialization");
    let manage = desktop[activation..]
        .find("app.manage(context)")
        .expect("Context registration");
    assert!(context < manage);
}

#[test]
fn commit_sequence_relation_accepts_rejects_and_checks_overflow() {
    assert_eq!(
        committed_sequence_for_selection(6).expect("SelectionCommitted has a Committed slot"),
        9
    );
    assert_ne!(
        committed_sequence_for_selection(6).expect("checked relation"),
        8,
        "only SelectionCommitted + 3 is the CommitPermit sequence"
    );
    assert_eq!(
        error_code(committed_sequence_for_selection(u64::MAX)),
        "portable_protocol_sequence"
    );

    let source = include_str!("../supervisor_activation.rs");
    assert!(source.contains("committed_sequence_for_selection(selection_entry.sequence)?"));
    assert!(source.contains("committed.sequence != expected_committed_sequence"));
}

#[test]
fn activation_always_reserves_a_fresh_normal_selection() {
    let source = include_str!("../supervisor_activation.rs");
    assert!(source.contains("Each activation owns a fresh normal v3 selection"));
    assert!(source.contains("let (_path, selection_hash) = append_selected("));
    assert!(!source.contains("current.selection_hash"));
}

#[test]
fn supervisor_refreshes_generation_identity_inside_each_activation_iteration() {
    let source = include_str!("../supervisor.rs");
    let loop_start = source
        .find("let job = KillOnCloseJob::create()?;\n    loop {")
        .expect("supervisor activation loop");
    let loop_source = &source[loop_start..];
    let identity = loop_source
        .find("authority.verify_generation_before_decode(&current.generation_root)?")
        .expect("per-generation identity capture");
    let activation = loop_source
        .find("let mut activated = activate_generation(")
        .expect("generation activation");
    let publication = loop_source
        .find("current = publish_next_generation")
        .expect("next-generation publication");

    assert!(identity < activation);
    assert!(activation < publication);
}

#[test]
fn portable_update_requests_remain_serializable_dtos() {
    let request = PortableUpdateRequest::Apply;
    let mut wire = Vec::new();
    write_message(&mut wire, &request).expect("encode request DTO");
    assert_eq!(
        String::from_utf8(wire).expect("UTF-8 JSON DTO"),
        "{\"action\":\"apply\"}\n"
    );
}
