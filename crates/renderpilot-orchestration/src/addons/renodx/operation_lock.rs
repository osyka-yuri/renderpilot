use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use renderpilot_domain::{AddonKind, GameId};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

static LOCKS: OnceLock<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> = OnceLock::new();

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

pub(super) fn blocking_lock(game_id: &GameId) -> OwnedMutexGuard<()> {
    lock_for(game_id).blocking_lock_owned()
}
