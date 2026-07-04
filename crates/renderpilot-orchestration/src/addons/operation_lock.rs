//! Per-game mutual-exclusion locks shared by every addon tool (RenoDX, Luma, …).
//!
//! Keyed by `game_id` alone — **not** namespaced per tool — so an install of one
//! tool and an install of another for the *same* game always serialize. This is
//! what makes the addon-exclusivity check-then-write sequence (see
//! `addons::exclusivity`) race-free: a tool that observes no foreign record/files
//! under this lock can trust that observation until it releases the lock.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use renderpilot_domain::GameId;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

static LOCKS: OnceLock<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> = OnceLock::new();
static SHARED_VULKAN_LOCK: OnceLock<Arc<AsyncMutex<()>>> = OnceLock::new();

/// Maximum number of per-game locks retained in the map. Entries with no
/// active lockers (`Arc::strong_count == 1`) are pruned when this count is
/// exceeded, so the map does not grow without bound for long-lived processes
/// scanning hundreds of games.
const MAX_LOCK_MAP_CAPACITY: usize = 256;

fn lock_for(game_id: &GameId) -> Arc<AsyncMutex<()>> {
    let key = game_id.as_str().to_owned();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks.lock().expect("addon operation lock map poisoned");

    if locks.len() >= MAX_LOCK_MAP_CAPACITY {
        locks.retain(|_, arc| Arc::strong_count(arc) > 1);
    }

    locks
        .entry(key)
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

pub(crate) async fn lock(game_id: &GameId) -> OwnedMutexGuard<()> {
    lock_for(game_id).lock_owned().await
}

/// Blocks the calling thread until the per-game lock is free.
///
/// # Panics
/// Panics if called from within an async execution context (tokio would
/// otherwise deadlock the runtime). Only call this from a sync entry point
/// that Tauri/CLI already runs on a blocking thread (e.g. via
/// `spawn_blocking`) — never from code reachable through `await`.
pub(crate) fn blocking_lock(game_id: &GameId) -> OwnedMutexGuard<()> {
    lock_for(game_id).blocking_lock_owned()
}

/// Attempts to acquire the per-game lock without blocking. Safe to call from
/// any context, including inside an async task — returns `None` immediately
/// if another operation already holds the lock, rather than panicking or
/// waiting.
pub(crate) fn try_lock(game_id: &GameId) -> Option<OwnedMutexGuard<()>> {
    lock_for(game_id).try_lock_owned().ok()
}

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
/// Panics if called from within an async execution context — see
/// [`blocking_lock`].
pub(crate) fn blocking_shared_vulkan_lock() -> OwnedMutexGuard<()> {
    SHARED_VULKAN_LOCK
        .get_or_init(|| Arc::new(AsyncMutex::new(())))
        .clone()
        .blocking_lock_owned()
}

/// Attempts to acquire the shared Vulkan layer lock without blocking. See
/// [`try_lock`] — same non-blocking, panic-free contract.
pub(crate) fn try_shared_vulkan_lock() -> Option<OwnedMutexGuard<()>> {
    SHARED_VULKAN_LOCK
        .get_or_init(|| Arc::new(AsyncMutex::new(())))
        .clone()
        .try_lock_owned()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_key_is_the_bare_game_id_shared_across_tools() {
        // Two different tools locking "the same game" must contend for the
        // identical mutex — the key carries no tool/kind namespace. `try_lock`
        // proves it: once the first guard is held, a second acquisition for the
        // same game id fails, regardless of which "tool" conceptually asked.
        let game_id = GameId::new("steam:12345").expect("valid game id");
        let first = try_lock(&game_id).expect("first lock acquires");
        assert!(
            try_lock(&game_id).is_none(),
            "a second tool locking the same game id must contend for the same lock"
        );
        drop(first);
        assert!(
            try_lock(&game_id).is_some(),
            "lock is released and re-acquirable after the guard drops"
        );
    }
}
