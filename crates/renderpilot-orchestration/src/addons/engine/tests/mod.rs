use std::assert_matches;
use std::fs;
use std::path::{Path, PathBuf};

use renderpilot_domain::AddonKind;

use super::changes::UndoOutcome;
use super::helpers::{self, ensure_safe_relative_path};
use super::*;
use crate::ServiceError;
use tempfile::tempdir;

fn renodx_like_plan() -> InstallPlan {
    InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::Create {
                name: "renodx-cp2077.addon64".to_owned(),
                bytes: b"addon-bytes".to_vec(),
            },
            FileOp::BackupAndReplace {
                name: "dxgi.dll".to_owned(),
                bytes: b"reshade-dll".to_vec(),
            },
            FileOp::MergeText {
                name: "ReShade.ini".to_owned(),
                default: String::new(),
                strategy: MergeStrategy::IniSetKeys {
                    sections: vec![IniSection {
                        name: "ADDON".to_owned(),
                        keys: vec![
                            (
                                "DisabledAddons".to_owned(),
                                "Generic Depth,Effect Runtime Sync".to_owned(),
                            ),
                            ("AddonPath".to_owned(), ".".to_owned()),
                        ],
                    }],
                },
            },
            FileOp::Create {
                name: "renderpilot-renodx.json".to_owned(),
                bytes: b"{}".to_vec(),
            },
        ],
    }
}

fn read(path: &Path) -> Vec<u8> {
    fs::read(path).expect("file should exist")
}

fn receipt_paths(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn lays_down_every_op_and_round_trips_to_clean_folder() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    fs::write(game.join("game.exe"), b"game").expect("write");

    let receipt = install(game, &renodx_like_plan()).expect("install");

    assert_eq!(read(&game.join("renodx-cp2077.addon64")), b"addon-bytes");
    assert_eq!(read(&game.join("dxgi.dll")), b"reshade-dll");
    let ini = String::from_utf8(read(&game.join("ReShade.ini"))).unwrap();
    assert_eq!(
        ini,
        "[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\nAddonPath=.\r\n"
    );
    assert!(game.join("renderpilot-renodx.json").is_file());
    // addon + proxy + ini + marker, no backups (clean folder).
    assert_eq!(receipt.created_files.len(), 4);
    assert!(receipt.backed_up_files.is_empty());
    // The sentinel is gone after a clean install.
    assert!(!is_install_torn(game, AddonKind::RenoDx));

    uninstall(&receipt.created_files, &receipt.backed_up_files).expect("uninstall");
    assert!(!game.join("renodx-cp2077.addon64").exists());
    assert!(!game.join("dxgi.dll").exists());
    assert!(!game.join("ReShade.ini").exists());
    assert!(!game.join("renderpilot-renodx.json").exists());
    assert_eq!(read(&game.join("game.exe")), b"game");
}

#[test]
fn backs_up_and_restores_a_preexisting_file() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    fs::write(game.join("dxgi.dll"), b"game-shipped").expect("write");

    let receipt = install(game, &renodx_like_plan()).expect("install");
    assert_eq!(read(&game.join("dxgi.dll")), b"reshade-dll");
    assert_eq!(receipt_paths(&receipt.backed_up_files), vec!["dxgi.dll"]);
    assert!(
        game.join("dxgi.dll.bak").exists(),
        "committed BackupAndReplace must keep the on-disk bak for uninstall"
    );

    uninstall(&receipt.created_files, &receipt.backed_up_files).expect("uninstall");
    assert_eq!(read(&game.join("dxgi.dll")), b"game-shipped");
}

#[test]
fn place_file_refuses_when_a_backup_already_exists() {
    // A surviving `.bak` is the game's original (or torn-install debris).
    // Deleting it would permanently lose those bytes — refuse instead.
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    fs::write(game.join("dxgi.dll"), b"current").expect("write");
    fs::write(game.join("dxgi.dll.bak"), b"game-original").expect("write bak");

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::BackupAndReplace {
            name: "dxgi.dll".to_owned(),
            bytes: b"new-host".to_vec(),
        }],
    };
    let error = install(game, &plan).expect_err("must refuse clobbering an existing bak");
    assert!(
        matches!(error, ServiceError::InvalidInput(_)),
        "unexpected error: {error}"
    );
    assert_eq!(read(&game.join("dxgi.dll")), b"current");
    assert_eq!(read(&game.join("dxgi.dll.bak")), b"game-original");
}

