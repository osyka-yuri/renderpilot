//! Catalog scan transport facade.

mod add_game;
mod auto;
mod remove_game;

pub use add_game::{add_game, inspect_game_install};
pub use auto::{AutoScanOutput, scan_auto_libraries, scan_auto_libraries_background_output};
pub use remove_game::{
    GameRemovalAssessment, NvapiClaimAbandonmentAuthorization, NvapiClaimAbandonmentToken,
    NvapiClaimAssessment, NvapiClaimDisposition, NvapiRemovalAssessment, NvapiSettingValueDto,
    assess_game_removal, remove_game_from_catalog, remove_game_from_catalog_authorized,
};
