//! Immutable admission contracts for released portable catalog versions.
//!
//! A released catalog is identified by a digest of a deterministic, read-only
//! SQLite schema projection. Keeping the projection here, instead of copying
//! the current schema registry, makes this migration boundary independent from
//! future schema changes while retaining the SQLite semantics needed to reject
//! a look-alike database.

use renderpilot_application::AppResult;
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::error::storage_error;

const PROJECTION_VERSION: &str = "portable-schema-projection-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ReleasedPortableCatalogContract {
    pub(super) version: i32,
    digest: &'static str,
}

impl ReleasedPortableCatalogContract {
    pub(super) const V15: Self = Self {
        version: 15,
        digest: V15_DIGEST,
    };

    pub(super) const V16: Self = Self {
        version: 16,
        digest: V16_DIGEST,
    };

    pub(super) fn validate_observational(self, connection: &Connection) -> AppResult<()> {
        let observed = projection_digest(connection)?;
        if observed == self.digest {
            Ok(())
        } else {
            Err(storage_error(format!(
                "released portable catalog v{} schema digest mismatch: expected {}, found {}",
                self.version, self.digest, observed
            )))
        }
    }
}

// These constants are intentionally immutable. They were generated from the
// real released v4 fixture by applying only the historical migration chain
// through v15 and v16, respectively. The release boundaries are commits
// `010f7b3e` (v15) and `80da673f` (v16). Do not regenerate them from CURRENT.
const V15_DIGEST: &str = "ccfdf6ea25ac33905aa08958726b71c039ab5ff6b9969953a33d797434f7826f";
const V16_DIGEST: &str = "9c052a4d42ae4d9cd83f2760a1df9b3ea062e44da3a3937424b8c2f57013e2ae";

fn projection_digest(connection: &Connection) -> AppResult<String> {
    let projection = schema_projection(connection)?;
    let digest = Sha256::digest(projection.as_bytes());
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn schema_projection(connection: &Connection) -> AppResult<String> {
    let mut records = Vec::new();
    let user_version: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| {
            storage_error(format!("could not inspect released user_version: {error}"))
        })?;
    records.push(format!("projection|{PROJECTION_VERSION}"));
    records.push(format!("user_version|{user_version}"));

    collect_schema_objects(connection, &mut records)?;
    collect_table_list(connection, &mut records)?;

    let object_names = non_internal_table_and_view_names(connection)?;
    for table in &object_names {
        collect_table_xinfo(connection, table, &mut records)?;
    }
    for table in &object_names {
        collect_foreign_keys(connection, table, &mut records)?;
        collect_indexes(connection, table, &mut records)?;
    }

    records.sort_unstable();
    Ok(records.join("\n") + "\n")
}

fn collect_schema_objects(connection: &Connection, records: &mut Vec<String>) -> AppResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT type, name, tbl_name, sql
             FROM sqlite_master
             WHERE name NOT LIKE 'sqlite_%'
             ORDER BY type, name",
        )
        .map_err(|error| storage_error(format!("could not prepare released objects: {error}")))?;
    let rows = statement
        .query_map([], |row| {
            let kind: String = row.get(0)?;
            let name: String = row.get(1)?;
            let owner: String = row.get(2)?;
            let sql: Option<String> = row.get(3)?;
            Ok((kind, name, owner, sql))
        })
        .map_err(|error| storage_error(format!("could not query released objects: {error}")))?;
    for row in rows {
        let (kind, name, owner, sql) = row
            .map_err(|error| storage_error(format!("could not read released objects: {error}")))?;
        records.push(format!(
            "object|{}|{}|{}|{}",
            field(&kind),
            field(&name),
            field(&owner),
            nullable_field(sql.as_deref().map(normalize_sql).as_deref())
        ));
    }
    Ok(())
}

