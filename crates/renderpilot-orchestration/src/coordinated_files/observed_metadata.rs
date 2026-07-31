use std::path::Path;

use renderpilot_detection::PeInspection;
use renderpilot_domain::{ComponentFile, LibraryTechnology};

/// Rebuilds byte-derived metadata from the authoritative file at this boundary.
///
/// One PE read supplies both the optional file version and the complete
/// compatibility profile for technologies whose live candidate matching relies
/// on PE facts. Missing or malformed observations are never replaced with
/// persisted or catalog metadata.
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
    if matches!(
        technology,
        LibraryTechnology::OpenVr | LibraryTechnology::XiphVorbis
    ) && let Some(profile) = inspection.compatibility_profile()
    {
        file = file.with_pe_compatibility(profile);
    }
    file
}

#[cfg(test)]
mod tests {
    use renderpilot_detection::PeInspection;
    use renderpilot_domain::{
        Architecture, ComponentFile, LibraryTechnology, PathRef, PeImportProfile, PeImportSet, xiph,
    };

    use super::with_observed_inspection;

    #[test]
    fn xiph_observation_retains_the_complete_live_topology_profile() {
        let vorbis = observe_xiph_file("libvorbis.dll", &["kernel32.dll"], "vorbis_info_init");
        let vorbis_file = observe_xiph_file(
            "libvorbisfile.dll",
            &["kernel32.dll", "libvorbis.dll"],
            "ov_open",
        );

        let vorbis_profile = vorbis.pe_compatibility().expect("Vorbis PE profile");
        assert_eq!(vorbis_profile.architecture(), Architecture::X86);
        assert_eq!(
            vorbis_profile.named_exports().names(),
            &["vorbis_info_init"]
        );
        assert_eq!(
            vorbis_profile
                .imports()
                .expect("Vorbis imports")
                .regular
                .names(),
            &["kernel32.dll"]
        );

        let layout = xiph::detect_layout(&[vorbis, vorbis_file]).expect("Xiph layout");
        assert_eq!(
            layout.file_name(xiph::XiphMember::Vorbis),
            Some("libvorbis.dll")
        );
        assert_eq!(
            layout.file_name(xiph::XiphMember::VorbisFile),
            Some("libvorbisfile.dll")
        );
    }

    fn observe_xiph_file(name: &str, regular_imports: &[&str], export: &str) -> ComponentFile {
        let inspection = PeInspection {
            architecture: Some(Architecture::X86),
            version: None,
            identity: Default::default(),
            export_names: Some(vec![export.to_owned()]),
            import_profile: Some(Ok(PeImportProfile {
                regular: PeImportSet::from_observed_names(
                    regular_imports
                        .iter()
                        .map(|name| (*name).to_owned())
                        .collect(),
                )
                .expect("imports"),
                delay: PeImportSet::default(),
            })),
        };
        let file =
            ComponentFile::new(PathRef::new(format!("C:/Game/{name}")).expect("component path"));

        with_observed_inspection(file, LibraryTechnology::XiphVorbis, &inspection)
    }
}
