//! Proof-constrained Xiph alias selection and member projection.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use renderpilot_domain::{
    ComponentFile, LibraryArtifact, LibraryComponent,
    xiph::{self, XiphMember},
};

use crate::{AppError, AppResult};

use super::{
    model::{ExternalAliasRequirements, ResolvedXiphTransition},
    paths::{TransitionWrite, insert_transition_write, join_target},
};

pub(super) fn xiph_transition_writes(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    external_aliases: &ExternalAliasRequirements,
) -> AppResult<(
    BTreeMap<String, TransitionWrite>,
    Option<ResolvedXiphTransition>,
)> {
    let mut installed = BTreeMap::new();
    let mut vendor_suffix = None;
    for file in component.files() {
        let name = runtime_name(file)?;
        let parsed = xiph::parse_runtime_file_name(name)
            .map_err(|error| AppError::invalid_input(error.to_string()))?
            .ok_or_else(|| AppError::invalid_input("Xiph target has an unsupported DLL alias"))?;
        if let Some(suffix) = parsed.vendor_suffix() {
            vendor_suffix.get_or_insert_with(|| suffix.to_owned());
        }
        if installed.insert(parsed.member(), (parsed, file)).is_some() {
            return Err(AppError::invalid_input(
                "Xiph component has duplicate semantic members",
            ));
        }
    }
    if installed.is_empty() {
        return Err(AppError::invalid_input(
            "Xiph component has no semantic members",
        ));
    }

    let mut candidates = BTreeMap::new();
    for file in artifact.files() {
        let name = runtime_name(file)?;
        let (member, _) = xiph::classify_canonical_file_name(name)
            .ok_or_else(|| AppError::invalid_input("Xiph artifact has an unsupported DLL alias"))?;
        if candidates
            .insert(member, (name.to_ascii_lowercase(), file))
            .is_some()
        {
            return Err(AppError::invalid_input(
                "Xiph artifact has duplicate semantic members",
            ));
        }
    }

    let no_aliases = BTreeSet::new();
    let proven_aliases = match external_aliases {
        ExternalAliasRequirements::NotRequired => &no_aliases,
        ExternalAliasRequirements::Proven(aliases) => aliases,
        ExternalAliasRequirements::Unproven => {
            return Err(AppError::invalid_input(
                "vendor-suffixed Xiph deployment requires a complete external alias proof",
            ));
        }
    };

    let mut writes = BTreeMap::new();
    let mut targets_by_member = BTreeMap::new();
    let mut candidate_sources = BTreeMap::new();
    for (member, (installed_name, installed_file)) in &installed {
        let (candidate_name, source) = candidates.get(member).ok_or_else(|| {
            AppError::invalid_input(format!(
                "Xiph package does not cover installed member: {}",
                member.as_slug()
            ))
        })?;
        if !installed_name.accepts_candidate_file_name(candidate_name) {
            return Err(AppError::invalid_input(
                "Xiph candidate name does not match the installed runtime alias",
            ));
        }
        let target_name = if installed_name.is_vendor()
            && proven_aliases.contains(installed_name.normalized_name())
        {
            installed_name.normalized_name()
        } else {
            candidate_name.as_str()
        };
        let directory = installed_file
            .path()
            .parent()
            .ok_or_else(|| AppError::invalid_input("Xiph target has no parent directory"))?;
        let target = join_target(directory, target_name)?;
        targets_by_member.insert(*member, target_name);
        candidate_sources.insert(*member, *source);
        insert_transition_write(
            &mut writes,
            TransitionWrite {
                target,
                source: (*source).clone(),
                member: Some(*member),
            },
        )?;
    }

    // Candidate imports use reviewed package names. A proof may preserve a
    // vendor alias only where doing so does not strand one of those imports.
    for (member, source) in candidate_sources {
        let profile = source.pe_compatibility().ok_or_else(|| {
            AppError::invalid_input("Xiph artifact member has no PE compatibility profile")
        })?;
        let imports = profile.imports().ok_or_else(|| {
            AppError::invalid_input("Xiph artifact member has no PE import profile")
        })?;
        for import in imports.regular.names().iter().chain(imports.delay.names()) {
            let Some(imported) = xiph::parse_runtime_file_name(import)
                .map_err(|error| AppError::invalid_input(error.to_string()))?
            else {
                continue;
            };
            let Some(target) = targets_by_member.get(&imported.member()) else {
                continue;
            };
            if *target != imported.normalized_name() {
                return Err(AppError::invalid_input(format!(
                    "required vendor alias for {} conflicts with canonical candidate dependency {}",
                    member.as_slug(),
                    imported.normalized_name()
                )));
            }
        }
    }

    let topology = xiph::detect_layout_with_file_names(
        installed
            .values()
            .map(|(parsed, file)| (parsed.normalized_name(), *file)),
    )
    .map(|layout| layout.topology().clone())
    .ok_or_else(|| AppError::invalid_input("Xiph component has an invalid runtime topology"))?;
    Ok((
        writes,
        Some(ResolvedXiphTransition {
            topology,
            vendor_suffix,
            external_aliases: (*proven_aliases).clone(),
        }),
    ))
}

pub(super) fn runtime_name(file: &ComponentFile) -> AppResult<&str> {
    file.install_as()
        .or_else(|| file.path().file_name())
        .ok_or_else(|| AppError::invalid_input("Xiph file has no runtime basename"))
}

pub(super) fn xiph_member_for_component_file(file: &ComponentFile) -> Option<XiphMember> {
    runtime_name(file)
        .ok()
        .and_then(|name| xiph::parse_runtime_file_name(name).ok().flatten())
        .map(|parsed| parsed.member())
}

pub(super) fn project_xiph_members<'a>(
    component: &LibraryComponent,
    artifact: &'a LibraryArtifact,
) -> AppResult<Vec<&'a ComponentFile>> {
    let mut installed_members = HashSet::new();
    for file in component.files() {
        let name = runtime_name(file)?;
        let runtime = xiph::parse_runtime_file_name(name)
            .map_err(|error| AppError::invalid_input(error.to_string()))?
            .ok_or_else(|| AppError::invalid_input("Xiph target has an unsupported DLL alias"))?;
        if runtime.is_vendor() {
            return Err(AppError::invalid_input(
                "vendor-suffixed Xiph aliases require resolve_transition with external alias proof",
            ));
        }
        let member = runtime.member();
        if !installed_members.insert(member) {
            return Err(AppError::invalid_input(
                "Xiph component has duplicate semantic members",
            ));
        }
    }

    let mut package_members = HashSet::new();
    let mut projected = Vec::with_capacity(installed_members.len());
    for file in artifact.files() {
        let name = file
            .install_as()
            .or_else(|| file.path().file_name())
            .ok_or_else(|| AppError::invalid_input("Xiph artifact member has no file name"))?;
        let member = xiph::classify_canonical_file_name(name)
            .map(|value| value.0)
            .ok_or_else(|| AppError::invalid_input("Xiph artifact has an unsupported DLL alias"))?;
        if !package_members.insert(member) {
            return Err(AppError::invalid_input(
                "Xiph artifact has duplicate semantic members",
            ));
        }
        if installed_members.contains(&member) {
            projected.push(file);
        }
    }

    let mut missing = installed_members
        .difference(&package_members)
        .map(|member| member.as_slug())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        missing.sort_unstable();
        return Err(AppError::invalid_input(format!(
            "Xiph package does not cover installed members: {}",
            missing.join(", ")
        )));
    }
    Ok(projected)
}
