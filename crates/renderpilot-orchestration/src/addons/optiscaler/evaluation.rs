//! Internal compatibility evaluation, independent from serialized DTOs.

use std::path::PathBuf;

use renderpilot_domain::{
    GameId, GraphicsApi, Launcher, OptiScalerInstallState, OptiScalerPrerequisiteBinding,
    Sha256Hash,
};

use super::types::{
    ManagedIniValue, OptiScalerAvailability, OptiScalerCompatibility,
    OptiScalerCompatibilityBlockCode, OptiScalerInstallSummary, OptiScalerModuleAvailability,
    OptiScalerModuleProvisioning, OptiScalerPrerequisiteState, OptiScalerProxyPlan,
    OptiScalerRelocationBlockCode,
};

/// Filesystem-ready proxy decision plus the evidence required to apply it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvaluatedProxyPlan {
    pub(crate) slot: PathBuf,
    pub(crate) chain_reshade: bool,
    pub(crate) downstream_path: Option<PathBuf>,
    pub(crate) conflict: Option<String>,
    pub(crate) reshade_source_path: Option<PathBuf>,
    pub(crate) reshade_source_sha256: Option<Sha256Hash>,
}

impl EvaluatedProxyPlan {
    fn into_wire(self) -> OptiScalerProxyPlan {
        OptiScalerProxyPlan {
            slot: display_path(self.slot),
            chain_reshade: self.chain_reshade,
            downstream_path: self.downstream_path.map(display_path),
            conflict: self.conflict,
        }
    }
}

/// Internal module projection assembled before the public response exists.
#[derive(Debug, Clone)]
pub(crate) struct EvaluatedModule {
    pub(crate) id: String,
    pub(crate) selected: bool,
    pub(crate) optional: bool,
    pub(crate) available: bool,
    pub(crate) provisioning: OptiScalerModuleProvisioning,
    pub(crate) requires: Vec<String>,
    pub(crate) conflicts: Vec<String>,
    pub(crate) description: String,
}

impl From<EvaluatedModule> for OptiScalerModuleAvailability {
    fn from(module: EvaluatedModule) -> Self {
        Self {
            id: module.id,
            selected: module.selected,
            optional: module.optional,
            available: module.available,
            provisioning: module.provisioning,
            requires: module.requires,
            conflicts: module.conflicts,
            description: module.description,
        }
    }
}

/// Evaluation of an explicit relocation candidate.
#[derive(Debug, Clone)]
pub(crate) struct EvaluatedRelocation {
    pub(crate) target_exe: PathBuf,
    pub(crate) blocked_reason: Option<String>,
    pub(crate) block_code: Option<OptiScalerRelocationBlockCode>,
}

/// Complete lifecycle evaluation. Conversion to the public DTO is deliberately
/// deferred until compatibility, modules, proxy planning, and drift are done.
#[derive(Debug, Clone)]
pub(crate) struct EvaluatedAvailability {
    pub(crate) game_id: GameId,
    pub(crate) launcher: Launcher,
    pub(crate) blocked_reason: Option<String>,
    pub(crate) compatibility_block_code: Option<OptiScalerCompatibilityBlockCode>,
    pub(crate) detected_apis: Vec<GraphicsApi>,
    pub(crate) accepted_prerequisite_binding: OptiScalerPrerequisiteBinding,
    pub(crate) compatibility: OptiScalerCompatibility,
    pub(crate) prerequisite: OptiScalerPrerequisiteState,
    pub(crate) selected_release: Option<String>,
    pub(crate) target_exe: Option<PathBuf>,
    pub(crate) relocation: Option<EvaluatedRelocation>,
    pub(crate) relocation_policy_block_code: Option<OptiScalerRelocationBlockCode>,
    pub(crate) target_dir: Option<PathBuf>,
    pub(crate) proxy: EvaluatedProxyPlan,
    pub(crate) modules: Vec<EvaluatedModule>,
    pub(crate) install_state: Option<OptiScalerInstallState>,
    pub(crate) drifted_paths: Vec<PathBuf>,
    pub(crate) update_available: bool,
    pub(crate) repair_required: bool,
    pub(crate) unmanaged: bool,
    pub(crate) maintenance_available: bool,
    pub(crate) maintenance_block_code: Option<OptiScalerCompatibilityBlockCode>,
    pub(crate) managed_ini_overrides: Vec<ManagedIniValue>,
}

