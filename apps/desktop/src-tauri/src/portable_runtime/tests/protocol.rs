use std::{
    fmt,
    io::{self, BufRead, Cursor, Read},
    path::PathBuf,
};

use renderpilot_orchestration::portable::RuntimePathsV1;
use serde::{Serialize, de::DeserializeOwned};

use super::{error_code, hash};
use crate::portable_runtime::{
    app_protocol::framing::MAX_FRAME_BYTES,
    app_protocol::{
        AppControlMessage, AppStatusMessage, CatalogMigrationOperation, CatalogMigrationReport,
        CommittedSelectionStartupMode, PortableAppSessionV1, PortableUpdateRequest,
        PortableUpdateResponse, StartupMode, TrialReady, read_message, write_message,
    },
    rpu::{MAXIMUM_SCHEMA, MINIMUM_SCHEMA, PORTABLE_APP_SESSION_PROTOCOL},
};

fn startup() -> PortableAppSessionV1 {
    let portable_root = PathBuf::from(r"C:\portable");
    let generation = portable_root
        .join(".renderpilot-generations/v1/objects")
        .join(hash('a'));
    let app = generation.join("renderpilot-app.exe");
    PortableAppSessionV1 {
        app_session_protocol: PORTABLE_APP_SESSION_PROTOCOL.to_owned(),
        epoch: hash('b'),
        generation_sha256: hash('c'),
        minimum_schema: MINIMUM_SCHEMA,
        maximum_schema: MAXIMUM_SCHEMA,
        transaction_id: "transaction".to_owned(),
        supervisor_session_transcript_sha256: hash('d'),
        portable_root_identity: hash('e'),
        generation_root_identity: hash('f'),
        mode: StartupMode::activation_trial(),
        runtime_paths: RuntimePathsV1::from_portable_root(portable_root, &generation, &app)
            .expect("derive exact portable test paths"),
        challenge: hash('1'),
        migration_permit_nonce: hash('2'),
        commit_permit_nonce: hash('3'),
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "tests assert both pre- and post-decode ownership of each DTO value"
)]
fn assert_wire_round_trip<T>(value: T, expected_json: &str)
where
    T: fmt::Debug + DeserializeOwned + Eq + Serialize,
{
    let mut frame = Vec::new();
    write_message(&mut frame, &value).expect("write exact protocol frame");
    assert_eq!(
        String::from_utf8(frame.clone()).expect("protocol JSON is UTF-8"),
        format!("{expected_json}\n")
    );
    let decoded: T = read_message(&mut Cursor::new(frame)).expect("read exact protocol frame");
    assert_eq!(decoded, value);
}