#[test]
fn merge_text_resolves_a_foreign_config_case_insensitively() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    // A foreign config under a different casing must be found, merged, and backed
    // up — not duplicated under the conventional name.
    fs::write(game.join("reshade.ini"), "[GENERAL]\r\nPreset=mine.ini\r\n").expect("write");

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::MergeText {
            name: "ReShade.ini".to_owned(),
            default: String::new(),
            strategy: MergeStrategy::IniSetKeys {
                sections: vec![IniSection {
                    name: "ADDON".to_owned(),
                    keys: vec![("AddonPath".to_owned(), ".".to_owned())],
                }],
            },
        }],
    };
    let receipt = install(game, &plan).expect("install");

    // The merge targeted the existing lower-cased file (its `.bak` proves it),
    // not a fresh `ReShade.ini`, and preserved the foreign keys.
    let merged = String::from_utf8(read(&game.join("reshade.ini"))).unwrap();
    assert!(merged.contains("Preset=mine.ini"));
    assert!(merged.contains("AddonPath=."));
    assert_eq!(receipt_paths(&receipt.backed_up_files), vec!["reshade.ini"]);
    assert!(game.join("reshade.ini.bak").exists());
}

#[test]
fn replace_over_a_missing_file_creates_it_with_no_backup() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::Replace {
            name: "renodx-cp2077.addon64".to_owned(),
            bytes: b"addon-v1".to_vec(),
        }],
    };
    let receipt = install(game, &plan).expect("install");

    assert_eq!(read(&game.join("renodx-cp2077.addon64")), b"addon-v1");
    assert_eq!(
        receipt_paths(&receipt.created_files),
        vec!["renodx-cp2077.addon64"]
    );
    assert!(receipt.backed_up_files.is_empty());
    assert!(!game.join("renodx-cp2077.addon64.bak").exists());
}

#[test]
fn replace_over_an_existing_file_overwrites_it_with_no_backup() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    fs::write(game.join("dxgi.dll"), b"old-reshade").expect("write");

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::Replace {
            name: "dxgi.dll".to_owned(),
            bytes: b"new-reshade".to_vec(),
        }],
    };
    let receipt = install(game, &plan).expect("install");

    assert_eq!(read(&game.join("dxgi.dll")), b"new-reshade");
    // The overwritten file counts as created (an uninstall deletes it outright),
    // never as backed-up (there is no `.bak` to restore from).
    assert_eq!(receipt_paths(&receipt.created_files), vec!["dxgi.dll"]);
    assert!(receipt.backed_up_files.is_empty());
    assert!(!game.join("dxgi.dll.bak").exists());
}

#[test]
fn replace_rolls_back_to_pre_write_bytes_when_a_later_op_fails() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    fs::write(game.join("dxgi.dll"), b"old-reshade").expect("write");

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::Replace {
                name: "dxgi.dll".to_owned(),
                bytes: b"new-reshade".to_vec(),
            },
            FileOp::Create {
                name: "../escape.dll".to_owned(),
                bytes: b"evil".to_vec(),
            },
        ],
    };
    install(game, &plan).expect_err("unsafe op should fail");

    // Rolled back to the pre-write bytes, in place — no `.bak` was ever involved.
    assert_eq!(read(&game.join("dxgi.dll")), b"old-reshade");
    assert!(!game.join("dxgi.dll.bak").exists());
    assert!(!is_install_torn(game, AddonKind::RenoDx));
}

#[test]
fn replace_rolls_back_to_absent_when_it_created_the_file() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::Replace {
                name: "renodx-cp2077.addon64".to_owned(),
                bytes: b"addon".to_vec(),
            },
            FileOp::Create {
                name: "../escape.dll".to_owned(),
                bytes: b"evil".to_vec(),
            },
        ],
    };
    install(game, &plan).expect_err("unsafe op should fail");

    assert!(!game.join("renodx-cp2077.addon64").exists());
    assert!(!game.join("renodx-cp2077.addon64.bak").exists());
}

