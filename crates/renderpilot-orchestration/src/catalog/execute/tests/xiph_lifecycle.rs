//! Cross-directory Xiph lifecycle with a proven empty external import set.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_application::{
    ArtifactRepository, ComponentRepository, OperationPlanRiskLevel, OperationPlanWarning,
    OperationRepository,
};
use renderpilot_domain::{
    Architecture, ArtifactId, ArtifactMetadata, ArtifactTrustLevel, ComponentFile,
    GameInstallation, LibraryArtifact, LibraryTechnology, RootAuthority, RuntimeTarget,
    Swappability,
};
use renderpilot_storage_sqlite::SqliteStorage;

use crate::Context;

use super::{
    apply_swap_with_current_safety, bak_of, observed_xiph_file, path_as_ref, sample_game_at,
    synthetic_xiph_pe, write,
};

#[test]
fn cross_directory_xiph_empty_import_proof_uses_plain_package() {
    let mut fixture = CrossDirectoryXiphFixture::setup();
    fixture.initial_normal_scan_and_register();
    fixture.assert_game_payload_intact();
    fixture.plan_and_apply();
    fixture.assert_post_apply_projection();
    fixture.assert_game_payload_intact();
    fixture.post_apply_normal_rescan();
    fixture.assert_game_payload_intact();
    fixture.rollback_and_assert();
    fixture.assert_game_payload_intact();
    fixture.post_rollback_normal_rescan();
    fixture.assert_game_payload_intact();
}

struct CrossDirectoryXiphFixture {
    _root: tempfile::TempDir,
    context: Context,
    game: GameInstallation,
    artifact: LibraryArtifact,
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
    new_wrapper_bytes: Vec<u8>,
    new_vorbis_bytes: Vec<u8>,
    new_ogg_bytes: Vec<u8>,
}

impl CrossDirectoryXiphFixture {
    fn setup() -> Self {
        let root = tempfile::tempdir().expect("root");
        let game_dir = root.path().join("game");
        let library_dir = root.path().join("library");
        let vorbis_dir = game_dir.join("Engine/Binaries/ThirdParty/Vorbis/Win64");
        let ogg_dir = game_dir.join("Engine/Binaries/ThirdParty/Ogg/Win64");
        fs::create_dir_all(&vorbis_dir).expect("Vorbis directory");
        fs::create_dir_all(&ogg_dir).expect("Ogg directory");
        fs::create_dir_all(&library_dir).expect("library directory");

        let game_payload = game_dir.join("Game.exe");
        let game_payload_bytes = synthetic_xiph_pe("RenderPilotTestGame", &[]);
        write(&game_payload, &game_payload_bytes);

        let old_wrapper = vorbis_dir.join("libvorbisfile_64.dll");
        let old_vorbis = vorbis_dir.join("libvorbis_64.dll");
        let old_ogg = ogg_dir.join("libogg_64.dll");
        let old_wrapper_bytes =
            synthetic_xiph_pe("ov_open", &["libvorbis_64.dll", "libogg_64.dll"]);
        let old_vorbis_bytes = synthetic_xiph_pe("vorbis_info_init", &["libogg_64.dll"]);
        let old_ogg_bytes = synthetic_xiph_pe("ogg_sync_init", &[]);
        write(&old_wrapper, &old_wrapper_bytes);
        write(&old_vorbis, &old_vorbis_bytes);
        write(&old_ogg, &old_ogg_bytes);

        let new_wrapper = library_dir.join("vorbisfile.dll");
        let new_vorbis = library_dir.join("vorbis.dll");
        let new_ogg = library_dir.join("ogg.dll");
        let new_wrapper_bytes = synthetic_xiph_pe("ov_open", &["vorbis.dll", "ogg.dll"]);
        let new_vorbis_bytes = synthetic_xiph_pe("vorbis_info_init", &["ogg.dll"]);
        let new_ogg_bytes = synthetic_xiph_pe("ogg_sync_init", &[]);
        write(&new_wrapper, &new_wrapper_bytes);
        write(&new_vorbis, &new_vorbis_bytes);
        write(&new_ogg, &new_ogg_bytes);

        let game = sample_game_at(&game_dir);
        let artifact = LibraryArtifact::new(
            ArtifactId::new("artifact:cross-directory-empty-proof").expect("artifact id"),
            LibraryTechnology::XiphVorbis,
            "vorbisfile.dll",
            vec![
                observed_xiph_file(&new_wrapper),
                observed_xiph_file(&new_vorbis),
                observed_xiph_file(&new_ogg),
            ],
            ArtifactTrustLevel::CatalogDownloaded,
        )
        .expect("Xiph artifact")
        .with_metadata(
            ArtifactMetadata::default().with_runtime_target(RuntimeTarget::new(Architecture::X64)),
        );

        Self {
            _root: root,
            context: Context::from_storage(SqliteStorage::in_memory().expect("storage")),
            game,
            artifact,
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
            new_wrapper_bytes,
            new_vorbis_bytes,
            new_ogg_bytes,
        }
    }

