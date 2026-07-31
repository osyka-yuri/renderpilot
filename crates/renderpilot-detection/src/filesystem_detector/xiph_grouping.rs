//! Partitions colocated Xiph DLLs by their observed import graph.

use std::collections::HashMap;

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
    let mut local_by_name = HashMap::new();
    for (local_index, global_index) in indices.iter().copied().enumerate() {
        local_by_name.insert(
            libraries[global_index].file_name().to_ascii_lowercase(),
            local_index,
        );
    }

    let mut disjoint = DisjointSet::new(indices.len());
    for (local_index, global_index) in indices.iter().copied().enumerate() {
        let Some(imports) = libraries[global_index]
            .pe_compatibility()
            .and_then(|profile| profile.imports())
        else {
            continue;
        };
        for imported in imports.regular.names().iter().chain(imports.delay.names()) {
            if let Some(&other) = local_by_name.get(imported) {
                disjoint.union(local_index, other);
            }
        }
    }

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

    let bases = components
        .iter()
        .map(|component| discriminator_base(libraries, component))
        .collect::<Vec<_>>();
    let mut base_counts = HashMap::<String, usize>::new();
    for base in &bases {
        *base_counts.entry(base.clone()).or_default() += 1;
    }
    for (component, base) in components.into_iter().zip(bases) {
        let discriminator = if base_counts.get(&base).copied().unwrap_or_default() == 1 {
            base
        } else {
            format!("{base}-{:016x}", stable_name_hash(libraries, &component))
        };
        for global_index in component {
            result.insert(global_index, discriminator.clone());
        }
    }
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

fn stable_name_hash(libraries: &[DetectedLibraryFile], component: &[usize]) -> u64 {
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
