//! Native registry authority for the shared Vulkan layer.

use renderpilot_platform_windows::vulkan_layer::LayerRegistry;

#[cfg(windows)]
use renderpilot_platform_windows::vulkan_layer::WindowsLayerRegistry;

/// Returns the process-native registry adapter used by shared Vulkan
/// operations. The adapter is a single static authority so observations and
/// transaction participants always use the same implementation.
pub(crate) fn native_registry() -> Option<&'static dyn LayerRegistry> {
    #[cfg(windows)]
    {
        static WINDOWS_REGISTRY: WindowsLayerRegistry = WindowsLayerRegistry;
        Some(&WINDOWS_REGISTRY)
    }
    #[cfg(not(windows))]
    {
        None
    }
}
