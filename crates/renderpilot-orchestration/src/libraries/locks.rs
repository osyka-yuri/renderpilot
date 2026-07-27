//! Keyed async locks for content-addressed library operations.
//!
//! A logical package id is the lifecycle identity while an artifact digest is
//! the physical content identity. Keeping both lock domains lets unrelated
//! packages proceed concurrently while preventing duplicate downloads or
//! partial writes for shared content.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

static LOCKS: OnceLock<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> = OnceLock::new();
const LOCK_MAP_EVICT_AT: usize = 512;

pub(crate) async fn acquire(key: impl Into<String>) -> OwnedMutexGuard<()> {
    let key = key.into();
    let lock = {
        let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
        let mut locks = locks.lock().expect("library lock map poisoned");
        if locks.len() >= LOCK_MAP_EVICT_AT {
            locks.retain(|_, lock| Arc::strong_count(lock) > 1);
        }
        locks
            .entry(key)
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    };
    lock.lock_owned().await
}

#[cfg(test)]
mod tests {
    use super::acquire;

    #[tokio::test]
    async fn same_key_is_serialized_but_other_keys_progress() {
        let first = acquire("package-id:a").await;
        let waiting = tokio::spawn(async { acquire("package-id:a").await });
        let other = tokio::spawn(async { acquire("package-id:b").await });
        tokio::task::yield_now().await;
        assert!(other.is_finished());
        assert!(!waiting.is_finished());
        drop(first);
        waiting.await.expect("waiting lock");
    }
}
