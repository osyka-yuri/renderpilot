//! Module selection, dependency projection, and native artifact binding.

use super::super::types::OptiScalerEvidence;
use super::*;
pub(crate) fn default_modules(manifest: &OptiScalerManifest, release_id: &str) -> HashSet<String> {
    let mut selected: HashSet<_> = manifest
        .modules
        .iter()
        .filter(|module| module.bundled_by_default || !module.optional)
        .filter(|module| module_available_for_release(manifest, release_id, &module.id))
        .map(|module| module.id.clone())
        .collect();
    close_dependencies(manifest, &mut selected);
    selected
}

pub(super) struct ProjectionRequest<'a> {
    pub(super) context: &'a Context,
    pub(super) manifest: &'a OptiScalerManifest,
    pub(super) release: Option<&'a OptiScalerRelease>,
    pub(super) components: &'a [GraphicsComponent],
    pub(super) installed_state: Option<&'a renderpilot_domain::OptiScalerInstallState>,
    pub(super) managed_bindings: &'a [ManagedAddonFile],
    pub(super) rule: Option<&'a crate::addons::optiscaler::compatibility_catalog::ResolvedVariant>,
    pub(super) evidence: &'a [OptiScalerEvidence],
}

pub(super) struct ModuleEvaluation {
    pub(super) modules: Vec<EvaluatedModule>,
}

pub(super) fn project(request: &ProjectionRequest<'_>) -> Result<ModuleEvaluation, ServiceError> {
    let ProjectionRequest {
        context,
        manifest,
        release,
        components,
        installed_state,
        managed_bindings,
        rule,
        evidence,
    } = *request;
    let owned_native_paths = optiscaler_owned_native_paths(managed_bindings);
    let auto_optipatcher = installed_state.is_none()
        && rule.is_some_and(|rule| {
            matches!(
                rule.optipatcher,
                crate::addons::optiscaler::compatibility_catalog::OptiPatcherPolicy::Recommended
            )
        })
        && evidence.iter().any(|item| {
            matches!(
                item.technology,
                GraphicsTechnology::IntelXeSs
                    | GraphicsTechnology::AmdFsr
                    | GraphicsTechnology::AmdFsrUpscaler
            )
        })
        && release.is_some_and(|release| {
            module_artifact_for_release(manifest, release, "optipatcher").is_some()
        });
    let mut selected_modules = installed_state
        .map(|state| state.modules.iter().cloned().collect())
        .or_else(|| release.map(|release| default_modules(manifest, release.id.as_str())))
        .unwrap_or_default();
    if native_sr_is_redundant(components, managed_bindings) {
        selected_modules.remove("nvidia_sr");
    }
    if auto_optipatcher {
        selected_modules.insert("optipatcher".to_owned());
    }
    let restricted_modules =
        rule.map_or_default(|rule| rule.restricted_modules.iter().collect::<HashSet<_>>());
    let nvidia_runtime_available = renderpilot_nvapi::Nvapi::get().is_some();
    let native_artifacts = native_artifact_universe(context)?;
    let downloadable_native_modules = manifest
        .modules
        .iter()
        .filter(|module| native_module_spec(&module.id).is_some())
        .filter(|module| best_native_module_artifact_from(&native_artifacts, &module.id).is_some())
        .map(|module| module.id.clone())
        .collect::<HashSet<_>>();
    let modules = manifest
        .modules
        .iter()
        .filter(|module| {
            module_is_installable(manifest, module)
                && (selected_modules.contains(&module.id)
                    || module_applies_to_game(
                        &module.id,
                        components,
                        &owned_native_paths,
                        nvidia_runtime_available,
                    ))
        })
        .map(|module| {
            let applicable = module_applies_to_game(
                &module.id,
                components,
                &owned_native_paths,
                nvidia_runtime_available,
            );
            let bundled = release.is_some_and(|release| {
                release
                    .members
                    .iter()
                    .any(|member| member.module == module.id)
                    || module.id == "core"
            });
            let pinned_artifact = release.is_some_and(|release| {
                module_artifact_for_release(manifest, release, &module.id).is_some()
            });
            let existing_game = native_component_available(components, &module.id);
            let catalog_download = downloadable_native_modules.contains(&module.id);
            let artifact_available =
                bundled || pinned_artifact || existing_game || catalog_download;
            let grandfathered = installed_state.is_some()
                && selected_modules.contains(&module.id)
                && artifact_available;
            let available = grandfathered
                || (applicable && !restricted_modules.contains(&module.id) && artifact_available);
            let provisioning = if !available {
                OptiScalerModuleProvisioning::Unavailable
            } else if bundled {
                OptiScalerModuleProvisioning::Bundled
            } else if pinned_artifact {
                OptiScalerModuleProvisioning::PinnedArtifact
            } else if existing_game {
                OptiScalerModuleProvisioning::ExistingGame
            } else {
                OptiScalerModuleProvisioning::CatalogDownload
            };
            EvaluatedModule {
                id: module.id.clone(),
                selected: selected_modules.contains(&module.id),
                optional: module.optional,
                available,
                provisioning,
                requires: module.requires.clone(),
                conflicts: module.conflicts.clone(),
                description: module.description.clone(),
            }
        })
        .collect();
    Ok(ModuleEvaluation { modules })
}