#[test]
fn canonical_dto_variants_have_golden_flat_json_and_round_trip() {
    assert_wire_round_trip(
        StartupMode::activation_trial(),
        r#"{"mode":"activation_trial"}"#,
    );
    assert_wire_round_trip(
        StartupMode::CommittedSelection(CommittedSelectionStartupMode {
            selection_record_sha256: "selection".to_owned(),
            committed_journal_sequence: 9,
            committed_transcript_sha256: "transcript".to_owned(),
        }),
        r#"{"mode":"committed_selection","selection_record_sha256":"selection","committed_journal_sequence":9,"committed_transcript_sha256":"transcript"}"#,
    );
    assert_wire_round_trip(
        CatalogMigrationOperation::validate_current(),
        r#"{"operation":"validate_current"}"#,
    );
    assert_wire_round_trip(
        CatalogMigrationOperation::upgrade_after_snapshot("snapshot"),
        r#"{"operation":"upgrade_after_snapshot","snapshot_receipt_sha256":"snapshot"}"#,
    );

    assert_wire_round_trip(PortableUpdateRequest::check(), r#"{"action":"check"}"#);
    assert_wire_round_trip(
        PortableUpdateRequest::download(),
        r#"{"action":"download"}"#,
    );
    assert_wire_round_trip(PortableUpdateRequest::apply(), r#"{"action":"apply"}"#);

    assert_wire_round_trip(
        PortableUpdateResponse::check(
            true,
            "1.0.0",
            "2.0.0",
            Some("2026-08-13".to_owned()),
            "notes",
        ),
        r#"{"result":"check","available":true,"current_version":"1.0.0","version":"2.0.0","date":"2026-08-13","body":"notes"}"#,
    );
    assert_wire_round_trip(
        PortableUpdateResponse::downloaded(42),
        r#"{"result":"downloaded","content_length":42}"#,
    );
    assert_wire_round_trip(
        PortableUpdateResponse::apply_accepted(),
        r#"{"result":"apply_accepted"}"#,
    );
    assert_wire_round_trip(
        PortableUpdateResponse::rejected("not-ready"),
        r#"{"result":"rejected","code":"not-ready"}"#,
    );

    let startup = startup();
    let expected_startup = format!(
        concat!(
            r#"{{"type":"startup","app_session_protocol":"{protocol}","epoch":"{epoch}","generation_sha256":"{generation}","minimum_schema":{minimum},"maximum_schema":{maximum},"transaction_id":"transaction","supervisor_session_transcript_sha256":"{session}","portable_root_identity":"{root_identity}","generation_root_identity":"{generation_identity}","mode":{{"mode":"activation_trial"}},"runtime_paths":{{"portable_root":"C:\\portable","data_root":"C:\\portable\\data","catalog_db_path":"C:\\portable\\data\\catalog.db","file_transactions_root":"C:\\portable\\data\\file-transactions","libraries_root":"C:\\portable\\data\\libraries","cdn_cache_root":"C:\\portable\\data","covers_root":"C:\\portable\\data\\covers","webview2_root":"C:\\portable\\data\\WebView2","authority_root":"C:\\portable\\.renderpilot-runtime-authority\\v1","generation_store_root":"C:\\portable\\.renderpilot-generations\\v1","selected_generation_root":"C:\\portable\\.renderpilot-generations/v1/objects\\{object}","selected_app_executable":"C:\\portable\\.renderpilot-generations/v1/objects\\{object}\\renderpilot-app.exe","update_root":"C:\\portable\\.renderpilot-update\\v2"}},"challenge":"{challenge}","migration_permit_nonce":"{migration_nonce}","commit_permit_nonce":"{commit_nonce}"}}"#
        ),
        protocol = PORTABLE_APP_SESSION_PROTOCOL,
        epoch = hash('b'),
        generation = hash('c'),
        minimum = MINIMUM_SCHEMA,
        maximum = MAXIMUM_SCHEMA,
        session = hash('d'),
        root_identity = hash('e'),
        generation_identity = hash('f'),
        object = hash('a'),
        challenge = hash('1'),
        migration_nonce = hash('2'),
        commit_nonce = hash('3'),
    );
    assert_wire_round_trip(AppControlMessage::startup(startup), &expected_startup);
    assert_wire_round_trip(
        AppControlMessage::migration_permit(
            CatalogMigrationOperation::upgrade_after_snapshot("snapshot"),
            15,
            16,
            "permit",
            "session",
        ),
        r#"{"type":"migration_permit","operation":{"operation":"upgrade_after_snapshot","snapshot_receipt_sha256":"snapshot"},"source_schema":15,"target_schema":16,"permit_nonce":"permit","supervisor_session_transcript_sha256":"session"}"#,
    );
    assert_wire_round_trip(
        AppControlMessage::activation_permit("activation", "selection", 6, "session"),
        r#"{"type":"activation_permit","activation_nonce":"activation","selection_record_sha256":"selection","journal_sequence":6,"supervisor_session_transcript_sha256":"session"}"#,
    );
    assert_wire_round_trip(
        AppControlMessage::commit_permit("selection", 9, "permit", "session"),
        r#"{"type":"commit_permit","selection_record_sha256":"selection","committed_journal_sequence":9,"permit_nonce":"permit","supervisor_session_transcript_sha256":"session"}"#,
    );
    assert_wire_round_trip(
        AppControlMessage::update_response("request", PortableUpdateResponse::apply_accepted()),
        r#"{"type":"update_response","request_id":"request","response":{"result":"apply_accepted"}}"#,
    );

    assert_wire_round_trip(
        AppStatusMessage::trial_hello("challenge"),
        r#"{"type":"trial_hello","challenge":"challenge"}"#,
    );
    assert_wire_round_trip(
        AppStatusMessage::trial_ready(TrialReady {
            transcript_sha256: "transcript".to_owned(),
            runtime_paths_sha256: "paths".to_owned(),
            schema_observed: 16,
            db_query_only: true,
            webview_profile_ready: true,
            ui_bundle_ready: true,
            visible_window_ready: true,
            event_loop_roundtrip: true,
            supervisor_session_transcript_sha256: "session".to_owned(),
        }),
        r#"{"type":"trial_ready","transcript_sha256":"transcript","runtime_paths_sha256":"paths","schema_observed":16,"db_query_only":true,"webview_profile_ready":true,"ui_bundle_ready":true,"visible_window_ready":true,"event_loop_roundtrip":true,"supervisor_session_transcript_sha256":"session"}"#,
    );
    assert_wire_round_trip(
        AppStatusMessage::migration_ack(
            CatalogMigrationReport {
                source_version: 15,
                target_version: 16,
                catalog_sha256: "catalog".to_owned(),
            },
            None,
            "permit",
            "session",
        ),
        r#"{"type":"migration_ack","report":{"source_version":15,"target_version":16,"catalog_sha256":"catalog"},"snapshot_receipt_sha256":null,"permit_nonce":"permit","supervisor_session_transcript_sha256":"session"}"#,
    );
    assert_wire_round_trip(
        AppStatusMessage::activation_ack("activation", "selection", true, true, "session"),
        r#"{"type":"activation_ack","activation_nonce":"activation","selection_record_sha256":"selection","visible_window_ready":true,"event_loop_roundtrip":true,"supervisor_session_transcript_sha256":"session"}"#,
    );
    assert_wire_round_trip(
        AppStatusMessage::commit_ack("selection", 9, "permit", "session"),
        r#"{"type":"commit_ack","selection_record_sha256":"selection","committed_journal_sequence":9,"permit_nonce":"permit","supervisor_session_transcript_sha256":"session"}"#,
    );
    assert_wire_round_trip(
        AppStatusMessage::update_request("request", PortableUpdateRequest::check()),
        r#"{"type":"update_request","request_id":"request","request":{"action":"check"}}"#,
    );
}

#[test]
fn decoded_preflight_rejects_duplicate_keys_at_every_protocol_depth() {
    let startup_wire = serde_json::to_string(&AppControlMessage::startup(startup()))
        .expect("serialize valid startup message");
    let duplicates = [
        r#"{"type":"trial_hello","type":"trial_hello","challenge":"x"}"#.to_owned(),
        startup_wire.replacen(
            r#""mode":"activation_trial""#,
            r#""mode":"activation_trial","mode":"activation_trial""#,
            1,
        ),
        startup_wire.replacen(
            r#""portable_root":"#,
            r#""portable_root":"forged","portable_root":"#,
            1,
        ),
        r#"{"type":"migration_permit","operation":{"operation":"validate_current","operation":"validate_current"},"source_schema":15,"target_schema":16,"permit_nonce":"permit","supervisor_session_transcript_sha256":"session"}"#.to_owned(),
        r#"{"type":"update_request","request_id":"request","request":{"action":"check","action":"check"}}"#.to_owned(),
        r#"{"type":"update_response","request_id":"request","response":{"result":"rejected","code":"one","code":"two"}}"#.to_owned(),
        r#"{"one":{"two":{"three":1,"three":2}}}"#.to_owned(),
    ];
    for duplicate in duplicates {
        assert_eq!(
            error_code(read_message::<serde_json::Value>(&mut Cursor::new(
                format!("{duplicate}\n")
            ))),
            "portable_protocol_invalid"
        );
    }
}

#[test]
fn derived_payloads_reject_unknown_fields_at_every_protocol_depth() {
    let startup_wire = serde_json::to_string(&AppControlMessage::startup(startup()))
        .expect("serialize valid startup message");
    let unknowns = [
        r#"{"type":"trial_hello","challenge":"x","extra":true}"#.to_owned(),
        startup_wire.replacen(
            r#""challenge":"#,
            r#""unexpected_startup":true,"challenge":"#,
            1,
        ),
        startup_wire.replacen(
            r#""update_root":"#,
            r#""unexpected_runtime_path":true,"update_root":"#,
            1,
        ),
        r#"{"type":"migration_permit","operation":{"operation":"validate_current","extra":true},"source_schema":15,"target_schema":16,"permit_nonce":"permit","supervisor_session_transcript_sha256":"session"}"#.to_owned(),
        r#"{"type":"update_request","request_id":"request","request":{"action":"check","extra":true}}"#.to_owned(),
        r#"{"type":"update_response","request_id":"request","response":{"result":"rejected","code":"x","extra":true}}"#.to_owned(),
    ];
    for unknown in unknowns {
        assert_eq!(
            error_code(read_message::<AppControlMessage>(&mut Cursor::new(
                format!("{unknown}\n")
            ))),
            "portable_protocol_invalid"
        );
    }
}

struct ObservedBuf {
    bytes: Vec<u8>,
    position: usize,
}

impl Read for ObservedBuf {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let available = &self.bytes[self.position..];
        let length = available.len().min(output.len());
        output[..length].copy_from_slice(&available[..length]);
        self.position += length;
        Ok(length)
    }
}

impl BufRead for ObservedBuf {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Ok(&self.bytes[self.position..])
    }

    fn consume(&mut self, amount: usize) {
        self.position += amount;
    }
}

#[test]
fn bounded_reader_preserves_the_newline_frame_and_never_consumes_beyond_the_probe() {
    let mut exact = vec![b' '; MAX_FRAME_BYTES - 5];
    exact.extend_from_slice(b"null\n");
    let value: serde_json::Value =
        read_message(&mut Cursor::new(exact)).expect("newline-inclusive maximum frame fits");
    assert!(value.is_null());

    assert_eq!(
        error_code(read_message::<serde_json::Value>(&mut Cursor::new(
            Vec::<u8>::new()
        ))),
        "portable_protocol_invalid"
    );
    assert_eq!(
        error_code(read_message::<serde_json::Value>(&mut Cursor::new(b"null"))),
        "portable_protocol_invalid"
    );

    let mut oversized = ObservedBuf {
        bytes: vec![b' '; MAX_FRAME_BYTES + 32],
        position: 0,
    };
    assert_eq!(
        error_code(read_message::<serde_json::Value>(&mut oversized)),
        "portable_protocol_invalid"
    );
    assert_eq!(
        oversized.position,
        MAX_FRAME_BYTES + 1,
        "the bounded overflow probe must not consume a byte beyond its cap"
    );
}