#[test]
fn uninstall_deletes_a_replaced_file_outright() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    fs::write(game.join("dxgi.dll"), b"old-reshade").expect("write");

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::Replace {
            name: "dxgi.dll".to_owned(),
            bytes: b"new-reshade".to_vec(),
        }],
    };
    let receipt = install(game, &plan).expect("install");

    // Unlike `BackupAndReplace`, uninstalling a `Replace`'d file does not bring
    // the old game-shipped bytes back — there was never a `.bak` to restore.
    uninstall(&receipt.created_files, &receipt.backed_up_files).expect("uninstall");
    assert!(!game.join("dxgi.dll").exists());
}

#[test]
fn ops_run_in_order_so_a_later_merge_sees_an_earlier_one() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::MergeText {
                name: "cfg.ini".to_owned(),
                default: String::new(),
                strategy: MergeStrategy::IniSetKeys {
                    sections: vec![IniSection {
                        name: "S".to_owned(),
                        keys: vec![("a".to_owned(), "1".to_owned())],
                    }],
                },
            },
            FileOp::MergeText {
                name: "cfg.ini".to_owned(),
                default: String::new(),
                strategy: MergeStrategy::IniSetKeys {
                    sections: vec![IniSection {
                        name: "S".to_owned(),
                        keys: vec![("b".to_owned(), "2".to_owned())],
                    }],
                },
            },
        ],
    };
    install(game, &plan).expect("install");

    let cfg = String::from_utf8(read(&game.join("cfg.ini"))).unwrap();
    assert_eq!(cfg, "[S]\r\na=1\r\nb=2\r\n");
}

#[test]
fn a_failed_op_rolls_back_every_prior_op_in_reverse() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    fs::write(game.join("dxgi.dll"), b"game-shipped").expect("write");

    // The third op has an unsafe name and is rejected; the first two (the addon
    // and the backup-and-replace of dxgi.dll) must be fully reverted.
    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::Create {
                name: "renodx.addon64".to_owned(),
                bytes: b"addon".to_vec(),
            },
            FileOp::BackupAndReplace {
                name: "dxgi.dll".to_owned(),
                bytes: b"reshade-dll".to_vec(),
            },
            FileOp::Create {
                name: "../escape.dll".to_owned(),
                bytes: b"evil".to_vec(),
            },
        ],
    };
    let error = install(game, &plan).expect_err("unsafe op should fail");
    assert_matches!(error, ServiceError::InvalidInput(_));

    // Folder restored: addon removed, original dxgi.dll back, no leftovers.
    assert!(!game.join("renodx.addon64").exists());
    assert_eq!(read(&game.join("dxgi.dll")), b"game-shipped");
    assert!(!game.join("dxgi.dll.bak").exists());
    // A clean rollback clears the sentinel.
    assert!(!is_install_torn(game, AddonKind::RenoDx));
}

#[test]
fn ini_set_keys_creates_section_then_replaces_in_place() {
    let create = MergeStrategy::IniSetKeys {
        sections: vec![IniSection {
            name: "ADDON".to_owned(),
            keys: vec![
                (
                    "DisabledAddons".to_owned(),
                    "Generic Depth,Effect Runtime Sync".to_owned(),
                ),
                ("AddonPath".to_owned(), ".".to_owned()),
            ],
        }],
    };
    assert_eq!(
        create.apply(""),
        "[ADDON]\r\nDisabledAddons=Generic Depth,Effect Runtime Sync\r\nAddonPath=.\r\n"
    );

    // Replaces an existing key in place (no duplication) and preserves foreign
    // sections, comments, and blank lines.
    let existing = "; mine\r\n[GENERAL]\r\nPreset=foo.ini\r\n\r\n[ADDON]\r\nDisabledAddons=Old\r\n";
    let merged = create.apply(existing);
    assert!(merged.contains("; mine"));
    assert!(merged.contains("[GENERAL]\r\nPreset=foo.ini"));
    assert_eq!(merged.matches("DisabledAddons=").count(), 1);
    assert!(merged.contains("DisabledAddons=Generic Depth,Effect Runtime Sync"));
    assert!(merged.contains("AddonPath=."));
}