pub(crate) fn validate_module_selection(
    manifest: &OptiScalerManifest,
    release_id: &str,
    requested: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<HashSet<String>, ServiceError> {
    let mut selected: HashSet<String> = requested
        .into_iter()
        .map(|module| module.as_ref().to_owned())
        .collect();
    selected.insert("core".to_owned());
    close_dependencies(manifest, &mut selected);
    for id in &selected {
        let module = manifest
            .modules
            .iter()
            .find(|module| &module.id == id)
            .ok_or_else(|| crate::failed(format!("unknown OptiScaler module {id}")))?;
        if !module_is_installable(manifest, module) {
            return Err(crate::failed(format!(
                "OptiScaler module {id} is an internal archive-validation group and cannot be installed"
            )));
        }
        let available = manifest
            .releases
            .iter()
            .find(|release| release.id == release_id)
            .is_some_and(|release| release.members.iter().any(|member| member.module == *id));
        let has_module_artifact = manifest
            .releases
            .iter()
            .find(|release| release.id == release_id)
            .is_some_and(|release| module_artifact_for_release(manifest, release, id).is_some());
        if !available && !has_module_artifact && native_module_spec(id).is_none() {
            return Err(crate::failed(format!(
                "module {id} has no pinned artifact for release {release_id}"
            )));
        }
        if let Some(conflict) = module
            .conflicts
            .iter()
            .find(|peer| selected.contains(*peer))
        {
            return Err(crate::failed(format!(
                "modules {id} and {conflict} conflict"
            )));
        }
    }
    Ok(selected)
}

/// Whether a manifest module has an installable payload or binds to a native
/// game component. Targetless groups such as licenses/setup documentation are
/// still verified as archive members, but are not lifecycle choices.
pub(crate) fn module_is_installable(
    manifest: &OptiScalerManifest,
    module: &super::super::types::OptiScalerModule,
) -> bool {
    module.artifact.is_some()
        || native_module_spec(&module.id).is_some()
        || manifest.releases.iter().any(|release| {
            release
                .members
                .iter()
                .any(|member| member.module == module.id)
        })
}

/// Conservative game-level eligibility for optional native output backends.
///
/// A native DLSS input means the game already has an SR implementation, so a
/// second OptiScaler SR binding adds no capability. RR is deliberately absent
/// from this generic module model because a DLL cannot create RR integration.
pub(super) fn module_applies_to_game(
    module_id: &str,
    components: &[GraphicsComponent],
    owned_native_paths: &HashSet<String>,
    nvidia_runtime_available: bool,
) -> bool {
    if module_id != "nvidia_sr" {
        return true;
    }
    if !nvidia_runtime_available {
        return false;
    }
    !native_component_available_outside_managed_paths(components, "nvidia_sr", owned_native_paths)
        && components.iter().any(|component| {
            matches!(
                component.technology(),
                GraphicsTechnology::IntelXeSs
                    | GraphicsTechnology::AmdFsr
                    | GraphicsTechnology::AmdFsrUpscaler
            )
        })
}

pub(super) fn optiscaler_owned_native_paths(
    managed_bindings: &[ManagedAddonFile],
) -> HashSet<String> {
    managed_bindings
        .iter()
        .filter(|managed| managed.mode() == ManagedFileMode::Owned)
        .map(|managed| crate::paths::normalized_key(Path::new(managed.path().as_str())))
        .collect()
}

fn native_component_available_outside_managed_paths(
    components: &[GraphicsComponent],
    module_id: &str,
    owned_native_paths: &HashSet<String>,
) -> bool {
    let Some(spec) = native_module_spec(module_id) else {
        return false;
    };
    components.iter().any(|component| {
        component.technology() == spec.technology
            && (component.files().is_empty()
                || component.files().iter().any(|file| {
                    !owned_native_paths.contains(&crate::paths::normalized_key(Path::new(
                        file.path().as_str(),
                    )))
                }))
    })
}

pub(crate) fn native_sr_is_redundant(
    components: &[GraphicsComponent],
    managed_bindings: &[ManagedAddonFile],
) -> bool {
    native_component_available_outside_managed_paths(
        components,
        "nvidia_sr",
        &optiscaler_owned_native_paths(managed_bindings),
    )
}

/// Native NVIDIA component required by an optional OptiScaler module.
///
/// This is deliberately a technology binding, not a fixed root filename or a
/// second NVIDIA downloader. The graphics catalog already owns discovery,
/// package updates and Streamline bundle coherence; OptiScaler only claims the
/// exact detected files it consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeModuleSpec {
    pub(crate) technology: GraphicsTechnology,
}

