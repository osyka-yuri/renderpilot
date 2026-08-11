use crate::portable_runtime::win32::job::KillOnCloseJob;

#[test]
fn job_owner_is_a_single_kill_on_close_handle() {
    let job = KillOnCloseJob::create().expect("create a private supervisor job");
    assert!(
        !job.raw().is_null(),
        "job creation returned a live native handle"
    );

    let source = include_str!("../win32/job.rs");
    assert!(source.contains("JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE"));
    assert!(!source.contains("pub fn release"));
    assert!(!source.contains("pub fn duplicate"));
}

#[test]
fn trial_process_contract_assigns_job_and_exact_pipe_handles_before_execution() {
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
    let startup = source
        .find("AppControlMessage::Startup")
        .expect("authenticated startup write");
    assert!(create < resume && resume < startup);
}
