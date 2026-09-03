//! Partitions colocated Xiph DLLs by their observed import graph.
//!
//! Canonical Xiph layouts retain their established naming-profile identity.
//! Vendor-suffixed runtime layouts deliberately use only a validated semantic
//! topology: basenames are opaque loader aliases, never an identity fallback.

use std::collections::{HashMap, HashSet};

use renderpilot_domain::{LibraryTechnology, xiph};

use super::DetectedLibraryFile;

pub(super) fn discriminators(libraries: &[DetectedLibraryFile]) -> HashMap<usize, String> {
    let mut by_directory = HashMap::<String, Vec<usize>>::new();
    for (index, library) in libraries.iter().enumerate() {
        if library.technology() == LibraryTechnology::XiphVorbis {
            by_directory
                .entry(parent_directory(library))
                .or_default()
                .push(index);
        }
    }

    let mut result = HashMap::new();
    for indices in by_directory.into_values() {
        assign_directory_discriminators(libraries, &indices, &mut result);
    }
    result
}

fn assign_directory_discriminators(
    libraries: &[DetectedLibraryFile],
    indices: &[usize],
    result: &mut HashMap<usize, String>,
) {
    let local_names = local_names(libraries, indices);
    let mut disjoint = DisjointSet::new(indices.len());
    for (local_index, global_index) in indices.iter().copied().enumerate() {
        let Some(imports) = libraries[global_index]
            .pe_compatibility()
            .and_then(|profile| profile.imports())
        else {
            continue;
        };
        for imported in imports.regular.names().iter().chain(imports.delay.names()) {
            let Some(matches) = local_names.get(imported) else {
                continue;
            };
            if matches.len() == 1 {
                disjoint.union(local_index, matches[0]);
            }
        }
    }

    let components = disjoint_components(indices, &mut disjoint);
    let canonical_components = components
        .iter()
        .filter(|component| !contains_vendor_runtime_name(libraries, component))
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    assign_canonical_discriminators(libraries, &canonical_components, result);

    let vendor_components = components
        .iter()
        .filter_map(|component| {
            vendor_discriminator(libraries, component, &local_names)
                .map(|discriminator| (component, discriminator))
        })
        .collect::<Vec<_>>();
    let mut counts = HashMap::<String, usize>::new();
    for (_, discriminator) in &vendor_components {
        *counts.entry(discriminator.clone()).or_default() += 1;
    }
    for (component, discriminator) in vendor_components {
        // Two different local closures with the same topology cannot receive
        // distinct stable vendor IDs. Suppress both instead of reintroducing a
        // basename-derived fallback.
        if counts.get(&discriminator).copied() != Some(1) {
            continue;
        }
        for global_index in component {
            result.insert(*global_index, discriminator.clone());
        }
    }
}

fn local_names(
    libraries: &[DetectedLibraryFile],
    indices: &[usize],
) -> HashMap<String, Vec<usize>> {
    let mut local_names = HashMap::<String, Vec<usize>>::new();
    for (local_index, global_index) in indices.iter().copied().enumerate() {
        local_names
            .entry(libraries[global_index].file_name().to_ascii_lowercase())
            .or_default()
            .push(local_index);
    }
    local_names
}

fn disjoint_components(indices: &[usize], disjoint: &mut DisjointSet) -> Vec<Vec<usize>> {
    let mut components = Vec::<Vec<usize>>::new();
    let mut component_by_root = HashMap::new();
    for (local_index, global_index) in indices.iter().copied().enumerate() {
        let root = disjoint.root(local_index);
        let component_index = *component_by_root.entry(root).or_insert_with(|| {
            components.push(Vec::new());
            components.len() - 1
        });
        components[component_index].push(global_index);
    }
    components
}

fn assign_canonical_discriminators(
    libraries: &[DetectedLibraryFile],
    components: &[&[usize]],
    result: &mut HashMap<usize, String>,
) {
    let bases = components
        .iter()
        .map(|component| discriminator_base(libraries, component))
        .collect::<Vec<_>>();
    let mut base_counts = HashMap::<String, usize>::new();
    for base in &bases {
        *base_counts.entry(base.clone()).or_default() += 1;
    }
    for (component, base) in components.iter().zip(bases) {
        let discriminator = if base_counts.get(&base).copied().unwrap_or_default() == 1 {
            base
        } else {
            format!(
                "{base}-{:016x}",
                stable_canonical_name_hash(libraries, component)
            )
        };
        for global_index in *component {
            result.insert(*global_index, discriminator.clone());
        }
    }
}

