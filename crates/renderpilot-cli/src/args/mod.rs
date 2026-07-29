//! CLI argument parsing.
//!
//! - [`command`] -- parsed `Command` enum
//! - [`cursor`] -- raw argv cursor + identifier helpers
//! - [`catalog`] -- scan/list/candidates/plan/apply/rollback
//! - [`addon`] -- renodx/luma subcommands

mod addon;
mod catalog;
pub(crate) mod command;
mod cursor;

#[cfg(test)]
mod tests;

use std::ffi::OsString;

use crate::CliError;
use renderpilot_orchestration::domain::AddonKind;

use self::addon::parse_addon_command;
use self::catalog::{
    parse_add_game_command, parse_apply_command, parse_candidates_command,
    parse_list_artifacts_command, parse_list_operations_command, parse_plan_rollback_command,
    parse_plan_swap_command, parse_rollback_command,
};
use self::command::Command;
use self::cursor::ArgCursor;

pub(crate) fn parse_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<command::Command, CliError> {
    let mut args = ArgCursor::new(args);
    let Some(first) = args.next_keyword()? else {
        return Ok(Command::Summary);
    };

    match first.as_str() {
        "--help" | "-h" => parse_flag_command(Command::Help, &mut args),
        "--version" | "-V" => parse_flag_command(Command::Version, &mut args),
        "add-game" => parse_add_game_command(&mut args),
        "list-artifacts" => parse_list_artifacts_command(&mut args),
        "list-operations" => parse_list_operations_command(&mut args),
        "candidates" => parse_candidates_command(&mut args),
        "plan-swap" => parse_plan_swap_command(&mut args),
        "plan-rollback" => parse_plan_rollback_command(&mut args),
        "apply" | "apply-operation" => parse_apply_command(&mut args),
        "rollback" => parse_rollback_command(&mut args),
        "renodx" => parse_addon_command(&mut args, AddonKind::RenoDx),
        "luma" => parse_addon_command(&mut args, AddonKind::Luma),
        _ => Err(CliError::UnknownArgument(first)),
    }
}

fn parse_flag_command(
    command: command::Command,
    args: &mut ArgCursor,
) -> Result<command::Command, CliError> {
    args.finish()?;
    Ok(command)
}
