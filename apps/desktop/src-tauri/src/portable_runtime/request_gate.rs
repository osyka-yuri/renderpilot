use std::sync::Mutex;

use super::error::{PortableRuntimeError, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestGateState {
    Open,
    Closing,
    ClosedRecoverable,
    ClosedUncertain,
}

/// Process-local request gate.  A supervisor retains cross-process admission;
/// this gate only prevents the App from issuing another portable request once a
/// transition could be uncertain.
#[derive(Debug)]
pub struct RequestGate(Mutex<RequestGateState>);

impl Default for RequestGate {
    fn default() -> Self {
        Self(Mutex::new(RequestGateState::Open))
    }
}

impl RequestGate {
    pub fn begin(&self) -> Result<()> {
        let mut state = self.0.lock().map_err(|_| {
            PortableRuntimeError::new("portable_request_gate", "request gate poisoned")
        })?;
        if *state != RequestGateState::Open {
            return Err(PortableRuntimeError::new(
                "portable_request_closed",
                "portable request gate was not open",
            ));
        }
        *state = RequestGateState::Closing;
        Ok(())
    }

    pub fn close_recoverable(&self) {
        if let Ok(mut state) = self.0.lock() {
            *state = RequestGateState::ClosedRecoverable;
        }
    }
    pub fn close_uncertain(&self) {
        if let Ok(mut state) = self.0.lock() {
            *state = RequestGateState::ClosedUncertain;
        }
    }
    pub fn is_uncertain(&self) -> bool {
        self.0
            .lock()
            .map_or(true, |state| *state == RequestGateState::ClosedUncertain)
    }
}
