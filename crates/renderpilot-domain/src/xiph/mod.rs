//! Canonical Xiph identities and validated Windows deployment graphs.
//!
//! The public facade is intentionally small. Naming, topology, ABI, and
//! release dimensions are independent concepts and remain separated internally.

mod abi;
mod layout;
mod naming;
mod release;

pub use abi::is_public_api_export;
pub use layout::{XiphLayout, detect_layout, detect_layout_with_file_names};
pub use naming::{XiphMember, XiphNameStyle, XiphNamingProfile, classify_file_name, file_name};
pub use release::{XiphReleaseAxes, XiphReleaseAxis, XiphReleaseVersions};

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use crate::{
        Architecture, ComponentFile, PackageVersion, PathRef, PeCompatibilityProfile, PeExportSet,
        PeImportProfile, PeImportSet,
    };

    use super::*;

    #[test]
    fn detects_fallout_style_embedded_file_core() {
        let files = vec![
            member("libvorbisfile.dll", &["kernel32.dll", "libvorbis.dll"]),
            member("libvorbis.dll", &["kernel32.dll"]),
        ];
        let layout = detect_layout(&files).expect("Fallout layout");
        assert_eq!(
            layout.members().collect::<Vec<_>>(),
            vec![XiphMember::VorbisFile, XiphMember::Vorbis]
        );
        assert_eq!(
            layout.dependencies(XiphMember::VorbisFile),
            Some(&BTreeSet::from([XiphMember::Vorbis]))
        );
        assert_eq!(
            layout.release_axes().iter().collect::<Vec<_>>(),
            vec![XiphReleaseAxis::Ogg, XiphReleaseAxis::Vorbis]
        );
        assert_eq!(layout.naming_profile(), XiphNamingProfile::Lib);
    }

    #[test]
    fn embedded_ogg_remains_a_required_release_axis() {
        let embedded = vec![
            ComponentFile::new(PathRef::new("C:/Game/libvorbisfile.dll").expect("path")),
            ComponentFile::new(PathRef::new("C:/Game/libvorbis.dll").expect("path")),
        ];
        assert_eq!(
            XiphReleaseAxes::from_component_files(&embedded)
                .expect("embedded axes")
                .iter()
                .collect::<Vec<_>>(),
            vec![XiphReleaseAxis::Ogg, XiphReleaseAxis::Vorbis]
        );

        let singleton = vec![ComponentFile::new(
            PathRef::new("C:/Game/libogg.dll").expect("path"),
        )];
        assert_eq!(
            XiphReleaseAxes::from_component_files(&singleton)
                .expect("Ogg axes")
                .iter()
                .collect::<Vec<_>>(),
            vec![XiphReleaseAxis::Ogg]
        );
    }

    #[test]
    fn release_versions_cross_the_catalog_boundary_only_with_required_axes() {
        let axes = XiphReleaseAxes::from_members([XiphMember::Vorbis]);
        let components = BTreeMap::from([
            (
                "ogg".to_owned(),
                PackageVersion::parse("1.3.6").expect("Ogg"),
            ),
            (
                "vorbis".to_owned(),
                PackageVersion::parse("1.3.7").expect("Vorbis"),
            ),
        ]);
        let versions = XiphReleaseVersions::from_catalog_components(&axes, &components)
            .expect("complete versions");

        assert_eq!(versions.to_catalog_components(), components);
        assert_eq!(
            versions.get(XiphReleaseAxis::Vorbis),
            Some(&PackageVersion::parse("1.3.7").expect("Vorbis"))
        );
    }

    #[test]
    fn detects_dmc_shared_graph_and_separate_encoder() {
        let files = vec![
            member(
                "libvorbisfile.dll",
                &["kernel32.dll", "libogg.dll", "libvorbis.dll"],
            ),
            member("libvorbisenc.dll", &["libvorbis.dll"]),
            member("libvorbis.dll", &["libogg.dll"]),
            member("libogg.dll", &[]),
        ];
        let layout = detect_layout(&files).expect("shared layout");
        assert_eq!(layout.members().count(), 4);
    }

    #[test]
    fn recognizes_case_and_all_reviewed_aliases() {
        assert_eq!(
            classify_file_name("vorbisFile.dll"),
            Some((XiphMember::VorbisFile, XiphNameStyle::Plain))
        );

        for member in XiphMember::ALL {
            for style in XiphNameStyle::ALL {
                let name = file_name(member, style);
                assert_eq!(classify_file_name(name), Some((member, style)));
            }
        }

        let files = vec![
            member("libvorbisfile-3.dll", &["libogg-0.dll", "libvorbis-0.dll"]),
            member("libvorbis-0.dll", &["libogg-0.dll"]),
            member("libogg-0.dll", &[]),
        ];
        assert!(detect_layout(&files).is_some());
    }

    #[test]
    fn accepts_import_proven_mixed_names_but_rejects_missing_or_wrong_aliases() {
        let mixed = vec![
            member("libvorbisfile.dll", &["libvorbis-0.dll"]),
            member("libvorbis-0.dll", &[]),
        ];
        assert_eq!(
            detect_layout(&mixed)
                .expect("valid mixed deployment")
                .naming_profile(),
            XiphNamingProfile::Mixed
        );
        assert_eq!(
            detect_layout(&[
                member("libvorbisfile.dll", &["libvorbis.dll"]),
                member("libvorbis-0.dll", &[]),
            ]),
            None
        );
    }

    fn member(name: &str, imports: &[&str]) -> ComponentFile {
        ComponentFile::new(PathRef::new(format!("C:/game/{name}")).expect("path"))
            .with_pe_compatibility(
                PeCompatibilityProfile::new(
                    Architecture::X86,
                    PeExportSet::from_observed_names(vec!["export".to_owned()]).expect("exports"),
                )
                .with_imports(PeImportProfile {
                    regular: PeImportSet::from_observed_names(
                        imports.iter().map(|name| (*name).to_owned()).collect(),
                    )
                    .expect("imports"),
                    delay: PeImportSet::default(),
                }),
            )
    }
}