#[test]
fn undo_outcome_drives_sentinel_retention() {
    assert!(UndoOutcome { failures: 0 }.is_complete());
    assert!(!UndoOutcome { failures: 1 }.is_complete());
}

#[test]
fn update_text_creates_a_missing_file_from_default() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::UpdateText {
            name: "ReShade.ini".to_owned(),
            default: String::new(),
            strategy: MergeStrategy::IniSetKeys {
                sections: vec![IniSection {
                    name: "ADDON".to_owned(),
                    keys: vec![("AddonPath".to_owned(), ".".to_owned())],
                }],
            },
        }],
    };
    let receipt = install(game, &plan).expect("install");
    let ini = String::from_utf8(read(&game.join("ReShade.ini"))).unwrap();
    assert_eq!(ini, "[ADDON]\r\nAddonPath=.\r\n");
    // UpdateText created this file from empty, so it's just as much this
    // install's own file as one written fresh — it's in created_files (for
    // uninstall to find), never backed up (there was nothing to back up).
    assert_eq!(receipt_paths(&receipt.created_files), vec!["ReShade.ini"]);
    assert!(receipt.backed_up_files.is_empty());
    assert!(!game.join("ReShade.ini.bak").exists());
}

#[test]
fn update_text_rolls_back_to_absent_when_it_created_the_file() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::UpdateText {
                name: "ReShade.ini".to_owned(),
                default: String::new(),
                strategy: MergeStrategy::IniSetKeys {
                    sections: vec![IniSection {
                        name: "ADDON".to_owned(),
                        keys: vec![("AddonPath".to_owned(), ".".to_owned())],
                    }],
                },
            },
            FileOp::Create {
                name: "../escape.dll".to_owned(),
                bytes: b"evil".to_vec(),
            },
        ],
    };
    install(game, &plan).expect_err("unsafe op should fail");
    // The file was absent before the plan; rollback of the UpdateText that
    // created it removes it again (no stub left behind, no `.bak`).
    assert!(!game.join("ReShade.ini").exists());
    assert!(!game.join("ReShade.ini.bak").exists());
}

#[test]
fn update_text_preserves_the_primary_bak_and_rolls_back_to_pre_update_bytes() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    // A pre-existing foreign config: a MergeText install backs it up and rewrites it.
    fs::write(game.join("ReShade.ini"), "[GENERAL]\r\nPreset=mine.ini\r\n").expect("write");

    let first = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::MergeText {
            name: "ReShade.ini".to_owned(),
            default: String::new(),
            strategy: MergeStrategy::IniSetKeys {
                sections: vec![IniSection {
                    name: "ADDON".to_owned(),
                    keys: vec![("AddonPath".to_owned(), ".".to_owned())],
                }],
            },
        }],
    };
    let first_receipt = install(game, &first).expect("first install");
    assert!(game.join("ReShade.ini.bak").exists());
    assert_eq!(first_receipt.backed_up_files.len(), 1);

    // A companion UpdateText adds a section without touching the existing `.bak`.
    let second = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::UpdateText {
            name: "ReShade.ini".to_owned(),
            default: String::new(),
            strategy: MergeStrategy::IniSetKeys {
                sections: vec![IniSection {
                    name: "RENODX-DLSSFIX".to_owned(),
                    keys: vec![("DLSSPath".to_owned(), "C:\\dlss.dll".to_owned())],
                }],
            },
        }],
    };
    let second_receipt = install(game, &second).expect("companion update");
    let ini = String::from_utf8(read(&game.join("ReShade.ini"))).unwrap();
    assert!(ini.contains("Preset=mine.ini"));
    assert!(ini.contains("AddonPath=."));
    assert!(ini.contains("[RENODX-DLSSFIX]"));
    assert!(ini.contains("DLSSPath=C:\\dlss.dll"));
    assert!(second_receipt.created_files.is_empty());
    assert!(second_receipt.backed_up_files.is_empty());
    // The primary install's `.bak` survives the companion update.
    assert!(game.join("ReShade.ini.bak").exists());

    // A failed follow-up op rolls the companion update back to its pre-update
    // bytes: `[RENODX-DLSSFIX]` stays (it was there before this plan), `Extra=`
    // (added by this plan) is gone, and the primary `.bak` is still intact.
    let third = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::UpdateText {
                name: "ReShade.ini".to_owned(),
                default: String::new(),
                strategy: MergeStrategy::IniSetKeys {
                    sections: vec![IniSection {
                        name: "ADDON".to_owned(),
                        keys: vec![("Extra".to_owned(), "1".to_owned())],
                    }],
                },
            },
            FileOp::Create {
                name: "../escape.dll".to_owned(),
                bytes: b"evil".to_vec(),
            },
        ],
    };
    install(game, &third).expect_err("unsafe op should fail");
    let ini = String::from_utf8(read(&game.join("ReShade.ini"))).unwrap();
    assert!(ini.contains("AddonPath=."));
    assert!(ini.contains("[RENODX-DLSSFIX]"));
    assert!(!ini.contains("Extra="));
    assert!(game.join("ReShade.ini.bak").exists());
}

