//! Thin adapters onto the existing Engine.ini and shared Vulkan operations.

mod engine;
#[cfg(test)]
mod tests;
mod vulkan;

pub(super) use engine::{EnginePendingClass, classify_engine_pending, release_engine_owner};
pub(super) use vulkan::{VulkanPendingClass, classify_vulkan_pending, unregister_vulkan_owner};

#[cfg(test)]
pub(super) use engine::release_operation_id as engine_release_operation_id;
#[cfg(test)]
pub(super) use vulkan::unregister_operation_id as vulkan_unregister_operation_id;
