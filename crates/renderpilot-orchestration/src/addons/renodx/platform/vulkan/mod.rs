/// Vulkan specific layer configuration and deployment.
pub mod apps_ini;
/// Program data folder.
pub mod program_data;
/// Public layer report mapping.
pub mod report;
/// Advisory shared-artifact record for the shared layer.
pub mod shared_artifact;
/// Pure layer validation invariants.
pub mod validation;

pub use crate::addons::renodx::dto::vulkan::{
    LayerDiagnosticReason, VulkanLayerActions, VulkanLayerArchitecture, VulkanLayerDetection,
    VulkanLayerFacts, VulkanLayerReport, VulkanLoaderVisibility,
};
pub use apps_ini::{register_app, unregister_app};
pub(crate) use program_data::current_layer_digest;
pub use program_data::{install_layer, layer_dir, remove_layer};
pub(crate) use report::conflict_is_standard_mutable;
pub use report::layer_report;
pub(crate) use shared_artifact::{
    forget_layer_record, record_detected_layer, record_downloaded_layer, stored_layer_channel,
    stored_layer_digest,
};
