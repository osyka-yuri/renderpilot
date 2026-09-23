use super::*;

pub(in crate::addons::optiscaler) fn apply_release_off_runtime(
    prepared: &ApplyPlan<'_>,
) -> Result<OptiScalerOperationResult, ServiceError> {
    let started = Instant::now();
    let result = apply_release(prepared);
    tracing::debug!(
        "OptiScaler release apply {} for {} finished in {} ms",
        prepared.release.id,
        prepared.game_id.as_str(),
        started.elapsed().as_millis()
    );
    result
}

/// Concrete NVIDIA library locations bound from the existing graphics catalog.
/// Paths are operation inputs (and later recoverable from the generic receipt),
/// while module selection remains stable manifest IDs in typed state.
#[derive(Debug, Clone, Default)]
pub(crate) struct NativeModulePaths {
    pub(in crate::addons::optiscaler) dlss_sr: Option<PathBuf>,
}

impl NativeModulePaths {
    pub(in crate::addons::optiscaler) fn from_targets(targets: &[NativeTarget]) -> Self {
        let mut paths = Self::default();
        for target in targets {
            if target.module_id == "nvidia_sr" {
                paths.dlss_sr = Some(target.destination.clone());
            }
        }
        paths
    }

    pub(crate) fn from_bindings(bindings: &[ManagedAddonFile]) -> Self {
        let mut paths = Self::default();
        for managed in bindings {
            let path = Path::new(managed.path().as_str());
            if native_module_matches_path("nvidia_sr", path) {
                paths.dlss_sr = Some(path.to_path_buf());
            }
        }
        paths
    }
}

#[derive(Debug, Clone)]
pub(in crate::addons::optiscaler) struct NativeTarget {
    pub(in crate::addons::optiscaler) module_id: String,
    pub(in crate::addons::optiscaler) source: PathBuf,
    pub(in crate::addons::optiscaler) destination: PathBuf,
}

#[derive(Debug, Clone)]
pub(in crate::addons::optiscaler) struct PreparedNativeModule {
    pub(in crate::addons::optiscaler) module_id: String,
    pub(in crate::addons::optiscaler) artifact: LibraryArtifact,
}

pub(in crate::addons::optiscaler) async fn prepare_native_modules(
    context: &Context,
    game_id: &GameId,
    modules: &HashSet<String>,
    progress: Option<&ProgressObserver<'_>>,
) -> Result<Vec<PreparedNativeModule>, ServiceError> {
    let components = context.storage().list_components_for_game(game_id)?;
    let missing = modules
        .iter()
        .filter(|module| native_module_spec(module).is_some())
        .filter(|module| !matcher::native_component_available(&components, module))
        .cloned()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(Vec::new());
    }

    // This reuses the same signed/hash-verified NVIDIA library manifest and
    // downloader as the native component cards. OptiScaler never owns a second
    // source catalogue.
    crate::libraries::get_or_fetch_catalog().await?;
    let mut prepared = Vec::new();
    for module_id in missing {
        let candidate = best_native_module_artifact_off_runtime(context, &module_id)?
            .ok_or_else(|| failed(format!(
                "no coherent NVIDIA library artifact is available for OptiScaler module {module_id}"
            )))?;
        let artifact = if native_artifact_files_are_valid_off_runtime(candidate.clone()).await? {
            candidate
        } else {
            crate::libraries::download_artifact(
                context,
                candidate.id().as_str().to_owned(),
                progress,
            )
            .await?;
            context
                .storage()
                .list_artifacts()?
                .into_iter()
                .find(|artifact| artifact.id() == candidate.id())
                .ok_or_else(|| {
                    failed(format!(
                        "downloaded NVIDIA artifact {} was not registered",
                        candidate.id().as_str()
                    ))
                })?
        };
        if !native_artifact_files_are_valid_off_runtime(artifact.clone()).await? {
            return Err(failed(format!(
                "NVIDIA artifact {} failed local hash or x64 validation",
                artifact.id().as_str()
            )));
        }
        prepared.push(PreparedNativeModule {
            module_id,
            artifact,
        });
    }
    Ok(prepared)
}

pub(in crate::addons::optiscaler) fn best_native_module_artifact_off_runtime(
    context: &Context,
    module_id: &str,
) -> Result<Option<LibraryArtifact>, ServiceError> {
    matcher::best_native_module_artifact(context, module_id)
}

pub(in crate::addons::optiscaler) async fn native_artifact_files_are_valid_off_runtime(
    artifact: LibraryArtifact,
) -> Result<bool, ServiceError> {
    tokio::task::spawn_blocking(move || native_artifact_files_are_valid(&artifact))
        .await
        .map_err(|error| failed(format!("NVIDIA artifact validation task failed: {error}")))
}

pub(in crate::addons::optiscaler) fn native_artifact_files_are_valid(
    artifact: &LibraryArtifact,
) -> bool {
    !artifact.files().is_empty()
        && artifact.files().iter().all(|file| {
            let path = Path::new(file.path().as_str());
            path.is_file()
                && super::super::identity::matches_optional(path, file.sha256())
                && renderpilot_detection::analyze_executable(path).architecture()
                    == Some(renderpilot_domain::Architecture::X64)
        })
}

pub(in crate::addons::optiscaler) fn ensure_selected_modules_available(
    availability: &EvaluatedAvailability,
    modules: &HashSet<String>,
) -> Result<(), ServiceError> {
    for id in modules {
        let module = availability
            .modules
            .iter()
            .find(|module| module.id == *id)
            .ok_or_else(|| failed(format!("unknown OptiScaler module {id}")))?;
        if !module.available {
            return Err(failed(format!(
                "OptiScaler module {id} is unavailable for the selected release, game rule, or local dependency set"
            )));
        }
    }
    Ok(())
}
