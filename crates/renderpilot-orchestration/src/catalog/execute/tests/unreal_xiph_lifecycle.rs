//! Detector, apply, and rollback tests for Unreal Xiph names using synthetic PE files.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::PathBuf;

use renderpilot_application::{
    ActiveCatalogPackage, ArtifactRepository, CandidateContext, ComponentRepository,
    InstalledReleaseState, OperationPlanRiskLevel, OperationPlanWarning,
    find_replacement_candidates,
};
use renderpilot_domain::{
    Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, GameInstallation,
    LibraryArtifact, LibraryTechnology, PackageRelease, PackageVersion, ReleaseChannel,
    RootAuthority, RuntimeTarget, Swappability,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::Context;

use super::{
    apply_swap_with_current_safety, bak_of, observed_xiph_file, path_as_ref, sample_game_at,
    synthetic_xiph_pe_with_delay, write,
};

#[test]
fn brothers_unreal_dual_alias_xiph_lifecycle_preserves_identity_and_originals() {
    let mut fixture = UnrealXiphFixture::setup();
    fixture.initial_normal_scan_and_register();

    let first =
        fixture.candidate_artifact("artifact:brothers-unreal-v1", "candidate-v1", 0xb1, true);
    fixture
        .context
        .storage()
        .upsert_artifact(&first)
        .expect("first Unreal catalog artifact");
    fixture.assert_candidate_is_visible_but_manual(&first);
    fixture.plan_and_apply(&first);
    fixture.assert_applied_and_original_backup(&first);
    fixture.assert_game_payload_intact();
    fixture.post_apply_normal_rescan(&first);

    let second =
        fixture.candidate_artifact("artifact:brothers-unreal-v2", "candidate-v2", 0xb2, true);
    fixture
        .context
        .storage()
        .upsert_artifact(&second)
        .expect("second Unreal catalog artifact");
    fixture.plan_and_apply(&second);
    fixture.assert_applied_and_original_backup(&second);
    fixture.assert_game_payload_intact();

    fixture.rollback_and_assert_originals();
    fixture.assert_game_payload_intact();
    fixture.post_rollback_normal_rescan();
}

#[test]
fn brothers_unreal_dual_alias_rejects_plain_candidates_without_writes() {
    let mut fixture = UnrealXiphFixture::setup();
    fixture.initial_normal_scan_and_register();
    let plain = fixture.candidate_artifact(
        "artifact:brothers-unreal-plain",
        "plain-candidate",
        0xc1,
        false,
    );
    fixture
        .context
        .storage()
        .upsert_artifact(&plain)
        .expect("plain Xiph candidate");

    let preview_error = crate::catalog::build_swap_plan(
        &fixture.context,
        fixture.game.id(),
        fixture.component_id(),
        plain.id(),
    )
    .err()
    .expect("plain names cannot satisfy external Unreal alias proof");
    assert!(
        preview_error.to_string().contains("alias"),
        "preview must report the missing exact external alias: {preview_error:?}"
    );
    let error = apply_swap_with_current_safety(
        &fixture.context,
        fixture.game.id(),
        fixture.component_id(),
        plain.id(),
    )
    .expect_err("plain package must not satisfy the external alias proof");
    assert!(
        !error.to_string().is_empty(),
        "a concrete compatibility error is returned"
    );
    fixture.assert_vendor_files_unchanged();
    fixture.assert_no_backups_or_encoder();
    fixture.assert_game_payload_intact();
}

struct UnrealXiphFixture {
    _root: tempfile::TempDir,
    context: Context,
    game: GameInstallation,
    component_id: Option<renderpilot_domain::ComponentId>,
    game_payload: PathBuf,
    game_payload_bytes: Vec<u8>,
    vorbis_dir: PathBuf,
    ogg_dir: PathBuf,
    old_wrapper: PathBuf,
    old_vorbis: PathBuf,
    old_ogg: PathBuf,
    old_wrapper_bytes: Vec<u8>,
    old_vorbis_bytes: Vec<u8>,
    old_ogg_bytes: Vec<u8>,
}

impl UnrealXiphFixture {
    fn setup() -> Self {
        let root = tempfile::tempdir().expect("root");
        let game_dir = root.path().join("game");
        let vorbis_dir = game_dir.join("Engine/Binaries/ThirdParty/Vorbis/Win64/VS2015");
        let ogg_dir = game_dir.join("Engine/Binaries/ThirdParty/Ogg/Win64/VS2015");
        let executable_dir = game_dir.join("Binaries/Win64");
        fs::create_dir_all(&vorbis_dir).expect("Unreal Vorbis directory");
        fs::create_dir_all(&ogg_dir).expect("Unreal Ogg directory");
        fs::create_dir_all(&executable_dir).expect("shipping executable directory");

        let game_payload = executable_dir.join("Brothers-Win64-Shipping.exe");
        let game_payload_bytes = synthetic_xiph_pe_with_delay(
            "RenderPilotBrothersFixture",
            &["libvorbis_64.dll"],
            &["libvorbisfile_64.dll"],
            0x91,
        );
        write(&game_payload, &game_payload_bytes);

        let old_wrapper = vorbis_dir.join("libvorbisfile_64.dll");
        let old_vorbis = vorbis_dir.join("libvorbis_64.dll");
        let old_ogg = ogg_dir.join("libogg_64.dll");
        let old_wrapper_bytes = synthetic_xiph_pe_with_delay(
            "ov_open",
            &["libvorbis_64.dll", "libogg_64.dll"],
            &[],
            0x81,
        );
        let old_vorbis_bytes =
            synthetic_xiph_pe_with_delay("vorbis_info_init", &["libogg_64.dll"], &[], 0x82);
        let old_ogg_bytes = synthetic_xiph_pe_with_delay("ogg_sync_init", &[], &[], 0x83);
        write(&old_wrapper, &old_wrapper_bytes);
        write(&old_vorbis, &old_vorbis_bytes);
        write(&old_ogg, &old_ogg_bytes);

        Self {
            _root: root,
            context: Context::from_storage(SqliteStorage::in_memory().expect("storage")),
            game: sample_game_at(&game_dir),
            component_id: None,
            game_payload,
            game_payload_bytes,
            vorbis_dir,
            ogg_dir,
            old_wrapper,
            old_vorbis,
            old_ogg,
            old_wrapper_bytes,
            old_vorbis_bytes,
            old_ogg_bytes,
        }
    }

    fn initial_normal_scan_and_register(&mut self) {
        let inspected_executable = renderpilot_detection::inspect_pe(&self.game_payload)
            .expect("inspect shipping executable")
            .compatibility_profile()
            .expect("shipping executable profile");
        let executable = inspected_executable
            .imports()
            .expect("complete shipping executable imports");
        assert_eq!(
            executable
                .regular
                .names()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["libvorbis_64.dll"]
        );
        assert_eq!(
            executable
                .delay
                .names()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["libvorbisfile_64.dll"]
        );

        crate::catalog::scan::scan_explicit_install(
            &self.context,
            self.game.install_path().as_str().into(),
            self.game.id().clone(),
            RootAuthority::UserConfirmed,
            None,
            crate::catalog::scan::ExplicitRootChange::Unchanged,
            &[],
        )
        .expect("normal scan must detect the Unreal Xiph component");
        let detected = self
            .context
            .storage()
            .list_components_for_game(self.game.id())
            .expect("detected Unreal Xiph component");
        assert_eq!(detected.len(), 1);
        assert_eq!(detected[0].technology(), LibraryTechnology::XiphVorbis);
        assert_eq!(detected[0].swappability(), Swappability::BundleOnly);
        assert_eq!(
            detected[0]
                .files()
                .iter()
                .map(|file| file.path().as_str().to_owned())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                path_as_ref(&self.old_wrapper).as_str().to_owned(),
                path_as_ref(&self.old_vorbis).as_str().to_owned(),
                path_as_ref(&self.old_ogg).as_str().to_owned(),
            ])
        );
        let component_id = detected[0].id().clone();
        assert!(component_id.as_str().contains("layout-v2"));
        self.component_id = Some(component_id);
    }

    fn candidate_artifact(
        &self,
        artifact_id: &str,
        directory_name: &str,
        marker: u8,
        unreal_names: bool,
    ) -> LibraryArtifact {
        let root = self.game.install_path();
        let candidate_dir = PathBuf::from(root.as_str())
            .parent()
            .expect("game parent")
            .join(directory_name);
        fs::create_dir_all(&candidate_dir).expect("candidate library directory");
        let names = if unreal_names {
            [
                "libvorbis_64.dll",
                "libvorbisfile_64.dll",
                "libvorbisenc_64.dll",
                "libogg_64.dll",
            ]
        } else {
            ["vorbis.dll", "vorbisfile.dll", "vorbisenc.dll", "ogg.dll"]
        };
        let dependency = |index: usize| names[index];
        let members: [(&str, &[&str]); 4] = [
            ("vorbis_info_init", &[dependency(3)]),
            ("ov_open", &[dependency(0), dependency(3)]),
            ("vorbis_encode_init", &[dependency(0)]),
            ("ogg_sync_init", &[]),
        ];
        let mut files = Vec::with_capacity(members.len());
        for (index, (export, imports)) in members.into_iter().enumerate() {
            let source = candidate_dir.join(names[index]);
            let bytes = synthetic_xiph_pe_with_delay(export, imports, &[], marker);
            write(&source, &bytes);
            files.push(observed_xiph_file(&source));
        }
        LibraryArtifact::new(
            ArtifactId::new(artifact_id).expect("artifact id"),
            LibraryTechnology::XiphVorbis,
            names[0],
            files,
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("Xiph artifact")
        .with_metadata(
            ArtifactMetadata::default().with_runtime_target(RuntimeTarget::new(Architecture::X64)),
        )
    }

    fn component_id(&self) -> &renderpilot_domain::ComponentId {
        self.component_id.as_ref().expect("registered component id")
    }

    fn assert_candidate_is_visible_but_manual(&self, artifact: &LibraryArtifact) {
        let groups = self.candidate_projection(std::slice::from_ref(artifact));
        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        assert_eq!(group.component_id(), self.component_id());
        assert!(
            group
                .candidates()
                .iter()
                .any(|candidate| candidate.artifact_id() == artifact.id()),
            "compatible Unreal package must be visible"
        );
        assert_eq!(
            group.automatic_candidate_artifact_id(),
            None,
            "external aliases keep the replacement manual-only"
        );
    }

    fn candidate_projection(
        &self,
        artifacts: &[LibraryArtifact],
    ) -> Vec<renderpilot_application::ComponentReplacementCandidates> {
        let active_catalog = artifacts
            .iter()
            .map(|artifact| {
                (
                    artifact.id().clone(),
                    ActiveCatalogPackage::new(
                        format!("package:{}", artifact.id()),
                        unreal_candidate_release(),
                        true,
                    ),
                )
            })
            .collect::<HashMap<_, _>>();
        let context = CandidateContext::new(HashSet::new(), active_catalog);
        let components = self
            .context
            .storage()
            .list_components_for_game(self.game.id())
            .expect("candidate projection components");
        find_replacement_candidates(&components, artifacts, &context)
    }

    fn plan_and_apply(&self, artifact: &LibraryArtifact) {
        let plan = crate::catalog::build_swap_plan(
            &self.context,
            self.game.id(),
            self.component_id(),
            artifact.id(),
        )
        .expect("Unreal _64 aliases must be compatible with the proven candidate");
        assert!(plan.plan.blockers().is_empty(), "plan: {:?}", plan.plan);
        assert_eq!(plan.plan.risk_level(), OperationPlanRiskLevel::High);
        assert!(
            plan.plan
                .warnings()
                .contains(&OperationPlanWarning::ConfirmationRequiredForSwappability),
            "BundleOnly remains a confirmation warning"
        );
        assert_eq!(
            plan.plan.files().len(),
            3,
            "only the three installed members are replacement targets"
        );

        let applied = apply_swap_with_current_safety(
            &self.context,
            self.game.id(),
            self.component_id(),
            artifact.id(),
        )
        .expect("reproved Unreal alias requirements must allow apply");
        assert_eq!(applied.updated_file_count, 3);
    }

    fn assert_vendor_files_unchanged(&self) {
        for (path, expected) in [
            (&self.old_wrapper, self.old_wrapper_bytes.as_slice()),
            (&self.old_vorbis, self.old_vorbis_bytes.as_slice()),
            (&self.old_ogg, self.old_ogg_bytes.as_slice()),
        ] {
            assert_eq!(fs::read(path).expect("original Xiph file"), expected);
        }
    }

    fn assert_no_backups_or_encoder(&self) {
        for path in [&self.old_wrapper, &self.old_vorbis, &self.old_ogg] {
            assert!(!bak_of(path).exists(), "no sidecar before successful apply");
        }
        assert!(!self.vorbis_dir.join("libvorbisenc_64.dll").exists());
        assert!(!self.ogg_dir.join("libvorbisenc_64.dll").exists());
    }

    fn assert_applied_and_original_backup(&self, artifact: &LibraryArtifact) {
        let encoder = self.vorbis_dir.join("libvorbisenc_64.dll");
        assert!(
            !encoder.exists(),
            "uninstalled package encoder must not be added"
        );
        for target in [&self.old_wrapper, &self.old_vorbis, &self.old_ogg] {
            let target_name = target
                .file_name()
                .and_then(|name| name.to_str())
                .expect("installed basename");
            let source = artifact
                .files()
                .iter()
                .find(|file| file.path().file_name() == Some(target_name))
                .expect("matching candidate member");
            assert_eq!(
                fs::read(target).expect("replaced installed member"),
                fs::read(source.path().as_str()).expect("candidate member bytes"),
                "installed {} receives its catalog candidate bytes",
                target.display()
            );
        }
        for path in [&self.old_wrapper, &self.old_vorbis, &self.old_ogg] {
            assert!(path.exists(), "existing _64 alias path stays in place");
            assert!(
                bak_of(path).exists(),
                "original bytes are protected by sidecar"
            );
        }
        for (path, expected) in [
            (&self.old_wrapper, self.old_wrapper_bytes.as_slice()),
            (&self.old_vorbis, self.old_vorbis_bytes.as_slice()),
            (&self.old_ogg, self.old_ogg_bytes.as_slice()),
        ] {
            assert_eq!(
                fs::read(bak_of(path)).expect("exact original sidecar"),
                expected
            );
        }
        let baseline = self
            .context
            .storage()
            .get_component_backup(self.component_id())
            .expect("rollback baseline query")
            .expect("original rollback baseline");
        assert_eq!(baseline.files().len(), 3);
        for (path, expected) in [
            (&self.old_wrapper, self.old_wrapper_bytes.as_slice()),
            (&self.old_vorbis, self.old_vorbis_bytes.as_slice()),
            (&self.old_ogg, self.old_ogg_bytes.as_slice()),
        ] {
            let record = baseline
                .files()
                .iter()
                .find(|file| file.path().as_str() == path_as_ref(path).as_str())
                .expect("original member in immutable baseline");
            assert_eq!(
                record.sha256(),
                Some(&renderpilot_detection::sha256_bytes(expected).expect("original hash"))
            );
        }
    }

    fn assert_game_payload_intact(&self) {
        assert_eq!(
            fs::read(&self.game_payload).expect("shipping executable"),
            self.game_payload_bytes
        );
    }

    fn post_apply_normal_rescan(&self, artifact: &LibraryArtifact) {
        crate::catalog::scan::scan_explicit_install(
            &self.context,
            self.game.install_path().as_str().into(),
            self.game.id().clone(),
            RootAuthority::UserConfirmed,
            None,
            crate::catalog::scan::ExplicitRootChange::Unchanged,
            &[],
        )
        .expect("normal scan after Unreal replacement");
        let components = self
            .context
            .storage()
            .list_components_for_game(self.game.id())
            .expect("post-apply components");
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].id(), self.component_id());
        let groups = self.candidate_projection(std::slice::from_ref(artifact));
        assert_eq!(
            groups.len(),
            1,
            "catalog-recognized install remains visible"
        );
        assert!(
            matches!(
                groups[0].installed_release(),
                InstalledReleaseState::Known {
                    catalog_release: Some(release),
                    ..
                } if release.version.as_str() == "1.3.7"
            ),
            "normal rescan recognizes the installed catalog release"
        );
        assert!(
            groups[0]
                .candidates()
                .iter()
                .all(|candidate| candidate.artifact_id() != artifact.id()),
            "the exact package already installed is omitted from replacements"
        );
    }

    fn rollback_and_assert_originals(&self) {
        super::rollback_component(&self.context, self.game.id(), self.component_id())
            .expect("rollback restores Unreal originals");
        self.assert_vendor_files_unchanged();
        for path in [&self.old_wrapper, &self.old_vorbis, &self.old_ogg] {
            assert!(
                !bak_of(path).exists(),
                "rollback consumes the original sidecar"
            );
        }
        assert!(!self.vorbis_dir.join("libvorbisenc_64.dll").exists());
    }

    fn post_rollback_normal_rescan(&self) {
        crate::catalog::scan::scan_explicit_install(
            &self.context,
            self.game.install_path().as_str().into(),
            self.game.id().clone(),
            RootAuthority::UserConfirmed,
            None,
            crate::catalog::scan::ExplicitRootChange::Unchanged,
            &[],
        )
        .expect("normal scan after Unreal rollback");
        let components = self
            .context
            .storage()
            .list_components_for_game(self.game.id())
            .expect("post-rollback component");
        assert_eq!(components.len(), 1);
        assert_eq!(components[0].id(), self.component_id());
    }
}

fn unreal_candidate_release() -> PackageRelease {
    PackageRelease {
        version: PackageVersion::parse("1.3.7").expect("Xiph release version"),
        channel: ReleaseChannel::Stable,
        label: Some("Xiph Unreal fixture".to_owned()),
        components: BTreeMap::from([
            (
                "ogg".to_owned(),
                PackageVersion::parse("1.3.6").expect("Ogg release version"),
            ),
            (
                "vorbis".to_owned(),
                PackageVersion::parse("1.3.7").expect("Vorbis release version"),
            ),
        ]),
    }
}
