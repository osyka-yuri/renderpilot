/// Vulkan specific layer configuration and deployment.
/// Program data folder.
pub mod program_data;
mod registry;
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
pub(crate) use program_data::current_layer_digest;
pub use program_data::layer_dir;
pub(crate) use registry::native_registry;
pub(crate) use report::conflict_is_standard_mutable;
pub use report::layer_report;
pub(crate) use shared_artifact::{stored_layer_channel, stored_layer_digest};
