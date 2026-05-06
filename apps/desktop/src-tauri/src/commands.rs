use renderpilot_cli::CliError;
use serde::Serialize;
use serde_json::Value;

type JsonCommandResult = Result<Value, CommandError>;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemAppearance {
    accent_color: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommandError {
    code: String,
    severity: CommandErrorSeverity,
    message_key: String,
    details: String,
    suggested_actions: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandErrorSeverity {
    Warning,
    Error,
}

impl CommandError {
    fn warning(
        code: impl Into<String>,
        message_key: impl Into<String>,
        details: impl Into<String>,
        suggested_actions: Vec<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: CommandErrorSeverity::Warning,
            message_key: message_key.into(),
            details: details.into(),
            suggested_actions,
        }
    }

    fn error(
        code: impl Into<String>,
        message_key: impl Into<String>,
        details: impl Into<String>,
        suggested_actions: Vec<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: CommandErrorSeverity::Error,
            message_key: message_key.into(),
            details: details.into(),
            suggested_actions,
        }
    }
}

fn into_command_error(error: CliError) -> CommandError {
    match error {
        CliError::InvalidGameId(game_id) => CommandError::warning(
            "invalid_game_id",
            "errors.invalid_game_id",
            format!("Invalid game id: {game_id}"),
            vec!["Refresh the games list and open the game again.".to_owned()],
        ),
        CliError::InvalidComponentId(component_id) => CommandError::warning(
            "invalid_component_id",
            "errors.invalid_component_id",
            format!("Invalid component id: {component_id}"),
            vec!["Reload the game details before building a new plan.".to_owned()],
        ),
        CliError::InvalidArtifactId(artifact_id) => CommandError::warning(
            "invalid_artifact_id",
            "errors.invalid_artifact_id",
            format!("Invalid artifact id: {artifact_id}"),
            vec!["Refresh replacement candidates and try again.".to_owned()],
        ),
        CliError::InvalidOperationId(operation_id) => CommandError::warning(
            "invalid_operation_id",
            "errors.invalid_operation_id",
            format!("Invalid operation id: {operation_id}"),
            vec!["Rebuild the plan or reload the operations list before retrying.".to_owned()],
        ),
        CliError::MissingArgument(argument) => CommandError::warning(
            "missing_argument",
            "errors.missing_argument",
            format!("Missing required argument: {argument}"),
            vec!["Retry the action after the required data is available.".to_owned()],
        ),
        CliError::UnexpectedArgument(argument) => CommandError::warning(
            "unexpected_argument",
            "errors.unexpected_argument",
            format!("Unexpected argument: {argument}"),
            vec!["Reload the desktop UI and retry the action.".to_owned()],
        ),
        CliError::UnknownArgument(argument) => CommandError::warning(
            "unknown_argument",
            "errors.unknown_argument",
            format!("Unknown argument: {argument}"),
            vec!["Reload the desktop UI and retry the action.".to_owned()],
        ),
        CliError::InvalidTechnology(technology) => CommandError::warning(
            "invalid_technology",
            "errors.invalid_technology",
            format!("Invalid technology: {technology}"),
            vec!["Refresh replacement candidates and try again.".to_owned()],
        ),
        CliError::NonUnicodeArgument => CommandError::warning(
            "non_unicode_argument",
            "errors.non_unicode_argument",
            "A command argument was not valid Unicode.",
            vec!["Retry the action with normalized text data.".to_owned()],
        ),
        CliError::CommandFailed(message) => command_failed_error(message),
        CliError::OutputSerializationFailed(message) => CommandError::error(
            "serialization_failed",
            "errors.serialization_failed",
            format!("Could not serialize command output: {message}"),
            vec!["Retry the action. If the problem persists, inspect the desktop logs.".to_owned()],
        ),
    }
}

fn command_failed_error(message: String) -> CommandError {
    if message.contains("confirmation token mismatch") {
        return CommandError::warning(
            "confirmation_token_mismatch",
            "errors.confirmation_token_mismatch",
            message,
            vec!["Rebuild the operation plan before applying it again.".to_owned()],
        );
    }

    if message.contains("game not found") {
        return CommandError::warning(
            "game_not_found",
            "errors.game_not_found",
            message,
            vec!["Refresh the catalog or scan the game folder again.".to_owned()],
        );
    }

    CommandError::error(
        "command_failed",
        "errors.command_failed",
        message,
        vec!["Retry the action. If the problem persists, inspect the desktop logs.".to_owned()],
    )
}

fn into_join_error(error: tauri::Error) -> CommandError {
    CommandError::error(
        "command_task_failed",
        "errors.command_task_failed",
        format!("Command task failed: {error}"),
        vec!["Retry the action. If the problem persists, restart the desktop app.".to_owned()],
    )
}

async fn run_json_command(
    task: impl FnOnce() -> Result<Value, renderpilot_cli::CliError> + Send + 'static,
) -> JsonCommandResult {
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(into_join_error)?
        .map_err(into_command_error)
}

#[tauri::command]
pub fn get_system_appearance() -> SystemAppearance {
    SystemAppearance {
        accent_color: read_system_accent_color(),
    }
}

#[tauri::command]
pub async fn scan_manual_folder(path: String) -> JsonCommandResult {
    run_json_command(move || renderpilot_cli::desktop::scan_manual_folder(path.into())).await
}

#[tauri::command]
pub async fn get_game_cards() -> JsonCommandResult {
    run_json_command(renderpilot_cli::desktop::get_game_cards).await
}

#[tauri::command]
pub async fn get_game_details(game_id: String) -> JsonCommandResult {
    run_json_command(move || renderpilot_cli::desktop::get_game_details(game_id)).await
}

#[tauri::command]
pub async fn build_swap_plan(
    game_id: String,
    component_id: String,
    artifact_id: String,
) -> JsonCommandResult {
    run_json_command(move || {
        renderpilot_cli::desktop::build_swap_plan(game_id, component_id, artifact_id)
    })
    .await
}

#[tauri::command]
pub async fn apply_operation_plan(
    operation_id: String,
    confirmation_token: String,
) -> JsonCommandResult {
    run_json_command(move || {
        renderpilot_cli::desktop::apply_operation_plan(operation_id, confirmation_token)
    })
    .await
}

#[tauri::command]
pub async fn rollback_operation(operation_id: String) -> JsonCommandResult {
    run_json_command(move || renderpilot_cli::desktop::rollback_operation(operation_id)).await
}

#[cfg(windows)]
fn read_system_accent_color() -> Option<String> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let dwm = hkcu.open_subkey("Software\\Microsoft\\Windows\\DWM").ok()?;
    let argb: u32 = dwm.get_value("ColorizationColor").ok()?;

    let rgb = argb & 0x00FF_FFFF;
    let red = ((rgb >> 16) & 0xFF) as u8;
    let green = ((rgb >> 8) & 0xFF) as u8;
    let blue = (rgb & 0xFF) as u8;

    Some(format!("#{red:02X}{green:02X}{blue:02X}"))
}

#[cfg(not(windows))]
fn read_system_accent_color() -> Option<String> {
    None
}
