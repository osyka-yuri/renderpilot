use std::path::Path;

use renderpilot_detection::PeInspection;
use renderpilot_domain::{ComponentFile, LibraryTechnology};

/// Rebuilds byte-derived metadata from the authoritative file at this boundary.
///
/// One PE read supplies both the optional file version and OpenVR's atomic
/// export-surface compatibility profile. Missing or malformed observations are
/// never replaced with persisted or catalog metadata.
pub(crate) fn with_observed_metadata(
    file: ComponentFile,
    technology: LibraryTechnology,
    bytes_path: &Path,
) -> ComponentFile {
    let Some(inspection) = renderpilot_detection::inspect_pe(bytes_path) else {
        return file;
    };
    with_observed_inspection(file, technology, &inspection)
}

/// Attaches metadata derived from an already-captured byte snapshot.
pub(crate) fn with_observed_inspection(
    mut file: ComponentFile,
    technology: LibraryTechnology,
    inspection: &PeInspection,
) -> ComponentFile {
    if let Some(version) = inspection.version.clone() {
        file = file.with_version(version);
    }
    if technology == LibraryTechnology::OpenVr
        && let Some(profile) = inspection.compatibility_profile()
    {
        file = file.with_pe_compatibility(profile);
    }
    file
}
