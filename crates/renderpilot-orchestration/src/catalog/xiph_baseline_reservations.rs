//! Durable rollback-baseline validation for vendor-suffixed Xiph layouts.

use std::path::Path;

use renderpilot_application::{AppError, AppResult};
use renderpilot_domain::{ComponentRollbackBaseline, xiph};

/// Detects the durable-state shape that needs reservation validation. It is
/// intentionally based on the immutable baseline, not the current component:
/// after a successful migration the vendor core files are correctly absent
/// from the active component.
pub(crate) fn is_vendor_xiph_baseline(baseline: &ComponentRollbackBaseline) -> bool {
    baseline.files().iter().any(|file| {
        file.path()
            .file_name()
            .and_then(|name| xiph::parse_runtime_file_name(name).ok().flatten())
            .is_some_and(|name| name.is_vendor())
    })
}

/// Rejects an indeterminate or recreated vendor original before any baseline
/// resolver can fall back to reading the live path.
pub(crate) fn verify_vendor_xiph_baseline_reservations(
    baseline: &ComponentRollbackBaseline,
) -> AppResult<()> {
    if !is_vendor_xiph_baseline(baseline) {
        return Ok(());
    }
    let expected = baseline.expected_active_files();
    if expected.is_empty() {
        return Err(AppError::invalid_input(
            "vendor-suffixed Xiph rollback baseline has no expected active projection",
        ));
    }
    for original in baseline.files() {
        let live = Path::new(original.path().as_str());
        let active = expected
            .iter()
            .any(|file| crate::paths::same_path(Path::new(file.path().as_str()), live));
        if active {
            continue;
        }
        let sidecar = crate::fs::backup_path(live)
            .map_err(|error| AppError::invalid_input(error.to_string()))?;
        match std::fs::symlink_metadata(live) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(AppError::invalid_input(format!(
                    "reserved vendor Xiph original unexpectedly exists at {}",
                    live.display()
                )));
            }
            Err(error) => {
                return Err(AppError::invalid_input(format!(
                    "cannot inspect reserved vendor Xiph original {}: {error}",
                    live.display()
                )));
            }
        }
        let expected_hash = original.sha256().ok_or_else(|| {
            AppError::invalid_input(format!(
                "vendor Xiph rollback baseline has no hash for {}",
                live.display()
            ))
        })?;
        crate::fs::verify_sidecar(&sidecar, expected_hash).map_err(|error| {
            AppError::invalid_input(format!(
                "reserved vendor Xiph original has no verified sidecar at {}: {error}",
                sidecar.display()
            ))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{ComponentFile, PathRef, Sha256Hash};

    use super::*;

    #[test]
    fn vendor_baseline_without_expected_active_projection_is_indeterminate() {
        let original =
            ComponentFile::new(PathRef::new("C:/Game/vorbis_vs2010_x64_rwdi.dll").expect("path"))
                .with_sha256(Sha256Hash::new("a".repeat(64)).expect("hash"));
        let baseline = ComponentRollbackBaseline::new(vec![original]);
        assert!(verify_vendor_xiph_baseline_reservations(&baseline).is_err());
    }
}
