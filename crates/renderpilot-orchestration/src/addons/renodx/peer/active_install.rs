//! Pure active-install composition for RenoDX.
//!
//! The caller seals phase-one and phase-three observations around the prepare
//! window.  This composer consumes those observations and the move-only,
//! already verified [`PreparedInstall`] without opening a context, touching the
//! filesystem, consulting storage, or re-reading an endpoint.

use renderpilot_domain::{
    InstalledAddon, PathRef, PlannedGameProxyTopology, RenoDxReshadeIniAuthority,
};

use crate::addons::renodx::install::PreparedInstall;
use crate::addons::renodx::peer::InstallActiveSnapshot;
use crate::addons::renodx::peer::config::{RenoDxConfigError, lower_config};
use crate::addons::renodx::peer::effects::{
    RenoDxPeerEffectAccumulator, RenoDxPeerEffectError, RenoDxPeerEffectGroup,
};
use crate::addons::renodx::peer::host::{
    RenoDxHostClassification, RenoDxHostError, classify_host, lower_owned_host,
};
use crate::addons::renodx::peer::record::{RenoDxRecordError, build_record};
use crate::addons::reshade::proxy::HostKind;
use crate::addons::shared_vulkan_mutation::FileIntent;
use crate::peer_mutation_executor::{ExactEndpointProgram, PeerPathSnapshot};

/// Move-only input crossing the prepare window.
///
/// `phase1` and `phase3` are both sealed snapshots.  The composer proves that
/// phase-three evidence is exactly the phase-one plan before lowering any
/// endpoint.  `owned` contains no handles or context, only prepared bytes and
/// their immutable upstream metadata.
#[derive(Debug)]
pub(crate) struct PreparedActiveInstall {
    pub(crate) phase1: InstallActiveSnapshot,
    pub(crate) owned: PreparedInstall,
    pub(crate) phase3: InstallActiveSnapshot,
}

impl PreparedActiveInstall {
    pub(crate) fn new(
        phase1: InstallActiveSnapshot,
        owned: PreparedInstall,
        phase3: InstallActiveSnapshot,
    ) -> Self {
        Self {
            phase1,
            owned,
            phase3,
        }
    }
}

/// Complete pure output for an active RenoDX install.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ActiveInstallComposition {
    record: InstalledAddon,
    program: ExactEndpointProgram,
    payloads: Vec<Option<Vec<u8>>>,
    game_intents: Vec<FileIntent>,
    planned_topology: Option<PlannedGameProxyTopology>,
    reshade_ini_authority: Option<RenoDxReshadeIniAuthority>,
}

type ActiveInstallCompositionParts = (
    InstalledAddon,
    ExactEndpointProgram,
    Vec<Option<Vec<u8>>>,
    Vec<FileIntent>,
    Option<PlannedGameProxyTopology>,
    Option<RenoDxReshadeIniAuthority>,
);

