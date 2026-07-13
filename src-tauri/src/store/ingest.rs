use std::cmp::min;
use std::sync::atomic::AtomicBool;

use chrono::{DateTime, Days, LocalResult, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::{OptionalExtension, Transaction, params};

use super::UsageStore;
use crate::error::{AppError, AppResult};
use crate::pricing::{
    PriceQuote, PriceableTokens, PricingCatalog, is_statistics_excluded_model, normalize_model_id,
};
use crate::source::{
    ChangeAction, CodexSource, ModelChanged, PARSER_VERSION, ParsedRecord, SessionMetadata,
    SourceChange, SourceCheckpoint, SourceCursor, SourceKind, StateSessionMetadata, StreamOutcome,
    ToolEvent, UsageEvent,
};

#[derive(Debug, Clone)]
pub struct FileApplyResult {
    pub source_key: String,
    pub session_id: Option<String>,
    pub bytes_read: u64,
    pub records_written: u64,
    pub parse_failures: u64,
    pub incomplete_tail: bool,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionResult {
    pub usage_events_deleted: u64,
    pub tool_events_deleted: u64,
    pub cutoff_utc_ms: i64,
}

impl UsageStore {
    pub fn source_checkpoints(&self) -> AppResult<Vec<SourceCheckpoint>> {
        self.with_reader(|connection| {
            let mut statement = connection.prepare(
                "SELECT source_key, relative_path, source_kind, session_id,
                        file_size, mtime_ns, prefix_hash, parser_version,
                        safe_offset, complete_line_offset,
                        current_model_raw, current_pricing_model_id, model_provider,
                        cumulative_input_tokens, cumulative_cached_input_tokens,
                        cumulative_output_tokens, cumulative_reasoning_tokens,
                        relevant_event_ordinal, open_task_turn_id, open_task_started_at_ms,
                        logs_rowid_watermark
                 FROM source_files
                 WHERE source_kind IN ('session', 'archived_session', 'state_db', 'logs_db')",
            )?;
            let rows = statement.query_map([], |row| {
                let kind: String = row.get(2)?;
                Ok(SourceCheckpoint {
                    source_key: row.get(0)?,
                    relative_path: row.get(1)?,
                    kind: source_kind_from_db(&kind),
                    session_id: row.get(3)?,
                    file_size: nonnegative_u64(row.get::<_, i64>(4)?),
                    mtime_ns: nonnegative_u128(row.get::<_, i64>(5)?),
                    prefix_hash: row.get(6)?,
                    parser_version: row.get(7)?,
                    cursor: SourceCursor {
                        safe_offset: nonnegative_u64(row.get::<_, i64>(8)?),
                        complete_line_offset: nonnegative_u64(row.get::<_, i64>(9)?),
                        current_model_raw: row.get(10)?,
                        current_pricing_model_id: row.get(11)?,
                        model_provider: row.get(12)?,
                        cumulative_input_tokens: nonnegative_u64(row.get::<_, i64>(13)?),
                        cumulative_cached_input_tokens: nonnegative_u64(row.get::<_, i64>(14)?),
                        cumulative_output_tokens: nonnegative_u64(row.get::<_, i64>(15)?),
                        cumulative_reasoning_tokens: nonnegative_u64(row.get::<_, i64>(16)?),
                        relevant_event_ordinal: nonnegative_u64(row.get::<_, i64>(17)?),
                        open_task_turn_id: row.get(18)?,
                        open_task_started_at_ms: row.get(19)?,
                        logs_rowid_watermark: nonnegative_u64(row.get::<_, i64>(20)?),
                    },
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
        })
    }

    /// 在单个 SQLite 事务中应用一个源文件。Replay 会先删除该文件旧派生记录；
    /// 只有完整换行对应的 cursor 会随事务提交，因此崩溃和取消都可安全续传。
    pub fn apply_source_change(
        &self,
        source: &dyn CodexSource,
        change: &SourceChange,
        pricing: &PricingCatalog,
        timezone: Tz,
        idle_gap_ms: u64,
        cancel: &AtomicBool,
    ) -> AppResult<FileApplyResult> {
        self.with_writer(|transaction| {
            if change.action == ChangeAction::Missing {
                transaction
                    .execute(
                        "UPDATE source_files
                     SET status = 'missing', last_seen_at_ms = ?2
                     WHERE source_key = ?1",
                        params![change.source_key, now_ms()],
                    )
                    .map_err(AppError::from)
                    .map_err(|error| ingest_stage("mark_missing", error))?;
                return Ok(empty_apply_result(change));
            }

            let previous_session: Option<String> = transaction
                .query_row(
                    "SELECT session_id FROM source_files WHERE source_key = ?1",
                    [&change.source_key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(AppError::from)
                .map_err(|error| ingest_stage("read_previous_session", error))?
                .flatten();
            let source_file_id = upsert_source_file(transaction, change, "indexing")
                .map_err(|error| ingest_stage("upsert_source", error))?;

            if change.action == ChangeAction::MetadataOnly {
                if let Some(session_id) =
                    previous_session.as_deref().or(change.session_id.as_deref())
                {
                    transaction
                        .execute(
                            "UPDATE sessions SET archived = ?2, updated_at_ms = ?3 WHERE id = ?1",
                            params![
                                session_id,
                                i64::from(change.kind == SourceKind::ArchivedSession),
                                now_ms()
                            ],
                        )
                        .map_err(AppError::from)
                        .map_err(|error| ingest_stage("metadata_session", error))?;
                }
                mark_source_ready(transaction, source_file_id, change, &change.cursor, None)
                    .map_err(|error| ingest_stage("metadata_ready", error))?;
                return Ok(empty_apply_result(change));
            }

            if change.action == ChangeAction::Replay
                && matches!(
                    change.kind,
                    SourceKind::Session | SourceKind::ArchivedSession
                )
            {
                clear_file_derivatives(transaction, source_file_id, previous_session.as_deref())
                    .map_err(|error| ingest_stage("clear_replay", error))?;
            }

            let mut context = load_context(
                transaction,
                source_file_id,
                change,
                previous_session.or_else(|| change.session_id.clone()),
            )
            .map_err(|error| ingest_stage("load_context", error))?;
            let outcome = source
                .stream(
                    change,
                    &mut |record| {
                        write_record(transaction, change, &mut context, record, pricing, timezone)
                    },
                    cancel,
                )
                .map_err(|error| ingest_stage("stream", error))?;

            let session_id = if let Some(candidate) = context.session_id.clone() {
                let exists = transaction
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM sessions WHERE id = ?1)",
                        [&candidate],
                        |row| row.get::<_, bool>(0),
                    )
                    .map_err(AppError::from)
                    .map_err(|error| ingest_stage("session_exists", error))?;
                exists.then_some(candidate)
            } else {
                None
            };
            if let Some(session_id) = session_id.as_deref() {
                rebuild_fallback_activity(
                    transaction,
                    session_id,
                    change.action,
                    idle_gap_ms,
                    timezone,
                )
                .map_err(|error| ingest_stage("fallback_activity", error))?;
                recompute_session_summary(transaction, session_id, &outcome)
                    .map_err(|error| ingest_stage("session_summary", error))?;
                rebuild_session_daily(transaction, session_id, change.action, timezone)
                    .map_err(|error| ingest_stage("session_daily", error))?;
            }
            mark_source_ready(
                transaction,
                source_file_id,
                change,
                &outcome.cursor,
                session_id.as_deref(),
            )
            .map_err(|error| ingest_stage("mark_ready", error))?;

            Ok(FileApplyResult {
                source_key: change.source_key.clone(),
                session_id,
                bytes_read: outcome.bytes_read,
                records_written: outcome.records_emitted,
                parse_failures: outcome.parse_failures,
                incomplete_tail: outcome.incomplete_tail,
                cancelled: outcome.cancelled,
            })
        })
    }

    /// 永久 session_daily 摘要先重建全局 rollup，再删除 90 天前事件明细。
    pub fn rebuild_rollups_and_prune(
        &self,
        now_utc_ms: i64,
        retain_days: u64,
        timezone: Tz,
    ) -> AppResult<RetentionResult> {
        self.with_writer(|transaction| {
            transaction.execute("DELETE FROM daily_usage_rollups", [])?;
            transaction.execute(
                "INSERT INTO daily_usage_rollups (
                    local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
                    workspace_id, model_provider, model_raw, pricing_model_id, archived,
                    session_count, active_ms, input_tokens, fresh_input_tokens,
                    cached_input_tokens, output_tokens, reasoning_tokens, total_tokens,
                    priced_cost_microusd, unpriced_event_count, last_activity_at_ms
                 )
                 SELECT local_date, timezone_id, MIN(day_start_utc_ms), MAX(day_end_utc_ms),
                        workspace_id, model_provider, model_raw, pricing_model_id, archived,
                        COUNT(DISTINCT session_id), SUM(active_ms), SUM(input_tokens),
                        SUM(fresh_input_tokens), SUM(cached_input_tokens), SUM(output_tokens),
                        SUM(reasoning_tokens), SUM(total_tokens), SUM(priced_cost_microusd),
                        SUM(unpriced_event_count), MAX(last_activity_at_ms)
                 FROM session_daily_usage
                 GROUP BY local_date, timezone_id, workspace_id, model_provider,
                          model_raw, pricing_model_id, archived",
                [],
            )?;

            transaction.execute("DELETE FROM daily_tool_rollups", [])?;
            transaction.execute(
                "INSERT INTO daily_tool_rollups (
                    local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
                    workspace_id, archived, tool_name, category, operation_kind,
                    call_count, session_count
                 )
                 SELECT local_date, timezone_id, MIN(day_start_utc_ms), MAX(day_end_utc_ms),
                        workspace_id, archived, tool_name, category, operation_kind,
                        SUM(call_count), COUNT(DISTINCT session_id)
                 FROM session_daily_tool
                 GROUP BY local_date, timezone_id, workspace_id, archived,
                          tool_name, category, operation_kind",
                [],
            )?;

            let cutoff = retention_cutoff_utc_ms(now_utc_ms, retain_days, timezone)?;
            let usage_deleted = transaction.execute(
                "DELETE FROM usage_events WHERE occurred_at_ms < ?1",
                [cutoff],
            )? as u64;
            let tools_deleted = transaction.execute(
                "DELETE FROM tool_events WHERE occurred_at_ms < ?1",
                [cutoff],
            )? as u64;
            Ok(RetentionResult {
                usage_events_deleted: usage_deleted,
                tool_events_deleted: tools_deleted,
                cutoff_utc_ms: cutoff,
            })
        })
    }
}

#[derive(Debug)]
struct ImportContext {
    source_file_id: i64,
    session_id: Option<String>,
    workspace_id: Option<String>,
    model_provider: String,
    model_raw: String,
    pricing_model_id: String,
    archived: bool,
}

fn empty_apply_result(change: &SourceChange) -> FileApplyResult {
    FileApplyResult {
        source_key: change.source_key.clone(),
        session_id: change.session_id.clone(),
        bytes_read: 0,
        records_written: 0,
        parse_failures: 0,
        incomplete_tail: false,
        cancelled: false,
    }
}

fn upsert_source_file(
    transaction: &Transaction<'_>,
    change: &SourceChange,
    status: &str,
) -> AppResult<i64> {
    transaction
        .query_row(
            "INSERT INTO source_files (
                source_key, relative_path, source_kind, file_size, mtime_ns,
                prefix_hash, safe_offset, complete_line_offset, session_id,
                current_model_raw, current_pricing_model_id, model_provider,
                cumulative_input_tokens, cumulative_cached_input_tokens,
                cumulative_output_tokens, cumulative_reasoning_tokens,
                relevant_event_ordinal, open_task_turn_id, open_task_started_at_ms,
                logs_rowid_watermark, parser_version, status, last_seen_at_ms
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23
             )
             ON CONFLICT(source_key) DO UPDATE SET
                relative_path = excluded.relative_path,
                source_kind = excluded.source_kind,
                file_size = excluded.file_size,
                mtime_ns = excluded.mtime_ns,
                prefix_hash = excluded.prefix_hash,
                parser_version = excluded.parser_version,
                status = excluded.status,
                last_seen_at_ms = excluded.last_seen_at_ms
             RETURNING id",
            params![
                change.source_key,
                change.relative_path,
                change.kind.as_str(),
                to_i64(change.file_size),
                u128_to_i64(change.mtime_ns),
                change.prefix_hash,
                to_i64(change.cursor.safe_offset),
                to_i64(change.cursor.complete_line_offset),
                change.session_id,
                change.cursor.current_model_raw,
                change.cursor.current_pricing_model_id,
                change.cursor.model_provider,
                to_i64(change.cursor.cumulative_input_tokens),
                to_i64(change.cursor.cumulative_cached_input_tokens),
                to_i64(change.cursor.cumulative_output_tokens),
                to_i64(change.cursor.cumulative_reasoning_tokens),
                to_i64(change.cursor.relevant_event_ordinal),
                change.cursor.open_task_turn_id,
                change.cursor.open_task_started_at_ms,
                to_i64(change.cursor.logs_rowid_watermark),
                PARSER_VERSION,
                status,
                now_ms(),
            ],
            |row| row.get(0),
        )
        .map_err(AppError::from)
}

fn mark_source_ready(
    transaction: &Transaction<'_>,
    source_file_id: i64,
    change: &SourceChange,
    cursor: &SourceCursor,
    session_id: Option<&str>,
) -> AppResult<()> {
    transaction.execute(
        "UPDATE source_files SET
            relative_path = ?2, source_kind = ?3, file_size = ?4, mtime_ns = ?5,
            prefix_hash = ?6, safe_offset = ?7, complete_line_offset = ?8,
            session_id = COALESCE(?9, session_id), current_model_raw = ?10,
            current_pricing_model_id = ?11, model_provider = ?12,
            cumulative_input_tokens = ?13, cumulative_cached_input_tokens = ?14,
            cumulative_output_tokens = ?15, cumulative_reasoning_tokens = ?16,
            relevant_event_ordinal = ?17, open_task_turn_id = ?18,
            open_task_started_at_ms = ?19, logs_rowid_watermark = ?20, parser_version = ?21,
            status = 'ready', last_error_code = NULL, last_seen_at_ms = ?22
         WHERE id = ?1",
        params![
            source_file_id,
            change.relative_path,
            change.kind.as_str(),
            to_i64(change.file_size),
            u128_to_i64(change.mtime_ns),
            change.prefix_hash,
            to_i64(cursor.safe_offset),
            to_i64(cursor.complete_line_offset),
            session_id,
            cursor.current_model_raw,
            cursor.current_pricing_model_id,
            cursor.model_provider,
            to_i64(cursor.cumulative_input_tokens),
            to_i64(cursor.cumulative_cached_input_tokens),
            to_i64(cursor.cumulative_output_tokens),
            to_i64(cursor.cumulative_reasoning_tokens),
            to_i64(cursor.relevant_event_ordinal),
            cursor.open_task_turn_id,
            cursor.open_task_started_at_ms,
            to_i64(cursor.logs_rowid_watermark),
            PARSER_VERSION,
            now_ms(),
        ],
    )?;
    Ok(())
}

fn clear_file_derivatives(
    transaction: &Transaction<'_>,
    source_file_id: i64,
    session_id: Option<&str>,
) -> AppResult<()> {
    transaction.execute(
        "DELETE FROM usage_events WHERE source_file_id = ?1",
        [source_file_id],
    )?;
    transaction.execute(
        "DELETE FROM tool_events WHERE source_file_id = ?1",
        [source_file_id],
    )?;
    if let Some(session_id) = session_id {
        transaction.execute(
            "DELETE FROM session_model_segments WHERE session_id = ?1",
            [session_id],
        )?;
        transaction.execute(
            "DELETE FROM activity_segments WHERE session_id = ?1",
            [session_id],
        )?;
        transaction.execute(
            "DELETE FROM session_daily_usage WHERE session_id = ?1",
            [session_id],
        )?;
        transaction.execute(
            "DELETE FROM session_daily_tool WHERE session_id = ?1",
            [session_id],
        )?;
        transaction.execute(
            "UPDATE sessions SET
                started_at_ms = NULL, ended_at_ms = NULL, active_ms = 0,
                active_method = 'unknown', active_is_estimate = 1,
                input_tokens = 0, fresh_input_tokens = 0, cached_input_tokens = 0,
                output_tokens = 0, reasoning_tokens = 0, total_tokens = 0,
                estimated_cost_microusd = NULL, unpriced_event_count = 0,
                integrity_status = 'partial', warning_count = 0
             WHERE id = ?1",
            [session_id],
        )?;
    }
    Ok(())
}

fn load_context(
    transaction: &Transaction<'_>,
    source_file_id: i64,
    change: &SourceChange,
    session_id: Option<String>,
) -> AppResult<ImportContext> {
    let existing = session_id.as_deref().and_then(|id| {
        transaction
            .query_row(
                "SELECT workspace_id, model_provider, latest_model_raw,
                            COALESCE(latest_pricing_model_id, '')
                     FROM sessions WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .ok()
            .flatten()
    });
    Ok(ImportContext {
        source_file_id,
        session_id,
        workspace_id: existing.as_ref().map(|value| value.0.clone()),
        model_provider: change
            .cursor
            .model_provider
            .clone()
            .or_else(|| existing.as_ref().map(|value| value.1.clone()))
            .unwrap_or_else(|| "unknown".to_string()),
        model_raw: change
            .cursor
            .current_model_raw
            .clone()
            .or_else(|| existing.as_ref().map(|value| value.2.clone()))
            .unwrap_or_else(|| "unknown".to_string()),
        pricing_model_id: change
            .cursor
            .current_pricing_model_id
            .clone()
            .or_else(|| existing.as_ref().map(|value| value.3.clone()))
            .unwrap_or_default(),
        archived: change.kind == SourceKind::ArchivedSession,
    })
}

fn ingest_stage(stage: &str, error: AppError) -> AppError {
    AppError::new(format!("ingest_{stage}_{}", error.code), "结构化源处理失败")
}

fn write_record(
    transaction: &Transaction<'_>,
    change: &SourceChange,
    context: &mut ImportContext,
    record: ParsedRecord,
    pricing: &PricingCatalog,
    timezone: Tz,
) -> AppResult<()> {
    let record_kind = match &record {
        ParsedRecord::SessionMetadata(_) => "session_metadata",
        ParsedRecord::StateSessionMetadata(_) => "state_session_metadata",
        ParsedRecord::LogsAdvanced(_) => "logs_advanced",
        ParsedRecord::ModelChanged(_) => "model_changed",
        ParsedRecord::Usage(_) => "usage",
        ParsedRecord::Activity(_) => "activity",
        ParsedRecord::Tool(_) => "tool",
    };
    let result = match record {
        ParsedRecord::SessionMetadata(metadata) => {
            apply_session_metadata(transaction, context, metadata)
        }
        ParsedRecord::StateSessionMetadata(metadata) => {
            apply_state_session_metadata(transaction, metadata)
        }
        ParsedRecord::LogsAdvanced(_) => Ok(()),
        ParsedRecord::ModelChanged(model) => {
            ensure_session(transaction, change, context, model.occurred_at_ms)?;
            apply_model_change(transaction, context, model)
        }
        ParsedRecord::Usage(event) => {
            ensure_session(transaction, change, context, Some(event.occurred_at_ms))?;
            apply_usage_event(transaction, change, context, event, pricing, timezone)
        }
        ParsedRecord::Activity(segment) => {
            ensure_session(transaction, change, context, Some(segment.started_at_ms))?;
            apply_activity_segment(transaction, context, segment, timezone)
        }
        ParsedRecord::Tool(event) => {
            ensure_session(transaction, change, context, Some(event.occurred_at_ms))?;
            apply_tool_event(transaction, change, context, event, timezone)
        }
    };
    result.map_err(|error| {
        AppError::new(
            format!("record_{record_kind}_{}", error.code),
            "结构化记录写入失败",
        )
    })
}

fn apply_state_session_metadata(
    transaction: &Transaction<'_>,
    metadata: StateSessionMetadata,
) -> AppResult<()> {
    let normalized_path = normalize_workspace_path(&metadata.cwd);
    let workspace_id = workspace_id(&normalized_path);
    upsert_workspace(transaction, &workspace_id, &normalized_path)?;
    let model = metadata.model_raw.as_deref().unwrap_or("unknown");
    let pricing_model = metadata
        .model_raw
        .as_deref()
        .map(normalize_model_id)
        .map(|normalized| normalized.exact)
        .unwrap_or_default();
    let short_id: String = metadata.session_id.chars().take(8).collect();
    transaction.execute(
        "INSERT INTO sessions (
            id, workspace_id, synthetic_title, started_at_ms, ended_at_ms,
            model_provider, latest_model_raw, latest_pricing_model_id,
            primary_model_raw, primary_pricing_model_id, archived,
            integrity_status, parser_version, updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?7, ?8, ?9, 'partial', ?10, ?5)
         ON CONFLICT(id) DO UPDATE SET
            workspace_id = excluded.workspace_id,
            started_at_ms = COALESCE(sessions.started_at_ms, excluded.started_at_ms),
            ended_at_ms = MAX(COALESCE(sessions.ended_at_ms, 0), excluded.ended_at_ms),
            model_provider = CASE WHEN excluded.model_provider <> 'unknown'
                THEN excluded.model_provider ELSE sessions.model_provider END,
            latest_model_raw = CASE WHEN excluded.latest_model_raw <> 'unknown'
                THEN excluded.latest_model_raw ELSE sessions.latest_model_raw END,
            latest_pricing_model_id = CASE WHEN excluded.latest_pricing_model_id <> ''
                THEN excluded.latest_pricing_model_id ELSE sessions.latest_pricing_model_id END,
            archived = excluded.archived,
            updated_at_ms = MAX(sessions.updated_at_ms, excluded.updated_at_ms)",
        params![
            metadata.session_id,
            workspace_id,
            format!("Session {short_id}"),
            metadata.created_at_ms,
            metadata.updated_at_ms,
            metadata.model_provider,
            model,
            pricing_model,
            i64::from(metadata.archived),
            PARSER_VERSION,
        ],
    )?;
    transaction.execute(
        "UPDATE session_daily_usage
         SET workspace_id = ?2, archived = ?3 WHERE session_id = ?1",
        params![
            metadata.session_id,
            workspace_id,
            i64::from(metadata.archived)
        ],
    )?;
    transaction.execute(
        "UPDATE session_daily_tool
         SET workspace_id = ?2, archived = ?3 WHERE session_id = ?1",
        params![
            metadata.session_id,
            workspace_id,
            i64::from(metadata.archived)
        ],
    )?;
    Ok(())
}

fn apply_session_metadata(
    transaction: &Transaction<'_>,
    context: &mut ImportContext,
    metadata: SessionMetadata,
) -> AppResult<()> {
    let normalized_path = normalize_workspace_path(metadata.cwd.as_deref().unwrap_or("(unknown)"));
    let workspace_id = workspace_id(&normalized_path);
    upsert_workspace(transaction, &workspace_id, &normalized_path)?;
    context.session_id = Some(metadata.session_id.clone());
    context.workspace_id = Some(workspace_id.clone());
    if let Some(provider) = metadata.model_provider.as_deref() {
        context.model_provider = provider.to_string();
    }
    if let Some(model) = metadata.legacy_model.as_deref() {
        context.model_raw = model.to_string();
        context.pricing_model_id = normalize_model_id(model).exact;
    }
    upsert_session(
        transaction,
        context,
        metadata.occurred_at_ms,
        &synthetic_title(&metadata.session_id, metadata.occurred_at_ms),
    )?;
    transaction.execute(
        "UPDATE source_files SET session_id = ?2, model_provider = ?3 WHERE id = ?1",
        params![
            context.source_file_id,
            metadata.session_id,
            context.model_provider
        ],
    )?;
    Ok(())
}

fn ensure_session(
    transaction: &Transaction<'_>,
    change: &SourceChange,
    context: &mut ImportContext,
    occurred_at_ms: Option<i64>,
) -> AppResult<()> {
    if context.session_id.is_none() {
        context.session_id = Some(change.source_key.trim_start_matches("jsonl:").to_string());
    }
    if context.workspace_id.is_none() {
        let unknown = "(unknown)";
        let id = workspace_id(unknown);
        upsert_workspace(transaction, &id, unknown)?;
        context.workspace_id = Some(id);
    }
    let id = context.session_id.as_deref().unwrap_or("unknown");
    upsert_session(
        transaction,
        context,
        occurred_at_ms,
        &synthetic_title(id, occurred_at_ms),
    )
}

fn upsert_workspace(
    transaction: &Transaction<'_>,
    id: &str,
    normalized_path: &str,
) -> AppResult<()> {
    let display_name = normalized_path
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(normalized_path);
    transaction.execute(
        "INSERT INTO workspaces (
            id, normalized_path, display_name, ignored, created_at_ms, updated_at_ms
         ) VALUES (?1, ?2, ?3, 0, ?4, ?4)
         ON CONFLICT(id) DO UPDATE SET
            normalized_path = excluded.normalized_path,
            display_name = COALESCE(workspaces.alias, excluded.display_name),
            updated_at_ms = excluded.updated_at_ms",
        params![id, normalized_path, display_name, now_ms()],
    )?;
    Ok(())
}

fn upsert_session(
    transaction: &Transaction<'_>,
    context: &ImportContext,
    occurred_at_ms: Option<i64>,
    synthetic_title: &str,
) -> AppResult<()> {
    let session_id = context.session_id.as_deref().unwrap_or("unknown");
    let workspace_id = context
        .workspace_id
        .as_deref()
        .unwrap_or("workspace:unknown");
    transaction.execute(
        "INSERT INTO sessions (
            id, source_file_id, workspace_id, synthetic_title, started_at_ms,
            ended_at_ms, model_provider, latest_model_raw, latest_pricing_model_id,
            primary_model_raw, primary_pricing_model_id, archived,
            integrity_status, parser_version, updated_at_ms
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, NULLIF(?8, ''),
            ?7, NULLIF(?8, ''), ?9, 'partial', ?10, ?11
         )
         ON CONFLICT(id) DO UPDATE SET
            source_file_id = excluded.source_file_id,
            workspace_id = excluded.workspace_id,
            synthetic_title = excluded.synthetic_title,
            started_at_ms = CASE
                WHEN sessions.started_at_ms IS NULL THEN excluded.started_at_ms
                WHEN excluded.started_at_ms IS NULL THEN sessions.started_at_ms
                ELSE MIN(sessions.started_at_ms, excluded.started_at_ms) END,
            ended_at_ms = CASE
                WHEN sessions.ended_at_ms IS NULL THEN excluded.ended_at_ms
                WHEN excluded.ended_at_ms IS NULL THEN sessions.ended_at_ms
                ELSE MAX(sessions.ended_at_ms, excluded.ended_at_ms) END,
            model_provider = CASE WHEN excluded.model_provider = 'unknown'
                THEN sessions.model_provider ELSE excluded.model_provider END,
            archived = excluded.archived,
            parser_version = excluded.parser_version,
            updated_at_ms = excluded.updated_at_ms",
        params![
            session_id,
            context.source_file_id,
            workspace_id,
            synthetic_title,
            occurred_at_ms,
            context.model_provider,
            context.model_raw,
            context.pricing_model_id,
            i64::from(context.archived),
            PARSER_VERSION,
            now_ms(),
        ],
    )?;
    Ok(())
}

fn apply_model_change(
    transaction: &Transaction<'_>,
    context: &mut ImportContext,
    model: ModelChanged,
) -> AppResult<()> {
    let session_id = context.session_id.as_deref().unwrap_or("unknown");
    let started_at = model.occurred_at_ms.unwrap_or(0);
    transaction.execute(
        "UPDATE session_model_segments
         SET ended_at_ms = ?2
         WHERE session_id = ?1 AND ended_at_ms IS NULL AND segment_index <> ?3",
        params![session_id, started_at, to_i64(model.event_ordinal)],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO session_model_segments (
            session_id, segment_index, started_at_ms, model_provider,
            model_raw, pricing_model_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, NULLIF(?6, ''))",
        params![
            session_id,
            to_i64(model.event_ordinal),
            started_at,
            model.model_provider,
            model.model_raw,
            model.pricing_model_id,
        ],
    )?;
    context.model_provider = model.model_provider;
    context.model_raw = model.model_raw;
    context.pricing_model_id = model.pricing_model_id;
    transaction.execute(
        "UPDATE sessions SET model_provider = ?2, latest_model_raw = ?3,
            latest_pricing_model_id = NULLIF(?4, ''), updated_at_ms = ?5
         WHERE id = ?1",
        params![
            session_id,
            context.model_provider,
            context.model_raw,
            context.pricing_model_id,
            now_ms(),
        ],
    )?;
    Ok(())
}

fn apply_usage_event(
    transaction: &Transaction<'_>,
    change: &SourceChange,
    context: &mut ImportContext,
    event: UsageEvent,
    pricing: &PricingCatalog,
    timezone: Tz,
) -> AppResult<()> {
    context.model_provider = event.model_provider.clone();
    context.model_raw = event.model_raw.clone();
    context.pricing_model_id = event.pricing_model_id.clone();
    if is_statistics_excluded_model(&event.model_raw) {
        return Ok(());
    }
    let session_id = context.session_id.as_deref().unwrap_or("unknown");
    let day = day_context(event.occurred_at_ms, timezone)?;
    let quote = pricing.quote(
        &event.model_provider,
        &event.model_raw,
        PriceableTokens {
            fresh_input: event.fresh_input_tokens,
            cached_input: event.cached_input_tokens,
            output: event.output_tokens,
            cache_write: 0,
        },
    );
    let (cost, revision, pricing_id, cost_excluded) = match quote {
        PriceQuote::Priced {
            pricing_id,
            revision,
            total_microusd,
        } => (Some(total_microusd), Some(revision), pricing_id, false),
        PriceQuote::Excluded => (None, None, event.pricing_model_id.clone(), true),
        PriceQuote::Unpriced => (None, None, event.pricing_model_id.clone(), false),
    };
    let event_key = format!("{}:usage:{}", change.source_key, event.event_ordinal);
    let inserted = transaction.execute(
        "INSERT OR IGNORE INTO usage_events (
            event_key, source_file_id, session_id, byte_offset, event_ordinal,
            occurred_at_ms, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
            model_provider, model_raw, pricing_model_id, input_tokens,
            fresh_input_tokens, cached_input_tokens, output_tokens, reasoning_tokens,
            total_tokens, estimated_cost_microusd, pricing_revision, integrity_status
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
            NULLIF(?13, ''), ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, 'complete'
         )",
        params![
            event_key,
            context.source_file_id,
            session_id,
            to_i64(event.byte_offset),
            to_i64(event.event_ordinal),
            event.occurred_at_ms,
            day.local_date,
            day.timezone_id,
            day.start_utc_ms,
            day.end_utc_ms,
            event.model_provider,
            event.model_raw,
            pricing_id,
            to_i64(event.input_tokens),
            to_i64(event.fresh_input_tokens),
            to_i64(event.cached_input_tokens),
            to_i64(event.output_tokens),
            to_i64(event.reasoning_tokens),
            to_i64(event.total_tokens),
            cost,
            revision,
        ],
    )?;
    if inserted == 0 {
        return Ok(());
    }

    transaction.execute(
        "INSERT INTO session_model_segments (
            session_id, segment_index, started_at_ms, model_provider,
            model_raw, pricing_model_id
         ) SELECT ?1, ?2, ?3, ?4, ?5, NULLIF(?6, '')
           WHERE NOT EXISTS (
             SELECT 1 FROM session_model_segments WHERE session_id = ?1
           )",
        params![
            session_id,
            to_i64(event.event_ordinal),
            event.occurred_at_ms,
            event.model_provider,
            event.model_raw,
            pricing_id,
        ],
    )?;
    transaction.execute(
        "UPDATE session_model_segments SET
            input_tokens = input_tokens + ?2,
            cached_input_tokens = cached_input_tokens + ?3,
            output_tokens = output_tokens + ?4,
            reasoning_tokens = reasoning_tokens + ?5,
            estimated_cost_microusd = CASE
                WHEN ?6 IS NULL THEN estimated_cost_microusd
                ELSE COALESCE(estimated_cost_microusd, 0) + ?6 END,
            unpriced_event_count = unpriced_event_count
                + CASE WHEN ?6 IS NULL AND ?7 = 0 THEN 1 ELSE 0 END
         WHERE id = (
            SELECT id FROM session_model_segments
            WHERE session_id = ?1 ORDER BY segment_index DESC LIMIT 1
         )",
        params![
            session_id,
            to_i64(event.input_tokens),
            to_i64(event.cached_input_tokens),
            to_i64(event.output_tokens),
            to_i64(event.reasoning_tokens),
            cost,
            i64::from(cost_excluded),
        ],
    )?;
    Ok(())
}

