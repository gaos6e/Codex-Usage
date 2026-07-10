use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{Connection, OpenFlags};

use super::{ParsedRecord, SourceChange, StateSessionMetadata, StreamOutcome};
use crate::error::AppResult;

pub(super) fn stream_state_db(
    change: &SourceChange,
    sink: &mut dyn FnMut(ParsedRecord) -> AppResult<()>,
    cancel: &AtomicBool,
) -> AppResult<StreamOutcome> {
    let connection = Connection::open_with_flags(
        &change.path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(std::time::Duration::from_millis(2_000))?;

    // 隐私边界：查询列清单刻意排除 title、preview、first_user_message 等正文列。
    let mut statement = connection.prepare(
        "SELECT id, cwd, model_provider, model,
                COALESCE(created_at_ms, created_at * 1000),
                COALESCE(updated_at_ms, updated_at * 1000), archived
         FROM threads ORDER BY id",
    )?;
    let mut rows = statement.query([])?;
    let mut emitted = 0_u64;
    let mut cancelled = false;
    while let Some(row) = rows.next()? {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        let created_at_ms = row.get::<_, i64>(4)?;
        let updated_at_ms = row.get::<_, i64>(5)?;
        sink(ParsedRecord::StateSessionMetadata(StateSessionMetadata {
            session_id: row.get(0)?,
            cwd: row.get(1)?,
            model_provider: row.get(2)?,
            model_raw: row.get(3)?,
            created_at_ms,
            updated_at_ms,
            archived: row.get::<_, i64>(6)? != 0,
        }))?;
        emitted = emitted.saturating_add(1);
    }

    let mut cursor = change.cursor.clone();
    if !cancelled {
        cursor.safe_offset = change.file_size;
        cursor.complete_line_offset = change.file_size;
    }
    Ok(StreamOutcome {
        cursor,
        bytes_read: if cancelled { 0 } else { change.file_size },
        records_emitted: emitted,
        parse_failures: 0,
        incomplete_tail: false,
        cancelled,
    })
}
