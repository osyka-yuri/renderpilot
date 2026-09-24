//! Linear upgrade steps from a known released schema version to CURRENT.
//!
//! Each step stamps only its target version. The orchestrator runs steps until
//! `user_version == CURRENT_SCHEMA_VERSION`, then validates once.

mod util;
mod v10_to_v11;
mod v11_to_v12;
mod v12_to_v13;
mod v13_to_v14;
mod v14_to_v15;
mod v15_to_v16;
mod v16_to_v17;
mod v17_to_v18;
mod v18_to_v19;
mod v19_to_v20;
mod v20_to_v21;
mod v4_to_v8;
mod v8_to_v9;
mod v9_to_v10;

use renderpilot_application::AppResult;
use rusqlite::Connection;

use crate::error::storage_error;

use super::version;

type StepFn = fn(&Connection) -> AppResult<()>;

pub(in crate::schema) const MINIMUM_PORTABLE_SCHEMA_VERSION: i32 = v4_to_v8::SOURCE_VERSION;

/// Ordered upgrade edges: `(from_version, to_version, apply)`.
const STEPS: &[(i32, i32, StepFn)] = &[
    (
        v4_to_v8::SOURCE_VERSION,
        v4_to_v8::TARGET_VERSION,
        v4_to_v8::apply,
    ),
    (8, 9, v8_to_v9::apply),
    (9, 10, v9_to_v10::apply),
    (10, 11, v10_to_v11::apply),
    (11, 12, v11_to_v12::apply),
    (12, 13, v12_to_v13::apply),
    (13, 14, v13_to_v14::apply),
    (
        v14_to_v15::SOURCE_VERSION,
        v14_to_v15::TARGET_VERSION,
        v14_to_v15::apply,
    ),
    (
        v15_to_v16::SOURCE_VERSION,
        v15_to_v16::TARGET_VERSION,
        v15_to_v16::apply,
    ),
    (
        v16_to_v17::SOURCE_VERSION,
        v16_to_v17::TARGET_VERSION,
        v16_to_v17::apply,
    ),
    (
        v17_to_v18::SOURCE_VERSION,
        v17_to_v18::TARGET_VERSION,
        v17_to_v18::apply,
    ),
    (
        v18_to_v19::SOURCE_VERSION,
        v18_to_v19::TARGET_VERSION,
        v18_to_v19::apply,
    ),
    (
        v19_to_v20::SOURCE_VERSION,
        v19_to_v20::TARGET_VERSION,
        v19_to_v20::apply,
    ),
    (
        v20_to_v21::SOURCE_VERSION,
        v20_to_v21::TARGET_VERSION,
        v20_to_v21::apply,
    ),
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

/// Runs the real released migration chain to a bounded test target.
///
/// Production always uses [`run_from`] through CURRENT.  Tests use this only
/// to construct an immutable v15/v16 released catalog without applying
/// CURRENT and then reverse-engineering its objects.
#[cfg(any(test, feature = "test-instrumentation"))]
pub(super) fn run_to_for_test(connection: &Connection, from: i32, target: i32) -> AppResult<()> {
    if target < from || target > super::CURRENT_SCHEMA_VERSION {
        return Err(storage_error(format!(
            "invalid bounded migration target {from}→{target}"
        )));
    }

    let mut version = from;
    while version < target {
        let Some(&(step_from, step_to, apply)) = STEPS.iter().find(|(f, _, _)| *f == version)
        else {
            return Err(storage_error(format!(
                "no catalog migration step from schema version {version}"
            )));
        };
        if step_to > target {
            return Err(storage_error(format!(
                "schema version {target} is not an exact released migration target"
            )));
        }
        apply(connection)?;
        let stamped = version::read(connection)?;
        if stamped != step_to {
            return Err(storage_error(format!(
                "catalog migration step {step_from}→{step_to} left user_version={stamped}"
            )));
        }
        version = stamped;
    }
    if version != target {
        return Err(storage_error(format!(
            "bounded migration expected schema version {target}, found {version}"
        )));
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
pub(super) fn run_v10_to_v11_for_test(connection: &Connection) -> AppResult<()> {
    v10_to_v11::apply(connection)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::super::CURRENT_SCHEMA_VERSION;
    use super::{STEPS, can_upgrade_from};

    #[test]
    fn can_upgrade_from_known_released_versions() {
        assert!(can_upgrade_from(4));
        assert!(can_upgrade_from(8));
        assert!(can_upgrade_from(9));
        assert!(can_upgrade_from(11));
    }

    #[test]
    fn can_upgrade_from_rejects_current_and_unknown() {
        assert!(!can_upgrade_from(CURRENT_SCHEMA_VERSION));
        assert!(!can_upgrade_from(3));
        assert!(!can_upgrade_from(5));
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

    #[test]
    fn v19_to_v20_adds_both_nullable_object_json_columns() {
        let connection = Connection::open_in_memory().expect("sqlite");
        connection
            .execute_batch(
                "CREATE TABLE installed_addons (
                     game_id TEXT PRIMARY KEY NOT NULL,
                     kind TEXT NOT NULL,
                     addon_file TEXT NOT NULL,
                     addon_version TEXT,
                     created_files_json TEXT NOT NULL,
                     backed_up_files_json TEXT NOT NULL,
                     managed_files_json TEXT NOT NULL,
                     tracked_sources_json TEXT NOT NULL,
                     host_kind TEXT,
                     reshade_channel TEXT,
                     registered_exe_path TEXT,
                     created_at INTEGER NOT NULL,
                     updated_at INTEGER NOT NULL
                 ) STRICT;
                 INSERT INTO installed_addons
                     (game_id, kind, addon_file, created_files_json,
                      backed_up_files_json, managed_files_json, tracked_sources_json,
                      created_at, updated_at)
                 VALUES ('steam:v19', 'renodx', 'C:/Game/renodx.addon64',
                         '[\"C:/Game/renodx.addon64\"]', '[]', '[]', '[]', 1, 1);
                 PRAGMA user_version = 19;",
            )
            .expect("released v19 fixture");

        super::v19_to_v20::apply(&connection).expect("v19 to v20");
        let version: i32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, 20);
        let addon_file: String = connection
            .query_row(
                "SELECT addon_file FROM installed_addons WHERE game_id = 'steam:v19'",
                [],
                |row| row.get(0),
            )
            .expect("legacy row");
        assert_eq!(addon_file, "C:/Game/renodx.addon64");
        let receipt: Option<String> = connection
            .query_row(
                "SELECT renodx_config_receipt_json FROM installed_addons WHERE game_id = 'steam:v19'",
                [],
                |row| row.get(0),
            )
            .expect("legacy receipt");
        assert_eq!(receipt, None);
        let journal: Option<String> = connection
            .query_row(
                "SELECT engine_config_journal_json FROM installed_addons WHERE game_id = 'steam:v19'",
                [],
                |row| row.get(0),
            )
            .expect("legacy journal");
        assert_eq!(journal, None);

        for (column, value) in [
            ("renodx_config_receipt_json", "[]"),
            ("engine_config_journal_json", "[]"),
        ] {
            let error = connection
                .execute(
                    &format!(
                        "UPDATE installed_addons SET {column} = ?1 WHERE game_id = 'steam:v19'"
                    ),
                    [value],
                )
                .expect_err("non-object JSON must be rejected");
            assert!(error.to_string().contains("CHECK constraint failed"));

            let error = connection
                .execute(
                    &format!(
                        "UPDATE installed_addons SET {column} = ?1 WHERE game_id = 'steam:v19'"
                    ),
                    ["not-json"],
                )
                .expect_err("malformed JSON must be rejected");
            assert!(error.to_string().contains("CHECK constraint failed"));
        }
    }
}
