use std::fs;

use renderpilot_orchestration::application::ComponentRepository;
use renderpilot_orchestration::domain::{LibraryTechnology, Swappability};

use crate::hash::sha256_hex;

use super::super::{
    CatalogFixture, TempGameFolder, args, path_string, sample_artifact, sample_component,
    sample_game,
};
use super::helpers::{
    FSR_ENTRY_POINT_FILE, dir_file_names, store_manual_game, store_single_file_fsr_component,
    store_written_fsr_bundle_component, write_fsr_bundle_artifact,
    write_versioned_component_members,
};

/// Headline FSR 3.1 -> FSR 4 scenario: the loader installs *as* the game's
/// `amd_fidelityfx_dx12.dll` entry point (replacing it; the original is backed up
/// once), while the upscaler and frame-generation members are added alongside under
/// their own names. Rollback restores the original entry point and removes the two
/// added members, leaving the directory clean.
mod downgrade;
mod upgrade;
