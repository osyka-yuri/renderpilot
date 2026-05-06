//! Command-line interface behavior for RenderPilot.

use std::ffi::OsString;

use renderpilot_application::app_info;

mod args;
mod backup_manager;
mod catalog;
mod commands;
pub mod desktop;
mod error;
mod output;

#[cfg(test)]
mod test_env;

pub use error::CliError;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Parses CLI arguments, executes the selected command, and returns stdout text.
pub fn run(args: impl IntoIterator<Item = OsString>) -> Result<String, CliError> {
    let command = args::parse_args(args)?;
    let info = app_info(VERSION);

    commands::render_command(command, info)
}
