//! Pure, complete resolution of a component replacement transition.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use renderpilot_domain::{
    ArtifactId, ComponentFile, ComponentId, LibraryArtifact, LibraryComponent, LibraryTechnology,
    PathRef, fsr, normalized_path_key,
    xiph::{self, XiphMember, XiphTopology},
};

use crate::{
    AppError, AppResult,
    dxc::{COMPILER_FILE_NAME, VALIDATOR_FILE_NAME},
};

/// Resolves the artifact members that one concrete component transition writes.
///
/// Most technologies install the complete artifact. Streamline and DXC
/// packages are intersected by installed file name; Xiph packages are
/// intersected by semantic member. A swap therefore never expands the
/// integration chosen by the game.
pub fn resolve_transition_members<'a>(
    component: &LibraryComponent,
    artifact: &'a LibraryArtifact,
) -> AppResult<Vec<&'a ComponentFile>> {
    if component.technology() != artifact.technology() {
        return Err(AppError::invalid_input(
            "component and artifact technologies do not match",
        ));
    }

    let members = match artifact.technology() {
        LibraryTechnology::NvidiaStreamline => {
            let installed = installed_file_names(component)?;
            project_package_members(component, artifact, &installed, component.files().len() > 1)?
        }
        LibraryTechnology::MicrosoftDxc => {
            let installed = installed_file_names(component)?;
            require_dxc_component_shape(&installed)?;
            project_package_members(component, artifact, &installed, true)?
        }
        LibraryTechnology::XiphVorbis => project_xiph_members(component, artifact)?,
        _ => {
            let members: Vec<_> = artifact.files().iter().collect();
            require_unique_resolved_targets(component, &members)?;
            members
        }
    };
    if members.is_empty() {
        return Err(AppError::invalid_input(
            "artifact has no installable files for this component",
        ));
    }

    Ok(members)
}

/// Pure external-import proof consumed by vendor-suffixed Xiph resolution.
///
/// The orchestration layer is responsible for producing this fact from the
/// full game-root import walk. It is deliberately data-only: resolution never
/// opens a file or attempts to infer dynamic `LoadLibrary` consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExternalAliasRequirements {
    /// The runtime has no vendor-suffixed alias to preserve.
    NotRequired,
    /// Every required exact installed alias, normalized to lowercase ASCII.
    Proven(BTreeSet<String>),
    /// The importer walk was absent, incomplete, or unstable.
    Unproven,
}

/// Sidecar state required before an original baseline member can be removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchiveMode {
    /// The original live baseline file must be archived during this mutation.
    Create,
    /// The live file is already absent and a same-mutation/persisted owned
    /// archive must be proved by the durable mutation layer.
    RequireOwnedArchive,
}

/// A candidate artifact member written to one resolved target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWrite {
    target: PathRef,
    source: ComponentFile,
    current: Option<ComponentFile>,
    baseline: Option<ComponentFile>,
    member: Option<XiphMember>,
}

#[allow(
    missing_docs,
    reason = "accessors repeat the documented resolved-write contract"
)]
impl ResolvedWrite {
    #[must_use]
    pub const fn target(&self) -> &PathRef {
        &self.target
    }

    #[must_use]
    pub const fn source(&self) -> &ComponentFile {
        &self.source
    }

    #[must_use]
    pub const fn current(&self) -> Option<&ComponentFile> {
        self.current.as_ref()
    }

    #[must_use]
    pub const fn baseline(&self) -> Option<&ComponentFile> {
        self.baseline.as_ref()
    }

    #[must_use]
    pub const fn member(&self) -> Option<XiphMember> {
        self.member
    }
}

/// An immutable baseline member that must remain sidecar-preserved while no
/// longer live after the transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedArchiveAndRemove {
    target: PathRef,
    baseline: ComponentFile,
    current: Option<ComponentFile>,
    mode: ArchiveMode,
    member: Option<XiphMember>,
}

#[allow(
    missing_docs,
    reason = "accessors repeat the documented archive contract"
)]
impl ResolvedArchiveAndRemove {
    #[must_use]
    pub const fn target(&self) -> &PathRef {
        &self.target
    }

    #[must_use]
    pub const fn baseline(&self) -> &ComponentFile {
        &self.baseline
    }

    #[must_use]
    pub const fn current(&self) -> Option<&ComponentFile> {
        self.current.as_ref()
    }

    #[must_use]
    pub const fn mode(&self) -> ArchiveMode {
        self.mode
    }

    #[must_use]
    pub const fn member(&self) -> Option<XiphMember> {
        self.member
    }
}

/// A current unowned addition that is deliberately removed without creating a
/// new immutable rollback sidecar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRemove {
    target: PathRef,
    current: ComponentFile,
    member: Option<XiphMember>,
}

#[allow(
    missing_docs,
    reason = "accessors repeat the documented removal contract"
)]
impl ResolvedRemove {
    #[must_use]
    pub const fn target(&self) -> &PathRef {
        &self.target
    }

    #[must_use]
    pub const fn current(&self) -> &ComponentFile {
        &self.current
    }

    #[must_use]
    pub const fn member(&self) -> Option<XiphMember> {
        self.member
    }
}

/// A baseline member intentionally restored and left live outside this
/// package's writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedUntouchedBaseline {
    target: PathRef,
    baseline: ComponentFile,
    current: ComponentFile,
    member: Option<XiphMember>,
}

#[allow(
    missing_docs,
    reason = "accessors repeat the documented untouched-baseline contract"
)]
impl ResolvedUntouchedBaseline {
    #[must_use]
    pub const fn target(&self) -> &PathRef {
        &self.target
    }

