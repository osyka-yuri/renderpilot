/// Availability preview query.
pub mod availability;
/// DLSS-Fix availability query.
pub mod dlss_fix;
/// ReShade host detection state mapped to the availability query's DTOs.
mod host_report;
/// Installed state query.
pub mod status;
/// Update check queries.
pub mod updates;
/// Shared Vulkan layer status query.
pub mod vulkan_layer;