impl ActiveInstallComposition {
    pub(crate) fn into_parts(self) -> ActiveInstallCompositionParts {
        (
            self.record,
            self.program,
            self.payloads,
            self.game_intents,
            self.planned_topology,
            self.reshade_ini_authority,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RenoDxActiveInstallError {
    Retry(crate::ServiceError),
    InvalidInput(&'static str),
    Path(PathRef),
    Host(RenoDxHostError),
    Config(RenoDxConfigError),
    Effects(RenoDxPeerEffectError),
    Record(RenoDxRecordError),
}

impl std::fmt::Display for RenoDxActiveInstallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Retry(error) => error.fmt(formatter),
            Self::InvalidInput(reason) => {
                write!(formatter, "invalid RenoDX active install: {reason}")
            }
            Self::Path(path) => write!(formatter, "invalid RenoDX active-install path: {path}"),
            Self::Host(error) => error.fmt(formatter),
            Self::Config(error) => error.fmt(formatter),
            Self::Effects(error) => error.fmt(formatter),
            Self::Record(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RenoDxActiveInstallError {}

impl From<RenoDxHostError> for RenoDxActiveInstallError {
    fn from(error: RenoDxHostError) -> Self {
        Self::Host(error)
    }
}

impl From<RenoDxConfigError> for RenoDxActiveInstallError {
    fn from(error: RenoDxConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<RenoDxPeerEffectError> for RenoDxActiveInstallError {
    fn from(error: RenoDxPeerEffectError) -> Self {
        Self::Effects(error)
    }
}

impl From<RenoDxRecordError> for RenoDxActiveInstallError {
    fn from(error: RenoDxRecordError) -> Self {
        Self::Record(error)
    }
}

/// Composes one active RenoDX install from sealed evidence and prepared bytes.
pub(crate) fn compose_active_install(
    input: PreparedActiveInstall,
) -> Result<ActiveInstallComposition, RenoDxActiveInstallError> {
    let PreparedActiveInstall {
        phase1,
        owned,
        phase3,
    } = input;
    phase1
        .ensure_phase3_matches(&phase3)
        .map_err(RenoDxActiveInstallError::Retry)?;

    let sealed_topology = phase3
        .topology()
        .ok_or(RenoDxActiveInstallError::InvalidInput(
            "active install is missing sealed topology",
        ))?;
    validate_prepared_shape(&phase3, sealed_topology, &owned)?;

    let mut accumulator = RenoDxPeerEffectAccumulator::new();
    lower_addon(&phase3, &owned, &mut accumulator)?;

    let (host_binding, planned_topology) = match owned.host_kind {
        HostKind::Proxy => {
            let classification = classify_host(&phase3, &owned)?;
            let binding = classification.binding().clone();
            let planned = classification.planned_topology().clone();
            if let RenoDxHostClassification::Owned(plan) = classification {
                lower_owned_host(
                    plan,
                    phase3
                        .host_preimage()
                        .ok_or(RenoDxActiveInstallError::InvalidInput(
                            "owned proxy host is missing its sealed preimage",
                        ))?,
                    phase3.acquisition_sidecar_preimage(),
                    &mut accumulator,
                )?;
            }
            (Some(binding), Some(planned))
        }
        HostKind::Vulkan => (
            None,
            Some(PlannedGameProxyTopology::Exact(sealed_topology.clone())),
        ),
    };

    let config = lower_config(&phase3, &owned, &mut accumulator)?;
    let (program, payloads, game_intents) = accumulator.finalize()?.into_parts();
    let record = build_record(
        &phase3,
        &owned,
        host_binding.as_ref(),
        config.created(),
        config.receipt(),
    )?;

    Ok(ActiveInstallComposition {
        record,
        program,
        payloads,
        game_intents,
        planned_topology,
        reshade_ini_authority: config.authority().cloned(),
    })
}

fn validate_prepared_shape(
    snapshot: &InstallActiveSnapshot,
    topology: &renderpilot_domain::GameProxyTopology,
    prepared: &PreparedInstall,
) -> Result<(), RenoDxActiveInstallError> {
    if !snapshot.feature().is_main_install() {
        return Err(RenoDxActiveInstallError::InvalidInput(
            "active composer accepts only a main RenoDX install feature",
        ));
    }
    if prepared
        .reshade_channel
        .is_some_and(|channel| channel != snapshot.requested_channel())
    {
        return Err(RenoDxActiveInstallError::InvalidInput(
            "prepared ReShade channel differs from the sealed requested channel",
        ));
    }
    if prepared.addon_bytes.is_empty() {
        return Err(RenoDxActiveInstallError::InvalidInput(
            "prepared RenoDX add-on bytes are empty",
        ));
    }
    if prepared.game_id != topology.game_id {
        return Err(RenoDxActiveInstallError::InvalidInput(
            "prepared game differs from the sealed active topology",
        ));
    }
    let expected_name = prepared.addon_file_name.trim();
    if expected_name.is_empty()
        || !snapshot
            .payload_path()
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(expected_name))
    {
        return Err(RenoDxActiveInstallError::Path(
            snapshot.payload_path().clone(),
        ));
    }

    match prepared.host_kind {
        HostKind::Proxy => {
            if snapshot.topology().is_none()
                || snapshot.host_path().is_none()
                || snapshot.host_preimage().is_none()
                || snapshot.host_assessment().is_none()
            {
                return Err(RenoDxActiveInstallError::InvalidInput(
                    "proxy install is missing sealed topology or host evidence",
                ));
            }
            if prepared.proxy_dll_name.trim().is_empty() {
                return Err(RenoDxActiveInstallError::InvalidInput(
                    "proxy install has no prepared proxy DLL name",
                ));
            }
            if snapshot.writes_host()
                && prepared.reshade_channel != Some(snapshot.requested_channel())
            {
                return Err(RenoDxActiveInstallError::InvalidInput(
                    "owned ReShade host is missing the sealed requested channel",
                ));
            }
        }
        HostKind::Vulkan => {
            if snapshot.host_path().is_some()
                || snapshot.host_preimage().is_some()
                || snapshot.host_assessment().is_some()
                || snapshot.acquisition_sidecar_preimage().is_some()
                || snapshot.writes_host()
            {
                return Err(RenoDxActiveInstallError::InvalidInput(
                    "Vulkan install carries proxy-only sealed host evidence",
                ));
            }
            if !prepared.proxy_dll_name.is_empty() || !prepared.reshade_dll_bytes.is_empty() {
                return Err(RenoDxActiveInstallError::InvalidInput(
                    "Vulkan install carries per-game ReShade host bytes",
                ));
            }
        }
    }
    Ok(())
}

fn lower_addon(
    snapshot: &InstallActiveSnapshot,
    prepared: &PreparedInstall,
    accumulator: &mut RenoDxPeerEffectAccumulator,
) -> Result<(), RenoDxActiveInstallError> {
    match snapshot.payload_preimage() {
        PeerPathSnapshot::Absent => accumulator.create(
            RenoDxPeerEffectGroup::Addon,
            snapshot.payload_path().clone(),
            prepared.addon_bytes.clone(),
        )?,
        PeerPathSnapshot::File(_) => {
            let before = snapshot.payload_preimage().file().ok_or(
                RenoDxActiveInstallError::InvalidInput(
                    "present add-on snapshot has no retained file metadata",
                ),
            )?;
            let before_bytes = snapshot.payload_preimage().bytes().ok_or(
                RenoDxActiveInstallError::InvalidInput(
                    "present add-on snapshot has no retained bytes",
                ),
            )?;
            accumulator.replace(
                RenoDxPeerEffectGroup::Addon,
                snapshot.payload_path().clone(),
                before,
                before_bytes.to_vec(),
                prepared.addon_bytes.clone(),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use renderpilot_domain::{
        FileReceipt, GameId, GameProxyTopology, PathRef, PlannedGameProxyTopology,
        ProxyImplementation, ProxyLink, ProxyRootPrestate, RenoDxReshadeIniFeature, Sha256Hash,
    };

    use super::*;
    use crate::addons::peer_lifecycle::PeerRoots;
    use crate::addons::renodx::peer::{
        InstallCommandVariant, RenoDxConfigSourceSeal, RenoDxRootSeal,
    };
    use crate::addons::reshade::types::{ReshadeChannel, ReshadeIniTweaks};

    fn hash(value: char) -> Sha256Hash {
        Sha256Hash::new(value.to_string().repeat(Sha256Hash::HEX_LENGTH)).expect("hash")
    }

    fn path(root: &Path, name: &str) -> PathRef {
        PathRef::new(root.join(name).to_string_lossy().into_owned()).expect("path")
    }

    fn topology(root: &Path, game_id: &GameId) -> GameProxyTopology {
        let root_slot = path(root, "dxgi.dll");
        GameProxyTopology {
            id: "optiscaler:renodx-active-vulkan-test".to_owned(),
            game_id: game_id.clone(),
            root_slot: root_slot.clone(),
            outer: ProxyLink {
                implementation: ProxyImplementation::OptiScaler,
                path: root_slot,
                receipt: FileReceipt::owned("outer", hash('c')).expect("receipt"),
            },
            downstream: None,
            downstream_origin: None,
            root_prestate: ProxyRootPrestate::Absent,
        }
    }

    fn snapshot(root: &Path, topology: Option<GameProxyTopology>) -> InstallActiveSnapshot {
        let root = crate::paths::canonicalize_existing(root).expect("canonical root");
        let root_ref = PathRef::new(root.to_string_lossy().into_owned()).expect("root ref");
        let ini_path = root.join("ReShade.ini");
        let executable = root.join("game.exe");
        InstallActiveSnapshot {
            variant: InstallCommandVariant::Catalog,
            feature: RenoDxReshadeIniFeature::Install,
            request_fingerprint: hash('a'),
            plan_fingerprint: hash('b'),
            requested_channel: ReshadeChannel::Stable,
            canonical_target_dir: root.clone(),
            canonical_target_dir_ref: root_ref.clone(),
            topology,
            root_seal: RenoDxRootSeal {
                canonical_game_root: root.clone(),
                canonical_game_root_ref: root_ref.clone(),
                config_source: RenoDxConfigSourceSeal::Absent {
                    exact_ini_path: ini_path.clone(),
                },
                effective_addon_root: root.clone(),
                payload_root: None,
                payload_root_ref: None,
                exact_ini_path: ini_path,
                exact_proxy_host: None,
                canonical_registered_exe: Some(executable.clone()),
                roots: PeerRoots::new(root, None).expect("roots"),
            },
            payload_path: path(&root_ref_to_path(&root_ref), "renodx.addon64"),
            payload_preimage: crate::peer_mutation_executor::PeerPathSnapshot::Absent,
            host_path: None,
            host_preimage: None,
            host_assessment: None,
            acquisition_sidecar_preimage: None,
            registered_executable: Some(
                PathRef::new(executable.to_string_lossy().into_owned()).expect("exe path"),
            ),
            writes_host: false,
            content: crate::addons::reshade::scan::ReshadeContent::Empty,
        }
    }

    fn root_ref_to_path(root: &PathRef) -> PathBuf {
        PathBuf::from(root.as_str())
    }

    fn prepared(game_id: GameId) -> PreparedInstall {
        PreparedInstall {
            game_id,
            host_kind: HostKind::Vulkan,
            processing_path: crate::addons::renodx::types::RenoDxProcessingPath::Unmanaged,
            renodx_config: None,
            proxy_dll_name: String::new(),
            addon_file_name: "renodx.addon64".to_owned(),
            addon_source_url: String::new(),
            source_digest: String::new(),
            source_etag: None,
            source_last_modified: None,
            addon_bytes: b"addon".to_vec(),
            reshade_dll_bytes: Vec::new(),
            reshade_source_url: String::new(),
            reshade_source_etag: None,
            reshade_last_modified: None,
            reshade_digest: String::new(),
            reshade_channel: None,
            ini_tweaks: ReshadeIniTweaks::default(),
        }
    }

    #[test]
    fn active_vulkan_preserves_exact_topology_without_proxy_host_endpoint() {
        let root = tempfile::tempdir().expect("root");
        let game_id = GameId::new("manual:renodx-vulkan-active-test").expect("game id");
        let expected = topology(root.path(), &game_id);
        let input = PreparedActiveInstall::new(
            snapshot(root.path(), Some(expected.clone())),
            prepared(game_id),
            snapshot(root.path(), Some(expected.clone())),
        );

        let composition = compose_active_install(input).expect("active Vulkan composition");

        assert_eq!(
            composition.planned_topology,
            Some(PlannedGameProxyTopology::Exact(expected))
        );
        assert_eq!(composition.program.endpoints().len(), 1);
        assert!(
            composition
                .program
                .endpoints()
                .iter()
                .all(|endpoint| endpoint.role()
                    != renderpilot_domain::PeerEndpointRole::TopologyDownstream)
        );
        assert_eq!(composition.game_intents.len(), 1);
        assert_eq!(composition.game_intents[0].after, Some(b"addon".to_vec()));
    }

    #[test]
    fn active_vulkan_rejects_missing_sealed_topology() {
        let root = tempfile::tempdir().expect("root");
        let game_id = GameId::new("manual:renodx-vulkan-no-topology-test").expect("game id");
        let input = PreparedActiveInstall::new(
            snapshot(root.path(), None),
            prepared(game_id),
            snapshot(root.path(), None),
        );

        let error = compose_active_install(input).expect_err("missing topology must fail closed");

        assert!(matches!(
            error,
            RenoDxActiveInstallError::InvalidInput("active install is missing sealed topology")
        ));
    }
}
