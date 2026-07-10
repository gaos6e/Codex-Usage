mod ingest;
mod maintenance;
mod pricing_store;
mod sync_runs;

pub use ingest::{FileApplyResult, RetentionResult, retention_cutoff_utc_ms};
pub use maintenance::WorkspaceSettingsRecord;
pub use pricing_store::{ModelPriceInput, ModelPriceRecord, RepriceResult};
pub use sync_runs::SyncRunProgress;

use std::path::{Path, PathBuf};
use std::time::Duration;

use r2d2::{Pool, PooledConnection};
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior};

use crate::error::{AppError, AppResult};
use crate::pricing::model_strength_sort_key;

const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../../migrations/0001_initial.sql"))];

/// v2 分析库的深 Module。连接配置、迁移、池化和事务策略留在实现内部。
#[derive(Clone)]
pub struct UsageStore {
    pool: Pool<SqliteConnectionManager>,
    path: PathBuf,
}

impl UsageStore {
    pub fn open(path: impl AsRef<Path>) -> AppResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let manager = SqliteConnectionManager::file(&path)
            .with_flags(
                OpenFlags::SQLITE_OPEN_READ_WRITE
                    | OpenFlags::SQLITE_OPEN_CREATE
                    | OpenFlags::SQLITE_OPEN_FULL_MUTEX,
            )
            .with_init(configure_connection);
        let pool = Pool::builder()
            .max_size(6)
            .min_idle(Some(1))
            .connection_timeout(Duration::from_secs(5))
            .build(manager)?;

        let store = Self { pool, path };
        store.migrate()?;
        Ok(store)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> AppResult<Self> {
        let manager = SqliteConnectionManager::memory().with_init(configure_connection);
        let pool = Pool::builder().max_size(1).build(manager)?;
        let store = Self {
            pool,
            path: PathBuf::from(":memory:"),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn schema_version(&self) -> AppResult<i64> {
        self.with_reader(|connection| {
            connection
                .query_row(
                    "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                    [],
                    |row| row.get(0),
                )
                .map_err(AppError::from)
        })
    }

    pub fn integrity_check(&self) -> AppResult<bool> {
        self.with_reader(|connection| {
            let result: String =
                connection.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
            Ok(result == "ok")
        })
    }

    pub fn database_size_bytes(&self) -> AppResult<u64> {
        if self.path == Path::new(":memory:") {
            return Ok(0);
        }
        Ok(std::fs::metadata(&self.path)?.len())
    }

    pub(crate) fn with_reader<T>(
        &self,
        operation: impl FnOnce(&Connection) -> AppResult<T>,
    ) -> AppResult<T> {
        let connection = self.connection()?;
        operation(&connection)
    }

    pub(crate) fn with_writer<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> AppResult<T>,
    ) -> AppResult<T> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = operation(&transaction)?;
        transaction.commit()?;
        Ok(value)
    }

    fn connection(&self) -> AppResult<PooledConnection<SqliteConnectionManager>> {
        self.pool.get().map_err(AppError::from)
    }

    fn migrate(&self) -> AppResult<()> {
        let mut connection = self.connection()?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at_ms INTEGER NOT NULL
            ) STRICT;",
        )?;
        let current: i64 = connection.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?;

        for (version, sql) in MIGRATIONS.iter().copied().filter(|(v, _)| *v > current) {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Exclusive)?;
            transaction.execute_batch(sql)?;
            transaction.execute(
                "INSERT INTO schema_migrations(version, name, applied_at_ms)
                 VALUES (?1, ?2, CAST(unixepoch('subsec') * 1000 AS INTEGER))",
                (version, format!("migration_{version:04}")),
            )?;
            transaction.commit()?;
        }
        Ok(())
    }
}

fn configure_connection(connection: &mut Connection) -> rusqlite::Result<()> {
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.set_prepared_statement_cache_capacity(128);
    connection.create_scalar_function(
        "model_strength_key",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |context| {
            let model = context.get::<String>(0)?;
            Ok(model_strength_sort_key(&model))
        },
    )?;
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA temp_store = MEMORY;
         PRAGMA cache_size = -20000;
         PRAGMA wal_autocheckpoint = 1000;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_required_schema_and_passes_integrity_check() {
        let store = UsageStore::open_in_memory().expect("open test database");

        assert_eq!(store.schema_version().expect("schema version"), 1);
        assert!(store.integrity_check().expect("integrity check"));

        let required = [
            "source_files",
            "sync_runs",
            "workspaces",
            "sessions",
            "usage_events",
            "session_model_segments",
            "activity_segments",
            "tool_events",
            "daily_usage_rollups",
            "daily_tool_rollups",
            "model_prices",
            "app_settings",
        ];
        store
            .with_reader(|connection| {
                for table in required {
                    let count: i64 = connection.query_row(
                        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                        [table],
                        |row| row.get(0),
                    )?;
                    assert_eq!(count, 1, "missing table {table}");
                }
                Ok(())
            })
            .expect("inspect schema");
    }

    #[test]
    fn opens_file_database_in_wal_mode() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let store = UsageStore::open(directory.path().join("usage.sqlite3")).expect("open store");

        store
            .with_reader(|connection| {
                let mode: String =
                    connection.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
                assert_eq!(mode, "wal");
                Ok(())
            })
            .expect("read journal mode");
    }
}
