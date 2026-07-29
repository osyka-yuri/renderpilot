//! CLI orchestration and presentation for the explicit add-game workflow.

use std::path::PathBuf;

use renderpilot_orchestration::ServiceError;

use crate::{args::command::AddGameRootChoiceArg, catalog, error::CliError};

use super::{CliOutput, render_json};

pub(super) fn add_game(
    context: &renderpilot_orchestration::Context,
    path: PathBuf,
    executable: Option<PathBuf>,
    root_choice: AddGameRootChoiceArg,
    allow_root_correction: bool,
) -> CliOutput {
    let inspection = catalog::inspect_game_install(context, &path)?;
    let root_choice = resolve_cli_root_choice(&inspection.decision, root_choice)?;
    let result = catalog::add_game(
        context,
        catalog::AddGameRequest {
            selected_root: path,
            root_choice,
            allow_root_correction,
            chosen_executable: executable,
            inspection_fingerprint: inspection.inspection_fingerprint,
        },
    )?;
    let warnings = result
        .warnings
        .iter()
        .map(add_game_warning_json)
        .collect::<Vec<_>>();
    let output = serde_json::json!({
        "gameId": result.game_id,
        "effectiveRoot": result.effective_root,
        "disposition": match result.disposition {
            catalog::AddGameDisposition::Added => "added",
            catalog::AddGameDisposition::Unchanged => "unchanged",
            catalog::AddGameDisposition::Updated => "updated",
            catalog::AddGameDisposition::RootCorrected => "root_corrected",
        },
        "rootAuthority": result.root_authority_name(),
        "detectedLibraryCount": result.detected_library_count,
        "consolidatedGameIds": result.consolidated_game_ids,
        "recoveryBundlePath": result.recovery_bundle_path,
        "warnings": warnings,
    });
    render_json(&output)
}

fn resolve_cli_root_choice(
    decision: &catalog::AddGameDecision,
    requested: AddGameRootChoiceArg,
) -> Result<catalog::AddGameRootChoice, CliError> {
    let root_choice = match requested {
        AddGameRootChoiceArg::Auto => return resolve_automatic_choice(decision),
        AddGameRootChoiceArg::Selected => catalog::AddGameRootChoice::Selected,
        AddGameRootChoiceArg::Recommended => catalog::AddGameRootChoice::Recommended,
    };

    decision
        .option_for(root_choice)
        .map(|_| root_choice)
        .ok_or_else(|| {
            ServiceError::invalid_input(format!(
                "requested root choice is not available; allowed actions: {}",
                format_decision_options(decision)
            ))
            .into()
        })
}

fn resolve_automatic_choice(
    decision: &catalog::AddGameDecision,
) -> Result<catalog::AddGameRootChoice, CliError> {
    match decision {
        catalog::AddGameDecision::Automatic { option } => Ok(option.root_choice),
        catalog::AddGameDecision::Review(review) => Err(ServiceError::invalid_input(format!(
            "add-game review is required; choose one of: {}",
            format_add_game_options(review.options())
        ))
        .into()),
        catalog::AddGameDecision::Unavailable { reasons } => {
            Err(ServiceError::invalid_input(format!(
                "selected folder cannot be added: {}",
                format_unavailable_reasons(reasons)
            ))
            .into())
        }
    }
}

fn format_decision_options(decision: &catalog::AddGameDecision) -> String {
    match decision {
        catalog::AddGameDecision::Automatic { option } => {
            format_add_game_options(std::slice::from_ref(option))
        }
        catalog::AddGameDecision::Review(review) => format_add_game_options(review.options()),
        catalog::AddGameDecision::Unavailable { reasons } => format_unavailable_reasons(reasons),
    }
}