#[test]
fn remove_deletes_a_file_and_cleans_up_its_backup_on_success() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    fs::write(game.join("renodx-dlssfix.addon64"), b"fix-bytes").expect("write");

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::Remove {
            name: "renodx-dlssfix.addon64".to_owned(),
        }],
    };
    let receipt = install(game, &plan).expect("install");
    assert!(!game.join("renodx-dlssfix.addon64").exists());
    // The `.bak` existed for rollback but is cleaned up on success.
    assert!(!game.join("renodx-dlssfix.addon64.bak").exists());
    assert!(receipt.created_files.is_empty());
    assert!(receipt.backed_up_files.is_empty());
}

#[test]
fn remove_is_a_noop_for_a_missing_file() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::Remove {
            name: "absent.addon64".to_owned(),
        }],
    };
    install(game, &plan).expect("missing file is a no-op");
    assert!(!game.join("absent.addon64").exists());
    assert!(!game.join("absent.addon64.bak").exists());
}

#[test]
fn remove_rolls_back_restoring_the_deleted_file() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    fs::write(game.join("renodx-dlssfix.addon64"), b"fix-bytes").expect("write");

    // The Remove succeeds, then an unsafe op fails; rollback must restore the file.
    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::Remove {
                name: "renodx-dlssfix.addon64".to_owned(),
            },
            FileOp::Create {
                name: "../escape.dll".to_owned(),
                bytes: b"evil".to_vec(),
            },
        ],
    };
    install(game, &plan).expect_err("unsafe op should fail");
    assert_eq!(read(&game.join("renodx-dlssfix.addon64")), b"fix-bytes");
    assert!(!game.join("renodx-dlssfix.addon64.bak").exists());
}

#[test]
fn ini_remove_keys_strips_named_keys_and_whole_sections() {
    let base = "[ADDON]\r\nAddonPath=.\r\nLoadFromDllMain=x.addon64\r\n\
                    [RENODX-DLSSFIX]\r\nDLSSPath=C:\\d.dll\r\nStreamlinePath=C:\\s.dll\r\n";
    let strategy = MergeStrategy::IniRemoveKeys {
        sections: vec![
            IniSectionRemoval {
                name: "ADDON".to_owned(),
                keys: vec!["LoadFromDllMain".to_owned()],
            },
            IniSectionRemoval {
                name: "RENODX-DLSSFIX".to_owned(),
                keys: Vec::new(),
            },
        ],
    };
    let merged = strategy.apply(base);
    assert_eq!(merged, "[ADDON]\r\nAddonPath=.\r\n");
}

// -----------------------------------------------------------------------
// CreateNested / uninstall_tree / directory cleanup
// -----------------------------------------------------------------------

