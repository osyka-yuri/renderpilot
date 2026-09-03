//! Canonical Xiph identities and validated Windows deployment graphs.
//!
//! The public facade is intentionally small. Naming, topology, ABI, and
//! release dimensions are independent concepts and remain separated internally.

mod abi;
mod layout;
mod naming;
mod release;
mod topology;

pub use abi::is_public_api_export;
pub use layout::{XiphLayout, detect_layout, detect_layout_with_file_names};
pub use naming::{
    XiphMember, XiphNameStyle, XiphNamingProfile, XiphRuntimeFileName, XiphRuntimeFileNameError,
    classify_canonical_file_name, classify_file_name, file_name, parse_runtime_file_name,
};
pub use release::{XiphReleaseAxes, XiphReleaseAxis, XiphReleaseVersions};
pub use topology::{
    XiphEdge, XiphTopology, XiphTopologyError, vendor_topology_discriminator,
    vendor_topology_preimage,
};

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

        let vendor = vec![
            ComponentFile::new(
                PathRef::new("C:/Game/vorbisfile_vs2010_x64_rwdi.dll").expect("path"),
            ),
            ComponentFile::new(PathRef::new("C:/Game/vorbis_vs2010_x64_rwdi.dll").expect("path")),
        ];
        assert_eq!(
            XiphReleaseAxes::from_component_files(&vendor)
                .expect("vendor axes")
                .iter()
                .collect::<Vec<_>>(),
            vec![XiphReleaseAxis::Ogg, XiphReleaseAxis::Vorbis]
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
    fn runtime_parser_keeps_canonical_parser_strict_and_preserves_opaque_suffixes() {
        assert_eq!(
            classify_canonical_file_name("VORBisFile.dll"),
            Some((XiphMember::VorbisFile, XiphNameStyle::Plain))
        );
        assert_eq!(
            classify_file_name("vorbisfile_vs2010_x64_rwdi.dll"),
            None,
            "the compatibility parser must never broaden to runtime names"
        );
        assert_eq!(
            classify_canonical_file_name("vorbisfile_vs2010_x64_rwdi.dll"),
            None,
            "catalog/package validation must remain canonical-only"
        );

        let dide = parse_runtime_file_name("VorbisFile_VS2010_x64_RWDI.DLL")
            .expect("DIDE name grammar")
            .expect("Xiph member");
        assert_eq!(dide.member(), XiphMember::VorbisFile);
        assert_eq!(dide.base_style(), XiphNameStyle::Plain);
        assert_eq!(dide.normalized_name(), "vorbisfile_vs2010_x64_rwdi.dll");
        assert_eq!(dide.vendor_suffix(), Some("_vs2010_x64_rwdi"));
        assert!(dide.is_vendor());

        for suffix in ["_vs2008_x64_rwdi", "_vs2012_x64_rwdi"] {
            let parsed = parse_runtime_file_name(&format!("vorbis{suffix}.dll"))
                .expect("opaque suffix grammar")
                .expect("Xiph member");
            assert_eq!(parsed.vendor_suffix(), Some(suffix));
        }
        assert!(
            parse_runtime_file_name("kernel32.dll")
                .expect("unrelated DLL")
                .is_none()
        );
    }

    #[test]
    fn runtime_parser_rejects_every_suffix_boundary() {
        let too_long_suffix = (0..8).map(|_| "a".repeat(32)).collect::<Vec<_>>().join("_");
        let invalid = vec![
            "vorbisfile_.dll".to_owned(),
            "vorbisfile__x.dll".to_owned(),
            "vorbisfile_-x.dll".to_owned(),
            "vorbisfile_x-.dll".to_owned(),
            "vorbisfile_x.y.dll".to_owned(),
            "vorbisfile_x y.dll".to_owned(),
            "vorbisfile_x!y.dll".to_owned(),
            "vorbisfile_é.dll".to_owned(),
            format!("vorbisfile_{}.dll", "1".repeat(33)),
            "vorbisfile_a_b_c_d_e_f_g_h_i.dll".to_owned(),
            format!("vorbisfile_{too_long_suffix}.dll"),
            "C:\\game\\vorbisfile.dll".to_owned(),
            "vorbisfile.dll.bak".to_owned(),
            "vorbisfile".to_owned(),
        ];
        for invalid in invalid {
            assert!(
                parse_runtime_file_name(&invalid).is_err(),
                "expected malformed runtime name: {invalid}"
            );
        }
        assert!(parse_runtime_file_name("libvorbis-4.dll").is_err());

        let max_suffix = [
            "a".repeat(31),
            "b".repeat(31),
            "c".repeat(31),
            "d".repeat(31),
        ]
        .join("_");
        let max_suffix = format!("_{max_suffix}");
        assert_eq!(max_suffix.len(), 128);
        assert_eq!(
            parse_runtime_file_name(&format!("vorbisfile{max_suffix}.dll"))
                .expect("128-byte suffix is valid")
                .expect("Xiph runtime")
                .vendor_suffix(),
            Some(max_suffix.as_str())
        );
        assert_eq!(
            parse_runtime_file_name(&format!("vorbisfile_{too_long_suffix}.dll"))
                .expect_err("overlong suffix must fail"),
            XiphRuntimeFileNameError::VendorSuffixTooLong
        );

        for unrelated in ["oggdec.dll", "vorbiscomment.dll"] {
            assert!(
                parse_runtime_file_name(unrelated)
                    .expect("unrelated Xiph-adjacent tool name must not be malformed")
                    .is_none(),
                "{unrelated} is not a canonical Xiph runtime member"
            );
        }
    }

    #[test]
    fn detects_dide_vendor_closure_and_supported_plain_hybrid() {
        let dide = vec![
            member(
                "vorbisfile_vs2010_x64_rwdi.dll",
                &[
                    "kernel32.dll",
                    "vorbis_vs2010_x64_rwdi.dll",
                    "ogg_vs2010_x64_rwdi.dll",
                ],
            ),
            member("vorbis_vs2010_x64_rwdi.dll", &["ogg_vs2010_x64_rwdi.dll"]),
            member("ogg_vs2010_x64_rwdi.dll", &[]),
        ];
        let layout = detect_layout(&dide).expect("DIDE vendor closure");
        assert_eq!(layout.naming_profile(), XiphNamingProfile::Plain);
        assert_eq!(
            layout.topology().vendor_discriminator(),
            "vendor-topology-v1-c792fb369a519e40bb3bda22747d014575a6d1139108e0d54dc2d4d7b7597734"
        );

        let hybrid = vec![
            member("vorbisfile_vs2010_x64_rwdi.dll", &["vorbis.dll", "ogg.dll"]),
            member("vorbis.dll", &["ogg.dll"]),
            member("ogg.dll", &[]),
        ];
        let hybrid_layout = detect_layout(&hybrid).expect("vendor/plain hybrid closure");
        assert_eq!(hybrid_layout.naming_profile(), XiphNamingProfile::Plain);

        let lib_vendor = vec![
            member("libvorbisfile_vendor-a.dll", &["libvorbis_vendor-a.dll"]),
            member("libvorbis_vendor-a.dll", &[]),
        ];
        assert!(detect_layout(&lib_vendor).is_some());

        let abi_vendor = vec![
            member(
                "libvorbisfile-3_vendor-a.dll",
                &["libvorbis-0_vendor-a.dll"],
            ),
            member("libvorbis-0_vendor-a.dll", &[]),
        ];
        assert!(detect_layout(&abi_vendor).is_some());

        let canonical_first_hybrid = vec![
            member("vorbis.dll", &["ogg.dll"]),
            member("vorbisfile_vs2010_x64_rwdi.dll", &["vorbis.dll", "ogg.dll"]),
            member("ogg.dll", &[]),
        ];
        assert!(detect_layout(&canonical_first_hybrid).is_some());
    }

    #[test]
    fn rejects_incoherent_vendor_closures_and_malformed_xiph_imports() {
        let different_suffixes = vec![
            member(
                "vorbisfile_vs2008_x64_rwdi.dll",
                &["vorbis_vs2012_x64_rwdi.dll"],
            ),
            member("vorbis_vs2012_x64_rwdi.dll", &[]),
        ];
        assert!(detect_layout(&different_suffixes).is_none());

        let mixed_vendor_styles = vec![
            member("libvorbisfile_vendor-a.dll", &["vorbis_vendor-a.dll"]),
            member("vorbis_vendor-a.dll", &[]),
        ];
        assert!(detect_layout(&mixed_vendor_styles).is_none());

        let non_plain_hybrid = vec![
            member("libvorbisfile_vendor-a.dll", &["libvorbis.dll"]),
            member("libvorbis.dll", &[]),
        ];
        assert!(detect_layout(&non_plain_hybrid).is_none());

        let malformed_import = vec![member(
            "vorbisfile_vs2010_x64_rwdi.dll",
            &["vorbisfile__bad.dll"],
        )];
        assert!(detect_layout(&malformed_import).is_none());
    }

    #[test]
    fn topology_validates_edges_connectivity_and_golden_discriminators() {
        let dide = XiphTopology::new(
            [XiphMember::VorbisFile, XiphMember::Vorbis, XiphMember::Ogg],
            [
                (XiphMember::VorbisFile, XiphMember::Vorbis),
                (XiphMember::VorbisFile, XiphMember::Ogg),
                (XiphMember::Vorbis, XiphMember::Ogg),
            ],
        )
        .expect("DIDE topology");
        let mut dide_preimage = b"renderpilot:xiph-vendor-topology:v1".to_vec();
        dide_preimage.extend_from_slice(&[
            0, 3, b'M', 1, b'M', 3, b'M', 4, 3, 0, b'E', 1, 3, b'E', 1, 4, b'E', 3, 4,
        ]);
        assert_eq!(dide.vendor_discriminator_preimage(), dide_preimage);
        assert_eq!(
            vendor_topology_discriminator(&dide),
            "vendor-topology-v1-c792fb369a519e40bb3bda22747d014575a6d1139108e0d54dc2d4d7b7597734"
        );
        assert_eq!(vendor_topology_preimage(&dide), dide_preimage);

        let ogg = XiphTopology::new([XiphMember::Ogg], []).expect("Ogg singleton");
        let mut ogg_preimage = b"renderpilot:xiph-vendor-topology:v1".to_vec();
        ogg_preimage.extend_from_slice(&[0, 1, b'M', 4, 0, 0]);
        assert_eq!(ogg.vendor_discriminator_preimage(), ogg_preimage);
        assert_eq!(
            ogg.vendor_discriminator(),
            "vendor-topology-v1-fa1fd426441a4b71db205ed9a47905359fedef730c35981925d366f007efb859"
        );

        assert!(XiphTopology::new([XiphMember::VorbisFile, XiphMember::Ogg], [],).is_err());
        assert!(
            XiphTopology::new(
                [XiphMember::Vorbis],
                [(XiphMember::Vorbis, XiphMember::Vorbis)],
            )
            .is_err()
        );
        assert!(
            XiphTopology::new(
                [XiphMember::VorbisFile, XiphMember::Ogg],
                [(XiphMember::Ogg, XiphMember::VorbisFile)],
            )
            .is_err()
        );
        assert_eq!(
            XiphTopology::new([], []).expect_err("empty topology must fail"),
            XiphTopologyError::Empty
        );
        assert_eq!(
            XiphTopology::new(
                [XiphMember::Vorbis, XiphMember::Ogg],
                [
                    (XiphMember::Vorbis, XiphMember::Ogg),
                    (XiphMember::Vorbis, XiphMember::Ogg),
                ],
            )
            .expect_err("duplicate edge must fail"),
            XiphTopologyError::DuplicateEdge {
                source: XiphMember::Vorbis,
                target: XiphMember::Ogg,
            }
        );
        assert_eq!(
            XiphTopology::new([XiphMember::Ogg], [(XiphMember::Vorbis, XiphMember::Ogg)],)
                .expect_err("missing endpoint must fail"),
            XiphTopologyError::EndpointMissing {
                source: XiphMember::Vorbis,
                target: XiphMember::Ogg,
            }
        );
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