    #[must_use]
    pub const fn baseline(&self) -> &ComponentFile {
        &self.baseline
    }

    #[must_use]
    pub const fn current(&self) -> &ComponentFile {
        &self.current
    }

    #[must_use]
    pub const fn member(&self) -> Option<XiphMember> {
        self.member
    }
}

/// One exhaustive path disposition in a resolved transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedPathDisposition {
    /// Copy a verified artifact member to the target, replacing or adding it.
    Write(ResolvedWrite),
    /// Sidecar-preserve an immutable original, then make the live target absent.
    ArchiveAndRemove(ResolvedArchiveAndRemove),
    /// Remove a current addition that has no immutable baseline identity.
    Remove(ResolvedRemove),
    /// Keep an immutable baseline member live without writing it.
    UntouchedBaseline(ResolvedUntouchedBaseline),
}

impl ResolvedPathDisposition {
    /// Returns the unique normalized target represented by this disposition.
    #[must_use]
    pub const fn target(&self) -> &PathRef {
        match self {
            Self::Write(value) => value.target(),
            Self::ArchiveAndRemove(value) => value.target(),
            Self::Remove(value) => value.target(),
            Self::UntouchedBaseline(value) => value.target(),
        }
    }
}

/// Semantic Xiph facts kept with a resolved transition instead of being
/// recomputed by preview, apply, rollback, or journal consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedXiphTransition {
    topology: XiphTopology,
    vendor_suffix: Option<String>,
    external_aliases: BTreeSet<String>,
}

#[allow(
    missing_docs,
    reason = "accessors repeat the documented Xiph transition contract"
)]
impl ResolvedXiphTransition {
    #[must_use]
    pub const fn topology(&self) -> &XiphTopology {
        &self.topology
    }

    #[must_use]
    pub fn vendor_suffix(&self) -> Option<&str> {
        self.vendor_suffix.as_deref()
    }

    #[must_use]
    pub const fn external_aliases(&self) -> &BTreeSet<String> {
        &self.external_aliases
    }
}

/// The sole pure transition contract for preview, filesystem planning, apply,
/// rollback, active-state rebuilding, and journaling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTransition {
    component_id: ComponentId,
    artifact_id: ArtifactId,
    target_directory: String,
    primary_target: PathRef,
    paths: Vec<ResolvedPathDisposition>,
    xiph: Option<ResolvedXiphTransition>,
}

#[allow(
    missing_docs,
    reason = "accessors repeat the documented resolved-transition contract"
)]
impl ResolvedTransition {
    #[must_use]
    pub const fn component_id(&self) -> &ComponentId {
        &self.component_id
    }

    #[must_use]
    pub const fn artifact_id(&self) -> &ArtifactId {
        &self.artifact_id
    }

    #[must_use]
    pub fn target_directory(&self) -> &str {
        &self.target_directory
    }

    #[must_use]
    pub const fn primary_target(&self) -> &PathRef {
        &self.primary_target
    }

    #[must_use]
    pub fn paths(&self) -> &[ResolvedPathDisposition] {
        &self.paths
    }

    #[must_use]
    pub const fn xiph(&self) -> Option<&ResolvedXiphTransition> {
        self.xiph.as_ref()
    }

    /// Expected live component files after successful application. This is
    /// derived only from the owned path partition. An untouched baseline
    /// disposition resolves to its immutable original, because application
    /// restores that sidecar before writing any new overlay members.
    #[must_use]
    pub fn expected_active(&self) -> Vec<ComponentFile> {
        self.paths
            .iter()
            .filter_map(|path| match path {
                ResolvedPathDisposition::Write(write) => {
                    Some(materialize_at(&write.source, write.target.clone()))
                }
                ResolvedPathDisposition::UntouchedBaseline(untouched) => {
                    Some(untouched.baseline.clone())
                }
                ResolvedPathDisposition::ArchiveAndRemove(_)
                | ResolvedPathDisposition::Remove(_) => None,
            })
            .collect()
    }

    /// Immutable originals whose live paths must remain reserved/absent after
    /// application. This is derived only from `ArchiveAndRemove` actions.
    #[must_use]
    pub fn reserved(&self) -> Vec<ComponentFile> {
        self.paths
            .iter()
            .filter_map(|path| match path {
                ResolvedPathDisposition::ArchiveAndRemove(archive) => {
                    Some(archive.baseline.clone())
                }
                ResolvedPathDisposition::Write(_)
                | ResolvedPathDisposition::Remove(_)
                | ResolvedPathDisposition::UntouchedBaseline(_) => None,
            })
            .collect()
    }
}

