// Thin re-export — implementation lives in renderpilot-orchestration.
pub(crate) use renderpilot_orchestration::addons::luma::manifest_store;
pub(crate) use renderpilot_orchestration::addons::luma::use_cases::commands::uninstall::uninstall;
pub(crate) use renderpilot_orchestration::addons::luma::use_cases::queries::status::status;
pub(crate) use renderpilot_orchestration::addons::luma::use_cases::queries::updates::{
    check_update, check_updates, unknown_updates_for_installed,
};
