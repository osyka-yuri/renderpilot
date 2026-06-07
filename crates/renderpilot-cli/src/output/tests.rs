use renderpilot_orchestration::application::app_info;

use super::{render_help, render_summary, render_version, HELP_HINT};

#[test]
fn summary_reuses_shared_help_hint() {
    let summary = render_summary(app_info("1.2.3"));

    assert!(summary.contains(HELP_HINT));
}

#[test]
fn help_and_version_end_with_newline() {
    let info = app_info("1.2.3");

    assert!(render_help(info).ends_with('\n'));
    assert_eq!(render_version(info), "RenderPilot 1.2.3\n");
}

#[test]
fn usage_lists_artifact_command() {
    let help = render_help(app_info("1.2.3"));

    assert!(help.contains("renderpilot list-artifacts [--technology <technology>]"));
    assert!(help.contains("renderpilot list-operations --game <game_id>"));
    assert!(help.contains("renderpilot candidates --game <game_id>"));
    assert!(help.contains(
        "renderpilot plan-swap --game <game_id> --component <component_id> --artifact <artifact_id>"
    ));
    assert!(help.contains(
        "renderpilot apply --game <game_id> --component <component_id> --artifact <artifact_id>"
    ));
    assert!(help.contains("renderpilot rollback --game <game_id> --component <component_id>"));
}
