//! Release verification utility for RenderPilot updater artifact signatures.
//!
//! Validates Tauri updater Minisign signatures (outer base64 encoded) against
//! release artifacts using the compile-time authoritative public key.

use std::path::Path;

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("updater signature verification failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let artifact = args.next().ok_or_else(|| {
        "Usage: renderpilot-updater-verifier <artifact-path> <signature-path>".to_owned()
    })?;
    let signature = args
        .next()
        .ok_or_else(|| "signature path is required".to_owned())?;
    if args.next().is_some() {
        return Err("expected exactly two arguments: <artifact-path> <signature-path>".to_owned());
    }

    let artifact_path = Path::new(&artifact);
    let signature_path = Path::new(&signature);

    renderpilot_updater_signature::verify_artifact_file(artifact_path, signature_path)?;
    println!("Verified updater signature for {}", artifact_path.display());
    Ok(())
}
