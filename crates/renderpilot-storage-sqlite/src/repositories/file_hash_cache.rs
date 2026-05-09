use renderpilot_application::AppResult;
use renderpilot_domain::{Sha256Hash, Version};
use rusqlite::params;

use crate::error::{invalid_row, storage_error};

use super::SqliteStorage;

/// A single row loaded from or written to the `file_hash_cache` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHashCacheRow {
    /// Normalized file path.
    pub path: String,
    /// File size in bytes.
    pub size: u64,
    /// Last-modified time as Unix milliseconds.
    pub modified_at: u64,
    /// SHA-256 hash of the file contents.
    pub sha256: Sha256Hash,
    /// Windows PE file version, or `None` if the file has none.
    pub version: Option<Version>,
}

const LOAD_FILE_HASH_CACHE_SQL: &str = "
    SELECT
        path AS cache_path,
        size AS cache_size,
        modified_at AS cache_modified_at,
        sha256 AS cache_sha256,
        version AS cache_version
    FROM file_hash_cache
    WHERE path = ?1
       OR path LIKE ?2 ESCAPE '\\'
    ORDER BY path
";

const LOAD_ALL_FILE_HASH_CACHE_SQL: &str = "
    SELECT
        path AS cache_path,
        size AS cache_size,
        modified_at AS cache_modified_at,
        sha256 AS cache_sha256,
        version AS cache_version
    FROM file_hash_cache
    ORDER BY path
";

const UPSERT_FILE_HASH_CACHE_SQL: &str = "
    INSERT INTO file_hash_cache
        (path, size, modified_at, sha256, version, updated_at)
    VALUES
        (?1, ?2, ?3, ?4, ?5, unixepoch('subsec') * 1000)
    ON CONFLICT(path) DO UPDATE SET
        size        = excluded.size,
        modified_at = excluded.modified_at,
        sha256      = excluded.sha256,
        version     = excluded.version,
        updated_at  = excluded.updated_at
";

const COL_PATH: &str = "cache_path";
const COL_SIZE: &str = "cache_size";
const COL_MODIFIED_AT: &str = "cache_modified_at";
const COL_SHA256: &str = "cache_sha256";
const COL_VERSION: &str = "cache_version";

impl SqliteStorage {
    /// Loads cache rows for `path_prefix`.
    ///
    /// The scope check is boundary-safe:
    ///
    /// ```text
    /// C:/Games/Game      matches C:/Games/Game/nvngx_dlss.dll
    /// C:/Games/Game      does not match C:/Games/GameExtra/nvngx_dlss.dll
    /// ```
    ///
    /// Bad persisted rows are treated as storage data errors and fail mapping.
    pub fn load_file_hash_cache(&self, path_prefix: &str) -> AppResult<Vec<FileHashCacheRow>> {
        let scope = CachePathScope::new(path_prefix);

        self.query_list(
            LOAD_FILE_HASH_CACHE_SQL,
            params![scope.exact_path(), scope.descendants_like_pattern()],
            |row| Ok(cache_row_from_sql(row)),
        )
    }

    /// Loads every cache row in one round-trip.
    ///
    /// Used by the auto-scan path, which scans many install directories in a
    /// row and benefits from a single bulk read of the cache instead of one
    /// `SELECT … LIKE` per game. The detector itself only consults rows whose
    /// path lies under the install directory it is currently scanning, so
    /// extra rows in the in-memory cache are harmless (they simply never
    /// match).
    pub fn load_all_file_hash_cache(&self) -> AppResult<Vec<FileHashCacheRow>> {
        self.query_list(LOAD_ALL_FILE_HASH_CACHE_SQL, params![], |row| {
            Ok(cache_row_from_sql(row))
        })
    }

    /// Upserts cache rows by normalized path.
    ///
    /// Existing rows are updated in place. Empty input is a no-op.
    pub fn save_file_hash_cache(&self, entries: &[FileHashCacheRow]) -> AppResult<()> {
        if entries.is_empty() {
            return Ok(());
        }

        self.with_transaction(|transaction| {
            let mut statement = transaction
                .prepare_cached(UPSERT_FILE_HASH_CACHE_SQL)
                .map_err(storage_error)?;

            for entry in entries {
                execute_cache_row_upsert(&mut statement, entry)?;
            }

            Ok(())
        })
    }
}