pub(crate) fn native_module_spec(module_id: &str) -> Option<NativeModuleSpec> {
    match module_id {
        "nvidia_sr" => Some(NativeModuleSpec {
            technology: GraphicsTechnology::DlssSuperResolution,
        }),
        _ => None,
    }
}

pub(crate) fn native_module_component<'a>(
    components: &'a [GraphicsComponent],
    module_id: &str,
) -> Option<&'a GraphicsComponent> {
    let spec = native_module_spec(module_id)?;
    components
        .iter()
        .filter(|component| component.technology() == spec.technology)
        .filter(|component| {
            component
                .files()
                .iter()
                .all(native_component_file_is_usable)
        })
        .max_by(|left, right| {
            renderpilot_domain::component_version_report(left.files(), spec.technology)
                .known_version()
                .cmp(
                    &renderpilot_domain::component_version_report(right.files(), spec.technology)
                        .known_version(),
                )
                .then_with(|| left.id().as_str().cmp(right.id().as_str()))
        })
}

pub(crate) fn native_module_matches_path(module_id: &str, path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    match module_id {
        "nvidia_sr" => name.eq_ignore_ascii_case("nvngx_dlss.dll"),
        _ => false,
    }
}

pub(crate) fn native_component_available(
    components: &[GraphicsComponent],
    module_id: &str,
) -> bool {
    native_module_component(components, module_id).is_some()
}

fn native_component_file_is_usable(file: &ComponentFile) -> bool {
    let path = Path::new(file.path().as_str());
    path.is_file()
        && renderpilot_detection::analyze_executable(path).architecture() == Some(Architecture::X64)
}

/// Best coherent artifact supplied by the existing NVIDIA graphics-library
/// subsystem. A manifest artifact may still be remote (`manifest://` paths);
/// lifecycle preparation materializes it through `libraries::download_artifact`.
pub(crate) fn best_native_module_artifact(
    context: &Context,
    module_id: &str,
) -> Result<Option<LibraryArtifact>, ServiceError> {
    Ok(best_native_module_artifact_from(
        &native_artifact_universe(context)?,
        module_id,
    ))
}

