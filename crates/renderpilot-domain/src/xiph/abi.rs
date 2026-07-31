//! Public ABI namespaces of the Xiph runtime members.

use super::XiphMember;

/// Returns whether a named PE export belongs to the supported public C API of
/// one Xiph runtime member.
///
/// Historical Windows builds exported implementation details alongside the
/// documented API. Those private symbols may legitimately disappear when a
/// coherent runtime bundle is upgraded and therefore are not part of the
/// replacement compatibility contract. A leading underscore is recognized as
/// x86 C decoration, but the original spelling is still compared verbatim by
/// the caller.
#[must_use]
pub fn is_public_api_export(member: XiphMember, export: &str) -> bool {
    let c_name = export.strip_prefix('_').unwrap_or(export);
    match member {
        XiphMember::VorbisFile => c_name.starts_with("ov_"),
        XiphMember::VorbisEnc => c_name.starts_with("vorbis_encode_"),
        XiphMember::Vorbis => c_name.starts_with("vorbis_"),
        XiphMember::Ogg => {
            c_name.starts_with("ogg_")
                || c_name.starts_with("oggpack_")
                || c_name.starts_with("oggpackB_")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_each_public_api_namespace_and_x86_decoration() {
        for (member, export) in [
            (XiphMember::VorbisFile, "ov_open"),
            (XiphMember::VorbisEnc, "vorbis_encode_init"),
            (XiphMember::Vorbis, "vorbis_info_init"),
            (XiphMember::Ogg, "ogg_sync_init"),
            (XiphMember::Ogg, "oggpack_read"),
            (XiphMember::Ogg, "oggpackB_write"),
        ] {
            assert!(is_public_api_export(member, export));
            assert!(is_public_api_export(member, &format!("_{export}")));
        }
    }

    #[test]
    fn excludes_private_diagnostics_and_cross_member_exports() {
        assert!(!is_public_api_export(
            XiphMember::Vorbis,
            "_analysis_output_always"
        ));
        assert!(!is_public_api_export(XiphMember::Vorbis, "_mapping_P"));
        assert!(!is_public_api_export(XiphMember::Vorbis, "ov_open"));
        assert!(!is_public_api_export(
            XiphMember::VorbisFile,
            "vorbis_info_init"
        ));
    }
}
