//! Inspect-and-confirm use case for one explicit game installation root.

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use renderpilot_domain::{GameId, Launcher, PathRef, RootAuthority, normalized_path_key};

use crate::{InvalidInstallRootReason, ServiceError};

#[cfg(all(test, windows))]
use super::install_paths;
use super::{
    CatalogScanChange, RootCorrectionAssessment, RootCorrectionStatus, install_boundary,
    root_correction,
};

mod confirmation;
mod decision;
mod fingerprint;
mod inspection;
mod relationship;
#[cfg(test)]
mod tests;
mod types;

pub use confirmation::add_game;
pub use inspection::inspect_game_install;
pub use types::*;

#[cfg(test)]
use confirmation::*;
use decision::{DecisionFacts, derive_add_game_decision};
use fingerprint::{compute_effective_root_fingerprint, compute_inspection_fingerprint};
use inspection::*;
use relationship::*;
