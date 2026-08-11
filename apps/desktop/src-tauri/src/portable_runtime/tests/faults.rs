#[test]
fn ui_and_commands_remain_request_only_without_journal_or_confirmation_mutation() {
    let commands = concat!(
        include_str!("../../commands/app_update/mod.rs"),
        include_str!("../../commands/app_update/portable.rs"),
        include_str!("../../commands/app_update/session.rs"),
    );
    for command in [
        "app_update_check",
        "app_update_download",
        "app_update_apply",
    ] {
        let start = commands.find(command).expect("portable updater command");
        let gate = commands[start..]
            .find("portable_request_open()?;")
            .expect("commit gate");
        assert!(gate < 500, "{command} must gate before protocol work");
    }
    assert!(commands.contains("CommandErrorKind::AppUpdateSupervisorFailed"));
    assert!(!commands.contains("app_update_confirm_started"));
    assert!(!commands.contains("journal::"));

    let gateway =
        include_str!("../../../../ui/src/features/app-updater/api/tauri-app-updater-gateway.ts");
    assert!(!gateway.contains("app_update_confirm_started"));
    assert!(!gateway.contains("journal"));
    let desktop = include_str!("../../../../ui/src/app/routes/DesktopApp.svelte");
    assert!(desktop.contains("invokeDesktop('portable_trial_ready')"));
}

#[test]
fn portable_runtime_has_no_legacy_helper_probe_or_resume_entrypoints() {
    let module = include_str!("../mod.rs");
    for forbidden in [
        "pub mod portable_update",
        "pub mod helper",
        "pub mod probe",
        "pub mod resume",
        "pub mod ui_confirm",
    ] {
        assert!(
            !module.contains(forbidden),
            "legacy module token remained: {forbidden}"
        );
    }
    let supervisor = include_str!("../supervisor.rs");
    assert!(supervisor.contains("dispatch_raw_or_supervisor"));
    assert!(supervisor.contains("retain_uncertain_authority"));

    let desktop = include_str!("../../lib.rs");
    assert!(desktop.contains("pub fn run_portable_supervisor() -> std::process::ExitCode"));
    assert!(desktop.contains("portable supervisor failed"));
    let raw = include_str!("../../bin/portable_supervisor.rs");
    assert!(raw.contains("fn main() -> std::process::ExitCode"));
}

#[test]
fn ordinary_command_boundary_is_closed_until_portable_commit() {
    let commands = include_str!("../../commands/mod.rs");
    let boundary = commands
        .find("impl CommandBoundary")
        .expect("central command boundary");
    let source = &commands[boundary..];
    assert!(source.contains("fn require_portable_commit"));
    assert_eq!(
        source.matches("self.require_portable_commit()?;").count(),
        2,
        "blocking and async command paths must both fail closed during TrialReadOnly"
    );
    assert!(source.contains("crate::portable_runtime::activation::require_committed()"));
}
