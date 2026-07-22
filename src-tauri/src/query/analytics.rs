use std::collections::BTreeMap;

use chrono::{Days, Utc};
use rusqlite::types::Value as SqlValue;
use rusqlite::{OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};

use super::{UsageFilters, UsageQuery, canonical_provider_sql, push_archive, rollup_conditions};
use crate::error::{AppError, AppResult};
use crate::source::PARSER_VERSION;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListQuery {
    pub filters: UsageFilters,
    /// 仅限制工作区列表。None 表示不限，Some([]) 表示显式不显示任何项目。
    #[serde(default)]
    pub workspace_ids: Option<Vec<String>>,
    #[serde(default)]
    pub search: String,
    #[serde(default)]
    pub sort: String,
    #[serde(default = "default_true")]
    pub descending: bool,
    #[serde(default)]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRow {
    pub id: String,
    pub label: String,
    pub normalized_path: String,
    pub ignored: bool,
    pub session_count: i64,
    pub total_tokens: i64,
    pub estimated_cost_microusd: Option<i64>,
    pub unpriced_event_count: i64,
    pub active_ms: i64,
    pub active_days: i64,
    pub last_activity_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCatalogItem {
    pub value: String,
    pub label: String,
    pub normalized_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRow {
    pub id: String,
    pub title: String,
    pub workspace_id: String,
    pub workspace_label: String,
    pub started_at_ms: Option<i64>,
    pub ended_at_ms: Option<i64>,
    pub active_ms: i64,
    pub active_method: String,
    pub active_is_estimate: bool,
    pub model_provider: String,
    pub latest_model: String,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub fresh_input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub estimated_cost_microusd: Option<i64>,
    pub unpriced_event_count: i64,
    pub archived: bool,
    pub integrity_status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRow {
    pub model: String,
    pub pricing_model_id: Option<String>,
    pub session_count: i64,
    pub input_tokens: i64,
    pub fresh_input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub total_tokens: i64,
    pub cache_hit_rate: Option<f64>,
    pub estimated_cost_microusd: Option<i64>,
    pub unpriced_event_count: i64,
    pub average_cost_microusd_per_million_tokens: Option<f64>,
    pub last_used_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeatmapMetric {
    Sessions,
    Tokens,
    ActiveTime,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeatmapSpan {
    Week,
    Month,
    Year,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapQuery {
    pub filters: UsageFilters,
    pub metric: HeatmapMetric,
    pub span: HeatmapSpan,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapPoint {
    pub date: String,
    pub value: i64,
    pub session_count: i64,
    pub total_tokens: i64,
    pub active_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapSnapshot {
    pub metric: HeatmapMetric,
    pub span: HeatmapSpan,
    pub start_date: String,
    pub end_date: String,
    pub points: Vec<HeatmapPoint>,
    pub max_value: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStat {
    pub tool_name: String,
    pub category: String,
    pub operation_kind: String,
    pub call_count: i64,
    pub session_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryStat {
    pub category: String,
    pub call_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolTrendPoint {
    pub date: String,
    pub call_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsSnapshot {
    pub total_calls: i64,
    pub unique_tools: i64,
    pub top_tools: Vec<ToolStat>,
    pub categories: Vec<CategoryStat>,
    pub trend: Vec<ToolTrendPoint>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSegmentRow {
    pub segment_index: i64,
    pub started_at_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub estimated_cost_microusd: Option<i64>,
    pub unpriced_event_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySegmentRow {
    pub segment_index: i64,
    pub started_at_ms: i64,
    pub ended_at_ms: i64,
    pub active_ms: i64,
    pub method: String,
    pub is_estimate: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEventRow {
    pub id: i64,
    pub occurred_at_ms: i64,
    pub provider: String,
    pub model: String,
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost_microusd: Option<i64>,
    pub integrity_status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub session: SessionRow,
    pub parsing: SessionParsingSummary,
    pub model_segments: Vec<ModelSegmentRow>,
    pub activity_segments: Vec<ActivitySegmentRow>,
    pub tools: Vec<ToolStat>,
    pub recent_usage_events: Vec<UsageEventRow>,
    pub retained_event_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionParsingSummary {
    pub source_kind: String,
    pub source_status: String,
    pub parser_version: i64,
    pub warning_count: i64,
    pub last_error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticSource {
    pub kind: String,
    pub relative_path: String,
    pub status: String,
    pub file_size: i64,
    pub safe_offset: i64,
    pub logs_rowid_watermark: i64,
    pub parser_version: i64,
    pub last_error_code: Option<String>,
    pub last_seen_at_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentSyncRun {
    pub id: String,
    pub mode: String,
    pub status: String,
    pub stage: String,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub files_completed: i64,
    pub files_total: i64,
    pub bytes_read: i64,
    pub records_skipped: i64,
    pub elapsed_ms: Option<i64>,
    pub parse_failures: i64,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSnapshot {
    pub database_size_bytes: u64,
    pub database_integrity_ok: bool,
    pub schema_version: i64,
    pub parser_version: i64,
    pub indexed_sessions: i64,
    pub retained_usage_events: i64,
    pub retained_tool_events: i64,
    pub sources: Vec<DiagnosticSource>,
    pub recent_runs: Vec<RecentSyncRun>,
    pub generated_at_ms: i64,
}

impl UsageQuery {
    pub fn workspaces(&self, query: &ListQuery) -> AppResult<Page<WorkspaceRow>> {
        let (page, page_size, offset) = page_args(query.page, query.page_size);
        let range = self.resolve_range(&query.filters.range, Utc::now().timestamp_millis())?;
        let (mut condition, mut values) = rollup_conditions(&query.filters, &range, "d");
        if let Some(workspace_ids) = query.workspace_ids.as_ref() {
            if workspace_ids.is_empty() {
                condition.push_str(" AND 1 = 0");
            } else {
                condition.push_str(" AND w.id IN (");
                condition.push_str(&vec!["?"; workspace_ids.len()].join(", "));
                condition.push(')');
                values.extend(workspace_ids.iter().cloned().map(SqlValue::Text));
            }
        }
        if !query.search.trim().is_empty() {
            condition.push_str(
                " AND (COALESCE(w.alias, w.display_name) LIKE ? OR w.normalized_path LIKE ?)",
            );
            let needle = format!("%{}%", query.search.trim());
            values.push(SqlValue::Text(needle.clone()));
            values.push(SqlValue::Text(needle));
        }
        let order = workspace_order(&query.sort);
        let direction = if query.descending { "DESC" } else { "ASC" };
        self.store.with_reader(|connection| {
            let count_sql = format!(
                "SELECT COUNT(*) FROM (
                    SELECT w.id FROM session_daily_usage d
                    JOIN workspaces w ON w.id = d.workspace_id
                    WHERE {condition} GROUP BY w.id
                 )"
            );
            let total: i64 =
                connection.query_row(&count_sql, params_from_iter(values.iter()), |row| {
                    row.get(0)
                })?;
            let sql = format!(
                "SELECT w.id, COALESCE(w.alias, w.display_name), w.normalized_path, w.ignored,
                        COUNT(DISTINCT d.session_id), SUM(d.total_tokens),
                        SUM(d.priced_cost_microusd), SUM(d.priced_event_count),
                        SUM(d.unpriced_event_count), SUM(d.active_ms),
                        COUNT(DISTINCT d.local_date), MAX(d.last_activity_at_ms)
                 FROM session_daily_usage d JOIN workspaces w ON w.id = d.workspace_id
                 WHERE {condition} GROUP BY w.id
                 ORDER BY {order} {direction}, w.id ASC LIMIT ? OFFSET ?"
            );
            let mut paged_values = values;
            paged_values.push(SqlValue::Integer(i64::from(page_size)));
            paged_values.push(SqlValue::Integer(offset));
            let mut statement = connection.prepare(&sql)?;
            let items = statement
                .query_map(params_from_iter(paged_values.iter()), |row| {
                    let priced_events: i64 = row.get(7)?;
                    Ok(WorkspaceRow {
                        id: row.get(0)?,
                        label: row.get(1)?,
                        normalized_path: row.get(2)?,
                        ignored: row.get::<_, i64>(3)? != 0,
                        session_count: row.get(4)?,
                        total_tokens: row.get(5)?,
                        estimated_cost_microusd: (priced_events > 0)
                            .then(|| row.get(6))
                            .transpose()?,
                        unpriced_event_count: row.get(8)?,
                        active_ms: row.get(9)?,
                        active_days: row.get(10)?,
                        last_activity_at_ms: row.get(11)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Page {
                items,
                page,
                page_size,
                total: total.max(0) as u64,
            })
        })
    }

    pub fn workspace_catalog(&self) -> AppResult<Vec<WorkspaceCatalogItem>> {
        self.store.with_reader(|connection| {
            let mut statement = connection.prepare(
                "SELECT w.id, COALESCE(w.alias, w.display_name), w.normalized_path
                 FROM workspaces w
                 WHERE w.ignored = 0
                 ORDER BY COALESCE((SELECT MAX(s.ended_at_ms) FROM sessions s
                                    WHERE s.workspace_id = w.id), w.updated_at_ms) DESC,
                          COALESCE(w.alias, w.display_name) COLLATE NOCASE ASC",
            )?;
            statement
                .query_map([], |row| {
                    Ok(WorkspaceCatalogItem {
                        value: row.get(0)?,
                        label: row.get(1)?,
                        normalized_path: row.get(2)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(AppError::from)
        })
    }

    pub fn sessions(&self, query: &ListQuery) -> AppResult<Page<SessionRow>> {
        let (page, page_size, offset) = page_args(query.page, query.page_size);
        let range = self.resolve_range(&query.filters.range, Utc::now().timestamp_millis())?;
        let mut clauses = vec![
            "s.workspace_id IN (SELECT id FROM workspaces WHERE ignored = 0)".to_string(),
            "COALESCE(s.ended_at_ms, s.started_at_ms, 0) >= ?".to_string(),
            "COALESCE(s.started_at_ms, 0) < ?".to_string(),
            "LOWER(TRIM(COALESCE(NULLIF(s.primary_model_raw, ''), s.latest_model_raw))) \
             <> 'codex-auto-review'"
                .to_string(),
        ];
        let mut values = vec![
            SqlValue::Integer(range.start_ms),
            SqlValue::Integer(range.end_ms),
        ];
        if let Some(workspace) = query.filters.workspace_id.as_deref() {
            clauses.push("s.workspace_id = ?".into());
            values.push(SqlValue::Text(workspace.into()));
        }
        if let Some(provider) = query.filters.model_provider.as_deref() {
            clauses.push(format!(
                "{} = ?",
                canonical_provider_sql("s.model_provider")
            ));
            values.push(SqlValue::Text(provider.into()));
        }
        if let Some(model) = query.filters.model.as_deref() {
            clauses
                .push("COALESCE(NULLIF(s.primary_model_raw, ''), s.latest_model_raw) = ?".into());
            values.push(SqlValue::Text(model.into()));
        }
        push_archive(&mut clauses, &mut values, query.filters.archived, "s");
        if !query.search.trim().is_empty() {
            clauses.push(
                "(s.synthetic_title LIKE ? OR COALESCE(w.alias, w.display_name) LIKE ?)".into(),
            );
            let needle = format!("%{}%", query.search.trim());
            values.push(SqlValue::Text(needle.clone()));
            values.push(SqlValue::Text(needle));
        }
        let condition = clauses.join(" AND ");
        let order = session_order(&query.sort);
        let direction = if query.descending { "DESC" } else { "ASC" };
        self.store.with_reader(|connection| {
            let count_sql = format!(
                "SELECT COUNT(*) FROM sessions s JOIN workspaces w ON w.id = s.workspace_id WHERE {condition}"
            );
            let total: i64 = connection.query_row(
                &count_sql,
                params_from_iter(values.iter()),
                |row| row.get(0),
            )?;
            let sql = format!(
                "SELECT s.id, s.synthetic_title, s.workspace_id,
                        COALESCE(w.alias, w.display_name), s.started_at_ms, s.ended_at_ms,
                        s.active_ms, s.active_method, s.active_is_estimate,
                        {provider}, COALESCE(NULLIF(s.primary_model_raw, ''), s.latest_model_raw),
                        s.total_tokens,
                        s.input_tokens, s.fresh_input_tokens, s.cached_input_tokens,
                        s.output_tokens, s.reasoning_tokens, s.estimated_cost_microusd,
                        s.unpriced_event_count, s.archived, s.integrity_status
                 FROM sessions s JOIN workspaces w ON w.id = s.workspace_id
                 WHERE {condition}
                 ORDER BY {order} {direction}, s.id ASC LIMIT ? OFFSET ?",
                provider = canonical_provider_sql("s.model_provider"),
            );
            let mut paged_values = values;
            paged_values.push(SqlValue::Integer(i64::from(page_size)));
            paged_values.push(SqlValue::Integer(offset));
            let mut statement = connection.prepare(&sql)?;
            let items = statement
                .query_map(params_from_iter(paged_values.iter()), session_from_row)?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Page { items, page, page_size, total: total.max(0) as u64 })
        })
    }

    pub fn models(&self, query: &ListQuery) -> AppResult<Page<ModelRow>> {
        let (page, page_size, offset) = page_args(query.page, query.page_size);
        let range = self.resolve_range(&query.filters.range, Utc::now().timestamp_millis())?;
        let (mut condition, mut values) = rollup_conditions(&query.filters, &range, "d");
        condition.push_str(
            " AND TRIM(LOWER(d.model_raw)) NOT IN ('', 'unknown', '(unknown)')\
              AND TRIM(LOWER(d.model_provider)) NOT IN ('', 'unknown', '(unknown)')",
        );
        if !query.search.trim().is_empty() {
            condition.push_str(" AND d.model_raw LIKE ?");
            let needle = format!("%{}%", query.search.trim());
            values.push(SqlValue::Text(needle));
        }
        let order = model_order(&query.sort);
        let direction = if query.descending { "DESC" } else { "ASC" };
        self.store.with_reader(|connection| {
            let count_sql = format!(
                "SELECT COUNT(*) FROM (
                    SELECT d.model_raw FROM session_daily_usage d
                    WHERE {condition} GROUP BY d.model_raw
                 )"
            );
            let total: i64 =
                connection.query_row(&count_sql, params_from_iter(values.iter()), |row| {
                    row.get(0)
                })?;
            let sql = format!(
                "SELECT d.model_raw, NULLIF(MAX(d.pricing_model_id), ''),
                        COUNT(DISTINCT d.session_id), SUM(d.input_tokens),
                        SUM(d.fresh_input_tokens), SUM(d.cached_input_tokens),
                        SUM(d.output_tokens), SUM(d.reasoning_tokens), SUM(d.total_tokens),
                        SUM(d.priced_cost_microusd), SUM(d.priced_event_count),
                        SUM(d.unpriced_event_count), MAX(d.last_activity_at_ms)
                 FROM session_daily_usage d WHERE {condition}
                 GROUP BY d.model_raw
                 ORDER BY {order} {direction}, d.model_raw ASC LIMIT ? OFFSET ?"
            );
            let mut paged_values = values;
            paged_values.push(SqlValue::Integer(i64::from(page_size)));
            paged_values.push(SqlValue::Integer(offset));
            let mut statement = connection.prepare(&sql)?;
            let items = statement
                .query_map(params_from_iter(paged_values.iter()), |row| {
                    let sessions: i64 = row.get(2)?;
                    let input: i64 = row.get(3)?;
                    let cached: i64 = row.get(5)?;
                    let total_tokens: i64 = row.get(8)?;
                    let cost: i64 = row.get(9)?;
                    let priced_events: i64 = row.get(10)?;
                    let priced_cost = (priced_events > 0).then_some(cost);
                    Ok(ModelRow {
                        model: row.get(0)?,
                        pricing_model_id: row.get(1)?,
                        session_count: sessions,
                        input_tokens: input,
                        fresh_input_tokens: row.get(4)?,
                        cached_input_tokens: cached,
                        output_tokens: row.get(6)?,
                        reasoning_tokens: row.get(7)?,
                        total_tokens,
                        cache_hit_rate: (input > 0).then_some(cached as f64 / input as f64),
                        estimated_cost_microusd: priced_cost,
                        unpriced_event_count: row.get(11)?,
                        average_cost_microusd_per_million_tokens: priced_cost
                            .map(|value| value as f64 * 1_000_000_f64 / total_tokens.max(1) as f64),
                        last_used_at_ms: row.get(12)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Page {
                items,
                page,
                page_size,
                total: total.max(0) as u64,
            })
        })
    }

    pub fn heatmap(&self, query: &HeatmapQuery) -> AppResult<HeatmapSnapshot> {
        let today = Utc::now().with_timezone(&self.timezone).date_naive();
        let start = today
            .checked_sub_days(Days::new(heatmap_day_count(query.span).saturating_sub(1)))
            .ok_or_else(|| AppError::new("invalid_heatmap_range", "热力图日期范围无效"))?;
        let mut clauses = vec![
            "d.local_date BETWEEN ? AND ?".to_string(),
            "d.workspace_id IN (SELECT id FROM workspaces WHERE ignored = 0)".to_string(),
            "LOWER(TRIM(d.model_raw)) <> 'codex-auto-review'".to_string(),
        ];
        let mut values = vec![
            SqlValue::Text(start.format("%Y-%m-%d").to_string()),
            SqlValue::Text(today.format("%Y-%m-%d").to_string()),
        ];
        if let Some(workspace) = query.filters.workspace_id.as_deref() {
            clauses.push("d.workspace_id = ?".into());
            values.push(SqlValue::Text(workspace.into()));
        }
        if let Some(provider) = query.filters.model_provider.as_deref() {
            clauses.push(format!(
                "{} = ?",
                canonical_provider_sql("d.model_provider")
            ));
            values.push(SqlValue::Text(provider.into()));
        }
        if let Some(model) = query.filters.model.as_deref() {
            clauses.push("d.model_raw = ?".into());
            values.push(SqlValue::Text(model.into()));
        }
        push_archive(&mut clauses, &mut values, query.filters.archived, "d");
        let rows = self.store.with_reader(|connection| {
            let sql = format!(
                "SELECT d.local_date, COUNT(DISTINCT d.session_id),
                        SUM(d.total_tokens), SUM(d.active_ms)
                 FROM session_daily_usage d WHERE {}
                 GROUP BY d.local_date ORDER BY d.local_date",
                clauses.join(" AND ")
            );
            let mut statement = connection.prepare(&sql)?;
            statement
                .query_map(params_from_iter(values.iter()), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(AppError::from)
        })?;
        let by_date: BTreeMap<_, _> = rows
            .into_iter()
            .map(|(date, sessions, tokens, active)| (date, (sessions, tokens, active)))
            .collect();
        let mut points = Vec::new();
        let mut cursor = start;
        let mut max_value = 0_i64;
        while cursor <= today {
            let date = cursor.format("%Y-%m-%d").to_string();
            let (session_count, total_tokens, active_ms) =
                by_date.get(&date).copied().unwrap_or_default();
            let value = match query.metric {
                HeatmapMetric::Sessions => session_count,
                HeatmapMetric::Tokens => total_tokens,
                HeatmapMetric::ActiveTime => active_ms,
            };
            max_value = max_value.max(value);
            points.push(HeatmapPoint {
                date,
                value,
                session_count,
                total_tokens,
                active_ms,
            });
            cursor = cursor
                .checked_add_days(Days::new(1))
                .ok_or_else(|| AppError::new("invalid_heatmap_range", "热力图日期溢出"))?;
        }
        Ok(HeatmapSnapshot {
            metric: query.metric,
            span: query.span,
            start_date: start.format("%Y-%m-%d").to_string(),
            end_date: today.format("%Y-%m-%d").to_string(),
            points,
            max_value,
        })
    }

    pub fn tools(&self, filters: &UsageFilters) -> AppResult<ToolsSnapshot> {
        let range = self.resolve_range(&filters.range, Utc::now().timestamp_millis())?;
        let (condition, values) = tool_conditions(
            filters,
            &range.start_date_string(),
            &range.end_date_string(),
        );
        self.store.with_reader(|connection| {
            let (total_calls, unique_tools): (i64, i64) = connection.query_row(
                &format!(
                    "SELECT COALESCE(SUM(t.call_count), 0), COUNT(DISTINCT t.tool_name)
                     FROM session_daily_tool t WHERE {condition}"
                ),
                params_from_iter(values.iter()),
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let top_tools = {
                let mut statement = connection.prepare(&format!(
                    "SELECT t.tool_name, t.category, t.operation_kind,
                            SUM(t.call_count), COUNT(DISTINCT t.session_id)
                     FROM session_daily_tool t WHERE {condition}
                     GROUP BY t.tool_name, t.category, t.operation_kind
                     ORDER BY SUM(t.call_count) DESC, t.tool_name LIMIT 10"
                ))?;
                statement
                    .query_map(params_from_iter(values.iter()), |row| {
                        Ok(ToolStat {
                            tool_name: row.get(0)?,
                            category: row.get(1)?,
                            operation_kind: row.get(2)?,
                            call_count: row.get(3)?,
                            session_count: row.get(4)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            let categories = {
                let mut statement = connection.prepare(&format!(
                    "SELECT t.category, SUM(t.call_count) FROM session_daily_tool t
                     WHERE {condition} GROUP BY t.category ORDER BY SUM(t.call_count) DESC"
                ))?;
                statement
                    .query_map(params_from_iter(values.iter()), |row| {
                        Ok(CategoryStat {
                            category: row.get(0)?,
                            call_count: row.get(1)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            let trend = {
                let mut statement = connection.prepare(&format!(
                    "SELECT t.local_date, SUM(t.call_count) FROM session_daily_tool t
                     WHERE {condition} GROUP BY t.local_date ORDER BY t.local_date"
                ))?;
                statement
                    .query_map(params_from_iter(values.iter()), |row| {
                        Ok(ToolTrendPoint {
                            date: row.get(0)?,
                            call_count: row.get(1)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            Ok(ToolsSnapshot {
                total_calls,
                unique_tools,
                top_tools,
                categories,
                trend,
            })
        })
    }

    pub fn session_detail(&self, session_id: &str) -> AppResult<SessionDetail> {
        self.store.with_reader(|connection| {
            let session = connection
                .query_row(
                    &format!("SELECT s.id, s.synthetic_title, s.workspace_id,
                            COALESCE(w.alias, w.display_name), s.started_at_ms, s.ended_at_ms,
                            s.active_ms, s.active_method, s.active_is_estimate,
                            {provider}, COALESCE(NULLIF(s.primary_model_raw, ''), s.latest_model_raw),
                            s.total_tokens,
                            s.input_tokens, s.fresh_input_tokens, s.cached_input_tokens,
                            s.output_tokens, s.reasoning_tokens, s.estimated_cost_microusd,
                            s.unpriced_event_count, s.archived, s.integrity_status
                     FROM sessions s JOIN workspaces w ON w.id = s.workspace_id
                     WHERE s.id = ?1
                       AND LOWER(TRIM(COALESCE(NULLIF(s.primary_model_raw, ''), s.latest_model_raw)))
                           <> 'codex-auto-review'",
                        provider = canonical_provider_sql("s.model_provider")
                    ),
                    [session_id],
                    session_from_row,
                )
                .optional()?
                .ok_or_else(|| AppError::new("session_not_found", "会话不存在"))?;
            let parsing = connection.query_row(
                "SELECT COALESCE(sf.source_kind, 'session'), COALESCE(sf.status, 'missing'),
                        s.parser_version, s.warning_count, sf.last_error_code
                 FROM sessions s LEFT JOIN source_files sf ON sf.id = s.source_file_id
                 WHERE s.id = ?1",
                [session_id],
                |row| {
                    Ok(SessionParsingSummary {
                        source_kind: row.get(0)?,
                        source_status: row.get(1)?,
                        parser_version: row.get(2)?,
                        warning_count: row.get(3)?,
                        last_error_code: row.get(4)?,
                    })
                },
            )?;
            let model_segments = {
                let mut statement = connection.prepare(
                    &format!("SELECT MIN(segment_index), MIN(started_at_ms), MAX(ended_at_ms),
                            {provider}, model_raw, SUM(input_tokens),
                            SUM(cached_input_tokens), SUM(output_tokens),
                            SUM(reasoning_tokens), SUM(estimated_cost_microusd),
                            SUM(unpriced_event_count)
                     FROM session_model_segments
                     WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'
                     GROUP BY model_provider, model_raw
                     ORDER BY MIN(started_at_ms), model_raw",
                        provider = canonical_provider_sql("model_provider")
                    ),
                )?;
                statement
                    .query_map([session_id], |row| {
                        Ok(ModelSegmentRow {
                            segment_index: row.get(0)?, started_at_ms: row.get(1)?,
                            ended_at_ms: row.get(2)?, provider: row.get(3)?, model: row.get(4)?,
                            input_tokens: row.get(5)?, cached_input_tokens: row.get(6)?,
                            output_tokens: row.get(7)?, reasoning_tokens: row.get(8)?,
                            estimated_cost_microusd: row.get(9)?, unpriced_event_count: row.get(10)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            let activity_segments = {
                let mut statement = connection.prepare(
                    "SELECT segment_index, started_at_ms, ended_at_ms, active_ms, method, is_estimate
                     FROM activity_segments
                     WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'
                     ORDER BY segment_index",
                )?;
                statement
                    .query_map([session_id], |row| {
                        Ok(ActivitySegmentRow {
                            segment_index: row.get(0)?, started_at_ms: row.get(1)?,
                            ended_at_ms: row.get(2)?, active_ms: row.get(3)?,
                            method: row.get(4)?, is_estimate: row.get::<_, i64>(5)? != 0,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            let tools = {
                let mut statement = connection.prepare(
                    "SELECT tool_name, category, operation_kind, SUM(call_count), 1
                     FROM session_daily_tool WHERE session_id = ?1
                     GROUP BY tool_name, category, operation_kind ORDER BY SUM(call_count) DESC",
                )?;
                statement
                    .query_map([session_id], |row| {
                        Ok(ToolStat {
                            tool_name: row.get(0)?, category: row.get(1)?, operation_kind: row.get(2)?,
                            call_count: row.get(3)?, session_count: row.get(4)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            let retained_event_count: i64 = connection.query_row(
                "SELECT COUNT(*) FROM usage_events
                 WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'",
                [session_id],
                |row| row.get(0),
            )?;
            let recent_usage_events = {
                let mut statement = connection.prepare(
                    &format!("SELECT id, occurred_at_ms, {provider}, model_raw,
                            input_tokens, cached_input_tokens, output_tokens,
                            reasoning_tokens, total_tokens, estimated_cost_microusd, integrity_status
                     FROM usage_events
                     WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'
                     ORDER BY occurred_at_ms DESC, id DESC LIMIT 200",
                        provider = canonical_provider_sql("model_provider")
                    ),
                )?;
                statement
                    .query_map([session_id], |row| {
                        Ok(UsageEventRow {
                            id: row.get(0)?, occurred_at_ms: row.get(1)?, provider: row.get(2)?,
                            model: row.get(3)?, input_tokens: row.get(4)?,
                            cached_input_tokens: row.get(5)?, output_tokens: row.get(6)?,
                            reasoning_tokens: row.get(7)?, total_tokens: row.get(8)?,
                            estimated_cost_microusd: row.get(9)?, integrity_status: row.get(10)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            Ok(SessionDetail {
                session, parsing, model_segments, activity_segments, tools,
                recent_usage_events, retained_event_count,
            })
        })
    }

    pub fn usage_events(
        &self,
        session_id: &str,
        page: u32,
        requested_page_size: u32,
    ) -> AppResult<Page<UsageEventRow>> {
        let (page, page_size, offset) = page_args(page, requested_page_size);
        self.store.with_reader(|connection| {
            let total: i64 = connection.query_row(
                "SELECT COUNT(*) FROM usage_events
                 WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'",
                [session_id],
                |row| row.get(0),
            )?;
            let mut statement = connection.prepare(&format!(
                "SELECT id, occurred_at_ms, {provider}, model_raw,
                        input_tokens, cached_input_tokens, output_tokens,
                        reasoning_tokens, total_tokens, estimated_cost_microusd, integrity_status
                 FROM usage_events
                 WHERE session_id = ?1 AND LOWER(TRIM(model_raw)) <> 'codex-auto-review'
                 ORDER BY occurred_at_ms DESC, id DESC LIMIT ?2 OFFSET ?3",
                provider = canonical_provider_sql("model_provider")
            ))?;
            let items = statement
                .query_map(
                    params![session_id, i64::from(page_size), offset],
                    usage_event_from_row,
                )?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Page {
                items,
                page,
                page_size,
                total: total.max(0) as u64,
            })
        })
    }

    pub fn diagnostics(&self) -> AppResult<DiagnosticsSnapshot> {
        let (indexed_sessions, retained_usage_events, retained_tool_events, sources, recent_runs) =
            self.store.with_reader(|connection| {
                let indexed_sessions = connection.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
                let retained_usage_events = connection.query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))?;
                let retained_tool_events = connection.query_row("SELECT COUNT(*) FROM tool_events", [], |row| row.get(0))?;
                let sources = {
                    let mut statement = connection.prepare(
                        "SELECT source_kind, relative_path, status, file_size, safe_offset,
                                logs_rowid_watermark, parser_version, last_error_code, last_seen_at_ms
                         FROM source_files ORDER BY source_kind, relative_path LIMIT 5000",
                    )?;
                    statement.query_map([], |row| {
                        Ok(DiagnosticSource {
                            kind: row.get(0)?, relative_path: row.get(1)?, status: row.get(2)?,
                            file_size: row.get(3)?, safe_offset: row.get(4)?,
                            logs_rowid_watermark: row.get(5)?, parser_version: row.get(6)?,
                            last_error_code: row.get(7)?, last_seen_at_ms: row.get(8)?,
                        })
                    })?.collect::<Result<Vec<_>, _>>()?
                };
                let recent_runs = {
                    let mut statement = connection.prepare(
                        "SELECT id, mode, status, stage, started_at_ms, finished_at_ms,
                                files_completed, files_total, bytes_read, records_skipped,
                                elapsed_ms, parse_failures, error_code
                         FROM sync_runs ORDER BY started_at_ms DESC LIMIT 20",
                    )?;
                    statement.query_map([], |row| {
                        Ok(RecentSyncRun {
                            id: row.get(0)?, mode: row.get(1)?, status: row.get(2)?, stage: row.get(3)?,
                            started_at_ms: row.get(4)?, finished_at_ms: row.get(5)?,
                            files_completed: row.get(6)?, files_total: row.get(7)?, bytes_read: row.get(8)?,
                            records_skipped: row.get(9)?, elapsed_ms: row.get(10)?,
                            parse_failures: row.get(11)?, error_code: row.get(12)?,
                        })
                    })?.collect::<Result<Vec<_>, _>>()?
                };
                Ok((indexed_sessions, retained_usage_events, retained_tool_events, sources, recent_runs))
            })?;
        Ok(DiagnosticsSnapshot {
            database_size_bytes: self.store.database_size_bytes()?,
            database_integrity_ok: self.store.integrity_check()?,
            schema_version: self.store.schema_version()?,
            parser_version: PARSER_VERSION,
            indexed_sessions,
            retained_usage_events,
            retained_tool_events,
            sources,
            recent_runs,
            generated_at_ms: Utc::now().timestamp_millis(),
        })
    }
}

fn session_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    Ok(SessionRow {
        id: row.get(0)?,
        title: row.get(1)?,
        workspace_id: row.get(2)?,
        workspace_label: row.get(3)?,
        started_at_ms: row.get(4)?,
        ended_at_ms: row.get(5)?,
        active_ms: row.get(6)?,
        active_method: row.get(7)?,
        active_is_estimate: row.get::<_, i64>(8)? != 0,
        model_provider: row.get(9)?,
        latest_model: row.get(10)?,
        total_tokens: row.get(11)?,
        input_tokens: row.get(12)?,
        fresh_input_tokens: row.get(13)?,
        cached_input_tokens: row.get(14)?,
        output_tokens: row.get(15)?,
        reasoning_tokens: row.get(16)?,
        estimated_cost_microusd: row.get(17)?,
        unpriced_event_count: row.get(18)?,
        archived: row.get::<_, i64>(19)? != 0,
        integrity_status: row.get(20)?,
    })
}

fn usage_event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UsageEventRow> {
    Ok(UsageEventRow {
        id: row.get(0)?,
        occurred_at_ms: row.get(1)?,
        provider: row.get(2)?,
        model: row.get(3)?,
        input_tokens: row.get(4)?,
        cached_input_tokens: row.get(5)?,
        output_tokens: row.get(6)?,
        reasoning_tokens: row.get(7)?,
        total_tokens: row.get(8)?,
        estimated_cost_microusd: row.get(9)?,
        integrity_status: row.get(10)?,
    })
}

fn page_args(page: u32, requested_size: u32) -> (u32, u32, i64) {
    let page_size = requested_size.clamp(1, 100);
    let offset = i64::from(page).saturating_mul(i64::from(page_size));
    (page, page_size, offset)
}

fn workspace_order(sort: &str) -> &'static str {
    match sort {
        "tokens" => "SUM(d.total_tokens)",
        "cost" => "SUM(d.priced_cost_microusd)",
        "active_time" => "SUM(d.active_ms)",
        "sessions" => "COUNT(DISTINCT d.session_id)",
        "name" => "COALESCE(w.alias, w.display_name) COLLATE NOCASE",
        _ => "MAX(d.last_activity_at_ms)",
    }
}

fn session_order(sort: &str) -> &'static str {
    match sort {
        "tokens" => "s.total_tokens",
        "cost" => "s.estimated_cost_microusd",
        "active_time" => "s.active_ms",
        "started" => "s.started_at_ms",
        _ => "COALESCE(s.ended_at_ms, s.started_at_ms)",
    }
}

fn model_order(sort: &str) -> &'static str {
    match sort {
        "sessions" => "COUNT(DISTINCT d.session_id)",
        "cost" => "SUM(d.priced_cost_microusd)",
        "name" => "model_strength_key(d.model_raw)",
        "recent" => "MAX(d.last_activity_at_ms)",
        _ => "SUM(d.total_tokens)",
    }
}

fn tool_conditions(
    filters: &UsageFilters,
    start_date: &str,
    end_date: &str,
) -> (String, Vec<SqlValue>) {
    let mut clauses = vec![
        "t.local_date BETWEEN ? AND ?".to_string(),
        "t.workspace_id IN (SELECT id FROM workspaces WHERE ignored = 0)".to_string(),
    ];
    let mut values = vec![
        SqlValue::Text(start_date.into()),
        SqlValue::Text(end_date.into()),
    ];
    if let Some(workspace) = filters.workspace_id.as_deref() {
        clauses.push("t.workspace_id = ?".into());
        values.push(SqlValue::Text(workspace.into()));
    }
    push_archive(&mut clauses, &mut values, filters.archived, "t");
    if filters.model_provider.is_some() || filters.model.is_some() {
        let mut nested = vec!["u.session_id = t.session_id".to_string()];
        if let Some(provider) = filters.model_provider.as_deref() {
            nested.push(format!(
                "{} = ?",
                canonical_provider_sql("u.model_provider")
            ));
            values.push(SqlValue::Text(provider.into()));
        }
        if let Some(model) = filters.model.as_deref() {
            nested.push("u.model_raw = ?".into());
            values.push(SqlValue::Text(model.into()));
        }
        clauses.push(format!(
            "EXISTS (SELECT 1 FROM session_daily_usage u WHERE {})",
            nested.join(" AND ")
        ));
    }
    (clauses.join(" AND "), values)
}

fn default_page_size() -> u32 {
    25
}

fn heatmap_day_count(span: HeatmapSpan) -> u64 {
    match span {
        HeatmapSpan::Week => 7,
        HeatmapSpan::Month => 30,
        HeatmapSpan::Year => 365,
    }
}
fn default_true() -> bool {
    true
}

impl Serialize for HeatmapMetric {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            Self::Sessions => "sessions",
            Self::Tokens => "tokens",
            Self::ActiveTime => "active_time",
        })
    }
}

impl Serialize for HeatmapSpan {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{ArchiveFilter, RangePreset, RangeSelection};

    fn filters() -> UsageFilters {
        UsageFilters {
            range: RangeSelection {
                preset: RangePreset::Custom,
                start_ms: Some(1_783_641_600_000),
                end_ms: Some(1_783_728_000_000),
                live_end: false,
            },
            workspace_id: None,
            model_provider: None,
            model: None,
            archived: ArchiveFilter::All,
        }
    }

    fn list_query() -> ListQuery {
        ListQuery {
            filters: filters(),
            workspace_ids: None,
            search: String::new(),
            sort: String::new(),
            descending: true,
            page: 0,
            page_size: 25,
        }
    }

    #[test]
    fn page_queries_are_server_paginated_and_keep_priced_values() {
        let query = crate::query::tests::seeded_query();
        let workspaces = query.workspaces(&list_query()).unwrap();
        let sessions = query.sessions(&list_query()).unwrap();
        let models = query.models(&list_query()).unwrap();
        let events = query.usage_events("s1", 0, 25).unwrap();
        let detail = query.session_detail("s1").unwrap();
        assert_eq!(workspaces.total, 1);
        assert_eq!(sessions.total, 1);
        assert_eq!(models.total, 1);
        assert_eq!(events.total, 1);
        assert_eq!(models.items[0].estimated_cost_microusd, Some(500));
        assert!(
            (models.items[0]
                .average_cost_microusd_per_million_tokens
                .unwrap()
                - 4_166_666.666_666_667)
                .abs()
                < 0.001
        );
        assert_eq!(detail.parsing.source_kind, "session");
        assert_eq!(detail.parsing.source_status, "ready");
    }

    #[test]
    fn workspace_visibility_only_limits_the_projects_list() {
        let query = crate::query::tests::seeded_query();
        let mut list = list_query();
        list.workspace_ids = Some(Vec::new());
        assert_eq!(query.workspaces(&list).unwrap().total, 0);

        list.workspace_ids = Some(vec!["w1".into()]);
        assert_eq!(query.workspaces(&list).unwrap().total, 1);
        assert_eq!(query.workspace_catalog().unwrap()[0].value, "w1");

        let dashboard = query.dashboard(&filters()).unwrap();
        assert_eq!(dashboard.hero.session_count, 1);
    }

    #[test]
    fn tools_and_heatmap_are_aggregated_in_sql() {
        let query = crate::query::tests::seeded_query();
        let tools = query.tools(&filters()).unwrap();
        assert_eq!(tools.total_calls, 3);
        assert_eq!(tools.top_tools[0].tool_name, "apply_patch");
        let heatmap = query
            .heatmap(&HeatmapQuery {
                filters: filters(),
                metric: HeatmapMetric::Tokens,
                span: HeatmapSpan::Month,
            })
            .unwrap();
        assert!(heatmap.points.iter().any(|point| point.total_tokens == 120));
        assert_eq!(heatmap.points.len(), 30);
    }

    #[test]
    fn heatmap_spans_are_trailing_days_instead_of_calendar_boundaries() {
        assert_eq!(heatmap_day_count(HeatmapSpan::Week), 7);
        assert_eq!(heatmap_day_count(HeatmapSpan::Month), 30);
        assert_eq!(heatmap_day_count(HeatmapSpan::Year), 365);
    }

    #[test]
    fn model_name_sort_merges_providers_and_hides_internal_or_unknown_rows() {
        let query = crate::query::tests::seeded_query();
        query
            .store
            .with_writer(|transaction| {
                for (provider, model, pricing_id) in [
                    ("openai", "gpt-5.6-terra", "gpt-5.6-terra"),
                    ("custom", "gpt-5.6-sol", "gpt-5.6-sol"),
                    ("openai", "gpt-5.5", "gpt-5.5"),
                    ("openai", "codex-auto-review", ""),
                    ("openai", "unknown", ""),
                ] {
                    transaction.execute(
                        "INSERT INTO session_daily_usage (
                            session_id, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
                            workspace_id, model_provider, model_raw, pricing_model_id, archived,
                            active_ms, input_tokens, fresh_input_tokens, cached_input_tokens,
                            output_tokens, reasoning_tokens, total_tokens, priced_cost_microusd,
                            priced_event_count, unpriced_event_count, last_activity_at_ms
                         ) VALUES ('s1', '2026-07-10', 'UTC', 1783641600000, 1783728000000,
                                   'w1', ?1, ?2, ?3, 0, 0, 10, 10, 0, 0, 0, 10,
                                   0, 0, 1, 1783641601000)",
                        (provider, model, pricing_id),
                    )?;
                }
                Ok(())
            })
            .unwrap();

        let mut request = list_query();
        request.sort = "name".to_string();
        let models = query.models(&request).unwrap();
        assert_eq!(
            models
                .items
                .iter()
                .map(|row| row.model.as_str())
                .collect::<Vec<_>>(),
            vec!["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.5"]
        );
        assert_eq!(models.items[0].total_tokens, 130);
    }

    #[test]
    fn top_tools_is_bounded_to_ten_rows() {
        let query = crate::query::tests::seeded_query();
        query
            .store
            .with_writer(|transaction| {
                for index in 0..12 {
                    transaction.execute(
                        "INSERT INTO session_daily_tool (
                            session_id, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
                            workspace_id, archived, tool_name, category, operation_kind,
                            call_count, last_activity_at_ms
                         ) VALUES ('s1', '2026-07-10', 'UTC', 1783641600000, 1783728000000,
                                   'w1', 0, ?1, 'other', 'read_only', 1, 1783641601000)",
                        [format!("tool-{index:02}")],
                    )?;
                }
                Ok(())
            })
            .unwrap();

        assert_eq!(query.tools(&filters()).unwrap().top_tools.len(), 10);
    }
}