fn execute_cache_row_upsert(
    statement: &mut rusqlite::CachedStatement<'_>,
    entry: &FileHashCacheRow,
) -> AppResult<()> {
    let size = sqlite_integer(entry.size, "file_hash_cache.size")?;
    let modified_at = sqlite_integer(entry.modified_at, "file_hash_cache.modified_at")?;
    let version = entry.version.as_ref().map(Version::as_str);

    statement
        .execute(params![
            entry.path.as_str(),
            size,
            modified_at,
            entry.sha256.as_str(),
            version,
        ])
        .map_err(storage_error)?;

    Ok(())
}

fn cache_row_from_sql(row: &rusqlite::Row<'_>) -> AppResult<FileHashCacheRow> {
    let path = read_string(row, COL_PATH)?;
    let size = read_u64(row, COL_SIZE, "file_hash_cache.size")?;
    let modified_at = read_u64(row, COL_MODIFIED_AT, "file_hash_cache.modified_at")?;
    let sha256 = read_sha256(row, COL_SHA256)?;
    let version = read_version(row, COL_VERSION)?;

    Ok(FileHashCacheRow {
        path,
        size,
        modified_at,
        sha256,
        version,
    })
}

fn read_string(row: &rusqlite::Row<'_>, column_name: &str) -> AppResult<String> {
    row.get(column_name).map_err(storage_error)
}

fn read_optional_string(row: &rusqlite::Row<'_>, column_name: &str) -> AppResult<Option<String>> {
    row.get(column_name).map_err(storage_error)
}

fn read_i64(row: &rusqlite::Row<'_>, column_name: &str) -> AppResult<i64> {
    row.get(column_name).map_err(storage_error)
}

fn read_u64(row: &rusqlite::Row<'_>, column_name: &str, entity_column: &str) -> AppResult<u64> {
    let value = read_i64(row, column_name)?;

    unsigned_integer(value, entity_column)
}

fn read_sha256(row: &rusqlite::Row<'_>, column_name: &str) -> AppResult<Sha256Hash> {
    let value = read_string(row, column_name)?;

    Sha256Hash::new(value).map_err(invalid_row)
}

fn read_version(row: &rusqlite::Row<'_>, column_name: &str) -> AppResult<Option<Version>> {
    let value = read_optional_string(row, column_name)?;

    value
        .map(|version| Version::parse(&version).map_err(invalid_row))
        .transpose()
}

fn unsigned_integer(value: i64, column: &str) -> AppResult<u64> {
    u64::try_from(value).map_err(|_| invalid_row(format!("{column} must be non-negative")))
}

fn sqlite_integer(value: u64, column: &str) -> AppResult<i64> {
    i64::try_from(value)
        .map_err(|_| invalid_row(format!("{column} does not fit in sqlite INTEGER")))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CachePathScope {
    exact_path: String,
    descendants_like_pattern: String,
}

impl CachePathScope {
    fn new(path_prefix: &str) -> Self {
        let exact_path = trim_trailing_separators_except_root(path_prefix).to_owned();
        let descendants_prefix = descendants_prefix(&exact_path);
        let descendants_like_pattern = format!("{}%", escape_like(&descendants_prefix));

        Self {
            exact_path,
            descendants_like_pattern,
        }
    }

    fn exact_path(&self) -> &str {
        &self.exact_path
    }

    fn descendants_like_pattern(&self) -> &str {
        &self.descendants_like_pattern
    }
}

fn descendants_prefix(path: &str) -> String {
    if path.is_empty() || is_root_path(path) || path.ends_with('/') {
        path.to_owned()
    } else {
        format!("{path}/")
    }
}

fn trim_trailing_separators_except_root(path: &str) -> &str {
    let mut trimmed = path;

    while has_redundant_trailing_separator(trimmed) {
        trimmed = &trimmed[..trimmed.len() - 1];
    }

    trimmed
}

fn has_redundant_trailing_separator(path: &str) -> bool {
    path.len() > 1 && path.ends_with('/') && !is_windows_drive_root(path)
}

fn is_root_path(path: &str) -> bool {
    path == "/" || is_windows_drive_root(path)
}

fn is_windows_drive_root(path: &str) -> bool {
    let bytes = path.as_bytes();

    bytes.len() == 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'/'
}

/// Escapes `%`, `_`, and `\` for `LIKE ... ESCAPE '\'`.
fn escape_like(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());

    for ch in value.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            escaped.push('\\');
        }

        escaped.push(ch);
    }

    escaped
}

