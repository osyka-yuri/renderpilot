use getrandom::fill;

use super::error::{PortableRuntimeError, Result};

/// The portable runtime has one CSPRNG boundary.  Transaction, pipe, epoch,
/// permit, and provenance nonces must never be derived from a PID, clock, or
/// mutable filesystem state.
pub(super) fn hex_32() -> Result<String> {
    let mut bytes = [0_u8; 32];
    fill(&mut bytes)
        .map_err(|error| PortableRuntimeError::new("portable_random", error.to_string()))?;
    Ok(hex(&bytes))
}

pub(super) fn bytes_32() -> Result<[u8; 32]> {
    let mut bytes = [0_u8; 32];
    fill(&mut bytes)
        .map_err(|error| PortableRuntimeError::new("portable_random", error.to_string()))?;
    Ok(bytes)
}

pub(super) fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}
