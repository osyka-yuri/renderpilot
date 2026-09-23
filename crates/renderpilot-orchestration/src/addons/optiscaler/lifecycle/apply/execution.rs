use super::super::adoption;
use super::super::*;
use super::plan::FilesystemApplyPlan;
use super::runtime;

pub(super) struct ApplyExecution<'prepared, 'context, 'artifacts> {
    pub(super) prepared: &'prepared ApplyPlan<'context>,
    pub(super) plan: &'prepared FilesystemApplyPlan<'artifacts>,
}

pub(super) struct ExecutionOutcome {
    pub(super) state: OptiScalerInstallState,
}

pub(super) struct PreparedApplyOutcome {
    pub(super) state: OptiScalerInstallState,
    pub(super) topology: GameProxyTopology,
    pub(super) before_topology: Option<GameProxyTopology>,
    pub(super) peer_transition: Option<adoption::PeerReceiptTransition>,
    pub(super) auxiliary_preservations:
        Vec<renderpilot_storage_sqlite::OptiScalerAuxiliaryPreservation>,
    pub(super) operation: OptiScalerOperationResult,
}

impl ApplyExecution<'_, '_, '_> {
    pub(super) fn merge_config(&self) -> ConfigMergeResult {
        let ApplyPlan {
            manifest,
            old_state,
            old_managed_files,
            old_config_base,
            release,
            modules,
            target,
            compatibility_invariants,
            ..
        } = self.prepared;
        let old_state = old_state.as_ref();
        let old_release = old_state.and_then(|state| {
            manifest
                .releases
                .iter()
                .find(|candidate| candidate.id == state.release_id)
        });
        let old_config = old_config_base
            .as_deref()
            .unwrap_or(&self.plan.config.new_config);
        let invariants = managed_config_invariants(
            release,
            modules,
            &target.dir,
            &self.plan.layout.native_paths,
            target.proxy.chain_reshade,
            compatibility_invariants,
        );
        let applicable_migrations = old_state.map_or_else(Vec::new, |state| {
            manifest
                .config_migrations
                .iter()
                .filter(|migration| {
                    migration.from_schema == state.config_schema
                        && migration.to_schema == release.config_schema
                })
                .cloned()
                .collect::<Vec<_>>()
        });
        let mut merge = three_way_merge(
            old_config,
            &self.plan.config.current_config,
            &self.plan.config.new_config,
            &applicable_migrations,
            &invariants,
        );
        let mut removals = Vec::new();
        let old_native_paths = NativeModulePaths::from_bindings(old_managed_files);
        let old_target_dir = old_state.map_or(target.dir.as_path(), |state| {
            Path::new(state.target_dir.as_str())
        });
        if old_state.is_some_and(|state| state.modules.iter().any(|id| id == "nvidia_sr"))
            && !modules.contains("nvidia_sr")
        {
            let old_modules = old_state.map_or_else(HashSet::new, |state| {
                state.modules.iter().cloned().collect::<HashSet<_>>()
            });
            let old_value = old_release
                .map(|release| {
                    managed_config_invariants(
                        release,
                        &old_modules,
                        old_target_dir,
                        &old_native_paths,
                        target.proxy.chain_reshade,
                        &[],
                    )
                })
                .and_then(|values| {
                    values.into_iter().find(|value| {
                        value.section.eq_ignore_ascii_case("Libraries")
                            && value.key.eq_ignore_ascii_case("NvngxDlssPath")
                    })
                })
                .unwrap_or_else(|| ManagedIniValue {
                    section: "Libraries".to_owned(),
                    key: "NvngxDlssPath".to_owned(),
                    value: ".\\nvngx_dlss.dll".to_owned(),
                });
            removals.push(old_value);
        }
        if old_state.is_some_and(|state| state.modules.iter().any(|id| id == "nvidia_rr"))
            && !modules.contains("nvidia_rr")
            && let Some(feature_dir) = rr_feature_receipt_dir(old_managed_files)
        {
            removals.push(ManagedIniValue {
                section: "Libraries".to_owned(),
                key: "NvngxFeaturePath".to_owned(),
                value: ini_library_path(old_target_dir, &feature_dir),
            });
        }
        if old_state.is_some_and(|state| state.modules.iter().any(|id| id == "optipatcher"))
            && !modules.contains("optipatcher")
        {
            removals.push(ManagedIniValue {
                section: "Plugins".to_owned(),
                key: "LoadAsiPlugins".to_owned(),
                value: "true".to_owned(),
            });
        }
        if old_release.is_some_and(release_has_private_runtime)
            && !release_has_private_runtime(release)
        {
            removals.push(ManagedIniValue {
                section: "Libraries".to_owned(),
                key: "OptiDllPath".to_owned(),
                value: ".\\OptiScaler".to_owned(),
            });
        }
        merge.bytes = remove_managed_values(&merge.bytes, &removals);
        merge
    }

    pub(super) fn write_release_files(
        &self,
        merged_config: &[u8],
        mutation: &mut PreparedFileMutation<'_>,
        changed: &mut Vec<String>,
        retained_fsr_baselines: &HashMap<String, OptiScalerReleaseFileBaseline>,
    ) -> Result<
        (
            Vec<OptiScalerFileReceipt>,
            Option<renderpilot_domain::OptiScalerConfigurationBaseline>,
        ),
        ServiceError,
    > {
        let ApplyPlan {
            release,
            modules,
            artifacts,
            ..
        } = self.prepared;
        let archive = &artifacts.archive;
        let mut receipts = Vec::new();
        let mut fresh_configuration_baseline = None;
        for member in selected_members(release, modules) {
            let Some(path) = self
                .plan
                .layout
                .new_paths
                .get(&member.archive_path.to_ascii_lowercase())
            else {
                continue;
            };
            let bytes = if member.archive_path == self.plan.config.config_member_archive_path {
                merged_config
            } else {
                archive.bytes(&member.archive_path)?
            };
            let role = match member.target.as_str() {
                "$proxy" => None,
                "OptiScaler.ini" => Some(OptiScalerFileRole::Configuration),
                _ => Some(OptiScalerFileRole::Runtime),
            };
            // The proxy participant is published by the topology executor so
            // its receipt can be taken from the same applied journal token.
            let Some(role) = role else { continue };
            let prior = prior_release_receipt(self.prepared.old_state.as_ref(), path);
            let will_write = self
                .plan
                .layout
                .release_write_paths
                .contains(&crate::paths::normalized_key(path));
            let applied = if will_write {
                let applied = mutation.write_file(path, bytes)?;
                changed.push(path.to_string_lossy().into_owned());
                applied
            } else {
                mutation.verify_unchanged(path)?
            };
            if role == OptiScalerFileRole::Configuration && self.prepared.old_state.is_none() {
                fresh_configuration_baseline = Some(if will_write {
                    mutation.configuration_baseline_for(applied)?
                } else if let Some(receipt) = self.plan.config.current_receipt.as_ref() {
                    renderpilot_domain::OptiScalerConfigurationBaseline::present(
                        receipt.clone(),
                        self.plan.config.current_config.clone(),
                    )
                    .map_err(|error| failed(error.to_string()))?
                } else {
                    renderpilot_domain::OptiScalerConfigurationBaseline::Absent
                });
            }
            let ownership = if will_write {
                FileOwnership::Owned
            } else {
                prior.map_or(FileOwnership::Reused, |receipt| {
                    receipt.installed.ownership()
                })
            };
            let installed = if role == OptiScalerFileRole::Configuration
                && !will_write
                && prior
                    .is_some_and(|receipt| receipt.installed.ownership() == FileOwnership::Reused)
            {
                // A no-write adopted configuration keeps its durable
                // provenance receipt. The Verify ordinal is bound to the
                // independently observed live user file, which may have
                // changed since adoption.
                prior.expect("checked above").installed.clone()
            } else {
                mutation.receipt_for_ordinal(applied.ordinal(), ownership)?
            };
            let cleanup = if role == OptiScalerFileRole::Configuration {
                if self.plan.config.target_mode == super::plan::ConfigTargetMode::RetargetToAbsent {
                    OptiScalerFileCleanup::RemoveIfUnchanged
                } else {
                    let baseline = self
                        .prepared
                        .old_state
                        .as_ref()
                        .map(OptiScalerInstallState::configuration_baseline)
                        .or(fresh_configuration_baseline.as_ref())
                        .ok_or_else(|| {
                            failed("OptiScaler configuration baseline was not captured")
                        })?;
                    canonical_configuration_cleanup(baseline, &installed)?
                }
            } else {
                OptiScalerFileCleanup::RemoveIfUnchanged
            };
            let baseline = retained_fsr_baselines
                .get(&crate::paths::normalized_key(path))
                .cloned()
                .unwrap_or(OptiScalerReleaseFileBaseline::Absent);
            if !matches!(baseline, OptiScalerReleaseFileBaseline::Absent)
                && role != OptiScalerFileRole::Runtime
            {
                return Err(failed(format!(
                    "OptiScaler retained FSR original is bound to a non-runtime target: {}",
                    path.display()
                )));
            }
            receipts.push(OptiScalerFileReceipt {
                path: path_ref(path)?,
                installed,
                role,
                cleanup,
                baseline,
            });
        }
        receipts.sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
        Ok((receipts, fresh_configuration_baseline))
    }

    pub(super) fn materialize_managed_files(
        &self,
        mutation: &mut PreparedFileMutation<'_>,
        changed: &mut Vec<String>,
    ) -> Result<Vec<renderpilot_domain::OptiScalerModuleRuntimeBinding>, ServiceError> {
        let native_targets = &self.plan.layout.native_targets;
        let native_expected_sha256 = &self.plan.layout.native_expected_sha256;
        let artifact_targets = &self.plan.layout.artifact_targets;
        let runtime_plans = &self.plan.layout.runtime_plans;
        let mut managed_files = Vec::new();
        for target in native_targets {
            let key = crate::paths::normalized_key(&target.destination);
            let expected = native_expected_sha256
                .get(&key)
                .ok_or_else(|| {
                    failed(format!(
                        "native target hash is missing for {}",
                        target.destination.display()
                    ))
                })?
                .clone();
            let runtime_plan = runtime_plans.get(&key).ok_or_else(|| {
                failed(format!(
                    "runtime plan is missing for native module {} at {}",
                    target.module_id,
                    target.destination.display()
                ))
            })?;
            let applied = match runtime_plan.step() {
                runtime::RuntimeApplyStep::WriteOwnedAbsent => {
                    let applied = mutation.copy_file_from_verified(
                        &target.source,
                        &target.destination,
                        &expected,
                    )?;
                    changed.push(target.destination.to_string_lossy().into_owned());
                    applied
                }
                runtime::RuntimeApplyStep::VerifyOwnedAbsent { .. }
                | runtime::RuntimeApplyStep::VerifyReusedPresent { .. } => {
                    mutation.verify_unchanged(&target.destination)?
                }
            };
            let installed = mutation
                .receipt_for_ordinal(applied.ordinal(), runtime_plan.installed_ownership())?;
            if installed.digest() != &expected
                || runtime_plan
                    .exact_receipt()
                    .is_some_and(|receipt| receipt != &installed)
            {
                return Err(failed(format!(
                    "native OptiScaler module {} changed after planning: {}",
                    target.module_id,
                    target.destination.display()
                )));
            }
            managed_files.push(renderpilot_domain::OptiScalerModuleRuntimeBinding {
                module: runtime_plan.module().to_owned(),
                path: path_ref(runtime_plan.path())?,
                installed,
                baseline: runtime_plan.baseline(),
            });
            tracing::debug!(
                "bound native OptiScaler module {} from {}",
                target.module_id,
                target.source.display()
            );
        }
        for (artifact, destination) in artifact_targets {
            let key = crate::paths::normalized_key(destination);
            let runtime_plan = runtime_plans.get(&key).ok_or_else(|| {
                failed(format!(
                    "runtime plan is missing for downloaded module {} at {}",
                    artifact.module_id,
                    destination.display()
                ))
            })?;
            let applied = match runtime_plan.step() {
                runtime::RuntimeApplyStep::WriteOwnedAbsent => {
                    let applied = mutation.write_file(destination, &artifact.bytes)?;
                    changed.push(destination.to_string_lossy().into_owned());
                    applied
                }
                runtime::RuntimeApplyStep::VerifyOwnedAbsent { .. }
                | runtime::RuntimeApplyStep::VerifyReusedPresent { .. } => {
                    mutation.verify_unchanged(destination)?
                }
            };
            let installed = mutation
                .receipt_for_ordinal(applied.ordinal(), runtime_plan.installed_ownership())?;
            if installed.digest() != runtime_plan.expected()
                || runtime_plan
                    .exact_receipt()
                    .is_some_and(|receipt| receipt != &installed)
            {
                return Err(failed(format!(
                    "OptiScaler module artifact {} changed after planning: {}",
                    artifact.module_id,
                    destination.display()
                )));
            }
            managed_files.push(renderpilot_domain::OptiScalerModuleRuntimeBinding {
                module: runtime_plan.module().to_owned(),
                path: path_ref(runtime_plan.path())?,
                installed,
                baseline: runtime_plan.baseline(),
            });
        }
        Ok(managed_files)
    }

    pub(super) fn build_receipts(
        &self,
        mutation: &PreparedFileMutation<'_>,
        topology: &GameProxyTopology,
        release_files: Vec<OptiScalerFileReceipt>,
        mut managed_files: Vec<renderpilot_domain::OptiScalerModuleRuntimeBinding>,
        fresh_configuration_baseline: Option<renderpilot_domain::OptiScalerConfigurationBaseline>,
    ) -> Result<ExecutionOutcome, ServiceError> {
        let ApplyPlan {
            manifest,
            game_id,
            old_state,
            release,
            modules,
            artifacts: _,
            target,
            ..
        } = self.prepared;
        let mut module_list = modules.iter().cloned().collect::<Vec<_>>();
        module_list.sort();
        managed_files.sort_by(|left, right| left.module.cmp(&right.module));
        let parts = renderpilot_domain::OptiScalerInstallStateParts {
            game_id: game_id.clone(),
            release_id: release.id.clone(),
            manifest_revision: manifest.revision.clone(),
            target_exe_path: path_ref(&target.exe)?,
            target_dir: path_ref(&target.dir)?,
            modules: module_list,
            release_files,
            runtime_bindings: managed_files,
            directory_receipts: {
                let mut receipts = old_state
                    .as_ref()
                    .map(|state| {
                        state
                            .directory_receipts
                            .iter()
                            .filter(|receipt| {
                                crate::paths::is_within(
                                    Path::new(receipt.path.as_str()),
                                    &target.dir,
                                )
                            })
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                for created in mutation.created_directory_receipts(&target.dir)? {
                    // A managed directory can have disappeared between
                    // operations.  Recreating it deliberately yields a new
                    // native identity, which supersedes the stale persisted
                    // receipt rather than leaving that receipt in the next
                    // state.
                    if let Some(existing) = receipts.iter_mut().find(|existing| {
                        crate::paths::same_path(
                            Path::new(existing.path.as_str()),
                            Path::new(created.path.as_str()),
                        )
                    }) {
                        *existing = created;
                    } else {
                        receipts.push(created);
                    }
                }
                receipts.sort_by(|left, right| left.path.as_str().cmp(right.path.as_str()));
                receipts
            },
            source: Some(self.plan.release_source_url.clone()),
            archive_sha256: Some(
                Sha256Hash::new(release.archive_sha256.clone())
                    .map_err(|error| failed(format!("invalid release archive hash: {error}")))?,
            ),
            proxy_topology_id: Some(topology.id.clone()),
            config_schema: release.config_schema,
            config_base_release: release.id.clone(),
            adoption_state: OptiScalerAdoptionState::Managed,
            prerequisite_binding: old_state
                .as_ref()
                .map_or(self.prepared.accepted_prerequisite_binding, |state| {
                    state.prerequisite_binding
                }),
            created_at: None,
            updated_at: None,
        };
        let state = if let Some(previous) = old_state.as_ref() {
            let previous_configuration = previous
                .configuration_receipt()
                .map_err(|error| failed(error.to_string()))?;
            let candidate_configuration = parts
                .release_files
                .iter()
                .find(|receipt| receipt.role == OptiScalerFileRole::Configuration)
                .ok_or_else(|| failed("OptiScaler candidate has no configuration receipt"))?;
            if previous_configuration.installed.ownership() == FileOwnership::Reused
                && candidate_configuration.installed.ownership() == FileOwnership::Owned
            {
                if self.plan.config.target_mode == super::plan::ConfigTargetMode::RetargetToAbsent {
                    OptiScalerInstallState::from_existing_with_configuration_retarget(
                        previous, parts,
                    )
                } else {
                    let acquisition_baseline = match self.plan.config.current_receipt.as_ref() {
                        Some(current_receipt) => {
                            renderpilot_domain::OptiScalerConfigurationBaseline::present(
                                current_receipt.clone(),
                                self.plan.config.current_config.clone(),
                            )
                            .map_err(|error| failed(error.to_string()))?
                        }
                        None => previous.configuration_baseline().clone(),
                    };
                    OptiScalerInstallState::from_existing_with_configuration_acquisition(
                        previous,
                        parts,
                        acquisition_baseline,
                    )
                }
            } else {
                OptiScalerInstallState::from_existing(previous, parts)
            }
        } else {
            renderpilot_domain::from_new_install(
                parts,
                fresh_configuration_baseline.ok_or_else(|| {
                    failed("fresh OptiScaler install did not capture a configuration baseline")
                })?,
            )
        }
        .map_err(|error| failed(error.to_string()))?;
        Ok(ExecutionOutcome { state })
    }
}

fn canonical_configuration_cleanup(
    baseline: &renderpilot_domain::OptiScalerConfigurationBaseline,
    installed: &FileReceipt,
) -> Result<OptiScalerFileCleanup, ServiceError> {
    match (baseline, installed.ownership()) {
        (renderpilot_domain::OptiScalerConfigurationBaseline::Absent, FileOwnership::Owned) => {
            Ok(OptiScalerFileCleanup::RemoveIfUnchanged)
        }
        (
            renderpilot_domain::OptiScalerConfigurationBaseline::Present { receipt, .. },
            FileOwnership::Owned,
        ) if receipt.ownership() == FileOwnership::Reused => {
            Ok(OptiScalerFileCleanup::PreserveCurrentThenRestoreBaseline)
        }
        (
            renderpilot_domain::OptiScalerConfigurationBaseline::Present { receipt, .. },
            FileOwnership::Reused,
        ) if receipt == installed => Ok(OptiScalerFileCleanup::PreserveUnchanged),
        _ => Err(failed(
            "OptiScaler configuration receipt has no canonical lifecycle cleanup",
        )),
    }
}