impl EvaluatedAvailability {
    pub(crate) fn into_wire(self) -> OptiScalerAvailability {
        let (relocation_target_exe, relocation_blocked_reason, relocation_block_code) =
            match self.relocation {
                Some(candidate) => (
                    Some(display_path(candidate.target_exe)),
                    candidate.blocked_reason,
                    candidate.block_code,
                ),
                None => (None, None, None),
            };
        OptiScalerAvailability {
            game_id: self.game_id.as_str().to_owned(),
            launcher: self.launcher,
            available: self.blocked_reason.is_none(),
            blocked_reason: self.blocked_reason,
            compatibility_block_code: self.compatibility_block_code,
            detected_apis: self.detected_apis,
            compatibility: self.compatibility,
            prerequisite: self.prerequisite,
            selected_release: self.selected_release,
            target_exe: self.target_exe.map(display_path),
            relocation_target_exe,
            relocation_blocked_reason,
            relocation_block_code,
            target_dir: self.target_dir.map(display_path),
            proxy: self.proxy.into_wire(),
            modules: self.modules.into_iter().map(Into::into).collect(),
            install_state: self
                .install_state
                .as_ref()
                .map(OptiScalerInstallSummary::from),
            drifted_paths: self.drifted_paths.into_iter().map(display_path).collect(),
            update_available: self.update_available,
            repair_required: self.repair_required,
            unmanaged: self.unmanaged,
            maintenance_available: self.maintenance_available,
            maintenance_block_code: self.maintenance_block_code,
        }
    }
}

fn display_path(path: PathBuf) -> String {
    path.into_os_string().to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addons::optiscaler::types::OptiScalerCompatibilityStatus;
    use renderpilot_domain::GameId;

    #[test]
    fn public_availability_is_built_without_lifecycle_only_fields() {
        let evaluation = EvaluatedAvailability {
            game_id: GameId::new("manual:evaluation-wire").expect("game id"),
            launcher: Launcher::Manual,
            blocked_reason: None,
            compatibility_block_code: None,
            detected_apis: Vec::new(),
            accepted_prerequisite_binding: OptiScalerPrerequisiteBinding::None,
            compatibility: OptiScalerCompatibility {
                status: OptiScalerCompatibilityStatus::Untested,
                declared_inputs: Vec::new(),
                launch: None,
                guidance: Vec::new(),
            },
            prerequisite: OptiScalerPrerequisiteState::None,
            selected_release: Some("stable".to_owned()),
            target_exe: Some(PathBuf::from("C:/Game/game.exe")),
            relocation: None,
            relocation_policy_block_code: None,
            target_dir: Some(PathBuf::from("C:/Game")),
            proxy: EvaluatedProxyPlan {
                slot: PathBuf::from("C:/Game/dxgi.dll"),
                chain_reshade: false,
                downstream_path: None,
                conflict: None,
                reshade_source_path: Some(PathBuf::from("C:/private/source.dll")),
                reshade_source_sha256: Some(Sha256Hash::new("a".repeat(64)).expect("hash")),
            },
            modules: Vec::new(),
            install_state: None,
            drifted_paths: Vec::new(),
            update_available: false,
            repair_required: false,
            unmanaged: false,
            maintenance_available: false,
            maintenance_block_code: None,
            managed_ini_overrides: vec![ManagedIniValue {
                section: "Private".to_owned(),
                key: "Internal".to_owned(),
                value: "secret".to_owned(),
            }],
        };

        let json = serde_json::to_value(evaluation.into_wire()).expect("serialize");
        assert!(json.get("managed_ini_overrides").is_none());
        assert!(json["proxy"].get("reshade_source_path").is_none());
        assert!(json["proxy"].get("reshade_source_sha256").is_none());
        assert!(!json.to_string().contains("C:/private/source.dll"));
    }
}