fn nested_plan() -> InstallPlan {
    InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::Create {
                name: "Luma-Game.addon".to_owned(),
                bytes: b"addon-bytes".to_vec(),
            },
            FileOp::CreateNested {
                relative_path: "Luma/Global/Copy_PS.hlsl".to_owned(),
                bytes: b"global-shader".to_vec(),
            },
            FileOp::CreateNested {
                relative_path: "Luma/Includes/Common.hlsl".to_owned(),
                bytes: b"includes-shader".to_vec(),
            },
            FileOp::CreateNested {
                relative_path: "Luma/Game/Tonemap.hlsl".to_owned(),
                bytes: b"game-shader".to_vec(),
            },
        ],
    }
}

#[test]
fn create_nested_lays_down_a_tree_and_round_trips_to_a_clean_folder() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();

    let receipt = install(game, &nested_plan()).expect("install");

    assert_eq!(
        read(&game.join("Luma").join("Global").join("Copy_PS.hlsl")),
        b"global-shader"
    );
    assert_eq!(
        read(&game.join("Luma").join("Includes").join("Common.hlsl")),
        b"includes-shader"
    );
    assert_eq!(
        read(&game.join("Luma").join("Game").join("Tonemap.hlsl")),
        b"game-shader"
    );
    assert_eq!(receipt.created_files.len(), 4);
    assert!(receipt.backed_up_files.is_empty());
    assert!(!is_install_torn(game, AddonKind::RenoDx));

    uninstall_tree(&receipt.created_files, &receipt.backed_up_files, game).expect("uninstall");

    assert!(!game.join("Luma-Game.addon").exists());
    assert!(
        !game.join("Luma").exists(),
        "the whole Luma/ tree should be gone"
    );
}

#[test]
fn create_nested_backs_up_and_restores_a_preexisting_nested_file() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    fs::create_dir_all(game.join("Luma").join("Global")).expect("mkdir");
    fs::write(
        game.join("Luma").join("Global").join("Copy_PS.hlsl"),
        b"game-shipped",
    )
    .expect("write");

    let receipt = install(game, &nested_plan()).expect("install");
    assert_eq!(
        read(&game.join("Luma").join("Global").join("Copy_PS.hlsl")),
        b"global-shader"
    );
    assert_eq!(
        receipt_paths(&receipt.backed_up_files),
        vec!["Copy_PS.hlsl"]
    );

    uninstall_tree(&receipt.created_files, &receipt.backed_up_files, game).expect("uninstall");
    assert_eq!(
        read(&game.join("Luma").join("Global").join("Copy_PS.hlsl")),
        b"game-shipped"
    );
    // The pre-existing `Luma/Global` directory was never created by this
    // install, so it must survive even though its file round-tripped back.
    assert!(game.join("Luma").join("Global").is_dir());
}

#[test]
fn create_nested_rollback_removes_created_files_and_directories_but_keeps_a_preexisting_dir() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    // `Luma/` itself pre-exists (e.g. left over from something unrelated);
    // only its `Global` child is newly created by this install.
    fs::create_dir(game.join("Luma")).expect("mkdir");

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![
            FileOp::CreateNested {
                relative_path: "Luma/Global/Copy_PS.hlsl".to_owned(),
                bytes: b"shader".to_vec(),
            },
            FileOp::Create {
                name: "../escape.dll".to_owned(),
                bytes: b"evil".to_vec(),
            },
        ],
    };
    install(game, &plan).expect_err("unsafe op should fail");

    assert!(
        !game.join("Luma").join("Global").exists(),
        "the newly created Global/ dir and its file must be rolled back"
    );
    assert!(
        game.join("Luma").is_dir(),
        "the pre-existing Luma/ dir must survive rollback"
    );
    assert!(!is_install_torn(game, AddonKind::RenoDx));
}

#[test]
fn create_nested_rollback_leaves_a_directory_non_empty_after_a_foreign_write_in_place() {
    // `remove_dir_if_empty` (the primitive both in-call rollback and
    // `cleanup_empty_dirs_best_effort` share) must treat a non-empty
    // directory as success without deleting its contents.
    let dir = tempdir().expect("tempdir");
    let nested = dir.path().join("a").join("b");
    fs::create_dir_all(&nested).expect("mkdir");
    fs::write(nested.join("user-file.txt"), b"keep me").expect("write");

    helpers::remove_dir_if_empty(&nested).expect("non-empty dir is not an error");

    assert!(nested.is_dir(), "non-empty directory must survive");
    assert!(nested.join("user-file.txt").is_file());
}

