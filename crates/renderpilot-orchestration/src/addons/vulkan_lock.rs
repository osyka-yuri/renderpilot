//! Serialization for add-on resources that are genuinely shared across games.
//!
//! Per-game serialization lives at [`crate::game_mutation_lock`] so catalog and
//! add-on commands contend on the same neutral boundary.

use std::sync::{Arc, OnceLock};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

static SHARED_VULKAN_LOCK: OnceLock<Arc<AsyncMutex<()>>> = OnceLock::new();

/// Proof that the caller exclusively owns the shared Vulkan mutation boundary.
pub(crate) struct SharedVulkanMutationGuard {
    _guard: OwnedMutexGuard<()>,
}

pub(crate) async fn shared_vulkan_lock() -> SharedVulkanMutationGuard {
    let guard = SHARED_VULKAN_LOCK
        .get_or_init(|| Arc::new(AsyncMutex::new(())))
        .clone()
        .lock_owned()
        .await;
    SharedVulkanMutationGuard { _guard: guard }
}

/// Blocks the calling thread until the shared Vulkan layer lock is free.
///
/// # Panics
/// Panics if called from within an async execution context (Tokio's
/// `blocking_lock_owned` panics to prevent runtime deadlock).
pub(crate) fn blocking_shared_vulkan_lock() -> SharedVulkanMutationGuard {
    let guard = SHARED_VULKAN_LOCK
        .get_or_init(|| Arc::new(AsyncMutex::new(())))
        .clone()
        .blocking_lock_owned();
    SharedVulkanMutationGuard { _guard: guard }
}

/// Attempts to acquire the shared Vulkan layer lock without blocking.
/// Non-blocking and panic-free — returns `None` immediately if another
/// operation already holds the lock.
pub(crate) fn try_shared_vulkan_lock() -> Option<SharedVulkanMutationGuard> {
    let guard = SHARED_VULKAN_LOCK
        .get_or_init(|| Arc::new(AsyncMutex::new(())))
        .clone()
        .try_lock_owned()
        .ok()?;
    Some(SharedVulkanMutationGuard { _guard: guard })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_lock_is_process_local_and_non_blocking_probe_is_one_shot() {
        let first = blocking_shared_vulkan_lock();
        assert!(try_shared_vulkan_lock().is_none());
        drop(first);
        assert!(try_shared_vulkan_lock().is_some());
    }
}
