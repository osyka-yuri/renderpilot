//! Command-line interface behavior for RenderPilot.

use std::ffi::OsString;

use renderpilot_orchestration::application::app_info;

mod args;
mod catalog;
mod commands;
mod error;
mod hash;
mod luma;
mod output;
mod renodx;

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

#[cfg(test)]
pub(crate) fn run_with_context<I, F>(args: I, open_context: F) -> Result<String, CliError>
where
    I: IntoIterator<Item = OsString>,
    F: FnOnce() -> Result<
        renderpilot_orchestration::Context,
        renderpilot_orchestration::ServiceError,
    >,
{
    let command = args::parse_args(args)?;
    let app_info = app_info(APP_VERSION);

    commands::render_command_with_context(command, app_info, open_context)
}
