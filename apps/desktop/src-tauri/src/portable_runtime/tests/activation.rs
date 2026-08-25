use std::{
    io::Cursor,
    sync::{
        Arc, Barrier, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use renderpilot_orchestration::portable::RuntimePathsV1;

use super::{error_code, hash, temp_root};
use crate::portable_runtime::{
    activation::{
        accept_activation_permit, accept_commit_permit, ensure_committed, is_committed,
        require_committed,
    },
    app_process::schema_observation_supported,
    app_protocol::{
        AppControlMessage, AppStatusMessage, PortableAppSessionV2, PortableUpdateRequest,
        StartupMode, committed_sequence_for_selection, read_message, write_message,
    },
    request_gate::RequestGate,
    rpu::{MAXIMUM_SCHEMA, MINIMUM_SCHEMA, PORTABLE_APP_SESSION_PROTOCOL},
    supervisor_activation::{activation_ack_matches, commit_ack_matches},
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

#[test]
fn trial_schema_observation_is_bound_to_the_signed_startup_contract() {
    let startup = startup();
    assert!(schema_observation_supported(&startup, 0));
    for schema in [MINIMUM_SCHEMA, 8, 15, MAXIMUM_SCHEMA] {
        assert!(schema_observation_supported(&startup, schema));
    }
    for schema in [1, 3, MAXIMUM_SCHEMA + 1, u32::MAX] {
        assert!(!schema_observation_supported(&startup, schema));
    }

    let mut crossed_epoch = startup;
    crossed_epoch.maximum_schema = MAXIMUM_SCHEMA + 1;
    assert_eq!(
        error_code(crossed_epoch.validate()),
        "portable_startup_invalid",
        "the App must not turn an exact native epoch into a future compatibility lane"
    );
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

    let mut wrong_app_session = valid.clone();
    wrong_app_session.app_session_protocol = "forged-portable-session".to_owned();
    assert_eq!(
        error_code(wrong_app_session.validate()),
        "portable_startup_invalid"
    );

    let message = AppControlMessage::startup(valid);
    let mut wire = Vec::new();
    write_message(&mut wire, &message).expect("serialize the exact startup DTO");
    let decoded: AppControlMessage = read_message(&mut Cursor::new(wire)).expect("decode DTO");
    assert!(matches!(decoded, AppControlMessage::Startup(_)));
}

#[test]
fn activation_and_commit_permits_require_the_authenticated_context_in_order() {
    let startup = startup();
    let activation = accept_activation_permit(
        AppControlMessage::activation_permit(hash('a'), hash('b'), 6, hash('f')),
        &startup,
    )
    .expect("bound ActivationPermit is accepted");
    assert_eq!(activation.journal_sequence, 6);

    assert_eq!(
        error_code(accept_activation_permit(
            AppControlMessage::activation_permit(hash('a'), hash('b'), 6, hash('0')),
            &startup,
        )),
        "portable_activation"
    );

    let commit = accept_commit_permit(
        AppControlMessage::commit_permit(hash('b'), 9, hash('e'), hash('f')),
        &activation.selection_record_sha256,
        activation.journal_sequence,
        &startup,
    )
    .expect("exact CommitPermit follows accepted activation");
    assert_eq!(commit.committed_journal_sequence, 9);

    assert_eq!(
        error_code(accept_commit_permit(
            AppControlMessage::commit_permit(hash('b'), 8, hash('e'), hash('f')),
            &activation.selection_record_sha256,
            activation.journal_sequence,
            &startup,
        )),
        "portable_activation"
    );
}

#[test]
fn supervisor_accepts_only_exact_visible_and_commit_acknowledgements() {
    let activation = AppStatusMessage::activation_ack(hash('a'), hash('b'), true, true, hash('c'));
    assert!(activation_ack_matches(
        &activation,
        &hash('a'),
        &hash('b'),
        &hash('c')
    ));
    assert!(!activation_ack_matches(
        &AppStatusMessage::activation_ack(hash('a'), hash('b'), false, true, hash('c')),
        &hash('a'),
        &hash('b'),
        &hash('c')
    ));

    let commit = AppStatusMessage::commit_ack(hash('b'), 9, hash('d'), hash('c'));
    assert!(commit_ack_matches(
        &commit,
        &hash('b'),
        9,
        &hash('d'),
        &hash('c')
    ));
    assert!(!commit_ack_matches(
        &AppStatusMessage::commit_ack(hash('b'), 8, hash('d'), hash('c')),
        &hash('b'),
        9,
        &hash('d'),
        &hash('c')
    ));
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
fn commit_sequence_relation_accepts_rejects_and_checks_overflow() {
    assert_eq!(
        committed_sequence_for_selection(6).expect("SelectionCommitted has a Committed slot"),
        9
    );
    assert_eq!(
        error_code(committed_sequence_for_selection(u64::MAX)),
        "portable_protocol_sequence"
    );
}

#[test]
fn portable_update_requests_remain_serializable_dtos() {
    let request = PortableUpdateRequest::apply();
    let mut wire = Vec::new();
    write_message(&mut wire, &request).expect("encode request DTO");
    assert_eq!(
        String::from_utf8(wire).expect("UTF-8 JSON DTO"),
        "{\"action\":\"apply\"}\n"
    );
}

#[test]
fn activation_concurrency_serializes_and_ensures_single_handshake() {
    let exchange = Arc::new(Mutex::new(()));
    let committed_gate = Arc::new(AtomicBool::new(false));
    let handshake_count = Arc::new(AtomicUsize::new(0));

    let barrier = Arc::new(Barrier::new(2));

    // Thread A: simulates Call A, acquiring the exchange lock and holding it during handshake
    let exchange_a = Arc::clone(&exchange);
    let gate_a = Arc::clone(&committed_gate);
    let count_a = Arc::clone(&handshake_count);
    let barrier_a = Arc::clone(&barrier);

    let handle_a = std::thread::spawn(move || {
        ensure_committed(&exchange_a, &gate_a, || {
            // Signal thread B that Thread A has entered the handshake (holding exchange lock)
            barrier_a.wait();
            // Simulate brief protocol delay
            std::thread::sleep(std::time::Duration::from_millis(25));
            count_a.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    });

    // Thread B: simulates Call B (e.g. rapid F5), starts while Thread A holds the lock
    let exchange_b = Arc::clone(&exchange);
    let gate_b = Arc::clone(&committed_gate);
    let count_b = Arc::clone(&handshake_count);
    let barrier_b = Arc::clone(&barrier);

    let handle_b = std::thread::spawn(move || {
        // Wait until Thread A is inside the handshake holding exchange lock
        barrier_b.wait();
        ensure_committed(&exchange_b, &gate_b, || {
            count_b.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
    });

    let res_a = handle_a.join().expect("thread A join");
    let res_b = handle_b.join().expect("thread B join");

    assert!(res_a.is_ok(), "Call A must succeed");
    assert!(res_b.is_ok(), "Call B must succeed as idempotent no-op");
    assert_eq!(
        handshake_count.load(Ordering::SeqCst),
        1,
        "Handshake closure must execute exactly once"
    );
    assert!(
        committed_gate.load(Ordering::Acquire),
        "Gate must be committed"
    );

    // Sequential Call C: fast-path / second no-op
    let count_c = Arc::clone(&handshake_count);
    let res_c = ensure_committed(&exchange, &committed_gate, || {
        count_c.fetch_add(1, Ordering::SeqCst);
        Ok(())
    });
    assert!(res_c.is_ok(), "Call C must succeed as idempotent no-op");
    assert_eq!(
        handshake_count.load(Ordering::SeqCst),
        1,
        "Sequential call must not re-execute handshake"
    );
}

#[test]
fn standalone_desktop_without_portable_session_remains_ungated() {
    assert!(!is_committed());
    assert!(require_committed().is_ok());
}