fn format_add_game_options(options: &[catalog::AddGameOption]) -> String {
    options
        .iter()
        .map(|option| {
            format!(
                "{}:{}",
                match option.root_choice {
                    catalog::AddGameRootChoice::Selected => "selected",
                    catalog::AddGameRootChoice::Recommended => "recommended",
                },
                match option.catalog_action {
                    catalog::AddGameCatalogAction::Add => "add",
                    catalog::AddGameCatalogAction::Rescan => "rescan",
                    catalog::AddGameCatalogAction::CorrectExistingRoot => "correct_existing_root",
                }
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_unavailable_reasons(reasons: &[catalog::AddGameUnavailableReason]) -> String {
    reasons
        .iter()
        .copied()
        .map(unavailable_reason_name)
        .collect::<Vec<_>>()
        .join(", ")
}

const fn unavailable_reason_name(reason: catalog::AddGameUnavailableReason) -> &'static str {
    match reason {
        catalog::AddGameUnavailableReason::MultipleInstalls => "multiple_installs",
        catalog::AddGameUnavailableReason::ContainsProvenInstall => "contains_proven_install",
        catalog::AddGameUnavailableReason::ContainsMultipleCatalogInstalls => {
            "contains_multiple_catalog_installs"
        }
        catalog::AddGameUnavailableReason::InsideExistingInstall => "inside_existing_install",
        catalog::AddGameUnavailableReason::NoReadableExecutable => "no_readable_executable",
        catalog::AddGameUnavailableReason::RootCorrectionBlocked => "root_correction_blocked",
    }
}

fn add_game_warning_json(warning: &catalog::AddGameWarning) -> serde_json::Value {
    let (code, message, parameters) = match warning {
        catalog::AddGameWarning::LegacyCardsConsolidated { count } => (
            "legacy_cards_consolidated",
            format!("consolidated {count} proven false legacy game card(s)"),
            serde_json::json!({ "count": count }),
        ),
        catalog::AddGameWarning::LegacyCardsRetained { count } => (
            "legacy_cards_retained",
            format!(
                "retained {count} legacy card(s) because independent-install evidence was inconclusive"
            ),
            serde_json::json!({ "count": count }),
        ),
        catalog::AddGameWarning::RecoveryBundleCreated { path } => (
            "recovery_bundle_created",
            format!("catalog state excluded by root correction was preserved in {path}"),
            serde_json::json!({ "path": path }),
        ),
        catalog::AddGameWarning::RootCorrectionHistoryArchived { path } => (
            "root_correction_history_archived",
            format!("operation history excluded by root correction was preserved in {path}"),
            serde_json::json!({ "path": path }),
        ),
        catalog::AddGameWarning::FilesystemProbeError => (
            "filesystem_probe_error",
            "the selected folder could not be inspected completely".to_owned(),
            serde_json::json!({}),
        ),
        catalog::AddGameWarning::InsideExistingInstall => (
            "inside_existing_install",
            "the selected folder belongs to an existing game; use that game root".to_owned(),
            serde_json::json!({}),
        ),
        catalog::AddGameWarning::NarrowsExistingInstall => (
            "narrows_existing_install",
            "the existing manual root appears to contain multiple game folders; confirming will correct that card to the selected folder".to_owned(),
            serde_json::json!({}),
        ),
        catalog::AddGameWarning::MultipleProvenInstalls => (
            "multiple_proven_installs",
            "the selected folder contains multiple proven game installations".to_owned(),
            serde_json::json!({}),
        ),
        catalog::AddGameWarning::ContainsProvenInstall => (
            "contains_proven_install",
            "the selected folder contains a proven game installation; use its exact root".to_owned(),
            serde_json::json!({}),
        ),
        catalog::AddGameWarning::ExplicitExecutableRequired => (
            "explicit_executable_required",
            "all valid executables look like launchers or helpers; choose one explicitly".to_owned(),
            serde_json::json!({}),
        ),
        catalog::AddGameWarning::NoReadableExecutable => (
            "no_readable_executable",
            "the selected folder cannot be added separately because it has no readable Windows PE executable".to_owned(),
            serde_json::json!({}),
        ),
    };
    serde_json::json!({
        "code": code,
        "message": message,
        "parameters": parameters,
    })
}