fn apply_activity_segment(
    transaction: &Transaction<'_>,
    context: &ImportContext,
    segment: crate::source::ActivitySegment,
    timezone: Tz,
) -> AppResult<()> {
    if is_statistics_excluded_model(&context.model_raw) {
        return Ok(());
    }
    let session_id = context.session_id.as_deref().unwrap_or("unknown");
    let day = day_context(segment.started_at_ms, timezone)?;
    transaction.execute(
        "INSERT OR IGNORE INTO activity_segments (
            session_id, segment_index, started_at_ms, ended_at_ms, active_ms,
            method, is_estimate, local_date, timezone_id, model_provider,
            model_raw, pricing_model_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, 'lifecycle', 0, ?6, ?7, ?8, ?9, ?10)",
        params![
            session_id,
            to_i64(segment.event_ordinal),
            segment.started_at_ms,
            segment.ended_at_ms,
            to_i64(segment.active_ms),
            day.local_date,
            day.timezone_id,
            context.model_provider,
            context.model_raw,
            context.pricing_model_id,
        ],
    )?;
    Ok(())
}

fn apply_tool_event(
    transaction: &Transaction<'_>,
    change: &SourceChange,
    context: &ImportContext,
    event: ToolEvent,
    timezone: Tz,
) -> AppResult<()> {
    if is_statistics_excluded_model(&context.model_raw) {
        return Ok(());
    }
    let session_id = context.session_id.as_deref().unwrap_or("unknown");
    let day = day_context(event.occurred_at_ms, timezone)?;
    let event_key = format!(
        "{}:tool:{}:{}",
        change.source_key, event.event_ordinal, event.sub_index
    );
    transaction.execute(
        "INSERT OR IGNORE INTO tool_events (
            event_key, source_file_id, session_id, byte_offset, event_ordinal,
            sub_index, occurred_at_ms, local_date, timezone_id,
            day_start_utc_ms, day_end_utc_ms, tool_name, category, operation_kind
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![
            event_key,
            context.source_file_id,
            session_id,
            to_i64(event.byte_offset),
            to_i64(event.event_ordinal),
            i64::from(event.sub_index),
            event.occurred_at_ms,
            day.local_date,
            day.timezone_id,
            day.start_utc_ms,
            day.end_utc_ms,
            event.tool_name,
            event.category.as_str(),
            event.operation_kind.as_str(),
        ],
    )?;
    Ok(())
}

