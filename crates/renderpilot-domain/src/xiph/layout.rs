//! Xiph member topology derived from strict PE import profiles.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::ComponentFile;

use super::naming::{XiphMember, XiphNameStyle, XiphNamingProfile, classify_file_name};
use super::release::XiphReleaseAxes;

/// A validated set of Xiph members and its exact internal import graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XiphLayout {
    names: BTreeMap<XiphMember, String>,
    dependencies: BTreeMap<XiphMember, BTreeSet<XiphMember>>,
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
    let files = files.into_iter().collect::<Vec<_>>();
    if files.is_empty() || files.len() > XiphMember::ALL.len() {
        return None;
    }

    let mut names = BTreeMap::new();
    let mut name_styles = Vec::<XiphNameStyle>::new();
    let mut members_by_name = HashMap::new();
    let mut files_by_member = HashMap::new();
    for (runtime_name, file) in &files {
        let name = runtime_name.to_ascii_lowercase();
        let (member, style) = classify_file_name(&name)?;
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
            let Some((imported_member, _)) = classify_file_name(imported) else {
                continue;
            };
            if members_by_name.get(imported.as_str()) != Some(&imported_member)
                || !edge_is_allowed(member, imported_member)
                || !imported_members.insert(imported_member)
            {
                return None;
            }
        }
        dependencies.insert(member, imported_members);
    }

    if files.len() > 1 && !is_connected(&names, &dependencies) {
        return None;
    }

    Some(XiphLayout {
        naming_profile: XiphNamingProfile::from_styles(name_styles),
        release_axes: XiphReleaseAxes::from_members(names.keys().copied()),
        names,
        dependencies,
    })
}

const fn edge_is_allowed(source: XiphMember, target: XiphMember) -> bool {
    matches!(
        (source, target),
        (XiphMember::Vorbis, XiphMember::Ogg)
            | (XiphMember::VorbisFile, XiphMember::Vorbis | XiphMember::Ogg)
            | (XiphMember::VorbisEnc, XiphMember::Vorbis | XiphMember::Ogg)
    )
}

fn is_connected(
    names: &BTreeMap<XiphMember, String>,
    dependencies: &BTreeMap<XiphMember, BTreeSet<XiphMember>>,
) -> bool {
    let Some(start) = names.keys().next().copied() else {
        return false;
    };
    let mut visited = HashSet::from([start]);
    let mut pending = vec![start];
    while let Some(current) = pending.pop() {
        for candidate in names.keys().copied() {
            let adjacent = dependencies
                .get(&current)
                .is_some_and(|values| values.contains(&candidate))
                || dependencies
                    .get(&candidate)
                    .is_some_and(|values| values.contains(&current));
            if adjacent && visited.insert(candidate) {
                pending.push(candidate);
            }
        }
    }
    visited.len() == names.len()
}
