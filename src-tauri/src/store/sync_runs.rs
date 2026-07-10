use rusqlite::params;

use super::UsageStore;
use crate::error::AppResult;

#[derive(Debug, Clone)]
pub struct SyncRunProgress<'a> {
    pub id: &'a str,
    pub status: &'a str,
    pub stage: &'a str,
    pub files_completed: u64,
    pub bytes_read: u64,
    pub events_written: u64,
    pub records_skipped: u64,
    pub parse_failures: u64,
    pub elapsed_ms: u64,
    pub error_code: Option<&'a str>,
}

impl UsageStore {
    pub fn begin_sync_run(
        &self,
        id: &str,
        mode: &str,
        files_total: u64,
        bytes_total: u64,
        started_at_ms: i64,
    ) -> AppResult<()> {
        self.with_writer(|transaction| {
            transaction.execute(
                "INSERT INTO sync_runs (
                    id, mode, status, stage, started_at_ms, files_total, bytes_total
                 ) VALUES (?1, ?2, 'running', 'planning', ?3, ?4, ?5)",
                params![
                    id,
                    mode,
                    started_at_ms,
                    to_i64(files_total),
                    to_i64(bytes_total)
                ],
            )?;
            Ok(())
        })
    }

    pub fn update_sync_run(&self, progress: &SyncRunProgress<'_>) -> AppResult<()> {
        self.with_writer(|transaction| {
            transaction.execute(
                "UPDATE sync_runs SET
                    status = ?2, stage = ?3, files_completed = ?4,
                    bytes_read = ?5, events_written = ?6, records_skipped = ?7,
                    parse_failures = ?8, elapsed_ms = ?9, error_code = ?10
                 WHERE id = ?1",
                params![
                    progress.id,
                    progress.status,
                    progress.stage,
                    to_i64(progress.files_completed),
                    to_i64(progress.bytes_read),
                    to_i64(progress.events_written),
                    to_i64(progress.records_skipped),
                    to_i64(progress.parse_failures),
                    to_i64(progress.elapsed_ms),
                    progress.error_code,
                ],
            )?;
            Ok(())
        })
    }

    pub fn finish_sync_run(
        &self,
        progress: &SyncRunProgress<'_>,
        finished_at_ms: i64,
    ) -> AppResult<()> {
        self.with_writer(|transaction| {
            transaction.execute(
                "UPDATE sync_runs SET
                    status = ?2, stage = ?3, files_completed = ?4,
                    bytes_read = ?5, events_written = ?6, records_skipped = ?7,
                    parse_failures = ?8, elapsed_ms = ?9, error_code = ?10,
                    finished_at_ms = ?11
                 WHERE id = ?1",
                params![
                    progress.id,
                    progress.status,
                    progress.stage,
                    to_i64(progress.files_completed),
                    to_i64(progress.bytes_read),
                    to_i64(progress.events_written),
                    to_i64(progress.records_skipped),
                    to_i64(progress.parse_failures),
                    to_i64(progress.elapsed_ms),
                    progress.error_code,
                    finished_at_ms,
                ],
            )?;
            Ok(())
        })
    }

    pub fn mark_source_error(&self, source_key: &str, error_code: &str) -> AppResult<()> {
        self.with_writer(|transaction| {
            transaction.execute(
                "UPDATE source_files SET status = 'error', last_error_code = ?2
                 WHERE source_key = ?1",
                params![source_key, error_code],
            )?;
            Ok(())
        })
    }
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