fn collect_table_list(connection: &Connection, records: &mut Vec<String>) -> AppResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT schema, name, type, ncol, wr, strict
             FROM pragma_table_list
             WHERE name NOT LIKE 'sqlite_%'
             ORDER BY schema, name",
        )
        .map_err(|error| {
            storage_error(format!("could not prepare released table list: {error}"))
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .map_err(|error| storage_error(format!("could not query released table list: {error}")))?;
    for row in rows {
        let (schema, name, kind, ncol, without_rowid, strict) = row.map_err(|error| {
            storage_error(format!("could not read released table list: {error}"))
        })?;
        records.push(format!(
            "table_list|{}|{}|{}|{ncol}|{without_rowid}|{strict}",
            field(&schema),
            field(&name),
            field(&kind)
        ));
    }
    Ok(())
}

fn non_internal_table_and_view_names(connection: &Connection) -> AppResult<Vec<String>> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_master
             WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%'
             ORDER BY name",
        )
        .map_err(|error| {
            storage_error(format!("could not prepare released table names: {error}"))
        })?;
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| storage_error(format!("could not query released table names: {error}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| storage_error(format!("could not read released table names: {error}")))
}

fn collect_table_xinfo(
    connection: &Connection,
    table: &str,
    records: &mut Vec<String>,
) -> AppResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT cid, name, type, \"notnull\", dflt_value, pk, hidden
             FROM pragma_table_xinfo(?1) ORDER BY cid",
        )
        .map_err(|error| {
            storage_error(format!("could not prepare released table_xinfo: {error}"))
        })?;
    let rows = statement
        .query_map([table], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })
        .map_err(|error| storage_error(format!("could not query released table_xinfo: {error}")))?;
    for row in rows {
        let (cid, name, kind, not_null, default_value, primary_key, hidden) =
            row.map_err(|error| {
                storage_error(format!("could not read released table_xinfo: {error}"))
            })?;
        records.push(format!(
            "table_xinfo|{}|{cid}|{}|{}|{not_null}|{}|{primary_key}|{hidden}",
            field(table),
            field(&name),
            field(&kind),
            nullable_field(default_value.as_deref())
        ));
    }
    Ok(())
}

fn collect_foreign_keys(
    connection: &Connection,
    table: &str,
    records: &mut Vec<String>,
) -> AppResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT id, seq, \"table\", \"from\", \"to\", on_update, on_delete, match
             FROM pragma_foreign_key_list(?1) ORDER BY id, seq",
        )
        .map_err(|error| {
            storage_error(format!("could not prepare released foreign keys: {error}"))
        })?;
    let rows = statement
        .query_map([table], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(|error| {
            storage_error(format!("could not query released foreign keys: {error}"))
        })?;
    for row in rows {
        let (id, seq, target, from, to, on_update, on_delete, match_kind) =
            row.map_err(|error| {
                storage_error(format!("could not read released foreign keys: {error}"))
            })?;
        records.push(format!(
            "foreign_key|{}|{id}|{seq}|{}|{}|{}|{}|{}|{}",
            field(table),
            field(&target),
            field(&from),
            nullable_field(to.as_deref()),
            field(&on_update),
            field(&on_delete),
            field(&match_kind)
        ));
    }
    Ok(())
}

fn collect_indexes(
    connection: &Connection,
    table: &str,
    records: &mut Vec<String>,
) -> AppResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT seq, name, \"unique\", origin, partial
             FROM pragma_index_list(?1) ORDER BY seq",
        )
        .map_err(|error| storage_error(format!("could not prepare released indexes: {error}")))?;
    let rows = statement
        .query_map([table], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|error| storage_error(format!("could not query released indexes: {error}")))?;
    for row in rows {
        let (seq, name, unique, origin, partial) = row
            .map_err(|error| storage_error(format!("could not read released indexes: {error}")))?;
        let canonical_name = canonical_index_name(&name);
        records.push(format!(
            "index_list|{}|{seq}|{}|{unique}|{}|{partial}",
            field(table),
            field(&canonical_name),
            field(&origin)
        ));
        collect_index_xinfo(connection, table, &name, &canonical_name, records)?;
    }
    Ok(())
}

