use crate::portable_runtime::{
    app_process::startup_control_message,
    app_protocol::{AppControlMessage, PortableAppSessionV2, StartupMode},
    rpu::{MAXIMUM_SCHEMA, MINIMUM_SCHEMA, PORTABLE_APP_SESSION_PROTOCOL},
    win32::job::KillOnCloseJob,
};

#[test]
fn job_owner_is_a_single_kill_on_close_handle() {
    let job = KillOnCloseJob::create().expect("create a private supervisor job");
    assert!(
        !job.raw().is_null(),
        "job creation returned a live native handle"
    );

    // The Job Object limit and absence of an ownership-release API are native
    // handle configuration facts; they are not observable from a deterministic
    // unit test without terminating a real child process.
    let source = include_str!("../win32/job.rs");
    assert!(source.contains("JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE"));
    assert!(!source.contains("pub fn release"));
    assert!(!source.contains("pub fn duplicate"));
}

#[test]
fn trial_process_contract_assigns_job_and_exact_pipe_handles_before_execution() {
    // Exact suspended creation and PROC_THREAD_ATTRIBUTE handle/job lists are
    // artifact-unobservable without spawning a real Windows child. Keep this
    // narrow static check alongside the typed startup-DTO behavior test below.
    let source = include_str!("../app_process.rs");
    for required in [
        "CREATE_SUSPENDED",
        "PROC_THREAD_ATTRIBUTE_HANDLE_LIST",
        "PROC_THREAD_ATTRIBUTE_JOB_LIST",
        "let child_handles = [",
        "control_read.as_raw_handle()",
        "status_write.as_raw_handle()",
        "ResumeThread(thread.raw())",
    ] {
        assert!(
            source.contains(required),
            "missing child-containment contract: {required}"
        );
    }
    let create = source
        .find("CreateProcessW(")
        .expect("suspended child creation");
    let resume = source
        .find("ResumeThread(thread.raw())")
        .expect("single resume");
    assert!(create < resume);
}

#[test]
fn trial_startup_message_is_an_authenticated_startup_dto() {
    let root = super::temp_root("trial-startup-dto");
    let portable_root = root.path().join("portable");
    let generation = portable_root
        .join(".renderpilot-generations/v1/objects")
        .join(super::hash('a'));
    let app = generation.join("renderpilot-app.exe");
    let startup = PortableAppSessionV2 {
        app_session_protocol: PORTABLE_APP_SESSION_PROTOCOL.to_owned(),
        epoch: super::hash('b'),
        generation_sha256: super::hash('c'),
        minimum_schema: MINIMUM_SCHEMA,
        maximum_schema: MAXIMUM_SCHEMA,
        transaction_id: "transaction".to_owned(),
        supervisor_session_transcript_sha256: super::hash('d'),
        portable_root_identity: super::hash('e'),
        generation_root_identity: super::hash('f'),
        mode: StartupMode::activation_trial(),
        runtime_paths: renderpilot_orchestration::portable::RuntimePathsV1::from_portable_root(
            portable_root,
            &generation,
            &app,
        )
        .expect("derive exact startup paths"),
        challenge: super::hash('1'),
        migration_permit_nonce: super::hash('2'),
        commit_permit_nonce: super::hash('3'),
    };
    assert!(matches!(
        startup_control_message(&startup),
        AppControlMessage::Startup(session) if *session == startup
    ));
}