fn vendor_discriminator(
    libraries: &[DetectedLibraryFile],
    component: &[usize],
    local_names: &HashMap<String, Vec<usize>>,
) -> Option<String> {
    if !contains_vendor_runtime_name(libraries, component)
        || !has_complete_compatible_pe(libraries, component)
        || !xiph_imports_resolve_uniquely(libraries, component, local_names)
    {
        return None;
    }

    let files = component
        .iter()
        .map(|index| libraries[*index].component_file())
        .collect::<Vec<_>>();
    let layout = xiph::detect_layout(&files)?;
    Some(layout.topology().vendor_discriminator())
}

fn contains_vendor_runtime_name(libraries: &[DetectedLibraryFile], component: &[usize]) -> bool {
    component.iter().any(|index| {
        xiph::parse_runtime_file_name(libraries[*index].file_name())
            .ok()
            .flatten()
            .is_some_and(|runtime_name| runtime_name.is_vendor())
    })
}

fn has_complete_compatible_pe(libraries: &[DetectedLibraryFile], component: &[usize]) -> bool {
    let mut architectures = HashSet::new();
    for index in component {
        let Some(profile) = libraries[*index].pe_compatibility() else {
            return false;
        };
        // `PeCompatibilityProfile` only exists after architecture, complete
        // named exports, and strict regular/delay import parsing all succeed.
        if profile.imports().is_none() {
            return false;
        }
        architectures.insert(profile.architecture());
    }
    architectures.len() == 1
}

fn xiph_imports_resolve_uniquely(
    libraries: &[DetectedLibraryFile],
    component: &[usize],
    local_names: &HashMap<String, Vec<usize>>,
) -> bool {
    component.iter().all(|index| {
        let Some(imports) = libraries[*index]
            .pe_compatibility()
            .and_then(|profile| profile.imports())
        else {
            return false;
        };
        imports
            .regular
            .names()
            .iter()
            .chain(imports.delay.names())
            .all(|imported| match xiph::parse_runtime_file_name(imported) {
                Ok(Some(_)) => local_names
                    .get(imported)
                    .is_some_and(|matches| matches.len() == 1),
                Ok(None) => true,
                Err(_) => false,
            })
    })
}

fn discriminator_base(libraries: &[DetectedLibraryFile], component: &[usize]) -> String {
    xiph::XiphNamingProfile::from_styles(
        component
            .iter()
            .filter_map(|index| xiph::classify_file_name(libraries[*index].file_name()))
            .map(|(_, style)| style),
    )
    .as_slug()
    .to_owned()
}

