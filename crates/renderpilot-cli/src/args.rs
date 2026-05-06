use std::{ffi::OsString, path::PathBuf};

use renderpilot_domain::{ArtifactId, ComponentId, GameId, GraphicsTechnology, OperationId};

use crate::CliError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Summary,
    Help,
    Version,
    ScanFolder {
        path: PathBuf,
    },
    ListArtifacts {
        technology: Option<GraphicsTechnology>,
    },
    ListOperations {
        game_id: GameId,
    },
    Candidates {
        game_id: GameId,
    },
    PlanSwap {
        game_id: GameId,
        component_id: ComponentId,
        artifact_id: ArtifactId,
    },
    Backup {
        operation_id: OperationId,
    },
    ApplyOperation {
        operation_id: OperationId,
    },
    RollbackOperation {
        operation_id: OperationId,
    },
}

pub(crate) fn parse_args(args: impl IntoIterator<Item = OsString>) -> Result<Command, CliError> {
    let mut args = ArgCursor::new(args);
    let Some(first) = args.next_keyword()? else {
        return Ok(Command::Summary);
    };

    match first.as_str() {
        "--help" | "-h" => parse_flag_command(Command::Help, &mut args),
        "--version" | "-V" => parse_flag_command(Command::Version, &mut args),
        "scan-folder" => parse_scan_folder_command(&mut args),
        "list-artifacts" => parse_list_artifacts_command(&mut args),
        "list-operations" => parse_list_operations_command(&mut args),
        "candidates" => parse_candidates_command(&mut args),
        "plan-swap" => parse_plan_swap_command(&mut args),
        "backup" => parse_backup_command(&mut args),
        "apply" | "apply-operation" => parse_apply_command(&mut args),
        "rollback" => parse_rollback_command(&mut args),
        _ => Err(CliError::UnknownArgument(first)),
    }
}

fn parse_flag_command(command: Command, args: &mut ArgCursor) -> Result<Command, CliError> {
    args.finish()?;

    Ok(command)
}

fn parse_scan_folder_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let path = args.next_required_path("<path>")?;
    args.finish()?;

    Ok(Command::ScanFolder { path })
}

fn parse_list_artifacts_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let mut technology = None;

    while let Some(argument) = args.next_keyword()? {
        match argument.as_str() {
            "--technology" => {
                let value = args.next_required_keyword("<technology>")?;
                let parsed = GraphicsTechnology::from_slug(&value)
                    .ok_or(CliError::InvalidTechnology(value))?;
                technology = Some(parsed);
            }
            _ => return Err(CliError::UnexpectedArgument(argument)),
        }
    }

    Ok(Command::ListArtifacts { technology })
}

fn parse_candidates_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let game_id = parse_named_identifier(args, "--game", "<game_id>", CliError::InvalidGameId)?;
    Ok(Command::Candidates { game_id })
}

fn parse_list_operations_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let game_id = parse_named_identifier(args, "--game", "<game_id>", CliError::InvalidGameId)?;
    Ok(Command::ListOperations { game_id })
}

fn parse_plan_swap_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let mut game_id = None;
    let mut component_id = None;
    let mut artifact_id = None;

    while let Some(argument) = args.next_keyword()? {
        match argument.as_str() {
            "--game" => {
                game_id = Some(parse_identifier_argument(
                    args,
                    "<game_id>",
                    CliError::InvalidGameId,
                )?);
            }
            "--component" => {
                component_id = Some(parse_identifier_argument(
                    args,
                    "<component_id>",
                    CliError::InvalidComponentId,
                )?);
            }
            "--artifact" => {
                artifact_id = Some(parse_identifier_argument(
                    args,
                    "<artifact_id>",
                    CliError::InvalidArtifactId,
                )?);
            }
            _ => return Err(CliError::UnexpectedArgument(argument)),
        }
    }

    Ok(Command::PlanSwap {
        game_id: game_id.ok_or(CliError::MissingArgument("<game_id>"))?,
        component_id: component_id.ok_or(CliError::MissingArgument("<component_id>"))?,
        artifact_id: artifact_id.ok_or(CliError::MissingArgument("<artifact_id>"))?,
    })
}

fn parse_backup_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let operation_id = parse_named_identifier(
        args,
        "--operation",
        "<operation_id>",
        CliError::InvalidOperationId,
    )?;
    Ok(Command::Backup { operation_id })
}

fn parse_apply_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let operation_id = parse_named_identifier(
        args,
        "--operation",
        "<operation_id>",
        CliError::InvalidOperationId,
    )?;
    Ok(Command::ApplyOperation { operation_id })
}

fn parse_rollback_command(args: &mut ArgCursor) -> Result<Command, CliError> {
    let operation_id = parse_named_identifier(
        args,
        "--operation",
        "<operation_id>",
        CliError::InvalidOperationId,
    )?;
    Ok(Command::RollbackOperation { operation_id })
}

