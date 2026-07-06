//! Process-local gate for Forced remote-manifest refresh (cooldown + single-flight).

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Minimum interval between completed Forced CDN batches.
pub const FORCE_MANIFEST_REFRESH_COOLDOWN: Duration = Duration::from_secs(90);

/// Result of attempting to begin a Forced manifest refresh.
#[derive(Debug)]
pub enum ForceRefreshPermit<'a> {
    /// Caller may run the network batch. Drop (or explicit finish) stamps cooldown.
    Granted(ForceRefreshGuard<'a>),
    /// Another Forced batch is already running.
    SkippedInFlight,
    /// Cooldown has not elapsed since the last completed Forced batch.
    SkippedCooldown {
        /// Remaining wait before the next Forced attempt may be granted.
        retry_after: Duration,
    },
}

#[derive(Debug, Default)]
struct GateState {
    in_flight: bool,
    last_completed_at: Option<Instant>,
}

/// Process-local (or test-local) gate: single-flight + cooldown after completion.
#[derive(Debug, Default)]
pub struct ForceRefreshGate {
    state: Mutex<GateState>,
}

impl ForceRefreshGate {
    /// Creates an idle gate with no prior completion stamp.
    pub fn new() -> Self {
        Self::default()
    }

    /// Tries to begin a Forced batch under `cooldown`.
    ///
    /// On [`ForceRefreshPermit::Granted`], the returned [`ForceRefreshGuard`] must
    /// stay alive for the whole batch. Dropping it clears in-flight and stamps
    /// the cooldown (including after panic or total CDN failure so spam stays
    /// limited).
    ///
    /// Prefer not to call [`ForceRefreshGate::finish`] directly from production
    /// code — the guard owns that transition. Calling `finish` without a prior
    /// grant still stamps cooldown (defensive; see tests).
    pub fn try_begin(&self, cooldown: Duration) -> ForceRefreshPermit<'_> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if state.in_flight {
            return ForceRefreshPermit::SkippedInFlight;
        }

        if let Some(last) = state.last_completed_at {
            let elapsed = last.elapsed();
            if elapsed < cooldown {
                return ForceRefreshPermit::SkippedCooldown {
                    retry_after: cooldown.saturating_sub(elapsed),
                };
            }
        }

        state.in_flight = true;
        ForceRefreshPermit::Granted(ForceRefreshGuard { gate: self })
    }

    /// Marks the Forced batch finished: clears in-flight and stamps cooldown.
    ///
    /// Prefer dropping [`ForceRefreshGuard`]. Safe if called without a prior
    /// grant, but that still applies the cooldown stamp.
    pub fn finish(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.in_flight = false;
        state.last_completed_at = Some(Instant::now());
    }

    /// Test helper: whether a batch is currently marked in flight.
    #[cfg(test)]
    pub(crate) fn is_in_flight(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .in_flight
    }
}

/// RAII permit for one Forced CDN batch. Dropping finishes the gate.
#[derive(Debug)]
pub struct ForceRefreshGuard<'a> {
    gate: &'a ForceRefreshGate,
}

impl Drop for ForceRefreshGuard<'_> {
    fn drop(&mut self) {
        self.gate.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn grants_first_attempt() {
        let gate = ForceRefreshGate::new();
        let permit = gate.try_begin(Duration::from_secs(90));
        assert!(matches!(permit, ForceRefreshPermit::Granted(_)));
        assert!(gate.is_in_flight());
        // keep permit alive until here
        drop(permit);
    }

    #[test]
    fn second_attempt_while_in_flight_is_skipped() {
        let gate = ForceRefreshGate::new();
        let _guard = match gate.try_begin(Duration::from_secs(90)) {
            ForceRefreshPermit::Granted(guard) => guard,
            other => panic!("expected grant, got {other:?}"),
        };
        assert!(matches!(
            gate.try_begin(Duration::from_secs(90)),
            ForceRefreshPermit::SkippedInFlight
        ));
    }

    #[test]
    fn drop_guard_stamps_cooldown() {
        let gate = ForceRefreshGate::new();
        let guard = match gate.try_begin(Duration::from_secs(90)) {
            ForceRefreshPermit::Granted(guard) => guard,
            other => panic!("expected grant, got {other:?}"),
        };
        drop(guard);
        assert!(!gate.is_in_flight());

        match gate.try_begin(Duration::from_secs(90)) {
            ForceRefreshPermit::SkippedCooldown { retry_after } => {
                assert!(retry_after > Duration::ZERO);
                assert!(retry_after <= Duration::from_secs(90));
            }
            other => panic!("expected cooldown skip, got {other:?}"),
        }
    }

    #[test]
    fn zero_cooldown_allows_immediate_retry_after_finish() {
        let gate = ForceRefreshGate::new();
        {
            let _guard = match gate.try_begin(Duration::ZERO) {
                ForceRefreshPermit::Granted(guard) => guard,
                other => panic!("expected grant, got {other:?}"),
            };
        }
        assert!(matches!(
            gate.try_begin(Duration::ZERO),
            ForceRefreshPermit::Granted(_)
        ));
    }

    #[test]
    fn finish_without_begin_stamps_cooldown() {
        let gate = ForceRefreshGate::new();
        gate.finish();
        assert!(!gate.is_in_flight());
        assert!(matches!(
            gate.try_begin(Duration::from_secs(60)),
            ForceRefreshPermit::SkippedCooldown { .. }
        ));
    }
}
