//! RenderPilot command-line binary.

use std::process::ExitCode;

fn main() -> ExitCode {
    match renderpilot_cli::run(std::env::args_os().skip(1)) {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            let exit_code = error.exit_code();
            eprintln!("{error}");

            ExitCode::from(exit_code)
        }
    }
}