#[cfg(test)]
mod tests {
    use renderpilot_application::AppErrorKind;
    use renderpilot_domain::{Sha256Hash, Version};

    use super::{escape_like, sqlite_integer, CachePathScope, FileHashCacheRow};
    use crate::SqliteStorage;

    const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const HASH_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const HASH_D: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

    #[test]
    fn file_hash_cache_roundtrips_by_escaped_prefix_and_upserts() {
        let storage = SqliteStorage::in_memory().expect("sqlite should open");

        let prefix = "C:/Games/Percent_%";
        let first_path = "C:/Games/Percent_%/GameA/nvngx_dlss.dll";
        let second_path = "C:/Games/Percent_%/GameB/nvngx_dlss.dll";

        storage
            .save_file_hash_cache(&[
                row(first_path, 10, 100, HASH_A, Some("1.0.0")),
                row(second_path, 20, 200, HASH_B, None),
                row(
                    "C:/Games/PercentAX/GameC/nvngx_dlss.dll",
                    30,
                    300,
                    HASH_C,
                    None,
                ),
            ])
            .expect("cache rows should save");

        storage
            .save_file_hash_cache(&[row(first_path, 11, 101, HASH_D, Some("2.0.0"))])
            .expect("cache row should upsert");

        let rows = storage
            .load_file_hash_cache(prefix)
            .expect("cache rows should load");

        assert_eq!(rows.len(), 2, "LIKE prefix must escape '%' and '_'");
        assert!(rows.iter().any(|row| row.path == second_path));

        let first = find_row(&rows, first_path);

        assert_eq!(first.size, 11);
        assert_eq!(first.modified_at, 101);
        assert_eq!(first.sha256, hash(HASH_D));
        assert_eq!(first.version, Some(parse_version("2.0.0")));
    }

    #[test]
    fn file_hash_cache_load_is_boundary_safe() {
        let storage = SqliteStorage::in_memory().expect("sqlite should open");

        let scoped_path = "C:/Games/Game/nvngx_dlss.dll";
        let sibling_prefix_path = "C:/Games/GameExtra/nvngx_dlss.dll";

        storage
            .save_file_hash_cache(&[
                row(scoped_path, 10, 100, HASH_A, None),
                row(sibling_prefix_path, 20, 200, HASH_B, None),
            ])
            .expect("cache rows should save");

        let rows = storage
            .load_file_hash_cache("C:/Games/Game")
            .expect("cache rows should load");

        assert_eq!(
            rows.len(),
            1,
            "cache prefix must not match sibling paths with the same string prefix",
        );
        assert_eq!(rows[0].path, scoped_path);
    }

    #[test]
    fn file_hash_cache_load_includes_exact_scope_path() {
        let storage = SqliteStorage::in_memory().expect("sqlite should open");

        let exact_path = "C:/Games/Game";
        let child_path = "C:/Games/Game/nvngx_dlss.dll";

        storage
            .save_file_hash_cache(&[
                row(exact_path, 10, 100, HASH_A, None),
                row(child_path, 20, 200, HASH_B, None),
            ])
            .expect("cache rows should save");

        let rows = storage
            .load_file_hash_cache(exact_path)
            .expect("cache rows should load");

        assert_eq!(rows.len(), 2);
        assert!(rows.iter().any(|row| row.path == exact_path));
        assert!(rows.iter().any(|row| row.path == child_path));
    }