    fn initial_normal_scan_and_register(&mut self) {
        let initial_scan = crate::catalog::scan::scan_explicit_install(
            &self.context,
            self.game.install_path().as_str().into(),
            self.game.id().clone(),
            RootAuthority::UserConfirmed,
            None,
            crate::catalog::scan::ExplicitRootChange::Unchanged,
            &[],
        )
        .expect("normal scan must detect the vendor Xiph component");
        assert_eq!(initial_scan.game.id(), self.game.id());
        let detected = self
            .context
            .storage()
            .list_components_for_game(self.game.id())
            .expect("detected component");
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
        assert!(
            component_id.as_str().contains("layout-v2"),
            "initial component id must come from detector identity"
        );
        self.component_id = Some(component_id);
        self.context
            .storage()
            .upsert_artifact(&self.artifact)
            .expect("artifact");
    }

    fn component_id(&self) -> &renderpilot_domain::ComponentId {
        self.component_id
            .as_ref()
            .expect("initial scan component id")
    }

    fn assert_game_payload_intact(&self) {
        assert_eq!(
            fs::read(&self.game_payload).expect("game payload remains readable"),
            self.game_payload_bytes,
            "normal scans, apply, and rollback must preserve the game executable"
        );
    }

    fn plan_and_apply(&self) {
        let plan = crate::catalog::build_swap_plan(
            &self.context,
            self.game.id(),
            self.component_id(),
            self.artifact.id(),
        )
        .expect("empty external-import proof must not block preview");
        assert!(plan.plan.blockers().is_empty(), "plan: {:?}", plan.plan);
        assert_eq!(plan.plan.risk_level(), OperationPlanRiskLevel::High);
        assert!(
            plan.plan
                .warnings()
                .contains(&OperationPlanWarning::ConfirmationRequiredForSwappability),
            "BundleOnly must remain a warning requiring user review, not a plan blocker"
        );
        assert_eq!(
            plan.plan.files().len(),
            6,
            "three writes plus three archives"
        );

        let apply = apply_swap_with_current_safety(
            &self.context,
            self.game.id(),
            self.component_id(),
            self.artifact.id(),
        )
        .expect("normal apply must reprove and accept Proven(empty)");
        assert_eq!(apply.updated_file_count, 6);
    }

    fn assert_post_apply_projection(&self) {
        self.assert_canonical_active_files();
        self.assert_vendor_files_archived();
        self.assert_stored_active_projection();
        self.assert_stored_rollback_baseline();
    }

    fn assert_canonical_active_files(&self) {
        for (path, expected) in self.canonical_files() {
            assert_eq!(fs::read(&path).expect("canonical target"), expected);
            assert!(
                !bak_of(&path).exists(),
                "new canonical target must not get a sidecar: {}",
                path.display()
            );
        }
    }

    fn assert_vendor_files_archived(&self) {
        for (path, expected) in self.vendor_files() {
            assert!(
                !path.exists(),
                "vendor live path must be archived: {}",
                path.display()
            );
            assert_eq!(
                fs::read(bak_of(path)).expect("vendor baseline sidecar"),
                expected
            );
        }
    }

    fn assert_stored_active_projection(&self) {
        let stored = self
            .context
            .storage()
            .list_components_for_game(self.game.id())
            .expect("stored components");
        assert_eq!(stored.len(), 1);
        let stored_paths = stored[0]
            .files()
            .iter()
            .map(|file| file.path().as_str().to_owned())
            .collect::<BTreeSet<_>>();
        let expected_paths = self
            .canonical_files()
            .into_iter()
            .map(|(path, _)| path_ref_text(&path))
            .collect::<BTreeSet<_>>();
        assert_eq!(stored_paths, expected_paths);

        for (path, bytes) in self.canonical_files() {
            let expected_hash = renderpilot_detection::sha256_bytes(bytes).expect("active hash");
            assert_eq!(
                stored[0]
                    .files()
                    .iter()
                    .find(|file| file.path().as_str() == path_ref_text(&path))
                    .and_then(ComponentFile::sha256),
                Some(&expected_hash),
                "stored active projection must retain the candidate bytes for {}",
                path.display()
            );
        }
    }

