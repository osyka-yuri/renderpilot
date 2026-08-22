use super::*;

#[test]
fn apply_migrates_v17_to_v18_with_the_singleton_shared_vulkan_fence() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("current baseline");
    connection
        .execute_batch(
            "DROP TABLE pending_shared_vulkan_mutations;
             PRAGMA user_version = 17;",
        )
        .expect("reduce to v17");

    apply(&mut connection).expect("v17 to v18");

    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    let columns = super::super::validation::physical_column_mismatches(&connection)
        .expect("physical contract")
        .into_iter()
        .filter(|value| value.contains("pending_shared_vulkan_mutations"))
        .collect::<Vec<_>>();
    assert!(
        columns.is_empty(),
        "shared mutation schema drift: {columns:?}"
    );
    let resource_key: String = connection
        .query_row(
            "SELECT resource_key FROM pending_shared_vulkan_mutations",
            [],
            |row| row.get(0),
        )
        .unwrap_or_else(|_| "absent".to_owned());
    assert_eq!(resource_key, "absent");
}

#[test]
fn malformed_current_shared_vulkan_constraints_trigger_a_rebuild() {
    let mut connection = open_test_connection();
    apply(&mut connection).expect("current baseline");
    connection
        .execute_batch(
            "DROP TABLE pending_shared_vulkan_mutations;
             CREATE TABLE pending_shared_vulkan_mutations (
                 resource_key TEXT PRIMARY KEY NOT NULL,
                 id TEXT UNIQUE NOT NULL,
                 scope TEXT NOT NULL,
                 game_id TEXT,
                 feature TEXT NOT NULL,
                 state TEXT NOT NULL,
                 manifest_json TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 updated_at INTEGER NOT NULL
             ) STRICT;
             PRAGMA user_version = 18;",
        )
        .expect("malformed current fixture");

    apply(&mut connection).expect("malformed current should rebuild");
    assert_eq!(user_version(&connection), CURRENT_SCHEMA_VERSION);
    assert!(
        super::super::ddl::pending_shared_vulkan_mutations::validates_observational(&connection)
            .expect("shared schema validation")
    );
}
