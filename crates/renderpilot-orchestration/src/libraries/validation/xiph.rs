//! Canonical Xiph package identity and member contracts.

use std::collections::HashMap;

use renderpilot_application::is_allowed_xiph_system_import;
use renderpilot_domain::{Architecture, PackageVersion, ReleaseChannel, xiph};

use crate::ServiceError;

use super::super::library_error;
use super::super::types::{
    LibraryArtifactRecord, LibraryPackage, LibraryProvenance, SignatureInfo,
};

pub(super) fn validate_identity(package: &LibraryPackage) -> Result<(), ServiceError> {
    let components = &package.release.components;
    let (Some(ogg_version), Some(vorbis_version)) =
        (components.get("ogg"), components.get("vorbis"))
    else {
        return Err(noncanonical_package(package));
    };
    let Some(LibraryProvenance::SourceBuild {
        sources,
        build_revision,
        patches,
        toolchain,
        ..
    }) = &package.provenance
    else {
        return Err(noncanonical_package(package));
    };
    let (Some(ogg_source), Some(vorbis_source)) = (sources.get("ogg"), sources.get("vorbis"))
    else {
        return Err(noncanonical_package(package));
    };

    let mut variant = package.variant.split('.');
    let topology = variant
        .next()
        .ok_or_else(|| noncanonical_package(package))?;
    let profile = match variant.next() {
        Some("plain") => xiph::XiphNameStyle::Plain,
        Some("lib") => xiph::XiphNameStyle::Lib,
        Some("abi") => xiph::XiphNameStyle::AbiMajor,
        _ => return Err(noncanonical_package(package)),
    };
    if variant.next().is_some() {
        return Err(noncanonical_package(package));
    }
    let expected_components: &[&str] = match topology {
        "shared" => &["vorbis", "vorbisfile", "vorbisenc", "ogg"],
        "embedded_ogg" => &["vorbis", "vorbisfile", "vorbisenc"],
        _ => return Err(noncanonical_package(package)),
    };
    let expected_architecture = match package.target.architecture {
        Architecture::X86 => "x86",
        Architecture::X64 => "x64",
    };
    let expected_package_id = format!(
        "xiph_vorbis.vorbis-{}.ogg-{}.r{build_revision}.{expected_architecture}.{}",
        vorbis_source.version, ogg_source.version, package.variant
    );
    let valid_members = package.members.len() == expected_components.len()
        && package
            .members
            .iter()
            .zip(expected_components)
            .enumerate()
            .all(|(index, (member, expected_component))| {
                let Some((semantic_member, actual_style)) =
                    xiph::classify_file_name(&member.install_as)
                else {
                    return false;
                };
                member.component == *expected_component
                    && semantic_member.as_slug() == *expected_component
                    && actual_style == profile
                    && member.install_as == xiph::file_name(semantic_member, profile)
                    && member.role == if index == 0 { "primary" } else { "support" }
            });
    if components.len() != 2
        || &package.release.version != vorbis_version
        || package.release.channel != ReleaseChannel::Stable
        || package.package_id != expected_package_id
        || !toolchain
            .runner_image
            .strip_prefix("windows-2025-vs2026@")
            .is_some_and(|version| !version.is_empty())
        || sources.len() != 2
        || patches
            .values()
            .any(|patch| !matches!(patch.source.as_str(), "ogg" | "vorbis"))
        || !valid_members
    {
        return Err(noncanonical_package(package));
    }

    for (component, version) in [("ogg", ogg_version), ("vorbis", vorbis_version)] {
        let source = if component == "ogg" {
            ogg_source
        } else {
            vorbis_source
        };
        let source_matches_release = PackageVersion::parse(&source.version)
            .is_ok_and(|source_version| source_version == *version);
        let expected_repository = format!("xiph/{component}");
        let expected_tag = if component == "vorbis" && source.version == "1.0" {
            "v1.0.0".to_owned()
        } else {
            format!("v{}", source.version)
        };
        let archive_prefix = format!(
            "https://downloads.xiph.org/releases/{component}/lib{component}-{}.tar.",
            source.version
        );
        let valid_git_identity = match (
            source.tag.as_deref(),
            source.tag_object_sha.as_deref(),
            source.commit_sha.as_deref(),
        ) {
            (Some(tag), Some(tag_object_sha), Some(commit_sha)) => {
                tag == expected_tag
                    && is_lower_hex_40(tag_object_sha)
                    && is_lower_hex_40(commit_sha)
            }
            (None, None, None) => {
                component == "ogg" && matches!(source.version.as_str(), "1.0" | "1.1")
            }
            _ => false,
        };
        let valid_archive = ["xz", "bz2", "gz"]
            .iter()
            .any(|extension| source.archive_url == format!("{archive_prefix}{extension}"));
        if source.repository != expected_repository
            || !source_matches_release
            || !valid_git_identity
            || !valid_archive
        {
            return Err(noncanonical_package(package));
        }
    }

    Ok(())
}

