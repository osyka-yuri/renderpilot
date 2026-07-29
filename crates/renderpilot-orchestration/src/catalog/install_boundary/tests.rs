use std::collections::BTreeSet;
use std::fs;
use std::sync::atomic::AtomicBool;

use crate::catalog::install_paths;
use tempfile::tempdir;

use super::*;

fn copy_pe(path: &Path) {
    fs::create_dir_all(path.parent().expect("parent")).expect("directory");
    let mut bytes = vec![0_u8; 0x84];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[0x3c..0x40].copy_from_slice(&0x80_u32.to_le_bytes());
    bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
    fs::write(path, bytes).expect("write synthetic PE");
}

#[test]
fn unreal_project_recommends_the_outer_distribution_root() {
    let temp = tempdir().expect("temp");
    let root = temp.path().join("Jedi Survivor");
    let project = root.join("SwGame");
    let binary_dir = project.join("Binaries/Win64");
    fs::create_dir_all(root.join("Engine/Binaries")).expect("engine");
    fs::create_dir_all(project.join("Content")).expect("content");
    copy_pe(&binary_dir.join("JediSurvivor.exe"));

    for selected in [&project, &binary_dir] {
        let assessment = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
            selected_root: selected,
            launcher_install_roots: &[],
            launcher_library_roots: &[],
            cancellation: None,
        });
        assert!(matches!(
            assessment.selected.kind,
            InstallBoundaryKind::EngineProjectSubtree | InstallBoundaryKind::BinarySubtree
        ));
        assert_eq!(
            assessment
                .recommendation
                .as_ref()
                .map(|recommendation| recommendation.root.as_path()),
            Some(root.as_path())
        );
        assert_eq!(
            assessment
                .recommendation
                .as_ref()
                .map(|recommendation| recommendation.source),
            Some(RootRecommendationSource::EngineDistributionRoot)
        );
    }

    let shared_engine = root.join("Engine/Binaries");
    let assessment = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
        selected_root: &shared_engine,
        launcher_install_roots: &[],
        launcher_library_roots: &[],
        cancellation: None,
    });
    assert_ne!(assessment.selected.kind, InstallBoundaryKind::SingleInstall);
    assert_eq!(
        assessment
            .recommendation
            .as_ref()
            .map(|recommendation| recommendation.root.as_path()),
        Some(root.as_path()),
        "the shared Engine/Binaries subtree must resolve to the distribution root"
    );

    let distribution = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
        selected_root: &root,
        launcher_install_roots: &[],
        launcher_library_roots: &[],
        cancellation: None,
    });
    assert_eq!(
        distribution.selected.kind,
        InstallBoundaryKind::SingleInstall
    );
    assert!(
        distribution.recommendation.is_none(),
        "a proven distribution root must not recommend its library parent",
    );
}

#[test]
fn root_executable_never_hides_multiple_child_installations() {
    let temp = tempdir().expect("temp");
    let root = temp.path().join("Games");
    copy_pe(&root.join("Play.exe"));
    copy_pe(&root.join("Game A/GameA.exe"));
    copy_pe(&root.join("Game B/GameB.exe"));

    let assessment = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
        selected_root: &root,
        launcher_install_roots: &[],
        launcher_library_roots: &[],
        cancellation: None,
    });
    assert_eq!(
        assessment.selected.kind,
        InstallBoundaryKind::MultipleInstallContainer
    );
}

#[test]
fn shared_intermediate_containers_do_not_merge_independent_installations() {
    let temp = tempdir().expect("temp");
    let root = temp.path().join("Library");
    let first = root.join("Bundle").join("Games").join("Game A");
    let second = root.join("Bundle").join("Games").join("Game B");
    copy_pe(&first.join("Bin").join("GameA.exe"));
    copy_pe(&second.join("Binaries").join("GameB.exe"));

    let assessment = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
        selected_root: &root,
        launcher_install_roots: &[],
        launcher_library_roots: &[],
        cancellation: None,
    });

    assert_eq!(
        assessment.selected.kind,
        InstallBoundaryKind::MultipleInstallContainer
    );
    let roots = assessment
        .selected
        .candidate_roots
        .iter()
        .map(|path| install_paths::install_path_match_key(&path.to_string_lossy()))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        roots,
        BTreeSet::from([
            install_paths::install_path_match_key(&first.to_string_lossy()),
            install_paths::install_path_match_key(&second.to_string_lossy()),
        ])
    );
}

#[test]
fn deep_container_executable_does_not_hide_multiple_child_installations() {
    let temp = tempdir().expect("temp");
    let root = temp.path().join("Library");
    let shared = root.join("Bundle").join("Games");
    copy_pe(&shared.join("Play.exe"));
    copy_pe(&shared.join("Game A").join("GameA.exe"));
    copy_pe(&shared.join("Game B").join("GameB.exe"));

    let assessment = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
        selected_root: &root,
        launcher_install_roots: &[],
        launcher_library_roots: &[],
        cancellation: None,
    });

    assert_eq!(
        assessment.selected.kind,
        InstallBoundaryKind::MultipleInstallContainer
    );
}

#[test]
fn one_nested_game_is_a_container_not_a_game_root() {
    let temp = tempdir().expect("temp");
    let root = temp.path().join("Games");
    copy_pe(&root.join("Only Game/Game.exe"));

    let assessment = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
        selected_root: &root,
        launcher_install_roots: &[],
        launcher_library_roots: &[],
        cancellation: None,
    });
    assert_eq!(
        assessment.selected.kind,
        InstallBoundaryKind::SingleInstallContainer
    );
}