    fn assert_stored_rollback_baseline(&self) {
        let baseline = self
            .context
            .storage()
            .get_component_backup(self.component_id())
            .expect("baseline query")
            .expect("first swap baseline");
        assert_eq!(baseline.files().len(), 3);
        assert_eq!(baseline.expected_active_files().len(), 3);
        let expected_active_paths = baseline
            .expected_active_files()
            .iter()
            .map(|file| file.path().as_str().to_owned())
            .collect::<BTreeSet<_>>();
        let canonical_paths = self
            .canonical_files()
            .into_iter()
            .map(|(path, _)| path_ref_text(&path))
            .collect::<BTreeSet<_>>();
        assert_eq!(expected_active_paths, canonical_paths);

        for (path, bytes) in self.vendor_files() {
            let expected_hash = renderpilot_detection::sha256_bytes(bytes).expect("baseline hash");
            assert_eq!(
                baseline
                    .files()
                    .iter()
                    .find(|file| file.path().as_str() == path_ref_text(path))
                    .and_then(ComponentFile::sha256)
                    .cloned(),
                Some(expected_hash)
            );
        }
    }

    fn canonical_files(&self) -> [(PathBuf, &[u8]); 3] {
        [
            (
                self.vorbis_dir.join("vorbisfile.dll"),
                self.new_wrapper_bytes.as_slice(),
            ),
            (
                self.vorbis_dir.join("vorbis.dll"),
                self.new_vorbis_bytes.as_slice(),
            ),
            (self.ogg_dir.join("ogg.dll"), self.new_ogg_bytes.as_slice()),
        ]
    }

    fn vendor_files(&self) -> [(&Path, &[u8]); 3] {
        [
            (&self.old_wrapper, self.old_wrapper_bytes.as_slice()),
            (&self.old_vorbis, self.old_vorbis_bytes.as_slice()),
            (&self.old_ogg, self.old_ogg_bytes.as_slice()),
        ]
    }

    fn post_apply_normal_rescan(&self) {
        let operation_count = self
            .context
            .storage()
            .list_operation_entries_for_game(self.game.id())
            .expect("post-apply operation history")
            .len();
        assert!(
            operation_count > 0,
            "apply must retain durable operation history"
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
        .expect("normal scan after apply must reconcile the stable component id");
        let post_apply = self
            .context
            .storage()
            .list_components_for_game(self.game.id())
            .expect("post-apply component");
        assert_eq!(post_apply.len(), 1);
        assert_eq!(post_apply[0].id(), self.component_id());
        assert_eq!(
            self.context
                .storage()
                .list_operation_entries_for_game(self.game.id())
                .expect("post-rescan operation history")
                .len(),
            operation_count,
            "normal rescan must not orphan operation history"
        );
        assert!(
            self.context
                .storage()
                .get_component_backup(self.component_id())
                .expect("post-apply baseline")
                .is_some()
        );
    }

    fn rollback_and_assert(&self) {
        super::rollback_component(&self.context, self.game.id(), self.component_id())
            .expect("rollback by stable id");
        for (path, expected) in [
            (&self.old_wrapper, self.old_wrapper_bytes.as_slice()),
            (&self.old_vorbis, self.old_vorbis_bytes.as_slice()),
            (&self.old_ogg, self.old_ogg_bytes.as_slice()),
        ] {
            assert_eq!(fs::read(path).expect("restored vendor bytes"), expected);
        }
        for path in [
            self.vorbis_dir.join("vorbisfile.dll"),
            self.vorbis_dir.join("vorbis.dll"),
            self.ogg_dir.join("ogg.dll"),
        ] {
            assert!(
                !path.exists(),
                "canonical path must be removed after rollback"
            );
        }
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
        .expect("normal scan after rollback must succeed");
        let post_rollback = self
            .context
            .storage()
            .list_components_for_game(self.game.id())
            .expect("post-rollback component");
        assert_eq!(post_rollback.len(), 1);
        assert_eq!(post_rollback[0].id(), self.component_id());
    }
}

fn path_ref_text(path: &Path) -> String {
    path_as_ref(path).as_str().to_owned()
}
