use renderpilot_orchestration::domain::{AddonKind, GameId};

use super::command::Command;
use super::cursor::{ArgCursor, parse_identifier_argument, parse_named_identifier};
use crate::CliError;
fn addon_status_command(kind: AddonKind, game_id: GameId) -> Command {
    match kind {
        AddonKind::RenoDx => Command::RenodxStatus { game_id },
        AddonKind::Luma => Command::LumaStatus { game_id },
    }
}

fn addon_uninstall_command(kind: AddonKind, game_id: GameId) -> Command {
    match kind {
        AddonKind::RenoDx => Command::RenodxUninstall { game_id },
        AddonKind::Luma => Command::LumaUninstall { game_id },
    }
}

fn addon_check_update_command(kind: AddonKind, game_id: GameId, deep: bool) -> Command {
    match kind {
        AddonKind::RenoDx => Command::RenodxCheckUpdate { game_id },
        AddonKind::Luma => Command::LumaCheckUpdate { game_id, deep },
    }
}

fn addon_check_updates_command(kind: AddonKind) -> Command {
    match kind {
        AddonKind::RenoDx => Command::RenodxCheckUpdates,
        AddonKind::Luma => Command::LumaCheckUpdates,
    }
}

fn addon_supports_deep_check(kind: AddonKind) -> bool {
    renderpilot_orchestration::addons::addon_supports_deep_check(kind)
}
pub(super) fn parse_addon_command(
    args: &mut ArgCursor,
    kind: AddonKind,
) -> Result<Command, CliError> {
    let Some(subcommand) = args.next_keyword()? else {
        return Err(CliError::MissingArgument(
            "<status|uninstall|check-update|check-updates>",
        ));
    };

    match subcommand.as_str() {
        "status" => {
            let game_id =
                parse_named_identifier(args, "--game", "<game_id>", CliError::InvalidGameId)?;
            Ok(addon_status_command(kind, game_id))
        }
        "uninstall" => {
            let game_id =
                parse_named_identifier(args, "--game", "<game_id>", CliError::InvalidGameId)?;
            Ok(addon_uninstall_command(kind, game_id))
        }
        "check-update" => parse_check_update_command(args, kind),
        "check-updates" => {
            args.finish()?;
            Ok(addon_check_updates_command(kind))
        }
        _ => Err(CliError::UnknownArgument(subcommand)),
    }
}

fn parse_check_update_command(args: &mut ArgCursor, kind: AddonKind) -> Result<Command, CliError> {
    let mut game_id = None;
    let mut deep = false;

    while let Some(argument) = args.next_keyword()? {
        match argument.as_str() {
            "--game" => {
                game_id = Some(parse_identifier_argument(
                    args,
                    "<game_id>",
                    CliError::InvalidGameId,
                )?);
            }
            "--deep" => {
                if !addon_supports_deep_check(kind) {
                    return Err(CliError::UnexpectedArgument("--deep".to_owned()));
                }
                deep = true;
            }
            _ => return Err(CliError::UnexpectedArgument(argument)),
        }
    }

    Ok(addon_check_update_command(
        kind,
        game_id.ok_or(CliError::MissingArgument("<game_id>"))?,
        deep,
    ))
}