fn rebuild_fallback_activity(
    transaction: &Transaction<'_>,
    session_id: &str,
    action: ChangeAction,
    idle_gap_ms: u64,
    timezone: Tz,
) -> AppResult<()> {
    let earliest_detail: Option<i64> = transaction.query_row(
        "SELECT MIN(occurred_at_ms) FROM usage_events WHERE session_id = ?1",
        [session_id],
        |row| row.get(0),
    )?;
    if action == ChangeAction::Replay {
        transaction.execute(
            "DELETE FROM activity_segments WHERE session_id = ?1 AND method <> 'lifecycle'",
            [session_id],
        )?;
    } else if let Some(earliest) = earliest_detail {
        transaction.execute(
            "DELETE FROM activity_segments
             WHERE session_id = ?1 AND method <> 'lifecycle' AND started_at_ms >= ?2",
            params![session_id, earliest],
        )?;
    }

    let lifecycle_count: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM activity_segments
         WHERE session_id = ?1 AND method = 'lifecycle'",
        [session_id],
        |row| row.get(0),
    )?;
    if lifecycle_count > 0 {
        return Ok(());
    }

    let points = {
        let mut statement = transaction.prepare(
            "SELECT occurred_at_ms, model_provider, model_raw,
                    COALESCE(pricing_model_id, '')
             FROM usage_events WHERE session_id = ?1
             ORDER BY occurred_at_ms, id",
        )?;
        statement
            .query_map([session_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (index, pair) in points.windows(2).enumerate() {
        let started = pair[0].0;
        let ended = pair[1].0;
        let gap = ended.saturating_sub(started);
        if gap <= 0 || gap as u64 > idle_gap_ms {
            continue;
        }
        let day = day_context(started, timezone)?;
        transaction.execute(
            "INSERT OR REPLACE INTO activity_segments (
                session_id, segment_index, started_at_ms, ended_at_ms, active_ms,
                method, is_estimate, local_date, timezone_id, model_provider,
                model_raw, pricing_model_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, 'idle_estimate', 1, ?6, ?7, ?8, ?9, ?10)",
            params![
                session_id,
                2_000_000_000_i64.saturating_add(index as i64),
                started,
                ended,
                gap,
                day.local_date,
                day.timezone_id,
                pair[0].1,
                pair[0].2,
                pair[0].3,
            ],
        )?;
    }
    Ok(())
}

fn recompute_session_summary(
    transaction: &Transaction<'_>,
    session_id: &str,
    outcome: &StreamOutcome,
) -> AppResult<()> {
    let totals = transaction.query_row(
        "SELECT COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(input_tokens - cached_input_tokens), 0),
                COALESCE(SUM(cached_input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(reasoning_tokens), 0),
                SUM(estimated_cost_microusd),
                COALESCE(SUM(unpriced_event_count), 0)
         FROM session_model_segments
         WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'",
        [session_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, i64>(6)?,
            ))
        },
    )?;
    let activity = transaction.query_row(
        "SELECT COALESCE(SUM(active_ms), 0),
                COALESCE(SUM(CASE WHEN method = 'lifecycle' THEN 1 ELSE 0 END), 0),
                COUNT(*)
         FROM activity_segments
         WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'",
        [session_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        },
    )?;
    let bounds = transaction.query_row(
        "SELECT MIN(timestamp_ms), MAX(timestamp_ms) FROM (
            SELECT started_at_ms AS timestamp_ms FROM session_model_segments
                WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'
            UNION ALL SELECT ended_at_ms FROM session_model_segments
                WHERE session_id = ?1 AND ended_at_ms IS NOT NULL
                  AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'
            UNION ALL SELECT started_at_ms FROM activity_segments
                WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'
            UNION ALL SELECT ended_at_ms FROM activity_segments
                WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'
            UNION ALL SELECT occurred_at_ms FROM usage_events
                WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'
            UNION ALL SELECT occurred_at_ms FROM tool_events WHERE session_id = ?1
         )",
        [session_id],
        |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
    )?;
    let existing_bounds = transaction.query_row(
        "SELECT started_at_ms, ended_at_ms FROM sessions WHERE id = ?1",
        [session_id],
        |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
    )?;
    let started = option_min(existing_bounds.0, bounds.0);
    let ended = option_max(existing_bounds.1, bounds.1);
    let primary = transaction
        .query_row(
            "SELECT model_provider, model_raw, COALESCE(pricing_model_id, '')
             FROM session_model_segments
             WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'
             GROUP BY model_provider, model_raw, pricing_model_id
             ORDER BY SUM(input_tokens + output_tokens) DESC, MAX(segment_index) DESC
             LIMIT 1",
            [session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let (active_method, active_estimate) = if activity.1 > 0 {
        ("lifecycle", 0_i64)
    } else if activity.2 > 0 {
        ("idle_estimate", 1_i64)
    } else {
        ("unknown", 1_i64)
    };
    let integrity = if outcome.parse_failures > 0 {
        "warning"
    } else if outcome.incomplete_tail {
        "partial"
    } else {
        "complete"
    };
    transaction.execute(
        "UPDATE sessions SET
            started_at_ms = ?2, ended_at_ms = ?3, active_ms = ?4,
            active_method = ?5, active_is_estimate = ?6,
            input_tokens = ?7, fresh_input_tokens = ?8, cached_input_tokens = ?9,
            output_tokens = ?10, reasoning_tokens = ?11,
            total_tokens = ?7 + ?10, estimated_cost_microusd = ?12,
            unpriced_event_count = ?13,
            model_provider = COALESCE(?14, model_provider),
            primary_model_raw = COALESCE(?15, primary_model_raw),
            primary_pricing_model_id = NULLIF(COALESCE(?16, primary_pricing_model_id), ''),
            integrity_status = ?17, warning_count = ?18, updated_at_ms = ?19
         WHERE id = ?1",
        params![
            session_id,
            started,
            ended,
            activity.0,
            active_method,
            active_estimate,
            totals.0,
            totals.1,
            totals.2,
            totals.3,
            totals.4,
            totals.5,
            totals.6,
            primary.as_ref().map(|value| value.0.as_str()),
            primary.as_ref().map(|value| value.1.as_str()),
            primary.as_ref().map(|value| value.2.as_str()),
            integrity,
            to_i64(outcome.parse_failures),
            now_ms(),
        ],
    )?;
    Ok(())
}

fn rebuild_session_daily(
    transaction: &Transaction<'_>,
    session_id: &str,
    action: ChangeAction,
    timezone: Tz,
) -> AppResult<()> {
    if action == ChangeAction::Replay {
        transaction.execute(
            "DELETE FROM session_daily_usage WHERE session_id = ?1",
            [session_id],
        )?;
        transaction.execute(
            "DELETE FROM session_daily_tool WHERE session_id = ?1",
            [session_id],
        )?;
    } else {
        transaction.execute(
            "DELETE FROM session_daily_usage
             WHERE session_id = ?1 AND (local_date, timezone_id) IN (
                SELECT local_date, timezone_id FROM usage_events WHERE session_id = ?1
                UNION SELECT local_date, timezone_id FROM activity_segments WHERE session_id = ?1
             )",
            [session_id],
        )?;
        transaction.execute(
            "DELETE FROM session_daily_tool
             WHERE session_id = ?1 AND (local_date, timezone_id) IN (
                SELECT local_date, timezone_id FROM tool_events WHERE session_id = ?1
             )",
            [session_id],
        )?;
    }

    transaction.execute(
        "INSERT INTO session_daily_usage (
            session_id, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
            workspace_id, model_provider, model_raw, pricing_model_id, archived,
            active_ms, input_tokens, fresh_input_tokens, cached_input_tokens,
            output_tokens, reasoning_tokens, total_tokens, priced_cost_microusd,
            priced_event_count, unpriced_event_count, last_activity_at_ms
         )
         SELECT e.session_id, e.local_date, e.timezone_id,
                MIN(e.day_start_utc_ms), MAX(e.day_end_utc_ms), s.workspace_id,
                e.model_provider, e.model_raw, COALESCE(e.pricing_model_id, ''), s.archived,
                0, SUM(e.input_tokens), SUM(e.fresh_input_tokens),
                SUM(e.cached_input_tokens), SUM(e.output_tokens),
                SUM(e.reasoning_tokens), SUM(e.total_tokens),
                COALESCE(SUM(e.estimated_cost_microusd), 0),
                SUM(CASE WHEN e.estimated_cost_microusd IS NOT NULL THEN 1 ELSE 0 END),
                SUM(CASE WHEN e.estimated_cost_microusd IS NULL
                          AND NOT (LOWER(e.model_provider) = 'openai'
                                   AND LOWER(e.model_raw) = 'codex-auto-review')
                         THEN 1 ELSE 0 END),
                MAX(e.occurred_at_ms)
         FROM usage_events e JOIN sessions s ON s.id = e.session_id
         WHERE e.session_id = ?1 AND LOWER(TRIM(e.model_raw)) <> 'codex-auto-review'
         GROUP BY e.session_id, e.local_date, e.timezone_id, s.workspace_id,
                  e.model_provider, e.model_raw, e.pricing_model_id, s.archived",
        [session_id],
    )?;

    let session_scope = transaction.query_row(
        "SELECT workspace_id, archived FROM sessions WHERE id = ?1",
        [session_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    )?;
    let segments = {
        let mut statement = transaction.prepare(
            "SELECT started_at_ms, ended_at_ms, active_ms, model_provider,
                    model_raw, COALESCE(pricing_model_id, '')
             FROM activity_segments
             WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'
             ORDER BY started_at_ms",
        )?;
        statement
            .query_map([session_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    for segment in segments {
        for piece in split_activity_segment(segment.0, segment.1, segment.2, timezone)? {
            transaction.execute(
                "INSERT INTO session_daily_usage (
                    session_id, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
                    workspace_id, model_provider, model_raw, pricing_model_id, archived,
                    active_ms, input_tokens, fresh_input_tokens, cached_input_tokens,
                    output_tokens, reasoning_tokens, total_tokens, priced_cost_microusd,
                    priced_event_count, unpriced_event_count, last_activity_at_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                           ?11, 0, 0, 0, 0, 0, 0, 0, 0, 0, ?12)
                 ON CONFLICT(session_id, local_date, timezone_id, model_provider,
                             model_raw, pricing_model_id, archived)
                 DO UPDATE SET active_ms = active_ms + excluded.active_ms,
                               last_activity_at_ms = MAX(last_activity_at_ms, excluded.last_activity_at_ms)",
                params![
                    session_id,
                    piece.day.local_date,
                    piece.day.timezone_id,
                    piece.day.start_utc_ms,
                    piece.day.end_utc_ms,
                    session_scope.0,
                    segment.3,
                    segment.4,
                    segment.5,
                    session_scope.1,
                    piece.active_ms,
                    piece.ended_at_ms,
                ],
            )?;
        }
    }

    transaction.execute(
        "INSERT INTO session_daily_tool (
            session_id, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
            workspace_id, archived, tool_name, category, operation_kind,
            call_count, last_activity_at_ms
         )
         SELECT e.session_id, e.local_date, e.timezone_id,
                MIN(e.day_start_utc_ms), MAX(e.day_end_utc_ms),
                s.workspace_id, s.archived, e.tool_name, e.category,
                e.operation_kind, COUNT(*), MAX(e.occurred_at_ms)
         FROM tool_events e JOIN sessions s ON s.id = e.session_id
         WHERE e.session_id = ?1
         GROUP BY e.session_id, e.local_date, e.timezone_id, s.workspace_id,
                  s.archived, e.tool_name, e.category, e.operation_kind",
        [session_id],
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
struct DayContext {
    local_date: String,
    timezone_id: String,
    start_utc_ms: i64,
    end_utc_ms: i64,
}

#[derive(Debug)]
struct ActivityPiece {
    day: DayContext,
    active_ms: i64,
    ended_at_ms: i64,
}

fn day_context(timestamp_ms: i64, timezone: Tz) -> AppResult<DayContext> {
    let utc = DateTime::<Utc>::from_timestamp_millis(timestamp_ms)
        .ok_or_else(|| AppError::new("invalid_timestamp", "时间戳超出支持范围"))?;
    day_context_for_date(utc.with_timezone(&timezone).date_naive(), timezone)
}

fn day_context_for_date(date: NaiveDate, timezone: Tz) -> AppResult<DayContext> {
    let next_date = date
        .succ_opt()
        .ok_or_else(|| AppError::new("invalid_date", "日期超出支持范围"))?;
    let start = resolve_day_start(date, timezone)?;
    let end = resolve_day_start(next_date, timezone)?;
    Ok(DayContext {
        local_date: date.format("%Y-%m-%d").to_string(),
        timezone_id: timezone.name().to_string(),
        start_utc_ms: start.with_timezone(&Utc).timestamp_millis(),
        end_utc_ms: end.with_timezone(&Utc).timestamp_millis(),
    })
}

fn resolve_day_start(date: NaiveDate, timezone: Tz) -> AppResult<DateTime<Tz>> {
    for minute in 0..=180_u32 {
        let time = NaiveTime::from_num_seconds_from_midnight_opt(minute * 60, 0)
            .ok_or_else(|| AppError::new("invalid_time", "本地时间无效"))?;
        match timezone.from_local_datetime(&date.and_time(time)) {
            LocalResult::Single(value) => return Ok(value),
            LocalResult::Ambiguous(first, second) => return Ok(min(first, second)),
            LocalResult::None => continue,
        }
    }
    Err(AppError::new("timezone_gap", "无法确定本地日期的 UTC 边界"))
}

fn split_activity_segment(
    started_at_ms: i64,
    ended_at_ms: i64,
    active_ms: i64,
    timezone: Tz,
) -> AppResult<Vec<ActivityPiece>> {
    if ended_at_ms <= started_at_ms {
        let day = day_context(started_at_ms, timezone)?;
        return Ok(vec![ActivityPiece {
            day,
            active_ms: active_ms.max(0),
            ended_at_ms,
        }]);
    }
    let total_span = ended_at_ms - started_at_ms;
    let mut cursor = started_at_ms;
    let mut allocated = 0_i64;
    let mut pieces = Vec::new();
    while cursor < ended_at_ms {
        let day = day_context(cursor, timezone)?;
        let piece_end = min(ended_at_ms, day.end_utc_ms);
        let piece_span = piece_end.saturating_sub(cursor);
        let is_last = piece_end >= ended_at_ms;
        let piece_active = if is_last {
            active_ms.saturating_sub(allocated)
        } else {
            active_ms.saturating_mul(piece_span) / total_span
        }
        .max(0);
        allocated = allocated.saturating_add(piece_active);
        pieces.push(ActivityPiece {
            day,
            active_ms: piece_active,
            ended_at_ms: piece_end,
        });
        cursor = piece_end;
    }
    Ok(pieces)
}

pub fn retention_cutoff_utc_ms(now_utc_ms: i64, retain_days: u64, timezone: Tz) -> AppResult<i64> {
    let utc = DateTime::<Utc>::from_timestamp_millis(now_utc_ms)
        .ok_or_else(|| AppError::new("invalid_timestamp", "时间戳超出支持范围"))?;
    let today = utc.with_timezone(&timezone).date_naive();
    let cutoff_date = today
        .checked_sub_days(Days::new(retain_days))
        .ok_or_else(|| AppError::new("invalid_date", "保留日期超出支持范围"))?;
    Ok(day_context_for_date(cutoff_date, timezone)?.start_utc_ms)
}

fn normalize_workspace_path(raw: &str) -> String {
    let replaced = if cfg!(windows) {
        raw.trim().replace('\\', "/")
    } else {
        raw.trim().to_string()
    };
    let mut normalized = String::with_capacity(replaced.len());
    let mut previous_slash = false;
    for character in replaced.chars() {
        let slash = character == '/';
        if slash && previous_slash && !normalized.is_empty() {
            continue;
        }
        normalized.push(character);
        previous_slash = slash;
    }
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }
    if normalized.is_empty() {
        "(unknown)".to_string()
    } else {
        normalized
    }
}

fn workspace_id(normalized_path: &str) -> String {
    let platform_identity = crate::platform::workspace_identity_path(normalized_path);
    let identity = if cfg!(windows) {
        platform_identity.to_ascii_lowercase()
    } else {
        platform_identity
    };
    let digest = blake3::hash(identity.as_bytes()).to_hex();
    format!("workspace:{}", &digest[..24])
}

fn synthetic_title(session_id: &str, timestamp_ms: Option<i64>) -> String {
    let date = timestamp_ms
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .map_or_else(
            || "未知日期".to_string(),
            |value| value.format("%Y-%m-%d").to_string(),
        );
    let short_id: String = session_id
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .take(8)
        .collect();
    format!("Session {date} · {short_id}")
}

fn source_kind_from_db(value: &str) -> SourceKind {
    match value {
        "archived_session" => SourceKind::ArchivedSession,
        "state_db" => SourceKind::StateDb,
        "logs_db" => SourceKind::LogsDb,
        _ => SourceKind::Session,
    }
}

fn option_min(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn option_max(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn u128_to_i64(value: u128) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn nonnegative_u64(value: i64) -> u64 {
    u64::try_from(value.max(0)).unwrap_or(0)
}

fn nonnegative_u128(value: i64) -> u128 {
    u128::try_from(value.max(0)).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::sync::Arc;
    use std::sync::atomic::AtomicBool;

    use chrono::TimeZone;
    use rusqlite::Connection;
    use serde_json::{Value, json};

    use super::*;
    use crate::source::{CodexSource, FsCodexSource};

    fn json_line(value: Value) -> String {
        serde_json::to_string(&value).unwrap()
    }

    fn synthetic_session(sensitive: &str) -> String {
        [
            json!({"timestamp":"2026-07-10T01:00:00Z","type":"session_meta","payload":{"id":"thread-integration","cwd":"C:\\workspace\\demo","model_provider":"openai"}}),
            json!({"timestamp":"2026-07-10T01:00:01Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}),
            json!({"timestamp":"2026-07-10T01:00:02Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1","started_at":"2026-07-10T01:00:02Z"}}),
            json!({"timestamp":"2026-07-10T01:00:03Z","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":format!("await tools.shell_command({{command: 'Get-Content {sensitive}'}}); await tools.apply_patch('{sensitive}');")}}),
            json!({"timestamp":"2026-07-10T01:00:04Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":20,"reasoning_output_tokens":5}}}}),
            json!({"timestamp":"2026-07-10T01:00:09Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","completed_at":"2026-07-10T01:00:09Z","duration_ms":7000,"last_agent_message":sensitive}}),
        ]
        .into_iter()
        .map(json_line)
        .collect::<Vec<_>>()
        .join("\n")
            + "\n"
    }

    #[test]
    fn day_boundaries_follow_dst_instead_of_assuming_24_hours() {
        let timezone: Tz = "America/New_York".parse().unwrap();
        let spring =
            day_context_for_date(NaiveDate::from_ymd_opt(2026, 3, 8).unwrap(), timezone).unwrap();
        let fall =
            day_context_for_date(NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(), timezone).unwrap();
        assert_eq!(spring.end_utc_ms - spring.start_utc_ms, 23 * 60 * 60 * 1000);
        assert_eq!(fall.end_utc_ms - fall.start_utc_ms, 25 * 60 * 60 * 1000);
    }

    #[test]
    fn retention_cutoff_is_local_midnight() {
        let timezone: Tz = "Asia/Shanghai".parse().unwrap();
        let now = Utc.with_ymd_and_hms(2026, 7, 10, 12, 0, 0).unwrap();
        let cutoff = retention_cutoff_utc_ms(now.timestamp_millis(), 90, timezone).unwrap();
        let local = DateTime::<Utc>::from_timestamp_millis(cutoff)
            .unwrap()
            .with_timezone(&timezone);
        assert_eq!(
            local.date_naive(),
            NaiveDate::from_ymd_opt(2026, 4, 11).unwrap()
        );
        assert_eq!(local.time(), NaiveTime::MIN);
    }

    #[test]
    fn workspace_normalization_preserves_readable_posix_unicode_and_spaces() {
        assert_eq!(
            normalize_workspace_path(" /Users/高帅/Project With Spaces/Δelta/ "),
            "/Users/高帅/Project With Spaces/Δelta"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_workspace_identity_remains_case_insensitive() {
        assert_eq!(normalize_workspace_path(r"C:\work\repo\\"), "C:/work/repo");
        assert_eq!(workspace_id(r"C:/work/repo"), workspace_id(r"c:/WORK/repo"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_missing_workspace_identity_is_case_conservative() {
        assert_eq!(
            normalize_workspace_path(r"/Users/test/Project\WithBackslash"),
            r"/Users/test/Project\WithBackslash"
        );
        assert_ne!(
            workspace_id("/Volumes/Missing/Project"),
            workspace_id("/Volumes/Missing/project")
        );
    }

    #[test]
    fn synthetic_title_never_contains_source_title_or_prompt() {
        let title = synthetic_title("thread-sensitive-id", Some(0));
        assert_eq!(title, "Session 1970-01-01 · threadse");
        assert!(!title.contains("prompt"));
    }

    #[test]
    fn replay_with_no_usage_records_creates_a_checkpoint_without_a_phantom_session() {
        let codex = tempfile::tempdir().unwrap();
        let sessions = codex.path().join("sessions/2026/07/10");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(
            sessions.join("rollout-metadata-pending.jsonl"),
            "{\"timestamp\":\"2026-07-10T01:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\"}}\n",
        )
        .unwrap();

        let store = UsageStore::open_in_memory().unwrap();
        let source = FsCodexSource::new(codex.path());
        let plan = source.plan(&[]).unwrap();
        let result = store
            .apply_source_change(
                &source,
                &plan.changes[0],
                &PricingCatalog::default(),
                chrono_tz::UTC,
                30 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert_eq!(result.records_written, 0);
        store
            .with_reader(|connection| {
                let session_count: i64 =
                    connection.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
                let status: String =
                    connection
                        .query_row("SELECT status FROM source_files", [], |row| row.get(0))?;
                assert_eq!(session_count, 0);
                assert_eq!(status, "ready");
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn auto_review_records_are_not_written_to_usage_activity_or_tool_statistics() {
        let codex = tempfile::tempdir().unwrap();
        let sessions = codex.path().join("sessions/2026/07/10");
        std::fs::create_dir_all(&sessions).unwrap();
        let content = [
            json!({"timestamp":"2026-07-10T01:00:00Z","type":"session_meta","payload":{"id":"auto-review","cwd":"C:/workspace/demo","model_provider":"codex_local_access"}}),
            json!({"timestamp":"2026-07-10T01:00:01Z","type":"turn_context","payload":{"model":"codex-auto-review"}}),
            json!({"timestamp":"2026-07-10T01:00:02Z","type":"event_msg","payload":{"type":"task_started","turn_id":"turn-1"}}),
            json!({"timestamp":"2026-07-10T01:00:03Z","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"ignored"}}),
            json!({"timestamp":"2026-07-10T01:00:04Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":10,"reasoning_output_tokens":2}}}}),
            json!({"timestamp":"2026-07-10T01:00:09Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","duration_ms":7000}}),
        ]
        .into_iter()
        .map(json_line)
        .collect::<Vec<_>>()
        .join("\n")
            + "\n";
        std::fs::write(sessions.join("rollout-auto-review.jsonl"), content).unwrap();

        let store = UsageStore::open_in_memory().unwrap();
        let source = FsCodexSource::new(codex.path());
        let plan = source.plan(&[]).unwrap();
        store
            .apply_source_change(
                &source,
                &plan.changes[0],
                &PricingCatalog::default(),
                chrono_tz::UTC,
                30 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();

        store
            .with_reader(|connection| {
                for table in [
                    "usage_events",
                    "activity_segments",
                    "tool_events",
                    "session_daily_usage",
                    "session_daily_tool",
                ] {
                    let count: i64 = connection.query_row(
                        &format!("SELECT COUNT(*) FROM {table}"),
                        [],
                        |row| row.get(0),
                    )?;
                    assert_eq!(count, 0, "{table} should not contain auto-review data");
                }
                let provider: String = connection.query_row(
                    "SELECT model_provider FROM sessions WHERE id = 'auto-review'",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!(provider, "custom");
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn metadata_only_session_recomputes_zero_activity_without_sql_nulls() {
        let codex = tempfile::tempdir().unwrap();
        let sessions = codex.path().join("sessions/2026/07/10");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(
            sessions.join("rollout-metadata-only.jsonl"),
            "{\"timestamp\":\"2026-07-10T01:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"metadata-only\",\"cwd\":\"C:\\\\workspace\\\\demo\",\"model_provider\":\"openai\"}}\n",
        )
        .unwrap();

        let store = UsageStore::open_in_memory().unwrap();
        let source = FsCodexSource::new(codex.path());
        let plan = source.plan(&[]).unwrap();
        store
            .apply_source_change(
                &source,
                &plan.changes[0],
                &PricingCatalog::default(),
                chrono_tz::UTC,
                30 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();
        store
            .with_reader(|connection| {
                let values: (i64, String) = connection.query_row(
                    "SELECT active_ms, integrity_status FROM sessions WHERE id = 'metadata-only'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                assert_eq!(values, (0, "complete".into()));
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn idle_gap_fallback_is_configurable_and_explicitly_marked_as_estimated() {
        let codex = tempfile::tempdir().unwrap();
        let sessions = codex.path().join("sessions/2026/07/10");
        std::fs::create_dir_all(&sessions).unwrap();
        let content = [
            json!({"timestamp":"2026-07-10T01:00:00Z","type":"session_meta","payload":{"id":"fallback-session","cwd":"C:/workspace/demo","model_provider":"openai"}}),
            json!({"timestamp":"2026-07-10T01:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":10,"reasoning_output_tokens":0}}}}),
            json!({"timestamp":"2026-07-10T01:05:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":200,"cached_input_tokens":0,"output_tokens":20,"reasoning_output_tokens":0}}}}),
        ]
        .into_iter()
        .map(json_line)
        .collect::<Vec<_>>()
        .join("\n")
            + "\n";
        std::fs::write(sessions.join("rollout-fallback.jsonl"), content).unwrap();
        let source = FsCodexSource::new(codex.path());

        let short_gap_store = UsageStore::open_in_memory().unwrap();
        let short_plan = source.plan(&[]).unwrap();
        short_gap_store
            .apply_source_change(
                &source,
                &short_plan.changes[0],
                &PricingCatalog::default(),
                chrono_tz::UTC,
                4 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();
        let short_active: i64 = short_gap_store
            .with_reader(|connection| {
                connection
                    .query_row(
                        "SELECT active_ms FROM sessions WHERE id = 'fallback-session'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(AppError::from)
            })
            .unwrap();
        assert_eq!(short_active, 0);

        let long_gap_store = UsageStore::open_in_memory().unwrap();
        let long_plan = source.plan(&[]).unwrap();
        long_gap_store
            .apply_source_change(
                &source,
                &long_plan.changes[0],
                &PricingCatalog::default(),
                chrono_tz::UTC,
                6 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();
        long_gap_store
            .with_reader(|connection| {
                let session: (i64, String, i64) = connection.query_row(
                    "SELECT active_ms, active_method, active_is_estimate
                     FROM sessions WHERE id = 'fallback-session'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?;
                assert_eq!(session, (5 * 60 * 1000, "idle_estimate".into(), 1));
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn retention_prunes_old_events_after_preserving_daily_and_session_rollups() {
        let codex = tempfile::tempdir().unwrap();
        let sessions = codex.path().join("sessions/2026/07/10");
        std::fs::create_dir_all(&sessions).unwrap();
        let content = [
            json!({"timestamp":"2026-03-01T01:00:00Z","type":"session_meta","payload":{"id":"retention-session","cwd":"C:/workspace/demo","model_provider":"openai"}}),
            json!({"timestamp":"2026-03-01T01:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0}}}}),
            json!({"timestamp":"2026-07-01T01:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":200,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0}}}}),
        ]
        .into_iter()
        .map(json_line)
        .collect::<Vec<_>>()
        .join("\n")
            + "\n";
        std::fs::write(sessions.join("rollout-retention.jsonl"), content).unwrap();

        let source = FsCodexSource::new(codex.path());
        let store = UsageStore::open_in_memory().unwrap();
        let plan = source.plan(&[]).unwrap();
        store
            .apply_source_change(
                &source,
                &plan.changes[0],
                &PricingCatalog::default(),
                chrono_tz::UTC,
                30 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();
        let retention = store
            .rebuild_rollups_and_prune(
                Utc.with_ymd_and_hms(2026, 7, 10, 12, 0, 0)
                    .unwrap()
                    .timestamp_millis(),
                90,
                chrono_tz::UTC,
            )
            .unwrap();
        assert_eq!(retention.usage_events_deleted, 1);
        store
            .with_reader(|connection| {
                let retained_events: i64 =
                    connection
                        .query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))?;
                let permanent_days: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM daily_usage_rollups WHERE workspace_id <> ''",
                    [],
                    |row| row.get(0),
                )?;
                let rollup_tokens: i64 = connection.query_row(
                    "SELECT COALESCE(SUM(total_tokens), 0) FROM daily_usage_rollups",
                    [],
                    |row| row.get(0),
                )?;
                let session_tokens: i64 = connection.query_row(
                    "SELECT total_tokens FROM sessions WHERE id = 'retention-session'",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!(retained_events, 1);
                assert_eq!(permanent_days, 2);
                assert_eq!(rollup_tokens, 200);
                assert_eq!(session_tokens, 200);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn state_and_logs_sources_use_safe_columns_and_incremental_log_watermarks() {
        const SENSITIVE: &str = "UNIQUE_STATE_LOG_PRIVACY_MARKER";
        let codex = tempfile::tempdir().unwrap();
        let state_path = codex.path().join("state_5.sqlite");
        let state = Connection::open(&state_path).unwrap();
        state
            .execute_batch(
                "CREATE TABLE threads (
                    id TEXT PRIMARY KEY, cwd TEXT NOT NULL, model_provider TEXT NOT NULL,
                    model TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
                    created_at_ms INTEGER, updated_at_ms INTEGER, archived INTEGER NOT NULL,
                    title TEXT NOT NULL, preview TEXT NOT NULL, first_user_message TEXT NOT NULL
                 );",
            )
            .unwrap();
        state
            .execute(
                "INSERT INTO threads VALUES (
                    'state-thread', 'C:/safe/project', 'openai', 'gpt-5.6-terra',
                    10, 20, 10000, 20000, 1, ?1, ?1, ?1
                 )",
                [SENSITIVE],
            )
            .unwrap();
        drop(state);

        let logs_path = codex.path().join("logs_2.sqlite");
        let logs = Connection::open(&logs_path).unwrap();
        logs.execute_batch(
            "CREATE TABLE logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                feedback_log_body TEXT, file TEXT
             );",
        )
        .unwrap();
        logs.execute(
            "INSERT INTO logs (feedback_log_body, file) VALUES (?1, ?1)",
            [SENSITIVE],
        )
        .unwrap();
        drop(logs);

        let source = FsCodexSource::new(codex.path());
        let plan = source.plan(&[]).unwrap();
        assert_eq!(plan.changes.len(), 2);
        assert_eq!(plan.changes[0].kind(), SourceKind::StateDb);
        assert_eq!(plan.changes[1].kind(), SourceKind::LogsDb);

        let database_dir = tempfile::tempdir().unwrap();
        let database_path = database_dir.path().join("usage.sqlite3");
        let store = UsageStore::open(&database_path).unwrap();
        for change in &plan.changes {
            store
                .apply_source_change(
                    &source,
                    change,
                    &PricingCatalog::default(),
                    chrono_tz::UTC,
                    30 * 60 * 1000,
                    &AtomicBool::new(false),
                )
                .unwrap();
        }
        store
            .with_reader(|connection| {
                let metadata: (String, String, i64, i64) = connection.query_row(
                    "SELECT model_provider, latest_model_raw, archived, input_tokens
                     FROM sessions WHERE id = 'state-thread'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
                assert_eq!(metadata, ("openai".into(), "gpt-5.6-terra".into(), 1, 0));
                Ok(())
            })
            .unwrap();
        let checkpoint = store
            .source_checkpoints()
            .unwrap()
            .into_iter()
            .find(|value| value.kind == SourceKind::LogsDb)
            .unwrap();
        assert_eq!(checkpoint.cursor.logs_rowid_watermark, 1);

        let logs = Connection::open(&logs_path).unwrap();
        logs.execute(
            "INSERT INTO logs (feedback_log_body, file) VALUES (?1, ?1)",
            [SENSITIVE],
        )
        .unwrap();
        drop(logs);
        let incremental = source.plan(&store.source_checkpoints().unwrap()).unwrap();
        let logs_change = incremental
            .changes
            .iter()
            .find(|value| value.kind() == SourceKind::LogsDb)
            .unwrap();
        store
            .apply_source_change(
                &source,
                logs_change,
                &PricingCatalog::default(),
                chrono_tz::UTC,
                30 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();
        let watermark = store
            .source_checkpoints()
            .unwrap()
            .into_iter()
            .find(|value| value.kind == SourceKind::LogsDb)
            .unwrap()
            .cursor
            .logs_rowid_watermark;
        assert_eq!(watermark, 2);

        let mut persisted = std::fs::read(&database_path).unwrap();
        let wal_path = database_path.with_extension("sqlite3-wal");
        if wal_path.exists() {
            persisted.extend(std::fs::read(wal_path).unwrap());
        }
        assert!(!String::from_utf8_lossy(&persisted).contains(SENSITIVE));
    }

    #[test]
    fn imports_appends_idempotently_and_never_persists_sensitive_arguments() {
        const SENSITIVE: &str = "UNIQUE_DB_PRIVACY_MARKER_C:\\secret\\source.rs";
        let codex = tempfile::tempdir().unwrap();
        let sessions = codex.path().join("sessions/2026/07/10");
        std::fs::create_dir_all(&sessions).unwrap();
        let session_path = sessions.join("rollout-integration.jsonl");
        std::fs::write(&session_path, synthetic_session(SENSITIVE)).unwrap();

        let database_dir = tempfile::tempdir().unwrap();
        let database_path = database_dir.path().join("usage.sqlite3");
        let store = UsageStore::open(&database_path).unwrap();
        let source = FsCodexSource::new(codex.path());
        let plan = source.plan(&[]).unwrap();
        assert_eq!(plan.changes.len(), 1);
        let result = store
            .apply_source_change(
                &source,
                &plan.changes[0],
                &PricingCatalog::default(),
                chrono_tz::UTC,
                30 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(!result.cancelled);

        store
            .with_reader(|connection| {
                let session: (i64, i64, i64, i64, String, String) = connection.query_row(
                    "SELECT input_tokens, cached_input_tokens, output_tokens, active_ms,
                            latest_model_raw, model_provider
                     FROM sessions WHERE id = 'thread-integration'",
                    [],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                        ))
                    },
                )?;
                assert_eq!(
                    session,
                    (100, 40, 20, 7000, "gpt-5.6-sol".into(), "openai".into())
                );
                let usage_count: i64 =
                    connection
                        .query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))?;
                let tool_count: i64 =
                    connection
                        .query_row("SELECT COUNT(*) FROM tool_events", [], |row| row.get(0))?;
                assert_eq!(usage_count, 1);
                assert_eq!(tool_count, 2);
                Ok(())
            })
            .unwrap();

        let checkpoints = store.source_checkpoints().unwrap();
        assert!(source.plan(&checkpoints).unwrap().changes.is_empty());

        let appended = json_line(
            json!({"timestamp":"2026-07-10T01:00:10Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":180,"cached_input_tokens":60,"output_tokens":50,"reasoning_output_tokens":10}}}}),
        );
        let mut file = OpenOptions::new().append(true).open(&session_path).unwrap();
        writeln!(file, "{appended}").unwrap();
        drop(file);
        let append_plan = source.plan(&store.source_checkpoints().unwrap()).unwrap();
        assert_eq!(append_plan.changes.len(), 1);
        assert_eq!(append_plan.changes[0].action(), ChangeAction::Append);
        store
            .apply_source_change(
                &source,
                &append_plan.changes[0],
                &PricingCatalog::default(),
                chrono_tz::UTC,
                30 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();
        store
            .with_reader(|connection| {
                let values: (i64, i64, i64, i64) = connection.query_row(
                    "SELECT input_tokens, cached_input_tokens, output_tokens, total_tokens
                     FROM sessions WHERE id = 'thread-integration'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
                assert_eq!(values, (180, 60, 50, 230));
                let usage_count: i64 =
                    connection
                        .query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))?;
                assert_eq!(usage_count, 2);
                Ok(())
            })
            .unwrap();

        std::fs::write(&session_path, synthetic_session(SENSITIVE)).unwrap();
        let replay_plan = source.plan(&store.source_checkpoints().unwrap()).unwrap();
        assert_eq!(replay_plan.changes.len(), 1);
        assert_eq!(replay_plan.changes[0].action(), ChangeAction::Replay);
        store
            .apply_source_change(
                &source,
                &replay_plan.changes[0],
                &PricingCatalog::default(),
                chrono_tz::UTC,
                30 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();
        store
            .with_reader(|connection| {
                let values: (i64, i64) = connection.query_row(
                    "SELECT input_tokens, total_tokens FROM sessions WHERE id = 'thread-integration'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                let event_count: i64 =
                    connection.query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))?;
                assert_eq!(values, (100, 120));
                assert_eq!(event_count, 1);
                Ok(())
            })
            .unwrap();

        let archived_dir = codex.path().join("archived_sessions");
        std::fs::create_dir_all(&archived_dir).unwrap();
        let archived_path = archived_dir.join("rollout-integration.jsonl");
        std::fs::rename(&session_path, &archived_path).unwrap();
        let archive_plan = source.plan(&store.source_checkpoints().unwrap()).unwrap();
        assert_eq!(archive_plan.changes.len(), 1);
        assert_eq!(archive_plan.changes[0].kind(), SourceKind::ArchivedSession);
        assert_eq!(archive_plan.changes[0].action(), ChangeAction::MetadataOnly);
        store
            .apply_source_change(
                &source,
                &archive_plan.changes[0],
                &PricingCatalog::default(),
                chrono_tz::UTC,
                30 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();
        let archived: i64 = store
            .with_reader(|connection| {
                connection
                    .query_row(
                        "SELECT archived FROM sessions WHERE id = 'thread-integration'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(AppError::from)
            })
            .unwrap();
        assert_eq!(archived, 1);

        store
            .rebuild_rollups_and_prune(
                Utc.with_ymd_and_hms(2026, 7, 10, 12, 0, 0)
                    .unwrap()
                    .timestamp_millis(),
                90,
                chrono_tz::UTC,
            )
            .unwrap();
        let mut persisted = std::fs::read(&database_path).unwrap();
        let wal = database_path.with_extension("sqlite3-wal");
        if wal.exists() {
            persisted.extend(std::fs::read(wal).unwrap());
        }
        assert!(!String::from_utf8_lossy(&persisted).contains(SENSITIVE));
        assert!(!String::from_utf8_lossy(&persisted).contains("secret\\source.rs"));

        let query = Arc::new(crate::query::UsageQuery::new(
            Arc::new(store.clone()),
            chrono_tz::UTC,
        ));
        let exporter = crate::export::UsageExporter::new(query);
        let export_path = database_dir.path().join("privacy-export.json");
        exporter
            .export_to_path(
                &crate::export::ExportRequest {
                    format: crate::export::ExportFormat::Json,
                    scope: crate::export::ExportScope::Sessions,
                    privacy: crate::export::ExportPrivacy::FullPath,
                    filters: crate::query::UsageFilters {
                        range: crate::query::RangeSelection {
                            preset: crate::query::RangePreset::Custom,
                            start_ms: Some(1_783_641_600_000),
                            end_ms: Some(1_783_728_000_000),
                            live_end: false,
                        },
                        workspace_id: None,
                        model_provider: None,
                        model: None,
                        archived: crate::query::ArchiveFilter::All,
                    },
                },
                export_path.to_string_lossy().as_ref(),
            )
            .unwrap();
        let exported = std::fs::read_to_string(export_path).unwrap();
        assert!(!exported.contains(SENSITIVE));
        assert!(!exported.contains("secret\\source.rs"));
    }

    #[test]
    fn cancellation_commits_only_safe_offset_and_resumes_without_source_change() {
        let codex = tempfile::tempdir().unwrap();
        let sessions = codex.path().join("sessions/2026/07/10");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(
            sessions.join("rollout-cancel.jsonl"),
            synthetic_session("PRIVATE_CANCEL_MARKER"),
        )
        .unwrap();
        let source = FsCodexSource::new(codex.path());
        let store = UsageStore::open_in_memory().unwrap();
        let first = source.plan(&[]).unwrap();
        let cancelled = AtomicBool::new(true);
        let result = store
            .apply_source_change(
                &source,
                &first.changes[0],
                &PricingCatalog::default(),
                chrono_tz::UTC,
                30 * 60 * 1000,
                &cancelled,
            )
            .unwrap();
        assert!(result.cancelled);
        assert_eq!(store.source_checkpoints().unwrap()[0].cursor.safe_offset, 0);

        let resume = source.plan(&store.source_checkpoints().unwrap()).unwrap();
        assert_eq!(resume.changes[0].action(), ChangeAction::Append);
        store
            .apply_source_change(
                &source,
                &resume.changes[0],
                &PricingCatalog::default(),
                chrono_tz::UTC,
                30 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();
        store
            .with_reader(|connection| {
                let count: i64 =
                    connection.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
                assert_eq!(count, 1);
                Ok(())
            })
            .unwrap();
    }
}