pub(super) fn validate_artifact_contract(
    package: &LibraryPackage,
    member_index: usize,
    artifact: &LibraryArtifactRecord,
) -> Result<(), ServiceError> {
    let member = package
        .members
        .get(member_index)
        .ok_or_else(|| noncanonical_package(package))?;
    let component = member.component.as_str();
    let version_component = if component == "ogg" { "ogg" } else { "vorbis" };
    let expected_version = package
        .release
        .components
        .get(version_component)
        .ok_or_else(|| noncanonical_package(package))?;
    let actual_version = artifact
        .file_version
        .as_ref()
        .and_then(|value| PackageVersion::parse(value).ok());
    if artifact.library_id != format!("xiph_{component}")
        || !artifact.file_name.eq_ignore_ascii_case(&member.install_as)
        || artifact.pe_named_exports.is_none()
        || !matches!(&artifact.signature, SignatureInfo::Unsigned)
        || actual_version
            .as_ref()
            .is_none_or(|version| version.numeric_core() != expected_version.numeric_core())
    {
        return Err(incomplete_member_contract(package));
    }

    let imports = artifact
        .pe_imports
        .as_ref()
        .ok_or_else(|| incomplete_member_contract(package))?;
    let names = package
        .members
        .iter()
        .map(|member| {
            (
                member.component.as_str(),
                member.install_as.to_ascii_lowercase(),
            )
        })
        .collect::<HashMap<_, _>>();
    let member_name = |component: &str| {
        names
            .get(component)
            .map(String::as_str)
            .ok_or_else(|| noncanonical_package(package))
    };
    let topology = package.variant.split('.').next().unwrap_or_default();
    let mut expected_imports = match component {
        "vorbis" if topology == "shared" => vec![member_name("ogg")?],
        "vorbis" => Vec::new(),
        "vorbisfile" if topology == "shared" => {
            vec![member_name("ogg")?, member_name("vorbis")?]
        }
        "vorbisfile" | "vorbisenc" => vec![member_name("vorbis")?],
        "ogg" => Vec::new(),
        _ => return Err(noncanonical_package(package)),
    };
    expected_imports.sort_unstable();
    let mut actual_imports = Vec::new();
    for name in imports.regular.names().iter().chain(imports.delay.names()) {
        if xiph::classify_file_name(name).is_some() {
            actual_imports.push(name.as_str());
        } else if !is_allowed_xiph_system_import(name) {
            return Err(library_error(format!(
                "package `{}` has unexpected Xiph dependency `{name}`",
                package.package_id
            )));
        }
    }
    actual_imports.sort_unstable();
    if actual_imports != expected_imports {
        return Err(library_error(format!(
            "package `{}` has an invalid Xiph import graph",
            package.package_id
        )));
    }
    Ok(())
}

fn incomplete_member_contract(package: &LibraryPackage) -> ServiceError {
    library_error(format!(
        "package `{}` has an incomplete Xiph member contract",
        package.package_id
    ))
}

fn noncanonical_package(package: &LibraryPackage) -> ServiceError {
    library_error(format!(
        "package `{}` is not a canonical Xiph Vorbis/Ogg source package",
        package.package_id
    ))
}

fn is_lower_hex_40(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
