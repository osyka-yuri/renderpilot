use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use renderpilot_domain::{AddonKind, GameId};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

static LOCKS: OnceLock<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> = OnceLock::new();
static SHARED_VULKAN_LOCK: OnceLock<Arc<AsyncMutex<()>>> = OnceLock::new();

fn lock_for(game_id: &GameId) -> Arc<AsyncMutex<()>> {
    let key = format!("{}:{}", AddonKind::RenoDx.as_str(), game_id.as_str());
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks.lock().expect("RenoDX operation lock map poisoned");
    locks
        .entry(key)
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

pub(super) async fn lock(game_id: &GameId) -> OwnedMutexGuard<()> {
    lock_for(game_id).lock_owned().await
}

/// Blocks the calling thread until the per-game lock is free.
///
/// # Panics
/// Panics if called from within an async execution context (tokio would
/// otherwise deadlock the runtime). Only call this from a sync entry point
/// that Tauri/CLI already runs on a blocking thread (e.g. via
/// `spawn_blocking`) — never from code reachable through `await`.
pub(super) fn blocking_lock(game_id: &GameId) -> OwnedMutexGuard<()> {
    lock_for(game_id).blocking_lock_owned()
}

/// Attempts to acquire the per-game lock without blocking. Safe to call from
/// any context, including inside an async task — returns `None` immediately
/// if another operation already holds the lock, rather than panicking or
/// waiting.
pub(super) fn try_lock(game_id: &GameId) -> Option<OwnedMutexGuard<()>> {
    lock_for(game_id).try_lock_owned().ok()
}

pub(super) async fn shared_vulkan_lock() -> OwnedMutexGuard<()> {
    SHARED_VULKAN_LOCK
        .get_or_init(|| Arc::new(AsyncMutex::new(())))
        .clone()
        .lock_owned()
        .await
}

/// Blocks the calling thread until the shared Vulkan layer lock is free.
///
/// # Panics
/// Panics if called from within an async execution context — see
/// [`blocking_lock`].
pub(super) fn blocking_shared_vulkan_lock() -> OwnedMutexGuard<()> {
    SHARED_VULKAN_LOCK
        .get_or_init(|| Arc::new(AsyncMutex::new(())))
        .clone()
        .blocking_lock_owned()
}

/// Attempts to acquire the shared Vulkan layer lock without blocking. See
/// [`try_lock`] — same non-blocking, panic-free contract.
pub(super) fn try_shared_vulkan_lock() -> Option<OwnedMutexGuard<()>> {
    SHARED_VULKAN_LOCK
        .get_or_init(|| Arc::new(AsyncMutex::new(())))
        .clone()
        .try_lock_owned()
        .ok()
}