/// Resolves one complete component transition without filesystem access.
///
/// `baseline` must be the immutable original files captured before the first
/// managed mutation. `external_aliases` is a proof result from orchestration,
/// never an inference performed here.
pub fn resolve_transition(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    baseline: &[ComponentFile],
    external_aliases: &ExternalAliasRequirements,
) -> AppResult<ResolvedTransition> {
    if component.technology() != artifact.technology() {
        return Err(AppError::invalid_input(
            "component and artifact technologies do not match",
        ));
    }

    let primary = component.files().first().ok_or_else(|| {
        AppError::invalid_input("component does not contain a primary transition target")
    })?;
    let target_directory = primary
        .path()
        .parent()
        .ok_or_else(|| AppError::invalid_input("component target has no parent directory"))?
        .to_owned();

    let current_by_path = file_map(component.files(), &target_directory, "component")?;
    let baseline_by_path = file_map(baseline, &target_directory, "baseline")?;
    let (writes, xiph) =
        transition_writes(component, artifact, external_aliases, &target_directory)?;

    let mut all_paths = BTreeSet::new();
    all_paths.extend(current_by_path.keys().cloned());
    all_paths.extend(baseline_by_path.keys().cloned());
    all_paths.extend(writes.keys().cloned());

    let mut explicit_removals = resolve_transition_removals(
        baseline,
        artifact,
        writes
            .values()
            .map(|write| write.target.file_name().unwrap_or_default()),
    )
    .into_iter()
    .map(|file| normalized_path_key(file.path().as_str()))
    .collect::<BTreeSet<_>>();
    if xiph.is_some() {
        // Only aliases the external importer proof selected remain live. Every
        // other vendor member is replaced by its canonical candidate target
        // and therefore becomes a rollback-reserved original.
        for file in component.files() {
            let is_vendor = runtime_name(file)
                .ok()
                .and_then(|name| xiph::parse_runtime_file_name(name).ok().flatten())
                .is_some_and(|parsed| parsed.is_vendor());
            if !is_vendor {
                continue;
            }
            let key = normalized_path_key(file.path().as_str());
            if !writes.contains_key(&key) {
                explicit_removals.insert(key);
            }
        }
    }

    let mut paths = Vec::with_capacity(all_paths.len());
    for key in all_paths {
        let current_file = current_by_path.get(&key).copied();
        let baseline_file = baseline_by_path.get(&key).copied();
        if let Some(write) = writes.get(&key) {
            paths.push(ResolvedPathDisposition::Write(ResolvedWrite {
                target: write.target.clone(),
                source: write.source.clone(),
                current: current_file.cloned(),
                baseline: baseline_file.cloned(),
                member: write.member,
            }));
            continue;
        }

        let member = current_file
            .or(baseline_file)
            .and_then(xiph_member_for_component_file);
        match (current_file, baseline_file) {
            (Some(current), Some(baseline)) if explicit_removals.contains(&key) => {
                paths.push(ResolvedPathDisposition::ArchiveAndRemove(
                    ResolvedArchiveAndRemove {
                        target: current.path().clone(),
                        baseline: baseline.clone(),
                        current: Some(current.clone()),
                        mode: ArchiveMode::Create,
                        member,
                    },
                ));
            }
            (Some(current), Some(baseline)) => {
                paths.push(ResolvedPathDisposition::UntouchedBaseline(
                    ResolvedUntouchedBaseline {
                        target: current.path().clone(),
                        baseline: baseline.clone(),
                        current: current.clone(),
                        member,
                    },
                ));
            }
            (None, Some(baseline)) => {
                paths.push(ResolvedPathDisposition::ArchiveAndRemove(
                    ResolvedArchiveAndRemove {
                        target: baseline.path().clone(),
                        baseline: baseline.clone(),
                        current: None,
                        mode: ArchiveMode::RequireOwnedArchive,
                        member,
                    },
                ));
            }
            (Some(current), None) => {
                paths.push(ResolvedPathDisposition::Remove(ResolvedRemove {
                    target: current.path().clone(),
                    current: current.clone(),
                    member,
                }));
            }
            (None, None) => {
                return Err(AppError::invalid_input(
                    "transition path partition was incomplete",
                ));
            }
        }
    }

    let primary_target = writes
        .get(&normalized_path_key(primary.path().as_str()))
        .map(|write| write.target.clone())
        .or_else(|| {
            paths.iter().find_map(|path| match path {
                ResolvedPathDisposition::Write(write) => Some(write.target.clone()),
                ResolvedPathDisposition::ArchiveAndRemove(_)
                | ResolvedPathDisposition::Remove(_)
                | ResolvedPathDisposition::UntouchedBaseline(_) => None,
            })
        })
        .ok_or_else(|| AppError::invalid_input("transition has no resolved write target"))?;

    Ok(ResolvedTransition {
        component_id: component.id().clone(),
        artifact_id: artifact.id().clone(),
        target_directory,
        primary_target,
        paths,
        xiph,
    })
}

#[derive(Debug, Clone)]
struct TransitionWrite {
    target: PathRef,
    source: ComponentFile,
    member: Option<XiphMember>,
}

fn transition_writes(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    external_aliases: &ExternalAliasRequirements,
    target_directory: &str,
) -> AppResult<(
    BTreeMap<String, TransitionWrite>,
    Option<ResolvedXiphTransition>,
)> {
    if component.technology() == LibraryTechnology::XiphVorbis {
        crate::validate_runtime_artifact(artifact)
            .map_err(|error| AppError::invalid_input(error.to_string()))?;
        crate::compatibility::ensure_transition_compatible_with_external_aliases(
            component,
            artifact,
            external_aliases,
        )
        .map_err(|error| AppError::invalid_input(error.to_string()))?;
        return xiph_transition_writes(component, artifact, external_aliases, target_directory);
    }

    let members = resolve_transition_members(component, artifact)?;
    let mut writes = BTreeMap::new();
    for source in members {
        let target_name = resolve_transition_install_target(component, source);
        let target = join_target(target_directory, &target_name)?;
        insert_transition_write(
            &mut writes,
            TransitionWrite {
                target,
                source: source.clone(),
                member: None,
            },
        )?;
    }
    Ok((writes, None))
}

