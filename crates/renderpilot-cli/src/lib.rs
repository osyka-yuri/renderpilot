//! Command-line interface behavior for RenderPilot.

use std::ffi::OsString;

use renderpilot_orchestration::application::app_info;

mod args;
mod catalog;
mod commands;
mod error;
mod hash;
mod output;

#[cfg(test)]
mod test_env;

pub use error::CliError;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Parses CLI arguments, executes the selected command, and returns stdout text.
///
/// `args` should use the same shape as process arguments, usually including the
/// executable name as the first item if `args::parse_args` expects it.
pub fn run<I>(args: I) -> Result<String, CliError>
where
    I: IntoIterator<Item = OsString>,
{
    let command = args::parse_args(args)?;
    let app_info = app_info(APP_VERSION);

    commands::render_command(command, app_info)
}
