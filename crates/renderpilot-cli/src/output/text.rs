use renderpilot_orchestration::application::AppInfo;

pub(crate) const HELP_HINT: &str = "Run `renderpilot --help` for usage.";

const USAGE: &str = "Usage:\n  renderpilot add-game <install-root> [--executable <path>] [--root-choice <auto|selected|recommended>] [--allow-root-correction]\n  renderpilot list-artifacts [--technology <technology>]\n  renderpilot list-operations --game <game_id>\n  renderpilot candidates --game <game_id>\n  renderpilot plan-swap --game <game_id> --component <component_id> --artifact <artifact_id>\n  renderpilot apply --game <game_id> --component <component_id> --artifact <artifact_id> [--confirmation-token <token>] [--safety-context-token <token>]\n  renderpilot plan-rollback --game <game_id> --component <component_id>\n  renderpilot rollback --game <game_id> --component <component_id>\n  renderpilot renodx status --game <game_id>\n  renderpilot renodx uninstall --game <game_id>\n  renderpilot renodx check-update --game <game_id>\n  renderpilot renodx check-updates\n  renderpilot luma status --game <game_id>\n  renderpilot luma uninstall --game <game_id>\n  renderpilot luma check-update --game <game_id> [--deep]\n  renderpilot luma check-updates\n  renderpilot --version\n  renderpilot --help\n";

pub(crate) fn render_summary(info: AppInfo) -> String {
    format!("{} CLI\n{HELP_HINT}\n", info.name())
}

pub(crate) fn render_help(info: AppInfo) -> String {
    format!("{} CLI\n\n{USAGE}", info.name())
}

pub(crate) fn render_version(info: AppInfo) -> String {
    format!("{}\n", info.version_line())
}