fn stable_canonical_name_hash(libraries: &[DetectedLibraryFile], component: &[usize]) -> u64 {
    let mut names = component
        .iter()
        .map(|index| libraries[*index].file_name().to_ascii_lowercase())
        .collect::<Vec<_>>();
    names.sort_unstable();
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in names.join("\0").bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn parent_directory(library: &DetectedLibraryFile) -> String {
    library.file_path().parent().unwrap_or_default().to_owned()
}

struct DisjointSet {
    parents: Vec<usize>,
}

impl DisjointSet {
    fn new(len: usize) -> Self {
        Self {
            parents: (0..len).collect(),
        }
    }

    fn root(&mut self, value: usize) -> usize {
        if self.parents[value] != value {
            self.parents[value] = self.root(self.parents[value]);
        }
        self.parents[value]
    }

    fn union(&mut self, left: usize, right: usize) {
        let left = self.root(left);
        let right = self.root(right);
        if left != right {
            self.parents[right] = left;
        }
    }
}

#[cfg(test)]
mod tests {
    use renderpilot_domain::{
        Architecture, ComponentKind, GameId, GameIdentity, GameInstallation, GameRuntime, Launcher,
        PathRef, PeCompatibilityProfile, PeExportSet, PeImportProfile, PeImportSet, Platform,
        Sha256Hash, Swappability,
    };

    use crate::{DetectionConfidence, VersionDetectionStatus};

    use super::*;

    const DIDE_DISCRIMINATOR: &str =
        "vendor-topology-v1-c792fb369a519e40bb3bda22747d014575a6d1139108e0d54dc2d4d7b7597734";

    #[test]
    fn validated_dide_vendor_closure_uses_semantic_topology_discriminator() {
        let libraries = dide("_vs2010_x64_rwdi");

        let discriminators = discriminators(&libraries);

        assert_eq!(discriminators.len(), 3);
        assert!(
            discriminators
                .values()
                .all(|value| value == DIDE_DISCRIMINATOR)
        );

        let components =
            super::super::group_into_components(&game(), &libraries).expect("vendor grouping");
        assert_eq!(components.len(), 1);
        assert!(components[0].id().as_str().contains(DIDE_DISCRIMINATOR));
        assert_eq!(components[0].swappability(), Swappability::BundleOnly);
    }

    #[test]
    fn vendor_versions_are_opaque_to_the_topology_identity() {
        for suffix in ["_vs2008_x64_rwdi", "_vs2012_x64_rwdi"] {
            let discriminators = discriminators(&dide(suffix));
            assert!(
                discriminators
                    .values()
                    .all(|value| value == DIDE_DISCRIMINATOR)
            );
        }
    }

    #[test]
    fn vendor_closure_without_complete_pe_facts_is_not_authenticated() {
        let mut libraries = dide("_vs2010_x64_rwdi");
        libraries[1].pe_compatibility = None;

        assert!(discriminators(&libraries).is_empty());
    }

    #[test]
    fn invalid_vendor_import_graph_is_not_authenticated() {
        let mut libraries = dide("_vs2010_x64_rwdi");
        libraries[0].pe_compatibility = Some(profile(
            Architecture::X64,
            &[
                "vorbis_vs2010_x64_rwdi.dll".to_owned(),
                "ogg_vs2010_x64_rwdi.dll".to_owned(),
            ],
        ));
        libraries[1].pe_compatibility = Some(profile(
            Architecture::X64,
            &["vorbisfile_vs2010_x64_rwdi.dll".to_owned()],
        ));

        assert!(discriminators(&libraries).is_empty());
    }

    #[test]
    fn same_directory_duplicate_vendor_topologies_are_suppressed_without_name_hashes() {
        let mut libraries = dide("_vs2008_x64_rwdi");
        libraries.extend(dide("_vs2012_x64_rwdi"));

        assert!(
            discriminators(&libraries).is_empty(),
            "two independently valid closures must not receive duplicate topology IDs"
        );

        let components = super::super::group_into_components(&game(), &libraries)
            .expect("ambiguous vendor grouping");
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].swappability(), Swappability::ReadOnly);
        assert!(!components[0].id().as_str().contains("vendor-topology-v1-"));
    }

    fn dide(suffix: &str) -> Vec<DetectedLibraryFile> {
        vec![
            library(
                &format!("vorbisfile{suffix}.dll"),
                Architecture::X64,
                &[format!("vorbis{suffix}.dll"), format!("ogg{suffix}.dll")],
            ),
            library(
                &format!("vorbis{suffix}.dll"),
                Architecture::X64,
                &[format!("ogg{suffix}.dll")],
            ),
            library(&format!("ogg{suffix}.dll"), Architecture::X64, &[]),
        ]
    }

    fn library(name: &str, architecture: Architecture, imports: &[String]) -> DetectedLibraryFile {
        DetectedLibraryFile {
            file_name: name.to_owned(),
            file_path: PathRef::new(format!("C:/game/{name}")).expect("path"),
            technology: LibraryTechnology::XiphVorbis,
            kind: ComponentKind::NativeLibrary,
            detection_confidence: DetectionConfidence::Medium,
            swappability: Swappability::ReadOnly,
            version: None,
            status: VersionDetectionStatus::UnknownVersion,
            sha256: Sha256Hash::new("0".repeat(64)).expect("sha"),
            observation: None,
            runtime_target: None,
            pe_compatibility: Some(profile(architecture, imports)),
        }
    }

    fn profile(architecture: Architecture, imports: &[String]) -> PeCompatibilityProfile {
        PeCompatibilityProfile::new(
            architecture,
            PeExportSet::from_observed_names(vec!["xiph_export".to_owned()]).expect("export set"),
        )
        .with_imports(PeImportProfile {
            regular: PeImportSet::from_observed_names(imports.to_vec()).expect("imports"),
            delay: PeImportSet::from_canonical_names(Vec::new()).expect("empty delay imports"),
        })
    }

    fn game() -> GameInstallation {
        let install_path = PathRef::new("C:/game").expect("install path");
        let identity = GameIdentity::new(
            GameId::new("manual:xiph-grouping-test").expect("game id"),
            "Xiph grouping test",
            Launcher::Manual,
        )
        .expect("game identity");
        GameInstallation::new(
            identity,
            Platform::Windows,
            GameRuntime::NativeWindows,
            install_path,
        )
    }
}
