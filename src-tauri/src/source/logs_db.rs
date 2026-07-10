use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, OpenFlags, params};

use super::{LogsAdvanced, ParsedRecord, SourceChange, StreamOutcome};
use crate::error::AppResult;

pub(super) fn stream_logs_db(
    change: &SourceChange,
    sink: &mut dyn FnMut(ParsedRecord) -> AppResult<()>,
    cancel: &AtomicBool,
) -> AppResult<StreamOutcome> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(cancelled(change));
    }
    let connection = Connection::open_with_flags(
        &change.path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(std::time::Duration::from_millis(2_000))?;
    let watermark = change.cursor.logs_rowid_watermark;

    // 只读主键和计数；feedback_log_body、file 等潜在敏感列从未被取出 SQLite。
    let (rows_seen, latest): (i64, Option<i64>) = connection.query_row(
        "SELECT COUNT(*), MAX(id) FROM logs WHERE id > ?1",
        params![i64::try_from(watermark).unwrap_or(i64::MAX)],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let latest = latest.map_or(watermark, |value| value.max(0) as u64);
    sink(ParsedRecord::LogsAdvanced(LogsAdvanced {
        rowid_watermark: latest,
        rows_seen: rows_seen.max(0) as u64,
    }))?;

    let mut cursor = change.cursor.clone();
    cursor.safe_offset = change.file_size;
    cursor.complete_line_offset = change.file_size;
    cursor.logs_rowid_watermark = latest;
    Ok(StreamOutcome {
        cursor,
        bytes_read: change.file_size.saturating_sub(change.cursor.safe_offset),
        records_emitted: rows_seen.max(0) as u64,
        parse_failures: 0,
        incomplete_tail: false,
        cancelled: false,
    })
}

fn cancelled(change: &SourceChange) -> StreamOutcome {
    StreamOutcome {
        cursor: change.cursor.clone(),
        bytes_read: 0,
        records_emitted: 0,
        parse_failures: 0,
        incomplete_tail: false,
        cancelled: true,
    }
}
