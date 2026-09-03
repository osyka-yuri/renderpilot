//! Xiph member topology derived from strict PE import profiles.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::ComponentFile;

use super::naming::{XiphMember, XiphNameStyle, XiphNamingProfile, parse_runtime_file_name};
use super::release::XiphReleaseAxes;
use super::topology::{XiphTopology, is_allowed_edge};

/// A validated set of Xiph members and its exact internal import graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XiphLayout {
    names: BTreeMap<XiphMember, String>,
    dependencies: BTreeMap<XiphMember, BTreeSet<XiphMember>>,
    topology: XiphTopology,
    naming_profile: XiphNamingProfile,
    release_axes: XiphReleaseAxes,
}

impl XiphLayout {
    /// Returns the exact lowercase basename used by a semantic member.
    #[must_use]
    pub fn file_name(&self, member: XiphMember) -> Option<&str> {
        self.names.get(&member).map(String::as_str)
    }

    /// Returns semantic members in stable order.
    pub fn members(&self) -> impl Iterator<Item = XiphMember> + '_ {
        self.names.keys().copied()
    }

    /// Returns the exact semantic dependencies of a member.
    #[must_use]
    pub fn dependencies(&self, member: XiphMember) -> Option<&BTreeSet<XiphMember>> {
        self.dependencies.get(&member)
    }

    /// Returns the validated semantic topology represented by this layout.
    #[must_use]
    pub const fn topology(&self) -> &XiphTopology {
        &self.topology
    }

    /// Returns the normalized DLL naming profile of this deployment.
    #[must_use]
    pub const fn naming_profile(&self) -> XiphNamingProfile {
        self.naming_profile
    }

    /// Returns upstream coordinates required by this physical deployment.
    #[must_use]
    pub const fn release_axes(&self) -> &XiphReleaseAxes {
        &self.release_axes
    }
}

/// Detects a supported topology from exact members and strict PE imports.
#[must_use]
pub fn detect_layout(files: &[ComponentFile]) -> Option<XiphLayout> {
    detect_layout_with_file_names(
        files
            .iter()
            .map(|file| (file.path().file_name().unwrap_or_default(), file)),
    )
}

/// Detects a layout using explicit runtime basenames.
#[must_use]
pub fn detect_layout_with_file_names<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a ComponentFile)>,
) -> Option<XiphLayout> {
    // Parse every member before deciding whether this is a hybrid deployment.
    // A canonical member may precede a vendor member in the input, so deciding
    // the hybrid rule while iterating would make validity order-dependent.
    let parsed_files = files
        .into_iter()
        .map(|(runtime_name, file)| {
            parse_runtime_file_name(runtime_name)
                .ok()
                .flatten()
                .map(|parsed| (file, parsed))
        })
        .collect::<Option<Vec<_>>>()?;
    if parsed_files.is_empty() || parsed_files.len() > XiphMember::ALL.len() {
        return None;
    }

    let mut vendor_axis = None;
    let mut has_nonplain_canonical_member = false;
    for (_, parsed) in &parsed_files {
        if parsed.is_vendor() {
            let axis = (parsed.vendor_suffix(), parsed.base_style());
            if let Some(expected) = vendor_axis {
                if axis != expected {
                    return None;
                }
            } else {
                vendor_axis = Some(axis);
            }
        } else if parsed.base_style() != XiphNameStyle::Plain {
            has_nonplain_canonical_member = true;
        }
    }
    if vendor_axis.is_some() && has_nonplain_canonical_member {
        // Canonical names participate only in a real vendor/canonical hybrid;
        // those canonical participants must be the plain artifact names. An
        // all-vendor closure remains valid for every reviewed base style.
        return None;
    }

    let mut names = BTreeMap::new();
    let mut name_styles = Vec::<XiphNameStyle>::new();
    let mut members_by_name = HashMap::new();
    let mut files_by_member = HashMap::new();
    for (file, parsed) in parsed_files {
        let name = parsed.normalized_name().to_owned();
        let member = parsed.member();
        let style = parsed.base_style();
        if names.insert(member, name.clone()).is_some()
            || members_by_name.insert(name, member).is_some()
        {
            return None;
        }
        files_by_member.insert(member, file);
        name_styles.push(style);
    }

    let mut dependencies = BTreeMap::new();
    for (member, file) in files_by_member {
        let imports = file.pe_compatibility()?.imports()?;
        let mut imported_members = BTreeSet::new();
        for imported in imports.regular.names().iter().chain(imports.delay.names()) {
            let imported_runtime = parse_runtime_file_name(imported).ok()?;
            let Some(imported_runtime) = imported_runtime else {
                continue;
            };
            let imported_member = imported_runtime.member();
            let imported_name = imported_runtime.normalized_name();
            if members_by_name.get(imported_name) != Some(&imported_member)
                || !is_allowed_edge(member, imported_member)
                || !imported_members.insert(imported_member)
            {
                return None;
            }
        }
        dependencies.insert(member, imported_members);
    }

    let topology = XiphTopology::new(
        names.keys().copied(),
        dependencies
            .iter()
            .flat_map(|(source, targets)| targets.iter().map(move |target| (*source, *target))),
    )
    .ok()?;

    Some(XiphLayout {
        naming_profile: XiphNamingProfile::from_styles(name_styles),
        release_axes: XiphReleaseAxes::from_members(names.keys().copied()),
        names,
        dependencies,
        topology,
    })
}