#[test]
fn payload_inside_the_only_nested_game_does_not_promote_its_library_parent() {
    let temp = tempdir().expect("temp");
    let root = temp.path().join("Games");
    let game = root.join("Only Game");
    copy_pe(&game.join("Bin/Game.exe"));
    fs::create_dir_all(game.join("Data")).expect("data");
    fs::write(game.join("Data/content.pak"), b"package").expect("package");

    let assessment = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
        selected_root: &root,
        launcher_install_roots: &[],
        launcher_library_roots: &[],
        cancellation: None,
    });

    assert_eq!(
        assessment.selected.kind,
        InstallBoundaryKind::SingleInstallContainer
    );
    assert_eq!(assessment.selected.candidate_roots, vec![game]);
}

#[test]
fn a_root_dll_does_not_turn_one_nested_game_into_a_distribution_root() {
    let temp = tempdir().expect("temp");
    let root = temp.path().join("Games");
    copy_pe(&root.join("Only Game/Game.exe"));
    fs::write(root.join("shared.dll"), b"dll").expect("root dll");

    let assessment = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
        selected_root: &root,
        launcher_install_roots: &[],
        launcher_library_roots: &[],
        cancellation: None,
    });

    assert_eq!(
        assessment.selected.kind,
        InstallBoundaryKind::SingleInstallContainer
    );
}

#[test]
fn packaged_payload_can_establish_a_non_engine_distribution_root() {
    let temp = tempdir().expect("temp");
    let root = temp.path().join("Packaged Game");
    copy_pe(&root.join("Bin/Game.exe"));
    fs::create_dir_all(root.join("Data")).expect("data");
    fs::write(root.join("Data/content.pak"), b"package").expect("package");

    let assessment = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
        selected_root: &root,
        launcher_install_roots: &[],
        launcher_library_roots: &[],
        cancellation: None,
    });

    assert_eq!(assessment.selected.kind, InstallBoundaryKind::SingleInstall);
    assert!(assessment.recommendation.is_none());
}

#[test]
fn valid_game_root_never_recommends_a_neighbor_container() {
    let temp = tempdir().expect("temp");
    let games = temp.path().join("Games");
    let selected = games.join("The Last of Us Part I");
    copy_pe(&selected.join("tlou-i.exe"));
    copy_pe(&games.join("Another Game/other.exe"));

    let assessment = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
        selected_root: &selected,
        launcher_install_roots: &[],
        launcher_library_roots: &[],
        cancellation: None,
    });

    assert_eq!(assessment.selected.kind, InstallBoundaryKind::SingleInstall);
    assert!(assessment.recommendation.is_none());
}

#[test]
fn black_flag_nested_component_directories_are_one_installation() {
    let temp = tempdir().expect("temp");
    let root = temp.path().join("Assassins Creed IV Black Flag");
    copy_pe(&root.join("AC4BFSP.exe"));
    fs::create_dir_all(root.join("D3D12")).expect("D3D12");
    fs::create_dir_all(root.join("NVStreamline/production")).expect("Streamline");
    fs::write(root.join("D3D12/D3D12Core.dll"), b"dll").expect("D3D12 component");
    fs::write(root.join("NVStreamline/production/nvngx_dlss.dll"), b"dll").expect("DLSS component");

    let assessment = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
        selected_root: &root,
        launcher_install_roots: &[],
        launcher_library_roots: &[],
        cancellation: None,
    });

    assert_eq!(assessment.selected.kind, InstallBoundaryKind::SingleInstall);
    assert!(assessment.recommendation.is_none());
}

#[test]
fn recommendation_selection_compares_the_whole_chain_by_evidence_priority() {
    let root_executable = RootRecommendation {
        root: PathBuf::from("D:/Games/Example/Bin"),
        source: RootRecommendationSource::RootExecutable,
        completeness: BoundaryCompleteness::Complete,
        evidence: BTreeSet::from([InstallBoundaryEvidenceKind::RootExecutable]),
    };
    let engine_distribution = RootRecommendation {
        root: PathBuf::from("D:/Games/Example"),
        source: RootRecommendationSource::EngineDistributionRoot,
        completeness: BoundaryCompleteness::Complete,
        evidence: BTreeSet::from([InstallBoundaryEvidenceKind::EngineDistributionRoot]),
    };

    let selected =
        choose_best_recommendation(vec![(1, root_executable), (3, engine_distribution.clone())])
            .expect("recommendation");

    assert_eq!(selected.root, engine_distribution.root);
    assert_eq!(
        selected.source,
        RootRecommendationSource::EngineDistributionRoot
    );
}

#[test]
fn cancelled_boundary_inspection_is_never_authoritative() {
    let temp = tempdir().expect("temp");
    let root = temp.path().join("Cancelled Game");
    copy_pe(&root.join("Game.exe"));
    let cancellation = AtomicBool::new(true);

    let assessment = InstallBoundaryAnalyzer::inspect(InstallBoundaryRequest {
        selected_root: &root,
        launcher_install_roots: &[],
        launcher_library_roots: &[],
        cancellation: Some(&cancellation),
    });

    assert_eq!(assessment.selected.kind, InstallBoundaryKind::Incomplete);
    assert_eq!(
        assessment.selected.completeness,
        BoundaryCompleteness::Incomplete
    );
    assert!(assessment.recommendation.is_none());
}