#[derive(Debug)]
struct ArgCursor {
    args: std::vec::IntoIter<OsString>,
}

impl ArgCursor {
    fn new(args: impl IntoIterator<Item = OsString>) -> Self {
        Self {
            args: args.into_iter().collect::<Vec<_>>().into_iter(),
        }
    }

    fn next_keyword(&mut self) -> Result<Option<String>, CliError> {
        self.args.next().map(parse_os_argument).transpose()
    }

    fn next_required_path(&mut self, argument_name: &'static str) -> Result<PathBuf, CliError> {
        self.args
            .next()
            .map(PathBuf::from)
            .ok_or(CliError::MissingArgument(argument_name))
    }

    fn next_required_keyword(&mut self, argument_name: &'static str) -> Result<String, CliError> {
        self.args
            .next()
            .map(parse_os_argument)
            .transpose()?
            .ok_or(CliError::MissingArgument(argument_name))
    }

    fn finish(&mut self) -> Result<(), CliError> {
        if let Some(extra) = self.next_keyword()? {
            return Err(CliError::UnexpectedArgument(extra));
        }

        Ok(())
    }
}

fn parse_os_argument(argument: OsString) -> Result<String, CliError> {
    argument
        .into_string()
        .map_err(|_| CliError::NonUnicodeArgument)
}

fn parse_named_identifier<T>(
    args: &mut ArgCursor,
    flag: &'static str,
    argument_name: &'static str,
    invalid: fn(String) -> CliError,
) -> Result<T, CliError>
where
    T: TryFrom<String>,
{
    let mut parsed = None;

    while let Some(argument) = args.next_keyword()? {
        if argument != flag {
            return Err(CliError::UnexpectedArgument(argument));
        }

        parsed = Some(parse_identifier_argument(args, argument_name, invalid)?);
    }

    parsed.ok_or(CliError::MissingArgument(argument_name))
}

