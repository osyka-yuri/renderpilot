//! Command-line interface behavior for RenderPilot.

use std::ffi::OsString;

use renderpilot_application::app_info;

mod args;
mod backup_manager;
mod catalog;
mod commands;
pub mod desktop;
mod error;
mod hash;
mod output;

#[cfg(test)]
mod test_env;

pub use error::CliError;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Serves image bytes for cover requests.
///
/// Handles requests of the form:
///
/// `http://rp-cover.localhost/<url-encoded-game-id>`
#[must_use]
pub fn cover_asset_protocol_response(request_path: &str) -> http::Response<Vec<u8>> {
    catalog::covers::cover_protocol_http_response(request_path)
}

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
