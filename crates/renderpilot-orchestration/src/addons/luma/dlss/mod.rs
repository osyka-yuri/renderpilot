//! Pure DLSS ownership planning for Luma.
//!
//! Policy and planning only — coordinated filesystem apply lives in
//! [`crate::coordinated_files`]. Catalog cascade composition for a disappearing
//! owned binding is a thin wrapper over [`crate::catalog::cascade`].

mod binding;
mod inspect;
mod plan;

#[cfg(test)]
mod tests;

pub(crate) use binding::{
    cascade_for_disappearing_owned, find_created_dlss, find_managed_dlss_binding,
    is_dlss_relative_path,
};
pub(crate) use plan::{PlannedDlss, plan_install, plan_release_binding, plan_update};
pub(crate) use renderpilot_detection::NVNGX_DLSS_FILE_NAME;
