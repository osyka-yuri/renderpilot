use renderpilot_application::AppInfo;

pub(crate) const HELP_HINT: &str = "Run `renderpilot --help` for usage.";

const USAGE: &str =
    "Usage:\n  renderpilot scan-folder <path>\n  renderpilot list-artifacts [--technology <technology>]\n  renderpilot list-operations --game <game_id>\n  renderpilot candidates --game <game_id>\n  renderpilot plan-swap --game <game_id> --component <component_id> --artifact <artifact_id>\n  renderpilot backup --operation <operation_id>\n  renderpilot apply --operation <operation_id>\n  renderpilot rollback --operation <operation_id>\n  renderpilot --version\n  renderpilot --help\n";

pub(crate) fn render_summary(info: AppInfo) -> String {
    format!("{} CLI\n{HELP_HINT}\n", info.name())
}

pub(crate) fn render_help(info: AppInfo) -> String {
    format!("{} CLI\n\n{USAGE}", info.name())
}

pub(crate) fn render_version(info: AppInfo) -> String {
    format!("{}\n", info.version_line())
}
