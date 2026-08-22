//! Per-game serialization for every command that can observe or mutate game files.
//!
//! The lock is feature-neutral: catalog scans/swaps and add-on lifecycle commands
//! contend on the same game id. Internal compound operations receive a
//! [`GameMutationGuard`] instead of acquiring the mutex again.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use renderpilot_domain::GameId;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

static LOCKS: OnceLock<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>> = OnceLock::new();
#[cfg(test)]
static LOCK_ATTEMPT_HOOKS: OnceLock<Mutex<HashMap<String, std::sync::mpsc::Sender<()>>>> =
    OnceLock::new();
const LOCK_MAP_EVICT_AT: usize = 256;

/// Proof that the caller exclusively owns the mutation boundary for one game.
pub(crate) struct GameMutationGuard {
    game_id: GameId,
    _guard: OwnedMutexGuard<()>,
}

/// Proof that the caller owns mutation boundaries for a sorted set of games.
pub(crate) struct GameMutationGuardSet {
    _guards: Vec<GameMutationGuard>,
}

impl GameMutationGuardSet {
    pub(crate) fn from_guards(guards: Vec<GameMutationGuard>) -> Self {
        Self { _guards: guards }
    }

    /// Protected identities in deterministic lock order.
    #[cfg(test)]
    pub(crate) fn game_ids(&self) -> impl Iterator<Item = &GameId> {
        self._guards.iter().map(GameMutationGuard::game_id)
    }
}

impl GameMutationGuard {
    /// Returns the game protected by this guard.
    pub(crate) fn game_id(&self) -> &GameId {
        &self.game_id
    }
}

fn mutex_for(game_id: &GameId) -> Arc<AsyncMutex<()>> {
    let key = game_id.as_str().to_owned();
    let locks = LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = locks.lock().unwrap_or_else(PoisonError::into_inner);

    if locks.len() >= LOCK_MAP_EVICT_AT {
        locks.retain(|_, arc| Arc::strong_count(arc) > 1);
    }

    locks
        .entry(key)
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

pub(crate) async fn lock(game_id: &GameId) -> GameMutationGuard {
    notify_lock_attempt(game_id);
    let guard = mutex_for(game_id).lock_owned().await;
    GameMutationGuard {
        game_id: game_id.clone(),
        _guard: guard,
    }
}

/// Blocks a synchronous mutation entry point until its game is available.
///
/// # Panics
///
/// Panics inside an async runtime. Async callers must use [`lock`].
pub(crate) fn blocking_lock(game_id: &GameId) -> GameMutationGuard {
    notify_lock_attempt(game_id);
    let guard = mutex_for(game_id).blocking_lock_owned();
    GameMutationGuard {
        game_id: game_id.clone(),
        _guard: guard,
    }
}

#[cfg(test)]
pub(crate) fn set_lock_attempt_hook(game_id: &GameId, sender: std::sync::mpsc::Sender<()>) {
    LOCK_ATTEMPT_HOOKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("lock attempt hook poisoned")
        .insert(game_id.as_str().to_owned(), sender);
}

#[cfg(test)]
fn notify_lock_attempt(game_id: &GameId) {
    let hooks = LOCK_ATTEMPT_HOOKS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(sender) = hooks
        .lock()
        .expect("lock attempt hook poisoned")
        .remove(game_id.as_str())
    {
        let _ = sender.send(());
    }
}

#[cfg(not(test))]
fn notify_lock_attempt(_game_id: &GameId) {}

/// Attempts to acquire the game lock without waiting.
///
/// Test-only: production mutation commands and `load_availability` use [`lock`].
#[cfg(test)]
pub(crate) fn try_lock(game_id: &GameId) -> Option<GameMutationGuard> {
    let guard = mutex_for(game_id).try_lock_owned().ok()?;
    Some(GameMutationGuard {
        game_id: game_id.clone(),
        _guard: guard,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_feature_contends_on_the_bare_game_id() {
        let game_id = GameId::new("steam:12345").expect("valid game id");
        let first = try_lock(&game_id).expect("first lock acquires");
        assert!(try_lock(&game_id).is_none());
        assert_eq!(first.game_id(), &game_id);
        drop(first);
        assert!(try_lock(&game_id).is_some());
    }

    #[test]
    fn multi_game_boundary_sorts_and_deduplicates_ids() {
        let temp = tempfile::tempdir().expect("temp");
        let context = crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context");
        let a = GameId::new("game:a").expect("a");
        let b = GameId::new("game:b").expect("b");
        let set = crate::mutation_boundary::enter_game_mutation_boundaries(
            &context,
            [b.clone(), a.clone(), b.clone()],
        )
        .expect("locks");
        assert_eq!(set.game_ids().collect::<Vec<_>>(), vec![&a, &b]);
    }

    #[test]
    fn reverse_multi_game_requests_complete_without_deadlock() {
        use std::sync::{Arc, Barrier, mpsc};
        use std::time::Duration;

        let temp = tempfile::tempdir().expect("temp");
        let context =
            Arc::new(crate::Context::open_at(temp.path().join("catalog.sqlite")).expect("context"));
        let barrier = Arc::new(Barrier::new(3));
        let (done_tx, done_rx) = mpsc::channel();

        for ids in [["game:b", "game:a"], ["game:a", "game:b"]] {
            let context = Arc::clone(&context);
            let barrier = Arc::clone(&barrier);
            let done_tx = done_tx.clone();
            std::thread::spawn(move || {
                let ids = ids.map(|id| GameId::new(id).expect("id"));
                barrier.wait();
                let guard = crate::mutation_boundary::enter_game_mutation_boundaries(&context, ids)
                    .expect("multi-game locks");
                done_tx.send(()).expect("completion");
                drop(guard);
            });
        }
        barrier.wait();
        done_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("first lock set should complete");
        done_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("reverse lock set should complete");
    }

    #[tokio::test]
    async fn same_game_waits_while_another_game_can_progress() {
        let first_id = GameId::new("steam:lock-a").expect("id");
        let second_id = GameId::new("steam:lock-b").expect("id");
        let first = lock(&first_id).await;
        assert!(try_lock(&first_id).is_none());
        assert!(try_lock(&second_id).is_some());

        let waiting_id = first_id.clone();
        let waiter = tokio::spawn(async move { lock(&waiting_id).await });
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());
        drop(first);
        let acquired = waiter.await.expect("waiter");
        assert_eq!(acquired.game_id(), &first_id);
    }
}