pub(super) fn native_artifact_universe(
    context: &Context,
) -> Result<Vec<LibraryArtifact>, ServiceError> {
    let mut artifacts = context.storage().list_artifacts()?;
    let downloaded = artifacts
        .iter()
        .map(|artifact| artifact.id().clone())
        .collect::<HashSet<_>>();
    let manifest_artifacts = crate::libraries::catalog_packages_as_artifacts()?;
    artifacts.extend(
        manifest_artifacts
            .into_iter()
            .filter(|artifact| !downloaded.contains(artifact.id())),
    );
    Ok(artifacts)
}

pub(super) fn best_native_module_artifact_from(
    artifacts: &[LibraryArtifact],
    module_id: &str,
) -> Option<LibraryArtifact> {
    let spec = native_module_spec(module_id)?;
    artifacts
        .iter()
        .filter(|artifact| artifact.technology() == spec.technology)
        .filter(|artifact| {
            artifact.source() == Some("manifest-download") || local_artifact_is_usable(artifact)
        })
        .max_by(|left, right| {
            left.version()
                .cmp(&right.version())
                .then_with(|| left.id().as_str().cmp(right.id().as_str()))
        })
        .cloned()
}

fn local_artifact_is_usable(artifact: &LibraryArtifact) -> bool {
    artifact.files().iter().all(|file| {
        let path = Path::new(file.path().as_str());
        path.is_file()
            && super::super::identity::matches_optional(path, file.sha256())
            && renderpilot_detection::analyze_executable(path).architecture()
                == Some(Architecture::X64)
    })
}

pub(crate) fn reconcile_modules_for_release(
    manifest: &OptiScalerManifest,
    release_id: &str,
    installed: &[String],
) -> Result<HashSet<String>, ServiceError> {
    let release = manifest
        .releases
        .iter()
        .find(|release| release.id == release_id)
        .ok_or_else(|| crate::failed(format!("unknown OptiScaler release {release_id}")))?;
    let available: HashSet<_> = release
        .members
        .iter()
        .map(|member| member.module.as_str())
        .collect();
    let requested = installed
        .iter()
        .filter(|id| {
            manifest.modules.iter().any(|module| module.id == **id)
                && (available.contains(id.as_str())
                    || module_artifact_for_release(manifest, release, id).is_some()
                    || native_module_spec(id).is_some())
        })
        .cloned()
        .collect::<Vec<_>>();
    validate_module_selection(manifest, release_id, &requested)
}

pub(crate) fn module_artifact_for_release<'a>(
    manifest: &'a OptiScalerManifest,
    release: &OptiScalerRelease,
    module_id: &str,
) -> Option<&'a super::super::types::OptiScalerModuleArtifact> {
    if release
        .members
        .iter()
        .any(|member| member.module == module_id)
    {
        return None;
    }
    manifest
        .modules
        .iter()
        .find(|module| module.id == module_id)?
        .artifact
        .as_ref()
}

fn module_available_for_release(
    manifest: &OptiScalerManifest,
    release_id: &str,
    module_id: &str,
) -> bool {
    manifest
        .releases
        .iter()
        .find(|release| release.id == release_id)
        .is_some_and(|release| {
            release
                .members
                .iter()
                .any(|member| member.module == module_id)
                || module_artifact_for_release(manifest, release, module_id).is_some()
                || native_module_spec(module_id).is_some()
        })
}

fn close_dependencies(manifest: &OptiScalerManifest, selected: &mut HashSet<String>) {
    loop {
        let before = selected.len();
        for module in &manifest.modules {
            if selected.contains(&module.id) {
                selected.extend(module.requires.iter().cloned());
            }
        }
        if selected.len() == before {
            break;
        }
    }
}
