//! Shared SQL fragments for catalog DDL.

/// Default expression for `created_at` / `updated_at` integer millisecond columns.
pub(super) const MS_UNIXEPOCH_DEFAULT: &str = "CAST(unixepoch('subsec') * 1000 AS INTEGER)";

/// Builds an `updated_at` touch trigger for a table with a single-column primary key.
pub(super) fn touch_updated_at_trigger(
    trigger_name: &str,
    table_name: &str,
    pk_column: &str,
) -> String {
    debug_assert!(!trigger_name.contains(' ') && !trigger_name.contains('\''));
    debug_assert!(!table_name.contains(' ') && !table_name.contains('\''));
    debug_assert!(!pk_column.contains(' ') && !pk_column.contains('\''));
    format!(
        r#"
CREATE TRIGGER IF NOT EXISTS {trigger_name}
AFTER UPDATE ON {table_name}
FOR EACH ROW
WHEN NEW.updated_at = OLD.updated_at
BEGIN
    UPDATE {table_name}
       SET updated_at = max(
           {default},
           OLD.updated_at + 1
       )
     WHERE {pk_column} = NEW.{pk_column};
END;
"#,
        default = MS_UNIXEPOCH_DEFAULT,
    )
}