fn collect_index_xinfo(
    connection: &Connection,
    table: &str,
    actual_name: &str,
    canonical_name: &str,
    records: &mut Vec<String>,
) -> AppResult<()> {
    let mut statement = connection
        .prepare(
            "SELECT seqno, cid, name, \"desc\", coll, key
             FROM pragma_index_xinfo(?1) ORDER BY seqno",
        )
        .map_err(|error| {
            storage_error(format!("could not prepare released index_xinfo: {error}"))
        })?;
    let rows = statement
        .query_map([actual_name], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .map_err(|error| storage_error(format!("could not query released index_xinfo: {error}")))?;
    for row in rows {
        let (seqno, cid, name, descending, collation, key) = row.map_err(|error| {
            storage_error(format!("could not read released index_xinfo: {error}"))
        })?;
        records.push(format!(
            "index_xinfo|{}|{}|{seqno}|{cid}|{}|{descending}|{}|{key}",
            field(table),
            field(canonical_name),
            nullable_field(name.as_deref()),
            field(&collation)
        ));
    }
    Ok(())
}

fn canonical_index_name(name: &str) -> String {
    if name.starts_with("sqlite_autoindex_") {
        "<autoindex>".to_owned()
    } else {
        name.to_owned()
    }
}

fn field(value: &str) -> String {
    format!("{}:{value}", value.len())
}

fn nullable_field(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("value:{}:{value}", value.len()),
        None => "null".to_owned(),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SqlToken {
    text: String,
}

fn normalize_sql(sql: &str) -> String {
    let mut tokens = tokenize_sql(sql);
    remove_create_if_not_exists(&mut tokens);
    tokens
        .into_iter()
        .map(|token| format!("{}:{}", token.text.len(), token.text))
        .collect::<Vec<_>>()
        .join("|")
}

fn tokenize_sql(sql: &str) -> Vec<SqlToken> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            byte if byte.is_ascii_whitespace() => index += 1,
            b'-' if bytes.get(index + 1) == Some(&b'-') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index += 2;
                while index + 1 < bytes.len() && !(bytes[index] == b'*' && bytes[index + 1] == b'/')
                {
                    index += 1;
                }
                index = (index + 2).min(bytes.len());
            }
            b'\'' => {
                let start = index;
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\'' {
                        index += 1;
                        if bytes.get(index) == Some(&b'\'') {
                            index += 1;
                            continue;
                        }
                        break;
                    }
                    index += 1;
                }
                tokens.push(SqlToken {
                    text: sql[start..index].to_owned(),
                });
            }
            b'"' | b'`' => {
                let delimiter = bytes[index];
                index += 1;
                let mut text = String::new();
                while index < bytes.len() {
                    if bytes[index] == delimiter {
                        index += 1;
                        if bytes.get(index) == Some(&delimiter) {
                            text.push(delimiter as char);
                            index += 1;
                            continue;
                        }
                        break;
                    }
                    text.push(bytes[index] as char);
                    index += 1;
                }
                tokens.push(SqlToken {
                    text: text.to_ascii_lowercase(),
                });
            }
            b'[' => {
                index += 1;
                let mut text = String::new();
                while index < bytes.len() && bytes[index] != b']' {
                    text.push(bytes[index] as char);
                    index += 1;
                }
                index = (index + 1).min(bytes.len());
                tokens.push(SqlToken {
                    text: text.to_ascii_lowercase(),
                });
            }
            byte if byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80 => {
                let start = index;
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric()
                        || bytes[index] == b'_'
                        || bytes[index] >= 0x80)
                {
                    index += 1;
                }
                tokens.push(SqlToken {
                    text: sql[start..index].to_ascii_lowercase(),
                });
            }
            _ => {
                let start = index;
                index += 1;
                if index < bytes.len() {
                    let pair = &bytes[start..=index];
                    if matches!(
                        pair,
                        b"<=" | b">=" | b"<>" | b"!=" | b"==" | b"||" | b"<<" | b">>" | b"->"
                    ) {
                        index += 1;
                    }
                }
                tokens.push(SqlToken {
                    text: sql[start..index].to_owned(),
                });
            }
        }
    }
    tokens
}

fn remove_create_if_not_exists(tokens: &mut Vec<SqlToken>) {
    let Some(create) = tokens.iter().position(|token| token.text == "create") else {
        return;
    };
    let end = (create + 10).min(tokens.len());
    let Some(if_not_exists) = (create..end).find(|&index| {
        tokens[index].text == "if"
            && tokens
                .get(index + 1)
                .is_some_and(|token| token.text == "not")
            && tokens
                .get(index + 2)
                .is_some_and(|token| token.text == "exists")
    }) else {
        return;
    };
    tokens.drain(if_not_exists..if_not_exists + 3);
}