fn parse_identifier_argument<T>(
    args: &mut ArgCursor,
    argument_name: &'static str,
    invalid: fn(String) -> CliError,
) -> Result<T, CliError>
where
    T: TryFrom<String>,
{
    let value = args.next_required_keyword(argument_name)?;

    T::try_from(value.clone()).map_err(|_| invalid(value))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use renderpilot_domain::{ArtifactId, ComponentId, GameId, GraphicsTechnology, OperationId};

    use super::{parse_args, Command};
    use crate::CliError;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_args_parse_as_summary() {
        assert_eq!(
            parse_args(Vec::new()).expect("valid args"),
            Command::Summary
        );
    }

    #[test]
    fn version_flag_parses() {
        assert_eq!(
            parse_args(args(&["--version"])).expect("valid args"),
            Command::Version
        );
    }

    #[test]
    fn scan_folder_requires_path() {
        let error = parse_args(args(&["scan-folder"])).expect_err("path is required");

        assert_eq!(error, CliError::MissingArgument("<path>"));
    }

    #[test]
    fn extra_arg_is_reported() {
        let error = parse_args(args(&["--version", "--bad"])).expect_err("extra arg should fail");

        assert_eq!(error, CliError::UnexpectedArgument("--bad".to_owned()));
    }

    #[test]
    fn scan_folder_rejects_extra_arg() {
        let error = parse_args(args(&["scan-folder", "game-dir", "--bad"]))
            .expect_err("extra arg should fail");

        assert_eq!(error, CliError::UnexpectedArgument("--bad".to_owned()));
    }

    #[test]
    fn list_artifacts_parses_without_filter() {
        assert_eq!(
            parse_args(args(&["list-artifacts"])).expect("valid args"),
            Command::ListArtifacts { technology: None }
        );
    }

    #[test]
    fn list_artifacts_parses_technology_filter() {
        assert_eq!(
            parse_args(args(&[
                "list-artifacts",
                "--technology",
                "dlss_super_resolution"
            ]))
            .expect("valid args"),
            Command::ListArtifacts {
                technology: Some(GraphicsTechnology::DlssSuperResolution)
            }
        );
    }

    #[test]
    fn list_artifacts_rejects_unknown_technology() {
        let error = parse_args(args(&["list-artifacts", "--technology", "bad-tech"]))
            .expect_err("unknown technology should fail");

        assert_eq!(error, CliError::InvalidTechnology("bad-tech".to_owned()));
    }

    #[test]
    fn candidates_requires_game_argument() {
        let error = parse_args(args(&["candidates"])).expect_err("game id should be required");

        assert_eq!(error, CliError::MissingArgument("<game_id>"));
    }

    #[test]
    fn candidates_parses_game_argument() {
        assert_eq!(
            parse_args(args(&["candidates", "--game", "manual:C:/Games/GameA"]))
                .expect("valid args"),
            Command::Candidates {
                game_id: GameId::new("manual:C:/Games/GameA").expect("game id should parse")
            }
        );
    }

    #[test]
    fn list_operations_requires_game_argument() {
        let error = parse_args(args(&["list-operations"])).expect_err("game id should be required");

        assert_eq!(error, CliError::MissingArgument("<game_id>"));
    }

    #[test]
    fn list_operations_parses_game_argument() {
        assert_eq!(
            parse_args(args(&[
                "list-operations",
                "--game",
                "manual:C:/Games/GameA"
            ]))
            .expect("valid args"),
            Command::ListOperations {
                game_id: GameId::new("manual:C:/Games/GameA").expect("game id should parse")
            }
        );
    }

    #[test]
    fn plan_swap_requires_all_identifiers() {
        let error = parse_args(args(&["plan-swap", "--game", "manual:C:/Games/GameA"]))
            .expect_err("component id should be required");

        assert_eq!(error, CliError::MissingArgument("<component_id>"));

        let error = parse_args(args(&[
            "plan-swap",
            "--game",
            "manual:C:/Games/GameA",
            "--component",
            "component:game-a:dlss",
        ]))
        .expect_err("artifact id should be required");

        assert_eq!(error, CliError::MissingArgument("<artifact_id>"));
    }

    #[test]
    fn plan_swap_parses_all_identifiers() {
        assert_eq!(
            parse_args(args(&[
                "plan-swap",
                "--game",
                "manual:C:/Games/GameA",
                "--component",
                "component:game-a:dlss",
                "--artifact",
                "artifact:dlss-3.7",
            ]))
            .expect("valid args"),
            Command::PlanSwap {
                game_id: GameId::new("manual:C:/Games/GameA").expect("game id should parse"),
                component_id: ComponentId::new("component:game-a:dlss")
                    .expect("component id should parse"),
                artifact_id: ArtifactId::new("artifact:dlss-3.7")
                    .expect("artifact id should parse"),
            }
        );
    }

    #[test]
    fn backup_requires_operation_id() {
        let error = parse_args(args(&["backup"])).expect_err("operation id should be required");

        assert_eq!(error, CliError::MissingArgument("<operation_id>"));
    }

    #[test]
    fn backup_parses_operation_id() {
        assert_eq!(
            parse_args(args(&[
                "backup",
                "--operation",
                "operation:replace_component:1:component:game-a:dlss:artifact:dlss-3.7",
            ]))
            .expect("valid args"),
            Command::Backup {
                operation_id: OperationId::new(
                    "operation:replace_component:1:component:game-a:dlss:artifact:dlss-3.7"
                )
                .expect("operation id should parse"),
            }
        );
    }

    #[test]
    fn apply_requires_operation_id() {
        let error = parse_args(args(&["apply"])).expect_err("operation id should be required");

        assert_eq!(error, CliError::MissingArgument("<operation_id>"));
    }

    #[test]
    fn apply_parses_operation_id() {
        assert_eq!(
            parse_args(args(&[
                "apply",
                "--operation",
                "operation:replace_component:1:component:game-a:dlss:artifact:dlss-3.7",
            ]))
            .expect("valid args"),
            Command::ApplyOperation {
                operation_id: OperationId::new(
                    "operation:replace_component:1:component:game-a:dlss:artifact:dlss-3.7"
                )
                .expect("operation id should parse"),
            }
        );
    }

    #[test]
    fn apply_operation_alias_parses_operation_id() {
        assert_eq!(
            parse_args(args(&[
                "apply-operation",
                "--operation",
                "operation:replace_component:1:component:game-a:dlss:artifact:dlss-3.7",
            ]))
            .expect("valid args"),
            Command::ApplyOperation {
                operation_id: OperationId::new(
                    "operation:replace_component:1:component:game-a:dlss:artifact:dlss-3.7"
                )
                .expect("operation id should parse"),
            }
        );
    }

    #[test]
    fn rollback_requires_operation_id() {
        let error = parse_args(args(&["rollback"])).expect_err("operation id should be required");

        assert_eq!(error, CliError::MissingArgument("<operation_id>"));
    }

    #[test]
    fn rollback_parses_operation_id() {
        assert_eq!(
            parse_args(args(&[
                "rollback",
                "--operation",
                "operation:replace_component:1:component:game-a:dlss:artifact:dlss-3.7",
            ]))
            .expect("valid args"),
            Command::RollbackOperation {
                operation_id: OperationId::new(
                    "operation:replace_component:1:component:game-a:dlss:artifact:dlss-3.7"
                )
                .expect("operation id should parse"),
            }
        );
    }
}