#[test]
fn uninstall_tree_cleans_up_a_reused_directory_left_empty_by_removal() {
    // A persisted record has no directory-creation log (only files) — so a
    // post-hoc `uninstall_tree`, unlike the in-call `Action::CreatedDir`
    // rollback, cannot distinguish "we created this dir" from "it pre-existed
    // and just happens to be empty now." Per the documented safety bar (only
    // *non-empty* directories survive; the boundary is never touched), a
    // directory reduced to empty by removing our own tracked files is cleaned
    // up either way — there is no content loss, since it is provably empty.
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    fs::create_dir_all(game.join("Luma").join("Global")).expect("mkdir");

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::CreateNested {
            relative_path: "Luma/Global/Copy_PS.hlsl".to_owned(),
            bytes: b"shader".to_vec(),
        }],
    };
    let receipt = install(game, &plan).expect("install");

    uninstall_tree(&receipt.created_files, &receipt.backed_up_files, game).expect("uninstall");
    assert!(
        !game
            .join("Luma")
            .join("Global")
            .join("Copy_PS.hlsl")
            .exists()
    );
    assert!(!game.join("Luma").join("Global").exists());
    assert!(!game.join("Luma").exists());
}

#[test]
fn uninstall_tree_leaves_a_directory_alone_when_it_still_holds_unrelated_content() {
    let dir = tempdir().expect("tempdir");
    let game = dir.path();
    fs::create_dir_all(game.join("Luma").join("Global")).expect("mkdir");
    fs::write(
        game.join("Luma").join("Global").join("user-preset.ini"),
        b"unrelated",
    )
    .expect("write");

    let plan = InstallPlan {
        kind: AddonKind::RenoDx,
        ops: vec![FileOp::CreateNested {
            relative_path: "Luma/Global/Copy_PS.hlsl".to_owned(),
            bytes: b"shader".to_vec(),
        }],
    };
    let receipt = install(game, &plan).expect("install");

    uninstall_tree(&receipt.created_files, &receipt.backed_up_files, game).expect("uninstall");
    assert!(
        !game
            .join("Luma")
            .join("Global")
            .join("Copy_PS.hlsl")
            .exists()
    );
    // A file this install never tracked keeps the directory (and its
    // ancestors) alive.
    assert!(
        game.join("Luma")
            .join("Global")
            .join("user-preset.ini")
            .is_file()
    );
    assert!(game.join("Luma").join("Global").is_dir());
    assert!(game.join("Luma").is_dir());
}

#[test]
fn relative_path_rejection_matrix() {
    // 17 components exceeds `MAX_RELATIVE_PATH_DEPTH` (16).
    let too_deep = "a/".repeat(17);
    let too_deep = too_deep.trim_end_matches('/');
    let cases = [
        "../escape.hlsl",
        "Luma/../escape.hlsl",
        r"C:\Windows\evil.hlsl",
        r"\\server\share\evil.hlsl",
        "/absolute/escape.hlsl",
        "Luma//Global.hlsl",
        "Luma/./Global.hlsl",
        "Luma/aux.hlsl",
        "Luma/trailing.space.hlsl ",
        "",
        too_deep,
    ];
    for raw in cases {
        let error = ensure_safe_relative_path("relative path", raw);
        assert!(error.is_err(), "expected `{raw}` to be rejected");
    }
}

#[test]
fn relative_path_accepts_a_deep_but_valid_shader_tree_path() {
    let path =
        ensure_safe_relative_path("relative path", "Luma/Borderlands 2/Includes/Common.hlsl")
            .expect("valid nested path");
    assert_eq!(
        path,
        PathBuf::from("Luma")
            .join("Borderlands 2")
            .join("Includes")
            .join("Common.hlsl")
    );
}

#[test]
fn relative_path_accepts_a_single_component_like_a_bare_create() {
    let path = ensure_safe_relative_path("relative path", "addon.dll").expect("valid");
    assert_eq!(path, PathBuf::from("addon.dll"));
}