fn xiph_transition_writes(
    component: &LibraryComponent,
    artifact: &LibraryArtifact,
    external_aliases: &ExternalAliasRequirements,
    target_directory: &str,
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
        let (member, style) = xiph::classify_canonical_file_name(name)
            .ok_or_else(|| AppError::invalid_input("Xiph artifact has an unsupported DLL alias"))?;
        if candidates
            .insert(member, (style, name.to_ascii_lowercase(), file))
            .is_some()
        {
            return Err(AppError::invalid_input(
                "Xiph artifact has duplicate semantic members",
            ));
        }
    }

    let proven_aliases = match external_aliases {
        ExternalAliasRequirements::NotRequired => BTreeSet::new(),
        ExternalAliasRequirements::Proven(aliases) => aliases.clone(),
        ExternalAliasRequirements::Unproven => {
            return Err(AppError::invalid_input(
                "vendor-suffixed Xiph deployment requires a complete external alias proof",
            ));
        }
    };

    let vendor_layout = installed.values().any(|(parsed, _)| parsed.is_vendor());
    if vendor_layout && proven_aliases.is_empty() {
        return Err(AppError::invalid_input(
            "vendor-suffixed Xiph deployment requires at least one external vendor alias",
        ));
    }

    let mut writes = BTreeMap::new();
    let mut targets_by_member = BTreeMap::new();
    let mut candidate_sources = BTreeMap::new();
    for (member, (installed_name, _)) in &installed {
        let (style, candidate_name, source) = candidates.get(member).ok_or_else(|| {
            AppError::invalid_input(format!(
                "Xiph package does not cover installed member: {}",
                member.as_slug()
            ))
        })?;
        if vendor_layout && *style != xiph::XiphNameStyle::Plain {
            return Err(AppError::invalid_input(
                "vendor-suffixed Xiph deployment requires plain canonical candidate DLL names",
            ));
        }
        let target_name = if installed_name.is_vendor()
            && proven_aliases.contains(installed_name.normalized_name())
        {
            installed_name.normalized_name().to_owned()
        } else {
            candidate_name.clone()
        };
        let target = join_target(target_directory, &target_name)?;
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

    // Candidate imports are canonical artifact imports. A proof may preserve a
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
            if target != imported.normalized_name() {
                return Err(AppError::invalid_input(format!(
                    "required vendor alias for {} conflicts with canonical candidate dependency {}",
                    member.as_slug(),
                    imported.normalized_name()
                )));
            }
        }
    }

    let runtime_files = component
        .files()
        .iter()
        .map(|file| runtime_name(file).map(|name| (name, file)))
        .collect::<AppResult<Vec<_>>>()?;
    let topology = xiph::detect_layout_with_file_names(runtime_files)
        .map(|layout| layout.topology().clone())
        .ok_or_else(|| AppError::invalid_input("Xiph component has an invalid runtime topology"))?;
    Ok((
        writes,
        Some(ResolvedXiphTransition {
            topology,
            vendor_suffix,
            external_aliases: proven_aliases,
        }),
    ))
}

fn insert_transition_write(
    writes: &mut BTreeMap<String, TransitionWrite>,
    write: TransitionWrite,
) -> AppResult<()> {
    let key = normalized_path_key(write.target.as_str());
    if writes.insert(key.clone(), write).is_some() {
        return Err(AppError::invalid_input(format!(
            "artifact resolves multiple members to install target: {key}"
        )));
    }
    Ok(())
}

fn file_map<'a>(
    files: &'a [ComponentFile],
    target_directory: &str,
    label: &str,
) -> AppResult<BTreeMap<String, &'a ComponentFile>> {
    let mut mapped = BTreeMap::new();
    for file in files {
        let name = file
            .path()
            .file_name()
            .ok_or_else(|| AppError::invalid_input(format!("{label} file has no file name")))?;
        if name.trim().is_empty() {
            return Err(AppError::invalid_input(format!(
                "{label} file has an empty file name"
            )));
        }
        let parent = file.path().parent().ok_or_else(|| {
            AppError::invalid_input(format!("{label} file has no parent directory"))
        })?;
        if normalized_path_key(parent) != normalized_path_key(target_directory) {
            return Err(AppError::invalid_input(format!(
                "{label} files do not share one transition directory"
            )));
        }
        if file.sha256().is_none() {
            return Err(AppError::invalid_input(format!(
                "{label} file is missing a SHA-256 hash"
            )));
        }
        let key = normalized_path_key(file.path().as_str());
        if mapped.insert(key, file).is_some() {
            return Err(AppError::invalid_input(format!(
                "{label} has duplicate normalized file paths"
            )));
        }
    }
    Ok(mapped)
}

fn join_target(directory: &str, name: &str) -> AppResult<PathRef> {
    if name.trim().is_empty() || name.contains('/') || name.contains('\\') {
        return Err(AppError::invalid_input(
            "artifact resolves an invalid install target",
        ));
    }
    let path = if directory.is_empty() {
        name.to_owned()
    } else {
        format!("{directory}/{name}")
    };
    PathRef::new(path).map_err(|error| {
        AppError::invalid_input(format!("invalid transition target path: {error}"))
    })
}

fn runtime_name(file: &ComponentFile) -> AppResult<&str> {
    file.install_as()
        .or_else(|| file.path().file_name())
        .ok_or_else(|| AppError::invalid_input("Xiph file has no runtime basename"))
}

fn xiph_member_for_component_file(file: &ComponentFile) -> Option<XiphMember> {
    runtime_name(file)
        .ok()
        .and_then(|name| xiph::parse_runtime_file_name(name).ok().flatten())
        .map(|parsed| parsed.member())
}

fn materialize_at(source: &ComponentFile, target: PathRef) -> ComponentFile {
    let mut materialized = ComponentFile::new(target);
    if let Some(version) = source.version() {
        materialized = materialized.with_version(version.clone());
    }
    if let Some(hash) = source.sha256() {
        materialized = materialized.with_sha256(hash.clone());
    }
    if let Some(profile) = source.pe_compatibility() {
        materialized = materialized.with_pe_compatibility(profile.clone());
    }
    materialized
}

