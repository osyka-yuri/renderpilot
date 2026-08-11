//! Release-only verifier for updater artifacts.
//!
//! It delegates to the same build-generated public key and Minisign verifier as
//! the portable runtime and is not shipped with either desktop distribution.

#[cfg(windows)]
fn main() {
    if let Err(error) = verify_from_args() {
        eprintln!("updater signature verification failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("updater artifact verification is supported only on Windows");
    std::process::exit(1);
}

#[cfg(windows)]
fn verify_from_args() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let artifact = args
        .next()
        .ok_or_else(|| "artifact path is required".to_owned())?;
    let signature = args
        .next()
        .ok_or_else(|| "signature path is required".to_owned())?;
    if args.next().is_some() {
        return Err("expected exactly two paths".to_owned());
    }

    let artifact = std::path::Path::new(&artifact);
    let signature = std::path::Path::new(&signature);
    renderpilot_desktop::verify_updater_artifact(artifact, signature)?;
    println!("Verified updater signature for {}", artifact.display());
    Ok(())
}
