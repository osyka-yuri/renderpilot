//! Linear upgrade steps from a known released schema version to CURRENT.
//!
//! Each step stamps only its target version. The orchestrator runs steps until
//! `user_version == CURRENT_SCHEMA_VERSION`, then validates once.

mod util;
mod v10_to_v11;
mod v8_to_v9;
mod v9_to_v10;

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_error;

use super::version;

type StepFn = fn(&Connection) -> AppResult<()>;

/// Ordered upgrade edges: `(from_version, to_version, apply)`.
const STEPS: &[(i32, i32, StepFn)] = &[
    (8, 9, v8_to_v9::apply),
    (9, 10, v9_to_v10::apply),
    (10, 11, v10_to_v11::apply),
];

/// Runs every step from the live `user_version` until CURRENT is reached.
pub(super) fn run_from(connection: &Connection, from: i32) -> AppResult<()> {
    let mut version = from;
    while version < super::CURRENT_SCHEMA_VERSION {
        let Some(&(step_from, step_to, apply)) = STEPS.iter().find(|(f, _, _)| *f == version)
        else {
            return Err(storage_error(format!(
                "no catalog migration step from schema version {version}"
            )));
        };
        apply(connection)?;
        let stamped = version::read(connection)?;
        if stamped != step_to {
            return Err(storage_error(format!(
                "catalog migration step {step_from}→{step_to} left user_version={stamped}"
            )));
        }
        version = stamped;
    }
    Ok(())
}

/// True when `from` can reach CURRENT via [`STEPS`] (strictly older than CURRENT).
pub(super) fn can_upgrade_from(from: i32) -> bool {
    if from >= super::CURRENT_SCHEMA_VERSION {
        return false;
    }
    let mut version = from;
    while version < super::CURRENT_SCHEMA_VERSION {
        match STEPS.iter().find(|(f, _, _)| *f == version) {
            Some(&(_, to, _)) => version = to,
            None => return false,
        }
    }
    version == super::CURRENT_SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::super::CURRENT_SCHEMA_VERSION;
    use super::{STEPS, can_upgrade_from};

    #[test]
    fn can_upgrade_from_known_released_versions() {
        assert!(can_upgrade_from(8));
        assert!(can_upgrade_from(9));
    }

    #[test]
    fn can_upgrade_from_rejects_current_and_unknown() {
        assert!(!can_upgrade_from(CURRENT_SCHEMA_VERSION));
        assert!(!can_upgrade_from(7));
        assert!(!can_upgrade_from(0));
        assert!(!can_upgrade_from(999));
    }

    #[test]
    fn steps_form_a_contiguous_chain_to_current() {
        let mut version = STEPS[0].0;
        for &(from, to, _) in STEPS {
            assert_eq!(from, version);
            assert!(to > from);
            version = to;
        }
        assert_eq!(version, CURRENT_SCHEMA_VERSION);
    }
}