/// Resolves the concrete basename written by one transition member.
///
/// This compatibility wrapper only preserves canonical Xiph aliases. Vendor
/// layouts must use [`resolve_transition`] with explicit external-alias proof;
/// the string-only legacy API cannot safely make that decision.
#[must_use]
pub fn resolve_transition_install_target(
    component: &LibraryComponent,
    artifact_file: &ComponentFile,
) -> String {
    if component.technology() == LibraryTechnology::XiphVorbis
        && let Some(artifact_name) = artifact_file
            .install_as()
            .or_else(|| artifact_file.path().file_name())
        && let Some((artifact_member, _)) = xiph::classify_canonical_file_name(artifact_name)
        && let Some(installed_name) = component.files().iter().find_map(|file| {
            let name = file.path().file_name()?;
            (xiph::classify_canonical_file_name(name).map(|value| value.0) == Some(artifact_member))
                .then_some(name)
        })
    {
        return installed_name.to_owned();
    }

    fsr::resolve_artifact_install_target(artifact_file, component.files())
}

/// Resolves installed files that a transition must remove in addition to its
/// writes.
///
/// A unified FSR backend supersedes stale split upscaling members, while
/// separately owned optional effects remain untouched. Callers supply the
/// already-resolved write targets so cleanup and installation cannot claim the
/// same path.
#[must_use]
pub fn resolve_transition_removals<'a, 'b>(
    removal_basis: &'a [ComponentFile],
    artifact: &LibraryArtifact,
    resolved_install_targets: impl IntoIterator<Item = &'b str>,
) -> Vec<&'a ComponentFile> {
    let target_is_unified_fsr = artifact.technology().family() == LibraryTechnology::AmdFsr
        && !fsr::is_split_marker(artifact.file_name());
    if !target_is_unified_fsr || !fsr::has_entry_point(removal_basis) {
        return Vec::new();
    }

    let planned_names: HashSet<String> = resolved_install_targets
        .into_iter()
        .map(str::to_ascii_lowercase)
        .collect();

    removal_basis
        .iter()
        .filter(|file| {
            file.path().file_name().is_some_and(|name| {
                fsr::is_upscaling_member(name)
                    && !planned_names.contains(&name.to_ascii_lowercase())
            })
        })
        .collect()
}

fn installed_file_names(component: &LibraryComponent) -> AppResult<HashSet<String>> {
    let mut names = HashSet::with_capacity(component.files().len());
    for file in component.files() {
        let name = file
            .path()
            .file_name()
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| AppError::invalid_input("component target has no file name"))?;
        if !names.insert(name) {
            return Err(AppError::invalid_input(
                "component has duplicate installed file targets",
            ));
        }
    }
    Ok(names)
}

fn project_xiph_members<'a>(
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

fn project_package_members<'a>(
    component: &LibraryComponent,
    artifact: &'a LibraryArtifact,
    installed: &HashSet<String>,
    require_full_coverage: bool,
) -> AppResult<Vec<&'a ComponentFile>> {
    let mut package_names = HashSet::with_capacity(artifact.files().len());
    let mut projected = Vec::new();

    for member in artifact.files() {
        let install_name =
            fsr::resolve_artifact_install_target(member, component.files()).to_ascii_lowercase();
        if install_name.trim().is_empty() {
            return Err(AppError::invalid_input(
                "artifact resolves an empty install target",
            ));
        }
        if !package_names.insert(install_name.clone()) {
            return Err(AppError::invalid_input(format!(
                "package has duplicate install target: {install_name}"
            )));
        }
        if installed.contains(&install_name) {
            projected.push(member);
        }
    }

    if require_full_coverage {
        require_package_coverage(installed, &package_names, artifact.technology().as_slug())?;
    }

    Ok(projected)
}

fn require_package_coverage(
    installed: &HashSet<String>,
    package_names: &HashSet<String>,
    technology: &str,
) -> AppResult<()> {
    let mut missing: Vec<&str> = installed
        .iter()
        .filter(|name| !package_names.contains(*name))
        .map(String::as_str)
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    missing.sort_unstable();
    Err(AppError::invalid_input(format!(
        "{technology} package does not cover installed files: {}",
        missing.join(", ")
    )))
}

fn require_dxc_component_shape(installed: &HashSet<String>) -> AppResult<()> {
    let has_compiler = installed.contains(COMPILER_FILE_NAME);
    let has_valid_size =
        installed.len() == 1 || (installed.len() == 2 && installed.contains(VALIDATOR_FILE_NAME));

    if has_compiler && has_valid_size {
        Ok(())
    } else {
        Err(AppError::invalid_input(format!(
            "DXC component must contain {COMPILER_FILE_NAME}, optionally paired with \
             {VALIDATOR_FILE_NAME}"
        )))
    }
}