    #[test]
    fn file_hash_cache_load_handles_trailing_separator_in_prefix() {
        let storage = SqliteStorage::in_memory().expect("sqlite should open");

        let path = "C:/Games/Game/nvngx_dlss.dll";

        storage
            .save_file_hash_cache(&[row(path, 10, 100, HASH_A, None)])
            .expect("cache rows should save");

        let rows = storage
            .load_file_hash_cache("C:/Games/Game/")
            .expect("cache rows should load");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, path);
    }

    #[test]
    fn file_hash_cache_load_from_windows_drive_root_covers_volume_children() {
        let storage = SqliteStorage::in_memory().expect("sqlite should open");

        let drive_child = "D:/SteamLibrary/Game/nvngx_dlss.dll";
        let other_drive_child = "E:/SteamLibrary/Game/nvngx_dlss.dll";

        storage
            .save_file_hash_cache(&[
                row(drive_child, 10, 100, HASH_A, None),
                row(other_drive_child, 20, 200, HASH_B, None),
            ])
            .expect("cache rows should save");

        let rows = storage
            .load_file_hash_cache("D:/")
            .expect("cache rows should load");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].path, drive_child);
    }

    #[test]
    fn file_hash_cache_rejects_values_that_do_not_fit_sqlite_integer() {
        let error = sqlite_integer(u64::MAX, "file_hash_cache.size")
            .expect_err("oversized cache value should fail");

        assert_eq!(error.kind(), &AppErrorKind::StorageFailed);
        assert!(error.message().contains("does not fit"));
    }

    #[test]
    fn file_hash_cache_schema_rejects_negative_metadata() {
        let storage = SqliteStorage::in_memory().expect("sqlite should open");
        let connection = storage.connection().expect("sqlite connection should lock");

        let error = connection
            .execute(
                "INSERT INTO file_hash_cache (path, size, modified_at, sha256)
                 VALUES (?1, ?2, ?3, ?4)",
                ("C:/Games/Game/nvngx_dlss.dll", -1_i64, 1_i64, HASH_A),
            )
            .expect_err("negative size should violate CHECK constraint");

        assert!(error.to_string().contains("CHECK constraint failed"));
    }

    #[test]
    fn cache_path_scope_uses_boundary_safe_descendant_pattern() {
        let scope = CachePathScope::new("C:/Games/Game");

        assert_eq!(scope.exact_path(), "C:/Games/Game");
        assert_eq!(scope.descendants_like_pattern(), "C:/Games/Game/%");
    }

    #[test]
    fn cache_path_scope_trims_trailing_separator_except_drive_root() {
        let regular_scope = CachePathScope::new("C:/Games/Game/");
        let drive_scope = CachePathScope::new("D:/");

        assert_eq!(regular_scope.exact_path(), "C:/Games/Game");
        assert_eq!(regular_scope.descendants_like_pattern(), "C:/Games/Game/%");

        assert_eq!(drive_scope.exact_path(), "D:/");
        assert_eq!(drive_scope.descendants_like_pattern(), "D:/%");
    }

    #[test]
    fn like_escape_escapes_wildcards_and_escape_char() {
        assert_eq!(escape_like(r"Percent_%\Game"), r"Percent\_\%\\Game");
    }

    fn row(
        path: &str,
        size: u64,
        modified_at: u64,
        sha256: &str,
        version: Option<&str>,
    ) -> FileHashCacheRow {
        FileHashCacheRow {
            path: path.to_owned(),
            size,
            modified_at,
            sha256: hash(sha256),
            version: version.map(parse_version),
        }
    }

    fn find_row<'a>(rows: &'a [FileHashCacheRow], path: &str) -> &'a FileHashCacheRow {
        rows.iter()
            .find(|row| row.path == path)
            .unwrap_or_else(|| panic!("expected row for path `{path}`"))
    }

    fn hash(value: &str) -> Sha256Hash {
        Sha256Hash::new(value).expect("sha256 should be valid")
    }

    fn parse_version(value: &str) -> Version {
        Version::parse(value).expect("version should be valid")
    }
}
