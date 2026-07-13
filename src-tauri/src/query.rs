use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use chrono::{Datelike, Days, LocalResult, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::params_from_iter;
use rusqlite::types::Value as SqlValue;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::store::UsageStore;

mod analytics;
pub use analytics::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RangePreset {
    Today,
    Last24Hours,
    Last7Days,
    Last14Days,
    Last30Days,
    Last90Days,
    All,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeSelection {
    pub preset: RangePreset,
    pub start_ms: Option<i64>,
    pub end_ms: Option<i64>,
    #[serde(default)]
    pub live_end: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveFilter {
    #[default]
    All,
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageFilters {
    pub range: RangeSelection,
    pub workspace_id: Option<String>,
    pub model_provider: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub archived: ArchiveFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Granularity {
    Hour,
    Day,
    Week,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedRangeDto {
    pub start_ms: i64,
    pub end_ms: i64,
    pub start_local_date: String,
    pub end_local_date: String,
    pub calendar_days: u64,
    pub granularity: Granularity,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeroMetrics {
    pub real_total_tokens: i64,
    pub input_tokens: i64,
    pub fresh_input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub cache_hit_rate: Option<f64>,
    pub estimated_cost_microusd: Option<i64>,
    pub unpriced_event_count: i64,
    pub session_count: i64,
    pub active_ms: i64,
    pub active_days: i64,
    pub average_tokens_per_day: f64,
    pub average_cost_microusd_per_day: Option<f64>,
    pub average_sessions_per_day: f64,
    pub average_active_ms_per_day: f64,
    pub peak_day: Option<String>,
    pub peak_day_tokens: i64,
    pub longest_active_streak_days: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendPoint {
    pub key: String,
    pub timestamp_ms: i64,
    pub input_tokens: i64,
    pub fresh_input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_tokens: i64,
    pub total_tokens: i64,
    pub estimated_cost_microusd: Option<i64>,
    pub unpriced_event_count: i64,
    pub session_count: i64,
    pub active_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterOptions {
    pub workspaces: Vec<FilterOption>,
    pub providers: Vec<FilterOption>,
    pub models: Vec<FilterOption>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DataState {
    Complete,
    Partial,
    Empty,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub resolved_range: ResolvedRangeDto,
    pub hero: HeroMetrics,
    pub trend: Vec<TrendPoint>,
    pub filter_options: FilterOptions,
    pub data_state: DataState,
    pub generated_at_ms: i64,
}

#[derive(Clone)]
pub struct UsageQuery {
    store: Arc<UsageStore>,
    timezone: Tz,
}

impl UsageQuery {
    pub fn new(store: Arc<UsageStore>, timezone: Tz) -> Self {
        Self { store, timezone }
    }

    pub fn dashboard(&self, filters: &UsageFilters) -> AppResult<DashboardSnapshot> {
        let now = Utc::now().timestamp_millis();
        let range = self.resolve_range(&filters.range, now)?;
        let boundary_detail_requested = self.boundary_detail_requested(&filters.range, &range)?;
        let boundary_detail_available =
            boundary_detail_requested && self.event_detail_available(&range, now)?;
        let mut hero = if range.granularity == Granularity::Hour || boundary_detail_available {
            self.detail_hero(filters, &range)?
        } else {
            self.rollup_hero(filters, &range)?
        };
        let activity_stats = self.activity_stats(filters, &range)?;
        hero.peak_day = activity_stats.peak_day;
        hero.peak_day_tokens = activity_stats.peak_tokens;
        hero.longest_active_streak_days = activity_stats.longest_streak_days;
        let trend = if range.granularity == Granularity::Hour {
            self.hourly_trend(filters, &range)?
        } else if boundary_detail_available {
            self.detail_daily_trend(filters, &range)?
        } else {
            self.rollup_trend(filters, &range)?
        };
        let options = self.filter_options(filters, &range)?;
        let warning_sessions = self.warning_session_count(filters, &range)?;
        let state = if hero.session_count == 0 && hero.real_total_tokens == 0 {
            DataState::Empty
        } else if warning_sessions > 0 || (boundary_detail_requested && !boundary_detail_available)
        {
            DataState::Partial
        } else {
            DataState::Complete
        };
        Ok(DashboardSnapshot {
            resolved_range: range.dto(),
            hero,
            trend,
            filter_options: options,
            data_state: state,
            generated_at_ms: now,
        })
    }

    fn event_detail_available(&self, range: &ResolvedRange, now_ms: i64) -> AppResult<bool> {
        let today = date_for_ms(now_ms, self.timezone)?;
        let cutoff = local_midnight_utc_ms(subtract_days(today, 90)?, self.timezone)?;
        Ok(range.start_ms >= cutoff)
    }

    fn boundary_detail_requested(
        &self,
        selection: &RangeSelection,
        range: &ResolvedRange,
    ) -> AppResult<bool> {
        if selection.preset != RangePreset::Custom || range.granularity == Granularity::Hour {
            return Ok(false);
        }
        let start_boundary = local_midnight_utc_ms(range.start_date, self.timezone)?;
        let next_date = range
            .end_date
            .checked_add_days(Days::new(1))
            .ok_or_else(|| AppError::new("invalid_range", "日期范围超出支持范围"))?;
        let end_boundary = local_midnight_utc_ms(next_date, self.timezone)?;
        Ok(range.start_ms != start_boundary || range.end_ms != end_boundary)
    }

    fn resolve_range(&self, selection: &RangeSelection, now_ms: i64) -> AppResult<ResolvedRange> {
        let now = Utc
            .timestamp_millis_opt(now_ms)
            .single()
            .ok_or_else(|| AppError::new("invalid_timestamp", "当前时间超出支持范围"))?;
        let local_today = now.with_timezone(&self.timezone).date_naive();
        let today_start = local_midnight_utc_ms(local_today, self.timezone)?;
        let (start_ms, end_ms) = match selection.preset {
            RangePreset::Today => (today_start, now_ms),
            RangePreset::Last24Hours => (now_ms.saturating_sub(24 * 60 * 60 * 1000), now_ms),
            RangePreset::Last7Days => (
                local_midnight_utc_ms(subtract_days(local_today, 6)?, self.timezone)?,
                now_ms,
            ),
            RangePreset::Last14Days => (
                local_midnight_utc_ms(subtract_days(local_today, 13)?, self.timezone)?,
                now_ms,
            ),
            RangePreset::Last30Days => (
                local_midnight_utc_ms(subtract_days(local_today, 29)?, self.timezone)?,
                now_ms,
            ),
            RangePreset::Last90Days => (
                local_midnight_utc_ms(subtract_days(local_today, 89)?, self.timezone)?,
                now_ms,
            ),
            RangePreset::All => {
                let earliest = self.store.with_reader(|connection| {
                    connection
                        .query_row("SELECT MIN(started_at_ms) FROM sessions", [], |row| {
                            row.get::<_, Option<i64>>(0)
                        })
                        .map_err(AppError::from)
                })?;
                (earliest.unwrap_or(today_start), now_ms)
            }
            RangePreset::Custom => {
                let start = selection
                    .start_ms
                    .ok_or_else(|| AppError::new("invalid_range", "自定义范围必须提供开始时间"))?;
                let end = if selection.live_end {
                    now_ms
                } else {
                    selection.end_ms.ok_or_else(|| {
                        AppError::new("invalid_range", "固定自定义范围必须提供结束时间")
                    })?
                };
                (start, end)
            }
        };
        if start_ms >= end_ms {
            return Err(AppError::new("invalid_range", "开始时间必须早于结束时间"));
        }
        let start_date = date_for_ms(start_ms, self.timezone)?;
        let end_date = date_for_ms(end_ms.saturating_sub(1), self.timezone)?;
        let calendar_days = end_date
            .signed_duration_since(start_date)
            .num_days()
            .saturating_add(1) as u64;
        let duration = end_ms.saturating_sub(start_ms);
        let granularity = if duration <= 24 * 60 * 60 * 1000 {
            Granularity::Hour
        } else if calendar_days <= 180 {
            Granularity::Day
        } else {
            Granularity::Week
        };
        Ok(ResolvedRange {
            start_ms,
            end_ms,
            start_date,
            end_date,
            calendar_days,
            granularity,
        })
    }

    fn detail_hero(&self, filters: &UsageFilters, range: &ResolvedRange) -> AppResult<HeroMetrics> {
        let (condition, values) = detail_conditions(filters, range, "s", "e");
        let usage = self.store.with_reader(|connection| {
            let sql = format!(
                "SELECT COALESCE(SUM(e.input_tokens), 0),
                        COALESCE(SUM(e.fresh_input_tokens), 0),
                        COALESCE(SUM(e.cached_input_tokens), 0),
                        COALESCE(SUM(e.output_tokens), 0),
                        COALESCE(SUM(e.reasoning_tokens), 0),
                        SUM(e.estimated_cost_microusd),
                        COALESCE(SUM(CASE WHEN e.estimated_cost_microusd IS NOT NULL THEN 1 ELSE 0 END), 0),
                        COALESCE(SUM(CASE WHEN e.estimated_cost_microusd IS NULL
                                          AND NOT (LOWER(e.model_provider) = 'openai'
                                                   AND LOWER(e.model_raw) = 'codex-auto-review')
                                         THEN 1 ELSE 0 END), 0),
                        COUNT(DISTINCT e.session_id), COUNT(DISTINCT e.local_date)
                 FROM usage_events e JOIN sessions s ON s.id = e.session_id
                 WHERE {condition}"
            );
            connection
                .query_row(&sql, params_from_iter(values.iter()), |row| {
                    Ok(HeroRow {
                        input: row.get(0)?,
                        fresh: row.get(1)?,
                        cached: row.get(2)?,
                        output: row.get(3)?,
                        reasoning: row.get(4)?,
                        cost: row.get(5)?,
                        priced_events: row.get(6)?,
                        unpriced_events: row.get(7)?,
                        sessions: row.get(8)?,
                        active_days: row.get(9)?,
                        active_ms: 0,
                    })
                })
                .map_err(AppError::from)
        })?;
        let active_ms = self.detail_active_ms(filters, range)?;
        Ok(hero_from_row(
            HeroRow { active_ms, ..usage },
            range.calendar_days,
        ))
    }

    fn detail_active_ms(&self, filters: &UsageFilters, range: &ResolvedRange) -> AppResult<i64> {
        let mut clauses = vec![
            "a.ended_at_ms > ?".to_string(),
            "a.started_at_ms < ?".to_string(),
        ];
        let mut values = vec![
            SqlValue::Integer(range.start_ms),
            SqlValue::Integer(range.end_ms),
        ];
        push_filters(&mut clauses, &mut values, filters, "s", Some("a"));
        let sql = format!(
            "SELECT COALESCE(SUM(
                CASE WHEN a.ended_at_ms <= a.started_at_ms THEN a.active_ms
                ELSE CAST(a.active_ms AS REAL) *
                    (MIN(a.ended_at_ms, ?) - MAX(a.started_at_ms, ?)) /
                    (a.ended_at_ms - a.started_at_ms) END
             ), 0.0)
             FROM activity_segments a JOIN sessions s ON s.id = a.session_id
             WHERE {}",
            clauses.join(" AND ")
        );
        let mut all_values = vec![
            SqlValue::Integer(range.end_ms),
            SqlValue::Integer(range.start_ms),
        ];
        all_values.extend(values);
        self.store.with_reader(|connection| {
            connection
                .query_row(&sql, params_from_iter(all_values.iter()), |row| {
                    row.get::<_, f64>(0).map(|value| value.round() as i64)
                })
                .map_err(AppError::from)
        })
    }

    fn rollup_hero(&self, filters: &UsageFilters, range: &ResolvedRange) -> AppResult<HeroMetrics> {
        let (condition, values) = rollup_conditions(filters, range, "d");
        let row = self.store.with_reader(|connection| {
            let sql = format!(
                "SELECT COALESCE(SUM(d.input_tokens), 0),
                        COALESCE(SUM(d.fresh_input_tokens), 0),
                        COALESCE(SUM(d.cached_input_tokens), 0),
                        COALESCE(SUM(d.output_tokens), 0),
                        COALESCE(SUM(d.reasoning_tokens), 0),
                        COALESCE(SUM(d.priced_cost_microusd), 0),
                        COALESCE(SUM(d.priced_event_count), 0),
                        COALESCE(SUM(d.unpriced_event_count), 0),
                        COUNT(DISTINCT d.session_id), COUNT(DISTINCT d.local_date),
                        COALESCE(SUM(d.active_ms), 0)
                 FROM session_daily_usage d WHERE {condition}"
            );
            connection
                .query_row(&sql, params_from_iter(values.iter()), |row| {
                    Ok(HeroRow {
                        input: row.get(0)?,
                        fresh: row.get(1)?,
                        cached: row.get(2)?,
                        output: row.get(3)?,
                        reasoning: row.get(4)?,
                        cost: Some(row.get(5)?),
                        priced_events: row.get(6)?,
                        unpriced_events: row.get(7)?,
                        sessions: row.get(8)?,
                        active_days: row.get(9)?,
                        active_ms: row.get(10)?,
                    })
                })
                .map_err(AppError::from)
        })?;
        Ok(hero_from_row(row, range.calendar_days))
    }

    fn hourly_trend(
        &self,
        filters: &UsageFilters,
        range: &ResolvedRange,
    ) -> AppResult<Vec<TrendPoint>> {
        let (condition, values) = detail_conditions(filters, range, "s", "e");
        self.store.with_reader(|connection| {
            let sql = format!(
                "SELECT (e.occurred_at_ms / 3600000) * 3600000 AS bucket,
                        SUM(e.input_tokens), SUM(e.fresh_input_tokens),
                        SUM(e.cached_input_tokens), SUM(e.output_tokens),
                        SUM(e.reasoning_tokens), SUM(e.total_tokens),
                        SUM(e.estimated_cost_microusd),
                        SUM(CASE WHEN e.estimated_cost_microusd IS NOT NULL THEN 1 ELSE 0 END),
                        SUM(CASE WHEN e.estimated_cost_microusd IS NULL
                                  AND NOT (LOWER(e.model_provider) = 'openai'
                                           AND LOWER(e.model_raw) = 'codex-auto-review')
                                 THEN 1 ELSE 0 END),
                        COUNT(DISTINCT e.session_id)
                 FROM usage_events e JOIN sessions s ON s.id = e.session_id
                 WHERE {condition} GROUP BY bucket ORDER BY bucket"
            );
            let mut statement = connection.prepare(&sql)?;
            let rows = statement.query_map(params_from_iter(values.iter()), |row| {
                let timestamp: i64 = row.get(0)?;
                let priced: i64 = row.get(8)?;
                Ok(TrendPoint {
                    key: timestamp.to_string(),
                    timestamp_ms: timestamp,
                    input_tokens: row.get(1)?,
                    fresh_input_tokens: row.get(2)?,
                    cached_input_tokens: row.get(3)?,
                    output_tokens: row.get(4)?,
                    reasoning_tokens: row.get(5)?,
                    total_tokens: row.get(6)?,
                    estimated_cost_microusd: if priced > 0 { row.get(7)? } else { None },
                    unpriced_event_count: row.get(9)?,
                    session_count: row.get(10)?,
                    active_ms: 0,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
        })
    }

    fn detail_daily_trend(
        &self,
        filters: &UsageFilters,
        range: &ResolvedRange,
    ) -> AppResult<Vec<TrendPoint>> {
        let (condition, values) = detail_conditions(filters, range, "s", "e");
        let mut daily = self.store.with_reader(|connection| {
            let sql = format!(
                "SELECT e.local_date, MIN(e.day_start_utc_ms), SUM(e.input_tokens),
                        SUM(e.fresh_input_tokens), SUM(e.cached_input_tokens),
                        SUM(e.output_tokens), SUM(e.reasoning_tokens), SUM(e.total_tokens),
                        SUM(e.estimated_cost_microusd),
                        SUM(CASE WHEN e.estimated_cost_microusd IS NOT NULL THEN 1 ELSE 0 END),
                        SUM(CASE WHEN e.estimated_cost_microusd IS NULL
                                  AND NOT (LOWER(e.model_provider) = 'openai'
                                           AND LOWER(e.model_raw) = 'codex-auto-review')
                                 THEN 1 ELSE 0 END),
                        COUNT(DISTINCT e.session_id)
                 FROM usage_events e JOIN sessions s ON s.id = e.session_id
                 WHERE {condition} GROUP BY e.local_date ORDER BY e.local_date"
            );
            let mut statement = connection.prepare(&sql)?;
            statement
                .query_map(params_from_iter(values.iter()), |row| {
                    let priced: i64 = row.get(9)?;
                    Ok(TrendPoint {
                        key: row.get(0)?,
                        timestamp_ms: row.get(1)?,
                        input_tokens: row.get(2)?,
                        fresh_input_tokens: row.get(3)?,
                        cached_input_tokens: row.get(4)?,
                        output_tokens: row.get(5)?,
                        reasoning_tokens: row.get(6)?,
                        total_tokens: row.get(7)?,
                        estimated_cost_microusd: if priced > 0 { row.get(8)? } else { None },
                        unpriced_event_count: row.get(10)?,
                        session_count: row.get(11)?,
                        active_ms: 0,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(AppError::from)
        })?;
        let (rollup_condition, rollup_values) = rollup_conditions(filters, range, "d");
        let active_by_date: BTreeMap<String, i64> = self.store.with_reader(|connection| {
            let sql = format!(
                "SELECT d.local_date, SUM(d.active_ms) FROM session_daily_usage d
                 WHERE {rollup_condition} GROUP BY d.local_date"
            );
            let mut statement = connection.prepare(&sql)?;
            statement
                .query_map(params_from_iter(rollup_values.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })?
                .collect::<Result<BTreeMap<_, _>, _>>()
                .map_err(AppError::from)
        })?;
        for point in &mut daily {
            point.active_ms = active_by_date.get(&point.key).copied().unwrap_or_default();
        }
        if range.granularity == Granularity::Week {
            self.group_weekly_with_distinct_sessions(filters, range, daily)
        } else {
            Ok(daily)
        }
    }

    fn rollup_trend(
        &self,
        filters: &UsageFilters,
        range: &ResolvedRange,
    ) -> AppResult<Vec<TrendPoint>> {
        let (condition, values) = rollup_conditions(filters, range, "d");
        let daily = self.store.with_reader(|connection| {
            let sql = format!(
                "SELECT d.local_date, MIN(d.day_start_utc_ms), SUM(d.input_tokens),
                        SUM(d.fresh_input_tokens), SUM(d.cached_input_tokens),
                        SUM(d.output_tokens), SUM(d.reasoning_tokens), SUM(d.total_tokens),
                        SUM(d.priced_cost_microusd), SUM(d.priced_event_count),
                        SUM(d.unpriced_event_count), COUNT(DISTINCT d.session_id),
                        SUM(d.active_ms)
                 FROM session_daily_usage d WHERE {condition}
                 GROUP BY d.local_date ORDER BY d.local_date"
            );
            let mut statement = connection.prepare(&sql)?;
            let rows = statement.query_map(params_from_iter(values.iter()), |row| {
                let priced: i64 = row.get(9)?;
                Ok(TrendPoint {
                    key: row.get(0)?,
                    timestamp_ms: row.get(1)?,
                    input_tokens: row.get(2)?,
                    fresh_input_tokens: row.get(3)?,
                    cached_input_tokens: row.get(4)?,
                    output_tokens: row.get(5)?,
                    reasoning_tokens: row.get(6)?,
                    total_tokens: row.get(7)?,
                    estimated_cost_microusd: if priced > 0 { Some(row.get(8)?) } else { None },
                    unpriced_event_count: row.get(10)?,
                    session_count: row.get(11)?,
                    active_ms: row.get(12)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
        })?;
        if range.granularity == Granularity::Week {
            self.group_weekly_with_distinct_sessions(filters, range, daily)
        } else {
            Ok(daily)
        }
    }

    fn group_weekly_with_distinct_sessions(
        &self,
        filters: &UsageFilters,
        range: &ResolvedRange,
        daily: Vec<TrendPoint>,
    ) -> AppResult<Vec<TrendPoint>> {
        let (condition, values) = rollup_conditions(filters, range, "d");
        let distinct = self.store.with_reader(|connection| {
            let sql = format!(
                "SELECT d.local_date, d.session_id FROM session_daily_usage d
                 WHERE {condition} GROUP BY d.local_date, d.session_id"
            );
            let mut statement = connection.prepare(&sql)?;
            statement
                .query_map(params_from_iter(values.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(AppError::from)
        })?;
        let mut sessions_by_week: BTreeMap<String, HashSet<String>> = BTreeMap::new();
        for (date, session_id) in distinct {
            let Ok(date) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") else {
                continue;
            };
            let week = date.iso_week();
            sessions_by_week
                .entry(format!("{}-W{:02}", week.year(), week.week()))
                .or_default()
                .insert(session_id);
        }
        let mut weekly = group_weekly(daily, self.timezone);
        for point in &mut weekly {
            point.session_count = sessions_by_week
                .get(&point.key)
                .map_or(0, |sessions| sessions.len() as i64);
        }
        Ok(weekly)
    }

    fn filter_options(
        &self,
        filters: &UsageFilters,
        range: &ResolvedRange,
    ) -> AppResult<FilterOptions> {
        self.store.with_reader(|connection| {
            let workspaces = {
                let mut statement = connection.prepare(
                    "SELECT DISTINCT w.id, COALESCE(w.alias, w.display_name)
                     FROM session_daily_usage d JOIN workspaces w ON w.id = d.workspace_id
                     WHERE d.local_date BETWEEN ?1 AND ?2 AND w.ignored = 0
                       AND LOWER(TRIM(d.model_raw)) <> 'codex-auto-review'
                     ORDER BY 2 COLLATE NOCASE",
                )?;
                statement
                    .query_map(
                        [range.start_date_string(), range.end_date_string()],
                        |row| {
                            Ok(FilterOption {
                                value: row.get(0)?,
                                label: row.get(1)?,
                            })
                        },
                    )?
                    .collect::<Result<Vec<_>, _>>()?
            };
            let mut provider_clauses = vec![
                "d.local_date BETWEEN ? AND ?".to_string(),
                "d.workspace_id IN (SELECT id FROM workspaces WHERE ignored = 0)".to_string(),
                "TRIM(LOWER(d.model_provider)) NOT IN ('', 'unknown', '(unknown)')".to_string(),
                "LOWER(TRIM(d.model_raw)) <> 'codex-auto-review'".to_string(),
            ];
            let mut provider_values = vec![
                SqlValue::Text(range.start_date_string()),
                SqlValue::Text(range.end_date_string()),
            ];
            push_archive(
                &mut provider_clauses,
                &mut provider_values,
                filters.archived,
                "d",
            );
            if let Some(workspace) = filters.workspace_id.as_deref() {
                provider_clauses.push("d.workspace_id = ?".to_string());
                provider_values.push(SqlValue::Text(workspace.to_string()));
            }
            let providers = query_simple_options(
                connection,
                &format!(
                    "SELECT {provider} AS provider, {provider}
                     FROM session_daily_usage d WHERE {}
                     GROUP BY provider
                     ORDER BY CASE provider WHEN 'openai' THEN 0 ELSE 1 END",
                    provider_clauses.join(" AND "),
                    provider = canonical_provider_sql("d.model_provider"),
                ),
                &provider_values,
            )?;
            let mut model_clauses = provider_clauses;
            let mut model_values = provider_values;
            if let Some(provider) = filters.model_provider.as_deref() {
                model_clauses.push(format!(
                    "{} = ?",
                    canonical_provider_sql("d.model_provider")
                ));
                model_values.push(SqlValue::Text(provider.to_string()));
            }
            model_clauses
                .push("TRIM(LOWER(d.model_raw)) NOT IN ('', 'unknown', '(unknown)')".to_string());
            let models = query_simple_options(
                connection,
                &format!(
                    "SELECT d.model_raw, d.model_raw
                     FROM session_daily_usage d WHERE {}
                     GROUP BY d.model_raw
                     ORDER BY model_strength_key(d.model_raw) DESC,
                              d.model_raw COLLATE NOCASE ASC",
                    model_clauses.join(" AND ")
                ),
                &model_values,
            )?;
            Ok(FilterOptions {
                workspaces,
                providers,
                models,
            })
        })
    }

    fn warning_session_count(
        &self,
        filters: &UsageFilters,
        range: &ResolvedRange,
    ) -> AppResult<i64> {
        let mut clauses = vec![
            "s.ended_at_ms >= ?".to_string(),
            "s.started_at_ms < ?".to_string(),
            "s.integrity_status <> 'complete'".to_string(),
        ];
        let mut values = vec![
            SqlValue::Integer(range.start_ms),
            SqlValue::Integer(range.end_ms),
        ];
        push_filters(&mut clauses, &mut values, filters, "s", None);
        let sql = format!(
            "SELECT COUNT(*) FROM sessions s WHERE {}",
            clauses.join(" AND ")
        );
        self.store.with_reader(|connection| {
            connection
                .query_row(&sql, params_from_iter(values.iter()), |row| row.get(0))
                .map_err(AppError::from)
        })
    }

    fn activity_stats(
        &self,
        filters: &UsageFilters,
        range: &ResolvedRange,
    ) -> AppResult<ActivityStats> {
        let (condition, values) = rollup_conditions(filters, range, "d");
        let days = self.store.with_reader(|connection| {
            let sql = format!(
                "SELECT d.local_date, SUM(d.total_tokens)
                 FROM session_daily_usage d WHERE {condition}
                 GROUP BY d.local_date HAVING SUM(d.total_tokens) > 0 OR SUM(d.active_ms) > 0
                 ORDER BY d.local_date"
            );
            let mut statement = connection.prepare(&sql)?;
            statement
                .query_map(params_from_iter(values.iter()), |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(AppError::from)
        })?;
        let mut peak_day = None;
        let mut peak_tokens = 0_i64;
        let mut longest_streak_days = 0_i64;
        let mut current_streak = 0_i64;
        let mut previous = None;
        for (date, tokens) in days {
            let parsed = NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok();
            current_streak = match (previous, parsed) {
                (Some(previous), Some(current))
                    if current.signed_duration_since(previous).num_days() == 1 =>
                {
                    current_streak.saturating_add(1)
                }
                _ => 1,
            };
            previous = parsed;
            longest_streak_days = longest_streak_days.max(current_streak);
            if tokens > peak_tokens {
                peak_tokens = tokens;
                peak_day = Some(date);
            }
        }
        Ok(ActivityStats {
            peak_day,
            peak_tokens,
            longest_streak_days,
        })
    }
}

struct ActivityStats {
    peak_day: Option<String>,
    peak_tokens: i64,
    longest_streak_days: i64,
}

#[derive(Debug, Clone)]
struct ResolvedRange {
    start_ms: i64,
    end_ms: i64,
    start_date: NaiveDate,
    end_date: NaiveDate,
    calendar_days: u64,
    granularity: Granularity,
}

impl ResolvedRange {
    fn start_date_string(&self) -> String {
        self.start_date.format("%Y-%m-%d").to_string()
    }

    fn end_date_string(&self) -> String {
        self.end_date.format("%Y-%m-%d").to_string()
    }

    fn dto(&self) -> ResolvedRangeDto {
        ResolvedRangeDto {
            start_ms: self.start_ms,
            end_ms: self.end_ms,
            start_local_date: self.start_date_string(),
            end_local_date: self.end_date_string(),
            calendar_days: self.calendar_days,
            granularity: self.granularity,
        }
    }
}

#[derive(Debug)]
struct HeroRow {
    input: i64,
    fresh: i64,
    cached: i64,
    output: i64,
    reasoning: i64,
    cost: Option<i64>,
    priced_events: i64,
    unpriced_events: i64,
    sessions: i64,
    active_days: i64,
    active_ms: i64,
}

fn hero_from_row(row: HeroRow, calendar_days: u64) -> HeroMetrics {
    let days = calendar_days.max(1) as f64;
    let total = row.input.saturating_add(row.output);
    let cost = (row.priced_events > 0).then_some(row.cost.unwrap_or(0));
    HeroMetrics {
        real_total_tokens: total,
        input_tokens: row.input,
        fresh_input_tokens: row.fresh,
        cached_input_tokens: row.cached,
        output_tokens: row.output,
        reasoning_tokens: row.reasoning,
        cache_hit_rate: (row.input > 0).then_some(row.cached as f64 / row.input as f64),
        estimated_cost_microusd: cost,
        unpriced_event_count: row.unpriced_events,
        session_count: row.sessions,
        active_ms: row.active_ms,
        active_days: row.active_days,
        average_tokens_per_day: total as f64 / days,
        average_cost_microusd_per_day: cost.map(|value| value as f64 / days),
        average_sessions_per_day: row.sessions as f64 / days,
        average_active_ms_per_day: row.active_ms as f64 / days,
        peak_day: None,
        peak_day_tokens: 0,
        longest_active_streak_days: 0,
    }
}

fn detail_conditions(
    filters: &UsageFilters,
    range: &ResolvedRange,
    session_alias: &str,
    event_alias: &str,
) -> (String, Vec<SqlValue>) {
    let mut clauses = vec![
        format!("{event_alias}.occurred_at_ms >= ?"),
        format!("{event_alias}.occurred_at_ms < ?"),
    ];
    let mut values = vec![
        SqlValue::Integer(range.start_ms),
        SqlValue::Integer(range.end_ms),
    ];
    push_filters(
        &mut clauses,
        &mut values,
        filters,
        session_alias,
        Some(event_alias),
    );
    (clauses.join(" AND "), values)
}

fn rollup_conditions(
    filters: &UsageFilters,
    range: &ResolvedRange,
    alias: &str,
) -> (String, Vec<SqlValue>) {
    let mut clauses = vec![
        format!("{alias}.local_date >= ?"),
        format!("{alias}.local_date <= ?"),
        format!("{alias}.workspace_id IN (SELECT id FROM workspaces WHERE ignored = 0)"),
        format!("LOWER(TRIM({alias}.model_raw)) <> 'codex-auto-review'"),
    ];
    let mut values = vec![
        SqlValue::Text(range.start_date_string()),
        SqlValue::Text(range.end_date_string()),
    ];
    if let Some(workspace) = filters.workspace_id.as_deref() {
        clauses.push(format!("{alias}.workspace_id = ?"));
        values.push(SqlValue::Text(workspace.to_string()));
    }
    if let Some(provider) = filters.model_provider.as_deref() {
        clauses.push(format!(
            "{} = ?",
            canonical_provider_sql(&format!("{alias}.model_provider"))
        ));
        values.push(SqlValue::Text(provider.to_string()));
    }
    if let Some(model) = filters.model.as_deref() {
        clauses.push(format!("{alias}.model_raw = ?"));
        values.push(SqlValue::Text(model.to_string()));
    }
    push_archive(&mut clauses, &mut values, filters.archived, alias);
    (clauses.join(" AND "), values)
}

fn push_filters(
    clauses: &mut Vec<String>,
    values: &mut Vec<SqlValue>,
    filters: &UsageFilters,
    session_alias: &str,
    event_alias: Option<&str>,
) {
    clauses.push(format!(
        "{session_alias}.workspace_id IN (SELECT id FROM workspaces WHERE ignored = 0)"
    ));
    if let Some(alias) = event_alias {
        clauses.push(format!(
            "LOWER(TRIM({alias}.model_raw)) <> 'codex-auto-review'"
        ));
    } else {
        clauses.push(format!(
            "LOWER(TRIM(COALESCE(NULLIF({session_alias}.primary_model_raw, ''), \
             {session_alias}.latest_model_raw))) <> 'codex-auto-review'"
        ));
    }
    if let Some(workspace) = filters.workspace_id.as_deref() {
        clauses.push(format!("{session_alias}.workspace_id = ?"));
        values.push(SqlValue::Text(workspace.to_string()));
    }
    if let Some(provider) = filters.model_provider.as_deref() {
        let alias = event_alias.unwrap_or(session_alias);
        clauses.push(format!(
            "{} = ?",
            canonical_provider_sql(&format!("{alias}.model_provider"))
        ));
        values.push(SqlValue::Text(provider.to_string()));
    }
    if let Some(model) = filters.model.as_deref() {
        let alias = event_alias.unwrap_or(session_alias);
        let column = if event_alias.is_some() {
            "model_raw"
        } else {
            "latest_model_raw"
        };
        clauses.push(format!("{alias}.{column} = ?"));
        values.push(SqlValue::Text(model.to_string()));
    }
    push_archive(clauses, values, filters.archived, session_alias);
}

fn push_archive(
    clauses: &mut Vec<String>,
    values: &mut Vec<SqlValue>,
    archived: ArchiveFilter,
    alias: &str,
) {
    match archived {
        ArchiveFilter::All => {}
        ArchiveFilter::Active => {
            clauses.push(format!("{alias}.archived = ?"));
            values.push(SqlValue::Integer(0));
        }
        ArchiveFilter::Archived => {
            clauses.push(format!("{alias}.archived = ?"));
            values.push(SqlValue::Integer(1));
        }
    }
}

fn query_simple_options(
    connection: &rusqlite::Connection,
    sql: &str,
    values: &[SqlValue],
) -> AppResult<Vec<FilterOption>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params_from_iter(values.iter()), |row| {
        Ok(FilterOption {
            value: row.get(0)?,
            label: row.get(1)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
}

fn canonical_provider_sql(column: &str) -> String {
    format!("CASE WHEN LOWER(TRIM({column})) = 'openai' THEN 'openai' ELSE 'custom' END")
}

fn group_weekly(points: Vec<TrendPoint>, timezone: Tz) -> Vec<TrendPoint> {
    let mut weeks: BTreeMap<(i32, u32), TrendPoint> = BTreeMap::new();
    for point in points {
        let Some(utc) = Utc.timestamp_millis_opt(point.timestamp_ms).single() else {
            continue;
        };
        let week = utc.with_timezone(&timezone).iso_week();
        let entry = weeks
            .entry((week.year(), week.week()))
            .or_insert_with(|| TrendPoint {
                key: format!("{}-W{:02}", week.year(), week.week()),
                timestamp_ms: point.timestamp_ms,
                input_tokens: 0,
                fresh_input_tokens: 0,
                cached_input_tokens: 0,
                output_tokens: 0,
                reasoning_tokens: 0,
                total_tokens: 0,
                estimated_cost_microusd: None,
                unpriced_event_count: 0,
                session_count: 0,
                active_ms: 0,
            });
        entry.input_tokens += point.input_tokens;
        entry.fresh_input_tokens += point.fresh_input_tokens;
        entry.cached_input_tokens += point.cached_input_tokens;
        entry.output_tokens += point.output_tokens;
        entry.reasoning_tokens += point.reasoning_tokens;
        entry.total_tokens += point.total_tokens;
        entry.estimated_cost_microusd =
            match (entry.estimated_cost_microusd, point.estimated_cost_microusd) {
                (Some(left), Some(right)) => Some(left + right),
                (None, Some(value)) | (Some(value), None) => Some(value),
                (None, None) => None,
            };
        entry.unpriced_event_count += point.unpriced_event_count;
        entry.session_count += point.session_count;
        entry.active_ms += point.active_ms;
    }
    weeks.into_values().collect()
}

fn subtract_days(date: NaiveDate, days: u64) -> AppResult<NaiveDate> {
    date.checked_sub_days(Days::new(days))
        .ok_or_else(|| AppError::new("invalid_range", "日期范围超出支持范围"))
}

fn date_for_ms(timestamp_ms: i64, timezone: Tz) -> AppResult<NaiveDate> {
    Utc.timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|value| value.with_timezone(&timezone).date_naive())
        .ok_or_else(|| AppError::new("invalid_timestamp", "时间戳超出支持范围"))
}

fn local_midnight_utc_ms(date: NaiveDate, timezone: Tz) -> AppResult<i64> {
    for minute in 0..=180_u32 {
        let time = NaiveTime::from_num_seconds_from_midnight_opt(minute * 60, 0)
            .ok_or_else(|| AppError::new("invalid_time", "本地时间无效"))?;
        match timezone.from_local_datetime(&date.and_time(time)) {
            LocalResult::Single(value) => return Ok(value.with_timezone(&Utc).timestamp_millis()),
            LocalResult::Ambiguous(first, second) => {
                return Ok(first.min(second).with_timezone(&Utc).timestamp_millis());
            }
            LocalResult::None => {}
        }
    }
    Err(AppError::new("timezone_gap", "无法确定本地日期边界"))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn seeded_query() -> UsageQuery {
        let store = Arc::new(UsageStore::open_in_memory().unwrap());
        store
            .with_writer(|transaction| {
                transaction.execute(
                    "INSERT INTO workspaces (id, normalized_path, display_name, ignored, created_at_ms, updated_at_ms)
                     VALUES ('w1', 'C:/repo', 'repo', 0, 0, 0)",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO source_files (
                        source_key, relative_path, source_kind, parser_version, status
                     ) VALUES ('jsonl:s1', 'sessions/s1.jsonl', 'session', 1, 'ready')",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO sessions (
                        id, source_file_id, workspace_id, synthetic_title, started_at_ms, ended_at_ms,
                        active_ms, active_method, active_is_estimate, model_provider,
                        latest_model_raw, primary_model_raw, input_tokens, fresh_input_tokens,
                        cached_input_tokens, output_tokens, reasoning_tokens, total_tokens,
                        estimated_cost_microusd, archived, integrity_status, parser_version, updated_at_ms
                     ) VALUES ('s1', (SELECT id FROM source_files WHERE source_key = 'jsonl:s1'),
                               'w1', 'Session 1', 1783641601000, 1783641602000, 500, 'lifecycle', 0,
                               'openai', 'gpt-5.6-sol', 'gpt-5.6-sol', 100, 60, 40,
                               20, 5, 120, 500, 0, 'complete', 1, 0)",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO session_daily_usage (
                        session_id, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
                        workspace_id, model_provider, model_raw, pricing_model_id, archived,
                        active_ms, input_tokens, fresh_input_tokens, cached_input_tokens,
                        output_tokens, reasoning_tokens, total_tokens, priced_cost_microusd,
                        priced_event_count, unpriced_event_count, last_activity_at_ms
                     ) VALUES ('s1', '2026-07-10', 'UTC', 1783641600000, 1783728000000,
                               'w1', 'openai', 'gpt-5.6-sol', 'gpt-5.6-sol', 0,
                               500, 100, 60, 40, 20, 5, 120, 500, 1, 0, 1783641601000)",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO usage_events (
                        event_key, source_file_id, session_id, byte_offset, event_ordinal,
                        occurred_at_ms, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
                        model_provider, model_raw, pricing_model_id, input_tokens,
                        fresh_input_tokens, cached_input_tokens, output_tokens, reasoning_tokens,
                        total_tokens, estimated_cost_microusd, pricing_revision, integrity_status
                     ) VALUES (
                        's1:1', (SELECT id FROM source_files WHERE source_key = 'jsonl:s1'),
                        's1', 1, 1, 1783641601000, '2026-07-10', 'UTC',
                        1783641600000, 1783728000000, 'openai', 'gpt-5.6-sol',
                        'gpt-5.6-sol', 100, 60, 40, 20, 5, 120, 500, 1, 'complete'
                     )",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO activity_segments (
                        session_id, segment_index, started_at_ms, ended_at_ms, active_ms,
                        method, is_estimate, local_date, timezone_id, model_provider,
                        model_raw, pricing_model_id
                     ) VALUES ('s1', 0, 1783641601000, 1783641601500, 500,
                               'lifecycle', 0, '2026-07-10', 'UTC', 'openai',
                               'gpt-5.6-sol', 'gpt-5.6-sol')",
                    [],
                )?;
                transaction.execute(
                    "INSERT INTO session_daily_tool (
                        session_id, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
                        workspace_id, archived, tool_name, category, operation_kind,
                        call_count, last_activity_at_ms
                     ) VALUES ('s1', '2026-07-10', 'UTC', 1783641600000, 1783728000000,
                               'w1', 0, 'apply_patch', 'edit', 'mutating', 3, 1783641601000)",
                    [],
                )?;
                Ok(())
            })
            .unwrap();
        UsageQuery::new(store, chrono_tz::UTC)
    }

    #[test]
    fn dashboard_uses_server_rollups_and_returns_cascading_options() {
        let query = seeded_query();
        let snapshot = query
            .dashboard(&UsageFilters {
                range: RangeSelection {
                    preset: RangePreset::Custom,
                    start_ms: Some(1_783_641_600_000),
                    end_ms: Some(1_783_814_400_000),
                    live_end: false,
                },
                workspace_id: None,
                model_provider: None,
                model: None,
                archived: ArchiveFilter::All,
            })
            .unwrap();
        assert_eq!(snapshot.hero.real_total_tokens, 120);
        assert_eq!(snapshot.hero.fresh_input_tokens, 60);
        assert_eq!(snapshot.hero.cache_hit_rate, Some(0.4));
        assert_eq!(snapshot.trend.len(), 1);
        assert_eq!(snapshot.filter_options.workspaces[0].label, "repo");
        assert_eq!(snapshot.filter_options.providers[0].value, "openai");
        assert_eq!(snapshot.filter_options.models[0].value, "gpt-5.6-sol");
    }

    #[test]
    fn dashboard_groups_non_openai_providers_and_excludes_auto_review() {
        let query = seeded_query();
        query
            .store
            .with_writer(|transaction| {
                for (provider, model, tokens) in [
                    ("codex_local_access", "gpt-5.5", 10_i64),
                    ("custom", "gpt-5.6-terra", 10_i64),
                    ("openai", "codex-auto-review", 1_000_i64),
                ] {
                    transaction.execute(
                        "INSERT INTO session_daily_usage (
                            session_id, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
                            workspace_id, model_provider, model_raw, pricing_model_id, archived,
                            active_ms, input_tokens, fresh_input_tokens, cached_input_tokens,
                            output_tokens, reasoning_tokens, total_tokens, priced_cost_microusd,
                            priced_event_count, unpriced_event_count, last_activity_at_ms
                         ) VALUES ('s1', '2026-07-10', 'UTC', 1783641600000, 1783728000000,
                                   'w1', ?1, ?2, ?2, 0, 0, ?3, ?3, 0, 0, 0, ?3,
                                   0, 0, 1, 1783641602000)",
                        (provider, model, tokens),
                    )?;
                }
                Ok(())
            })
            .unwrap();

        let mut filters = UsageFilters {
            range: RangeSelection {
                preset: RangePreset::Custom,
                start_ms: Some(1_783_641_600_000),
                end_ms: Some(1_783_814_400_000),
                live_end: false,
            },
            workspace_id: None,
            model_provider: None,
            model: None,
            archived: ArchiveFilter::All,
        };
        let snapshot = query.dashboard(&filters).unwrap();
        assert_eq!(snapshot.hero.real_total_tokens, 140);
        assert_eq!(
            snapshot
                .filter_options
                .providers
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>(),
            vec!["openai", "custom"]
        );
        assert!(
            snapshot
                .filter_options
                .models
                .iter()
                .all(|option| option.value != "codex-auto-review")
        );

        filters.model_provider = Some("custom".to_string());
        let custom = query.dashboard(&filters).unwrap();
        assert_eq!(custom.hero.real_total_tokens, 20);
        assert_eq!(
            custom
                .filter_options
                .models
                .iter()
                .map(|option| option.value.as_str())
                .collect::<Vec<_>>(),
            vec!["gpt-5.6-terra", "gpt-5.5"]
        );
    }

    #[test]
    fn range_resolution_selects_hour_day_and_week_granularity() {
        let query = seeded_query();
        let now = 2_000_000_000_000_i64;
        let hour = query
            .resolve_range(
                &RangeSelection {
                    preset: RangePreset::Last24Hours,
                    start_ms: None,
                    end_ms: None,
                    live_end: false,
                },
                now,
            )
            .unwrap();
        assert_eq!(hour.granularity, Granularity::Hour);
        let day = query
            .resolve_range(
                &RangeSelection {
                    preset: RangePreset::Last30Days,
                    start_ms: None,
                    end_ms: None,
                    live_end: false,
                },
                now,
            )
            .unwrap();
        assert_eq!(day.granularity, Granularity::Day);
        let week = query
            .resolve_range(
                &RangeSelection {
                    preset: RangePreset::Custom,
                    start_ms: Some(now - 365 * 24 * 60 * 60 * 1000),
                    end_ms: Some(now),
                    live_end: false,
                },
                now,
            )
            .unwrap();
        assert_eq!(week.granularity, Granularity::Week);
    }

    #[test]
    fn custom_range_uses_event_boundaries_instead_of_whole_day_rollups() {
        let query = seeded_query();
        let snapshot = query
            .dashboard(&UsageFilters {
                range: RangeSelection {
                    preset: RangePreset::Custom,
                    start_ms: Some(1_783_641_601_500),
                    end_ms: Some(1_783_641_602_500),
                    live_end: false,
                },
                workspace_id: None,
                model_provider: None,
                model: None,
                archived: ArchiveFilter::All,
            })
            .unwrap();
        assert_eq!(snapshot.hero.real_total_tokens, 0);
        assert!(snapshot.trend.is_empty());
    }
}