fn require_unique_resolved_targets(
    component: &LibraryComponent,
    members: &[&ComponentFile],
) -> AppResult<()> {
    let mut targets = HashSet::with_capacity(members.len());
    for member in members {
        let target =
            fsr::resolve_artifact_install_target(member, component.files()).to_ascii_lowercase();
        if target.trim().is_empty() {
            return Err(AppError::invalid_input(
                "artifact resolves an empty install target",
            ));
        }
        if !targets.insert(target.clone()) {
            return Err(AppError::invalid_input(format!(
                "artifact resolves multiple members to install target: {target}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use renderpilot_domain::{
        Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, ComponentId, ComponentKind,
        GameId, PathRef, PeCompatibilityProfile, PeExportSet, PeImportProfile, PeImportSet,
        RuntimeTarget, Sha256Hash, Swappability,
    };

    use super::*;

    fn file(path: &str, hash: char) -> ComponentFile {
        ComponentFile::new(PathRef::new(path).expect("path"))
            .with_sha256(Sha256Hash::new(hash.to_string().repeat(64)).expect("hash"))
    }

    fn streamline_component(names: &[&str]) -> LibraryComponent {
        names.iter().fold(
            LibraryComponent::new(
                ComponentId::new("component:streamline-transition").expect("component"),
                GameId::new("game:streamline-transition").expect("game"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::NvidiaStreamline,
                Swappability::BundleOnly,
            ),
            |component, name| component.with_file(file(&format!("C:/Game/{name}"), 'f')),
        )
    }

    fn streamline_artifact(names: &[&str]) -> LibraryArtifact {
        let files = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                file(
                    &format!("C:/Library/{name}"),
                    char::from(b'a' + index as u8),
                )
            })
            .collect();
        LibraryArtifact::new(
            ArtifactId::new("artifact:streamline-transition").expect("artifact"),
            LibraryTechnology::NvidiaStreamline,
            names[0],
            files,
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
    }

    fn dxc_component(names: &[&str]) -> LibraryComponent {
        names.iter().fold(
            LibraryComponent::new(
                ComponentId::new("component:dxc-transition").expect("component"),
                GameId::new("game:dxc-transition").expect("game"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::MicrosoftDxc,
                if names.len() > 1 {
                    Swappability::BundleOnly
                } else {
                    Swappability::Swappable
                },
            ),
            |component, name| component.with_file(file(&format!("C:/Game/{name}"), 'f')),
        )
    }

    fn dxc_package() -> LibraryArtifact {
        LibraryArtifact::new(
            ArtifactId::new("artifact:dxc-transition").expect("artifact"),
            LibraryTechnology::MicrosoftDxc,
            COMPILER_FILE_NAME,
            vec![
                file(&format!("C:/Library/{COMPILER_FILE_NAME}"), 'a'),
                file(&format!("C:/Library/{VALIDATOR_FILE_NAME}"), 'b'),
            ],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
    }

    fn xiph_component(names: &[&str]) -> LibraryComponent {
        names.iter().fold(
            LibraryComponent::new(
                ComponentId::new("component:xiph-transition").expect("component"),
                GameId::new("game:xiph-transition").expect("game"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::XiphVorbis,
                Swappability::BundleOnly,
            ),
            |component, name| component.with_file(file(&format!("C:/Game/{name}"), 'f')),
        )
    }

    fn xiph_artifact(names: &[&str]) -> LibraryArtifact {
        let files = names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                file(
                    &format!("C:/Library/{name}"),
                    char::from(b'a' + index as u8),
                )
            })
            .collect();
        LibraryArtifact::new(
            ArtifactId::new("artifact:xiph-transition").expect("artifact"),
            LibraryTechnology::XiphVorbis,
            names[0],
            files,
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
    }

    fn dide_member(name: &str, imports: &[&str], hash: char, root: &str) -> ComponentFile {
        let member = xiph::parse_runtime_file_name(name)
            .expect("runtime name")
            .expect("Xiph member")
            .member();
        let export = match member {
            XiphMember::VorbisFile => "ov_open",
            XiphMember::VorbisEnc => "vorbis_encode_init",
            XiphMember::Vorbis => "vorbis_info_init",
            XiphMember::Ogg => "ogg_sync_init",
        };
        ComponentFile::new(PathRef::new(format!("{root}/{name}")).expect("path"))
            .with_sha256(Sha256Hash::new(hash.to_string().repeat(64)).expect("hash"))
            .with_pe_compatibility(
                PeCompatibilityProfile::new(
                    Architecture::X64,
                    PeExportSet::from_observed_names(vec![export.to_owned()]).expect("exports"),
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

    fn dide_component() -> LibraryComponent {
        [
            dide_member(
                "vorbisfile_vs2010_x64_rwdi.dll",
                &["vorbis_vs2010_x64_rwdi.dll", "ogg_vs2010_x64_rwdi.dll"],
                '1',
                "C:/Game",
            ),
            dide_member(
                "vorbis_vs2010_x64_rwdi.dll",
                &["ogg_vs2010_x64_rwdi.dll"],
                '2',
                "C:/Game",
            ),
            dide_member("ogg_vs2010_x64_rwdi.dll", &[], '3', "C:/Game"),
        ]
        .into_iter()
        .fold(
            LibraryComponent::new(
                ComponentId::new("component:dide").expect("component"),
                GameId::new("game:dide").expect("game"),
                ComponentKind::NativeLibrary,
                LibraryTechnology::XiphVorbis,
                Swappability::BundleOnly,
            ),
            LibraryComponent::with_file,
        )
    }

    fn dide_artifact(files: Vec<ComponentFile>) -> LibraryArtifact {
        let primary = files[0].path().file_name().expect("primary").to_owned();
        LibraryArtifact::new(
            ArtifactId::new("artifact:xiph:dide").expect("artifact"),
            LibraryTechnology::XiphVorbis,
            primary,
            files,
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact")
        .with_metadata(
            ArtifactMetadata::default().with_runtime_target(RuntimeTarget::new(Architecture::X64)),
        )
    }

    fn canonical_dide_artifact() -> LibraryArtifact {
        dide_artifact(vec![
            dide_member(
                "vorbisfile.dll",
                &["vorbis.dll", "ogg.dll"],
                'a',
                "C:/Library",
            ),
            dide_member("vorbis.dll", &["ogg.dll"], 'b', "C:/Library"),
            dide_member("ogg.dll", &[], 'c', "C:/Library"),
        ])
    }

    #[test]
    fn xiph_transition_writes_only_members_present_in_the_game() {
        let component = xiph_component(&["libvorbisfile.dll", "libvorbis.dll", "libogg.dll"]);
        let package = xiph_artifact(&[
            "libvorbis.dll",
            "libvorbisfile.dll",
            "libvorbisenc.dll",
            "libogg.dll",
        ]);

        let members = resolve_transition_members(&component, &package).expect("transition");
        let targets = members
            .iter()
            .map(|member| resolve_transition_install_target(&component, member))
            .collect::<Vec<_>>();

        assert_eq!(
            targets,
            ["libvorbis.dll", "libvorbisfile.dll", "libogg.dll"],
            "the optional encoder must not expand a three-member game integration"
        );
    }

    #[test]
    fn streamline_transition_uses_only_installed_targets_and_requires_coverage() {
        let component = streamline_component(&["sl.common.dll", "sl.interposer.dll"]);
        let complete = streamline_artifact(&["sl.common.dll", "sl.dlss.dll", "sl.interposer.dll"]);
        let members = resolve_transition_members(&component, &complete).expect("transition");
        assert_eq!(
            members
                .iter()
                .filter_map(|file| file.path().file_name())
                .collect::<Vec<_>>(),
            ["sl.common.dll", "sl.interposer.dll"]
        );

        let incomplete = streamline_artifact(&["sl.common.dll", "sl.dlss.dll"]);
        assert!(
            resolve_transition_members(&component, &incomplete)
                .expect_err("coverage")
                .message()
                .contains("sl.interposer.dll")
        );
    }

    #[test]
    fn streamline_transition_rejects_an_empty_intersection() {
        let error = resolve_transition_members(
            &streamline_component(&["sl.common.dll"]),
            &streamline_artifact(&["sl.dlss.dll"]),
        )
        .expect_err("empty transition");
        assert!(error.message().contains("no installable files"));
    }

    #[test]
    fn dxc_transition_keeps_a_standalone_compiler_standalone() {
        let component = dxc_component(&[COMPILER_FILE_NAME]);
        let package = dxc_package();

        let members = resolve_transition_members(&component, &package).expect("transition");
        assert_eq!(members.len(), 1);
        assert_eq!(
            members[0].path().file_name(),
            Some(COMPILER_FILE_NAME),
            "a standalone game integration must remain standalone"
        );
    }

    #[test]
    fn dxc_transition_keeps_an_installed_pair_complete() {
        let component = dxc_component(&[COMPILER_FILE_NAME, VALIDATOR_FILE_NAME]);
        let package = dxc_package();

        let members = resolve_transition_members(&component, &package).expect("transition");
        assert_eq!(
            members
                .iter()
                .filter_map(|file| file.path().file_name())
                .collect::<Vec<_>>(),
            [COMPILER_FILE_NAME, VALIDATOR_FILE_NAME],
            "an installed pair must remain a two-file integration"
        );
    }

    #[test]
    fn dxc_transition_rejects_a_validator_without_a_compiler() {
        let component = dxc_component(&[VALIDATOR_FILE_NAME]);

        let error = resolve_transition_members(&component, &dxc_package())
            .expect_err("dxil.dll alone is not a valid DXC integration");
        assert!(error.message().contains(COMPILER_FILE_NAME));
    }

    #[test]
    fn dxc_transition_requires_the_package_to_cover_an_installed_pair() {
        let component = dxc_component(&[COMPILER_FILE_NAME, VALIDATOR_FILE_NAME]);
        let incomplete = LibraryArtifact::new(
            ArtifactId::new("artifact:dxc-incomplete").expect("artifact"),
            LibraryTechnology::MicrosoftDxc,
            COMPILER_FILE_NAME,
            vec![file(&format!("C:/Library/{COMPILER_FILE_NAME}"), 'a')],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact");

        let error = resolve_transition_members(&component, &incomplete)
            .expect_err("the installed validator must be covered");
        assert!(error.message().contains(VALIDATOR_FILE_NAME));
    }

    #[test]
    fn transition_rejects_a_technology_mismatch() {
        let component = streamline_component(&["sl.common.dll"]);
        let mismatched = LibraryArtifact::new(
            ArtifactId::new("artifact:mismatched-transition").expect("artifact"),
            LibraryTechnology::DlssSuperResolution,
            "nvngx_dlss.dll",
            vec![file("C:/Library/nvngx_dlss.dll", 'a')],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact");
        assert!(
            resolve_transition_members(&component, &mismatched)
                .expect_err("technology mismatch")
                .message()
                .contains("technologies do not match")
        );
    }

    #[test]
    fn transition_rejects_duplicate_resolved_targets() {
        let component = streamline_component(&["sl.common.dll"]);
        let duplicate = streamline_artifact(&["sl.common.dll", "SL.COMMON.DLL"]);
        assert!(
            resolve_transition_members(&component, &duplicate)
                .expect_err("duplicate target")
                .message()
                .contains("duplicate install target")
        );
    }

    #[test]
    fn fsr_reswap_reserves_missing_split_baseline_member_from_immutable_baseline() {
        // A previous unified FSR deployment owns only the entry point live;
        // the original split upscaler survives only in its immutable sidecar.
        // Re-applying a unified target must keep that split original reserved
        // instead of silently dropping it from the transition partition.
        let baseline = vec![
            file("C:/Game/amd_fidelityfx_dx12.dll", 'a'),
            file("C:/Game/amd_fidelityfx_upscaler_dx12.dll", 'b'),
        ];
        let component = LibraryComponent::new(
            ComponentId::new("component:fsr-reswap").expect("component"),
            GameId::new("game:fsr-reswap").expect("game"),
            ComponentKind::NativeLibrary,
            LibraryTechnology::AmdFsr,
            Swappability::BundleOnly,
        )
        .with_file(file("C:/Game/amd_fidelityfx_dx12.dll", 'c'));
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:fsr-unified").expect("artifact"),
            LibraryTechnology::AmdFsr,
            "amd_fidelityfx_dx12.dll",
            vec![file("C:/Library/amd_fidelityfx_dx12.dll", 'd')],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("artifact");

        let transition = resolve_transition(
            &component,
            &artifact,
            &baseline,
            &ExternalAliasRequirements::NotRequired,
        )
        .expect("FSR reswap transition");

        assert!(transition.paths().iter().any(|path| {
            matches!(path, ResolvedPathDisposition::ArchiveAndRemove(archive)
                if archive.target().file_name() == Some("amd_fidelityfx_upscaler_dx12.dll")
                    && archive.baseline() == &baseline[1]
                    && archive.current().is_none()
                    && archive.mode() == ArchiveMode::RequireOwnedArchive)
        }));
        assert_eq!(
            transition
                .reserved()
                .iter()
                .filter_map(|file| file.path().file_name())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["amd_fidelityfx_upscaler_dx12.dll"])
        );
    }

    #[test]
    fn dide_transition_preserves_only_proven_wrapper_and_archives_vendor_core() {
        let component = dide_component();
        let baseline = component.files().to_vec();
        let resolved = resolve_transition(
            &component,
            &canonical_dide_artifact(),
            &baseline,
            &ExternalAliasRequirements::Proven(BTreeSet::from([
                "vorbisfile_vs2010_x64_rwdi.dll".to_owned()
            ])),
        )
        .expect("DIDE transition resolves");

        assert_eq!(resolved.paths().len(), 5);
        assert_eq!(
            resolved
                .paths()
                .iter()
                .filter(|path| matches!(path, ResolvedPathDisposition::Write(_)))
                .count(),
            3
        );
        assert_eq!(
            resolved
                .paths()
                .iter()
                .filter(|path| matches!(path, ResolvedPathDisposition::ArchiveAndRemove(_)))
                .count(),
            2
        );
        assert!(resolved.paths().iter().any(|path| {
            matches!(path, ResolvedPathDisposition::Write(write)
                if write.target().file_name() == Some("vorbisfile_vs2010_x64_rwdi.dll"))
        }));
        assert!(resolved.paths().iter().any(|path| {
            matches!(path, ResolvedPathDisposition::Write(write)
                if write.target().file_name() == Some("vorbis.dll"))
        }));
        assert!(resolved.paths().iter().any(|path| {
            matches!(path, ResolvedPathDisposition::Write(write)
                if write.target().file_name() == Some("ogg.dll"))
        }));
        assert_eq!(
            resolved
                .reserved()
                .iter()
                .filter_map(|file| file.path().file_name())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["ogg_vs2010_x64_rwdi.dll", "vorbis_vs2010_x64_rwdi.dll",])
        );
        assert_eq!(
            resolved
                .expected_active()
                .iter()
                .filter_map(|file| file.path().file_name())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["ogg.dll", "vorbis.dll", "vorbisfile_vs2010_x64_rwdi.dll"])
        );
    }

    #[test]
    fn vendor_xiph_requires_nonempty_exact_external_alias_proof() {
        let component = dide_component();
        let baseline = component.files().to_vec();
        for aliases in [
            ExternalAliasRequirements::Unproven,
            ExternalAliasRequirements::Proven(BTreeSet::new()),
            ExternalAliasRequirements::Proven(BTreeSet::from(["not-xiph.dll".to_owned()])),
        ] {
            assert!(
                resolve_transition(&component, &canonical_dide_artifact(), &baseline, &aliases)
                    .is_err()
            );
        }
    }

    #[test]
    fn vendor_xiph_rejects_alias_that_strands_a_canonical_candidate_import() {
        let component = dide_component();
        let error = resolve_transition(
            &component,
            &canonical_dide_artifact(),
            component.files(),
            &ExternalAliasRequirements::Proven(BTreeSet::from([
                "vorbisfile_vs2010_x64_rwdi.dll".to_owned(),
                "vorbis_vs2010_x64_rwdi.dll".to_owned(),
            ])),
        )
        .expect_err("canonical Vorbis import must remain live");
        assert!(
            error
                .message()
                .contains("conflicts with canonical candidate dependency")
        );
    }

    #[test]
    fn vendor_xiph_rejects_nonplain_catalog_candidate() {
        let component = dide_component();
        let artifact = dide_artifact(vec![
            dide_member(
                "libvorbisfile.dll",
                &["libvorbis.dll", "libogg.dll"],
                'a',
                "C:/Library",
            ),
            dide_member("libvorbis.dll", &["libogg.dll"], 'b', "C:/Library"),
            dide_member("libogg.dll", &[], 'c', "C:/Library"),
        ]);
        let error = resolve_transition(
            &component,
            &artifact,
            component.files(),
            &ExternalAliasRequirements::Proven(BTreeSet::from([
                "vorbisfile_vs2010_x64_rwdi.dll".to_owned()
            ])),
        )
        .expect_err("vendor target accepts only plain canonical candidates");
        assert!(error.message().contains("plain canonical candidate"));
    }
}
