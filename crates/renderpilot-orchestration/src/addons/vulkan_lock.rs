//! Serialization for add-on resources that are genuinely shared across games.
//!
//! Per-game serialization lives at [`crate::game_mutation_lock`] so catalog and
//! add-on commands contend on the same neutral boundary.

use std::sync::{Arc, OnceLock};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

static SHARED_VULKAN_LOCK: OnceLock<Arc<AsyncMutex<()>>> = OnceLock::new();

pub(crate) async fn shared_vulkan_lock() -> OwnedMutexGuard<()> {
    SHARED_VULKAN_LOCK
        .get_or_init(|| Arc::new(AsyncMutex::new(())))
        .clone()
        .lock_owned()
        .await
}

/// Blocks the calling thread until the shared Vulkan layer lock is free.
///
/// # Panics
/// Panics if called from within an async execution context (Tokio's
/// `blocking_lock_owned` panics to prevent runtime deadlock).
pub(crate) fn blocking_shared_vulkan_lock() -> OwnedMutexGuard<()> {
    SHARED_VULKAN_LOCK
        .get_or_init(|| Arc::new(AsyncMutex::new(())))
        .clone()
        .blocking_lock_owned()
}

/// Attempts to acquire the shared Vulkan layer lock without blocking.
/// Non-blocking and panic-free — returns `None` immediately if another
/// operation already holds the lock.
pub(crate) fn try_shared_vulkan_lock() -> Option<OwnedMutexGuard<()>> {
    SHARED_VULKAN_LOCK
        .get_or_init(|| Arc::new(AsyncMutex::new(())))
        .clone()
        .try_lock_owned()
        .ok()
}
