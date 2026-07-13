use std::str::FromStr;

use chrono::Utc;
use chrono_tz::Tz;
use rusqlite::{OptionalExtension, params};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::UsageStore;
use crate::error::{AppError, AppResult};
use crate::pricing::{
    BUILTIN_PRICING_REVISION, ModelPrice, OFFICIAL_PRICING_SOURCE, PriceQuote, PriceableTokens,
    PricingCatalog, builtin_model_prices, canonical_model_provider,
};
use crate::pricing_update::TrustedPriceRow;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPriceRecord {
    pub provider: String,
    pub pricing_id: String,
    pub display_name: String,
    pub input_per_million_usd: String,
    pub output_per_million_usd: String,
    pub cache_read_per_million_usd: String,
    pub cache_write_per_million_usd: Option<String>,
    pub is_builtin: bool,
    pub is_overridden: bool,
    pub is_deleted: bool,
    pub revision: i64,
    pub source_url: Option<String>,
    pub source_updated_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPriceInput {
    pub provider: String,
    pub pricing_id: String,
    pub display_name: String,
    pub input_per_million_usd: String,
    pub output_per_million_usd: String,
    pub cache_read_per_million_usd: String,
    pub cache_write_per_million_usd: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepriceResult {
    pub events_repriced: u64,
    pub model_segments_repriced: u64,
    pub daily_rows_repriced: u64,
}

#[derive(Debug)]
struct DailyUsageForReprice {
    session_id: String,
    local_date: String,
    timezone_id: String,
    day_start_utc_ms: i64,
    day_end_utc_ms: i64,
    workspace_id: String,
    model_provider: String,
    model_raw: String,
    archived: i64,
    active_ms: i64,
    input_tokens: i64,
    fresh_input_tokens: i64,
    cached_input_tokens: i64,
    output_tokens: i64,
    reasoning_tokens: i64,
    total_tokens: i64,
    event_count: i64,
    last_activity_at_ms: i64,
}

impl UsageStore {
    pub fn apply_trusted_prices(
        &self,
        rows: &[TrustedPriceRow],
        fetched_at_ms: i64,
    ) -> AppResult<()> {
        for row in rows {
            canonical_price(&row.input_per_million_usd)?;
            canonical_price(&row.output_per_million_usd)?;
            canonical_price(&row.cache_read_per_million_usd)?;
            if let Some(value) = row.cache_write_per_million_usd.as_deref() {
                canonical_price(value)?;
            }
        }
        self.with_writer(|transaction| {
            for row in rows {
                transaction.execute(
                    "INSERT INTO model_prices (
                        pricing_id, provider, display_name,
                        input_per_million_usd, output_per_million_usd,
                        cache_read_per_million_usd, cache_write_per_million_usd,
                        default_input_per_million_usd, default_output_per_million_usd,
                        default_cache_read_per_million_usd, default_cache_write_per_million_usd,
                        is_builtin, is_overridden, is_deleted, revision,
                        source_url, source_updated_at_ms, created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?4, ?5, ?6, ?7,
                               1, 0, 0, 1, ?8, ?9, ?9, ?9)
                     ON CONFLICT(provider, pricing_id) DO UPDATE SET
                        display_name = excluded.display_name,
                        default_input_per_million_usd = excluded.default_input_per_million_usd,
                        default_output_per_million_usd = excluded.default_output_per_million_usd,
                        default_cache_read_per_million_usd = excluded.default_cache_read_per_million_usd,
                        default_cache_write_per_million_usd = excluded.default_cache_write_per_million_usd,
                        input_per_million_usd = CASE WHEN model_prices.is_overridden = 0
                            THEN excluded.input_per_million_usd ELSE model_prices.input_per_million_usd END,
                        output_per_million_usd = CASE WHEN model_prices.is_overridden = 0
                            THEN excluded.output_per_million_usd ELSE model_prices.output_per_million_usd END,
                        cache_read_per_million_usd = CASE WHEN model_prices.is_overridden = 0
                            THEN excluded.cache_read_per_million_usd ELSE model_prices.cache_read_per_million_usd END,
                        cache_write_per_million_usd = CASE WHEN model_prices.is_overridden = 0
                            THEN excluded.cache_write_per_million_usd ELSE model_prices.cache_write_per_million_usd END,
                        is_builtin = 1, revision = model_prices.revision + 1,
                        source_url = excluded.source_url,
                        source_updated_at_ms = excluded.source_updated_at_ms,
                        updated_at_ms = excluded.updated_at_ms",
                    params![
                        row.pricing_id,
                        row.provider,
                        row.display_name,
                        row.input_per_million_usd,
                        row.output_per_million_usd,
                        row.cache_read_per_million_usd,
                        row.cache_write_per_million_usd,
                        OFFICIAL_PRICING_SOURCE,
                        fetched_at_ms,
                    ],
                )?;
            }
            Ok(())
        })
    }

    pub fn seed_builtin_prices(&self) -> AppResult<()> {
        let prices = builtin_model_prices();
        self.with_writer(|transaction| {
            for price in prices {
                let cache_write = price.cache_write_per_million_usd.map(|value| value.to_string());
                transaction.execute(
                    "INSERT INTO model_prices (
                        pricing_id, provider, display_name,
                        input_per_million_usd, output_per_million_usd,
                        cache_read_per_million_usd, cache_write_per_million_usd,
                        default_input_per_million_usd, default_output_per_million_usd,
                        default_cache_read_per_million_usd, default_cache_write_per_million_usd,
                        is_builtin, is_overridden, is_deleted, revision,
                        source_url, source_updated_at_ms, created_at_ms, updated_at_ms
                     ) VALUES (
                        ?1, ?2, ?1, ?3, ?4, ?5, ?6, ?3, ?4, ?5, ?6,
                        1, 0, 0, ?7, ?8, ?9, ?10, ?10
                     )
                     ON CONFLICT(provider, pricing_id) DO UPDATE SET
                        default_input_per_million_usd = excluded.default_input_per_million_usd,
                        default_output_per_million_usd = excluded.default_output_per_million_usd,
                        default_cache_read_per_million_usd = excluded.default_cache_read_per_million_usd,
                        default_cache_write_per_million_usd = excluded.default_cache_write_per_million_usd,
                        input_per_million_usd = CASE WHEN model_prices.is_overridden = 0
                            THEN excluded.input_per_million_usd ELSE model_prices.input_per_million_usd END,
                        output_per_million_usd = CASE WHEN model_prices.is_overridden = 0
                            THEN excluded.output_per_million_usd ELSE model_prices.output_per_million_usd END,
                        cache_read_per_million_usd = CASE WHEN model_prices.is_overridden = 0
                            THEN excluded.cache_read_per_million_usd ELSE model_prices.cache_read_per_million_usd END,
                        cache_write_per_million_usd = CASE WHEN model_prices.is_overridden = 0
                            THEN excluded.cache_write_per_million_usd ELSE model_prices.cache_write_per_million_usd END,
                        is_builtin = 1, revision = MAX(model_prices.revision, excluded.revision),
                        source_url = excluded.source_url,
                        source_updated_at_ms = excluded.source_updated_at_ms,
                        updated_at_ms = excluded.updated_at_ms",
                    params![
                        price.pricing_id,
                        price.provider,
                        price.input_per_million_usd.to_string(),
                        price.output_per_million_usd.to_string(),
                        price.cache_read_per_million_usd.to_string(),
                        cache_write,
                        BUILTIN_PRICING_REVISION,
                        OFFICIAL_PRICING_SOURCE,
                        official_snapshot_ms(),
                        Utc::now().timestamp_millis(),
                    ],
                )?;
            }
            Ok(())
        })
    }

    pub fn model_prices(&self, include_deleted: bool) -> AppResult<Vec<ModelPriceRecord>> {
        self.with_reader(|connection| {
            let sql = if include_deleted {
                "SELECT provider, pricing_id, display_name, input_per_million_usd,
                        output_per_million_usd, cache_read_per_million_usd,
                        cache_write_per_million_usd, is_builtin, is_overridden,
                        is_deleted, revision, source_url, source_updated_at_ms
                 FROM model_prices
                 ORDER BY model_strength_key(pricing_id) DESC,
                          pricing_id COLLATE NOCASE ASC, provider COLLATE NOCASE ASC"
            } else {
                "SELECT provider, pricing_id, display_name, input_per_million_usd,
                        output_per_million_usd, cache_read_per_million_usd,
                        cache_write_per_million_usd, is_builtin, is_overridden,
                        is_deleted, revision, source_url, source_updated_at_ms
                 FROM model_prices WHERE is_deleted = 0
                 ORDER BY model_strength_key(pricing_id) DESC,
                          pricing_id COLLATE NOCASE ASC, provider COLLATE NOCASE ASC"
            };
            let mut statement = connection.prepare(sql)?;
            let rows = statement.query_map([], |row| {
                Ok(ModelPriceRecord {
                    provider: row.get(0)?,
                    pricing_id: row.get(1)?,
                    display_name: row.get(2)?,
                    input_per_million_usd: row.get(3)?,
                    output_per_million_usd: row.get(4)?,
                    cache_read_per_million_usd: row.get(5)?,
                    cache_write_per_million_usd: row.get(6)?,
                    is_builtin: row.get::<_, i64>(7)? != 0,
                    is_overridden: row.get::<_, i64>(8)? != 0,
                    is_deleted: row.get::<_, i64>(9)? != 0,
                    revision: row.get(10)?,
                    source_url: row.get(11)?,
                    source_updated_at_ms: row.get(12)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
        })
    }

    pub fn pricing_catalog(&self) -> AppResult<PricingCatalog> {
        let records = self.model_prices(false)?;
        let prices = records
            .into_iter()
            .map(|record| {
                Ok(ModelPrice {
                    provider: record.provider,
                    pricing_id: record.pricing_id,
                    input_per_million_usd: parse_price(&record.input_per_million_usd)?,
                    output_per_million_usd: parse_price(&record.output_per_million_usd)?,
                    cache_read_per_million_usd: parse_price(&record.cache_read_per_million_usd)?,
                    cache_write_per_million_usd: record
                        .cache_write_per_million_usd
                        .as_deref()
                        .map(parse_price)
                        .transpose()?,
                    revision: record.revision,
                })
            })
            .collect::<AppResult<Vec<_>>>()?;
        Ok(PricingCatalog::new(prices))
    }

    pub fn save_model_price(&self, input: &ModelPriceInput) -> AppResult<()> {
        validate_price_input(input)?;
        let provider = match canonical_model_provider(&input.provider).as_str() {
            "openai" => "openai".to_string(),
            _ => "custom".to_string(),
        };
        let pricing_id = input.pricing_id.trim().to_ascii_lowercase();
        let now = Utc::now().timestamp_millis();
        self.with_writer(|transaction| {
            let existing_builtin: Option<i64> = transaction
                .query_row(
                    "SELECT is_builtin FROM model_prices
                     WHERE provider = ?1 AND pricing_id = ?2",
                    params![provider, pricing_id],
                    |row| row.get(0),
                )
                .optional()?;
            if existing_builtin.is_some() {
                transaction.execute(
                    "UPDATE model_prices SET
                        display_name = ?3, input_per_million_usd = ?4,
                        output_per_million_usd = ?5, cache_read_per_million_usd = ?6,
                        cache_write_per_million_usd = ?7,
                        is_overridden = CASE WHEN is_builtin = 1 THEN 1 ELSE is_overridden END,
                        is_deleted = 0, revision = revision + 1, updated_at_ms = ?8
                     WHERE provider = ?1 AND pricing_id = ?2",
                    params![
                        provider,
                        pricing_id,
                        input.display_name.trim(),
                        canonical_price(&input.input_per_million_usd)?,
                        canonical_price(&input.output_per_million_usd)?,
                        canonical_price(&input.cache_read_per_million_usd)?,
                        canonical_optional_price(input.cache_write_per_million_usd.as_deref())?,
                        now,
                    ],
                )?;
            } else {
                let input_price = canonical_price(&input.input_per_million_usd)?;
                let output_price = canonical_price(&input.output_per_million_usd)?;
                let cache_read = canonical_price(&input.cache_read_per_million_usd)?;
                let cache_write =
                    canonical_optional_price(input.cache_write_per_million_usd.as_deref())?;
                transaction.execute(
                    "INSERT INTO model_prices (
                        pricing_id, provider, display_name,
                        input_per_million_usd, output_per_million_usd,
                        cache_read_per_million_usd, cache_write_per_million_usd,
                        default_input_per_million_usd, default_output_per_million_usd,
                        default_cache_read_per_million_usd, default_cache_write_per_million_usd,
                        is_builtin, is_overridden, is_deleted, revision,
                        created_at_ms, updated_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?4, ?5, ?6, ?7,
                               0, 0, 0, 1, ?8, ?8)",
                    params![
                        pricing_id,
                        provider,
                        input.display_name.trim(),
                        input_price,
                        output_price,
                        cache_read,
                        cache_write,
                        now,
                    ],
                )?;
            }
            Ok(())
        })
    }

    pub fn delete_model_price(&self, provider: &str, pricing_id: &str) -> AppResult<()> {
        self.with_writer(|transaction| {
            let builtin: Option<i64> = transaction
                .query_row(
                    "SELECT is_builtin FROM model_prices WHERE provider = ?1 AND pricing_id = ?2",
                    params![
                        provider.to_ascii_lowercase(),
                        pricing_id.to_ascii_lowercase()
                    ],
                    |row| row.get(0),
                )
                .optional()?;
            if builtin == Some(1) {
                transaction.execute(
                    "UPDATE model_prices SET is_deleted = 1, updated_at_ms = ?3
                     WHERE provider = ?1 AND pricing_id = ?2",
                    params![
                        provider.to_ascii_lowercase(),
                        pricing_id.to_ascii_lowercase(),
                        Utc::now().timestamp_millis()
                    ],
                )?;
            } else {
                transaction.execute(
                    "DELETE FROM model_prices WHERE provider = ?1 AND pricing_id = ?2",
                    params![
                        provider.to_ascii_lowercase(),
                        pricing_id.to_ascii_lowercase()
                    ],
                )?;
            }
            Ok(())
        })
    }

    pub fn restore_builtin_price(&self, provider: &str, pricing_id: &str) -> AppResult<()> {
        self.with_writer(|transaction| {
            let changed = transaction.execute(
                "UPDATE model_prices SET
                    input_per_million_usd = default_input_per_million_usd,
                    output_per_million_usd = default_output_per_million_usd,
                    cache_read_per_million_usd = default_cache_read_per_million_usd,
                    cache_write_per_million_usd = default_cache_write_per_million_usd,
                    is_overridden = 0, is_deleted = 0, revision = revision + 1,
                    updated_at_ms = ?3
                 WHERE provider = ?1 AND pricing_id = ?2 AND is_builtin = 1",
                params![
                    provider.to_ascii_lowercase(),
                    pricing_id.to_ascii_lowercase(),
                    Utc::now().timestamp_millis()
                ],
            )?;
            if changed == 0 {
                return Err(AppError::new("price_not_builtin", "未找到可恢复的内置价格"));
            }
            Ok(())
        })
    }

    pub fn reprice_all(&self, timezone: Tz) -> AppResult<RepriceResult> {
        let catalog = self.pricing_catalog()?;
        let result = self.with_writer(|transaction| {
            let events = {
                let mut statement = transaction.prepare(
                    "SELECT id, model_provider, model_raw, fresh_input_tokens,
                            cached_input_tokens, output_tokens FROM usage_events",
                )?;
                statement
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            for event in &events {
                let quote = catalog.quote(
                    &event.1,
                    &event.2,
                    PriceableTokens {
                        fresh_input: nonnegative(event.3),
                        cached_input: nonnegative(event.4),
                        output: nonnegative(event.5),
                        cache_write: 0,
                    },
                );
                let (pricing_id, revision, cost, _) = quote_parts(quote);
                transaction.execute(
                    "UPDATE usage_events SET pricing_model_id = NULLIF(?2, ''),
                        pricing_revision = ?3, estimated_cost_microusd = ?4
                     WHERE id = ?1",
                    params![event.0, pricing_id, revision, cost],
                )?;
            }

            let segments = {
                let mut statement = transaction.prepare(
                    "SELECT id, model_provider, model_raw, input_tokens,
                            cached_input_tokens, output_tokens
                     FROM session_model_segments",
                )?;
                statement
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            for segment in &segments {
                let quote = catalog.quote(
                    &segment.1,
                    &segment.2,
                    PriceableTokens {
                        fresh_input: nonnegative(segment.3.saturating_sub(segment.4)),
                        cached_input: nonnegative(segment.4),
                        output: nonnegative(segment.5),
                        cache_write: 0,
                    },
                );
                let (pricing_id, _, cost, excluded) = quote_parts(quote);
                transaction.execute(
                    "UPDATE session_model_segments SET pricing_model_id = NULLIF(?2, ''),
                        estimated_cost_microusd = ?3,
                        unpriced_event_count = CASE WHEN ?3 IS NULL AND ?4 = 0 THEN 1 ELSE 0 END
                     WHERE id = ?1",
                    params![segment.0, pricing_id, cost, i64::from(excluded)],
                )?;
            }

            let daily = {
                let mut statement = transaction.prepare(
                    "SELECT session_id, local_date, timezone_id,
                            day_start_utc_ms, day_end_utc_ms, workspace_id,
                            model_provider, model_raw, archived, active_ms,
                            input_tokens, fresh_input_tokens, cached_input_tokens,
                            output_tokens, reasoning_tokens, total_tokens,
                            priced_event_count + unpriced_event_count,
                            last_activity_at_ms
                     FROM session_daily_usage",
                )?;
                statement
                    .query_map([], |row| {
                        Ok(DailyUsageForReprice {
                            session_id: row.get(0)?,
                            local_date: row.get(1)?,
                            timezone_id: row.get(2)?,
                            day_start_utc_ms: row.get(3)?,
                            day_end_utc_ms: row.get(4)?,
                            workspace_id: row.get(5)?,
                            model_provider: row.get(6)?,
                            model_raw: row.get(7)?,
                            archived: row.get(8)?,
                            active_ms: row.get(9)?,
                            input_tokens: row.get(10)?,
                            fresh_input_tokens: row.get(11)?,
                            cached_input_tokens: row.get(12)?,
                            output_tokens: row.get(13)?,
                            reasoning_tokens: row.get(14)?,
                            total_tokens: row.get(15)?,
                            event_count: row.get(16)?,
                            last_activity_at_ms: row.get(17)?,
                        })
                    })?
                    .collect::<Result<Vec<_>, _>>()?
            };
            // 价格 ID 是永久汇总主键的一部分。先清空再按新 ID 合并回写，避免旧的
            // “未定价”行与既有已定价行在主键更新时发生冲突。
            transaction.execute("DELETE FROM session_daily_usage", [])?;
            for row in &daily {
                let quote = catalog.quote(
                    &row.model_provider,
                    &row.model_raw,
                    PriceableTokens {
                        fresh_input: nonnegative(row.fresh_input_tokens),
                        cached_input: nonnegative(row.cached_input_tokens),
                        output: nonnegative(row.output_tokens),
                        cache_write: 0,
                    },
                );
                let (next_pricing_id, _, cost, excluded) = quote_parts(quote);
                let priced_count = if cost.is_some() { row.event_count } else { 0 };
                let unpriced_count = if cost.is_none() && !excluded {
                    row.event_count
                } else {
                    0
                };
                transaction.execute(
                    "INSERT INTO session_daily_usage (
                        session_id, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
                        workspace_id, model_provider, model_raw, pricing_model_id, archived,
                        active_ms, input_tokens, fresh_input_tokens, cached_input_tokens,
                        output_tokens, reasoning_tokens, total_tokens, priced_cost_microusd,
                        priced_event_count, unpriced_event_count, last_activity_at_ms
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                               ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)
                     ON CONFLICT(session_id, local_date, timezone_id, model_provider,
                                 model_raw, pricing_model_id, archived)
                     DO UPDATE SET
                        day_start_utc_ms = MIN(day_start_utc_ms, excluded.day_start_utc_ms),
                        day_end_utc_ms = MAX(day_end_utc_ms, excluded.day_end_utc_ms),
                        active_ms = active_ms + excluded.active_ms,
                        input_tokens = input_tokens + excluded.input_tokens,
                        fresh_input_tokens = fresh_input_tokens + excluded.fresh_input_tokens,
                        cached_input_tokens = cached_input_tokens + excluded.cached_input_tokens,
                        output_tokens = output_tokens + excluded.output_tokens,
                        reasoning_tokens = reasoning_tokens + excluded.reasoning_tokens,
                        total_tokens = total_tokens + excluded.total_tokens,
                        priced_cost_microusd = priced_cost_microusd + excluded.priced_cost_microusd,
                        priced_event_count = priced_event_count + excluded.priced_event_count,
                        unpriced_event_count = unpriced_event_count + excluded.unpriced_event_count,
                        last_activity_at_ms = MAX(last_activity_at_ms, excluded.last_activity_at_ms)",
                    params![
                        row.session_id,
                        row.local_date,
                        row.timezone_id,
                        row.day_start_utc_ms,
                        row.day_end_utc_ms,
                        row.workspace_id,
                        row.model_provider,
                        row.model_raw,
                        next_pricing_id,
                        row.archived,
                        row.active_ms,
                        row.input_tokens,
                        row.fresh_input_tokens,
                        row.cached_input_tokens,
                        row.output_tokens,
                        row.reasoning_tokens,
                        row.total_tokens,
                        cost.unwrap_or(0),
                        priced_count,
                        unpriced_count,
                        row.last_activity_at_ms,
                    ],
                )?;
            }

            transaction.execute(
                "UPDATE sessions SET
                    estimated_cost_microusd = (
                        SELECT SUM(estimated_cost_microusd)
                        FROM session_model_segments m WHERE m.session_id = sessions.id
                    ),
                    unpriced_event_count = COALESCE((
                        SELECT SUM(unpriced_event_count)
                        FROM session_model_segments m WHERE m.session_id = sessions.id
                    ), 0),
                    updated_at_ms = ?1",
                [Utc::now().timestamp_millis()],
            )?;
            Ok(RepriceResult {
                events_repriced: events.len() as u64,
                model_segments_repriced: segments.len() as u64,
                daily_rows_repriced: daily.len() as u64,
            })
        })?;
        self.rebuild_rollups_and_prune(Utc::now().timestamp_millis(), 90, timezone)?;
        Ok(result)
    }
}

fn quote_parts(quote: PriceQuote) -> (String, Option<i64>, Option<i64>, bool) {
    match quote {
        PriceQuote::Priced {
            pricing_id,
            revision,
            total_microusd,
        } => (pricing_id, Some(revision), Some(total_microusd), false),
        PriceQuote::Excluded => (String::new(), None, None, true),
        PriceQuote::Unpriced => (String::new(), None, None, false),
    }
}

fn validate_price_input(input: &ModelPriceInput) -> AppResult<()> {
    if input.provider.trim().is_empty()
        || input.pricing_id.trim().is_empty()
        || input.display_name.trim().is_empty()
    {
        return Err(AppError::new(
            "invalid_price",
            "提供方、模型 ID 和名称不能为空",
        ));
    }
    canonical_price(&input.input_per_million_usd)?;
    canonical_price(&input.output_per_million_usd)?;
    canonical_price(&input.cache_read_per_million_usd)?;
    canonical_optional_price(input.cache_write_per_million_usd.as_deref())?;
    Ok(())
}

fn parse_price(value: &str) -> AppResult<Decimal> {
    let parsed = Decimal::from_str(value)
        .map_err(|_| AppError::new("invalid_price", "价格必须是非负十进制数"))?;
    if parsed.is_sign_negative() {
        return Err(AppError::new("invalid_price", "价格不能为负数"));
    }
    Ok(parsed)
}

fn canonical_price(value: &str) -> AppResult<String> {
    Ok(parse_price(value.trim())?.normalize().to_string())
}

fn canonical_optional_price(value: Option<&str>) -> AppResult<Option<String>> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(canonical_price)
        .transpose()
}

fn nonnegative(value: i64) -> u64 {
    u64::try_from(value.max(0)).unwrap_or(0)
}

fn official_snapshot_ms() -> i64 {
    1_783_641_600_000
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use serde_json::json;

    use super::*;
    use crate::source::{CodexSource, FsCodexSource};

    #[test]
    fn seeds_official_gpt_5_6_prices_and_preserves_user_override() {
        let store = UsageStore::open_in_memory().unwrap();
        store.seed_builtin_prices().unwrap();
        let prices = store.model_prices(false).unwrap();
        assert_eq!(
            prices
                .iter()
                .take(4)
                .map(|price| price.pricing_id.as_str())
                .collect::<Vec<_>>(),
            vec!["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5"]
        );
        let sol = prices
            .iter()
            .find(|price| price.pricing_id == "gpt-5.6-sol")
            .unwrap();
        assert_eq!(sol.input_per_million_usd, "5");
        assert_eq!(sol.cache_read_per_million_usd, "0.5");
        assert_eq!(sol.cache_write_per_million_usd.as_deref(), Some("6.25"));
        assert_eq!(sol.output_per_million_usd, "30");
        assert_eq!(sol.source_url.as_deref(), Some(OFFICIAL_PRICING_SOURCE));

        store
            .save_model_price(&ModelPriceInput {
                provider: "openai".into(),
                pricing_id: "gpt-5.6-sol".into(),
                display_name: "Custom Sol".into(),
                input_per_million_usd: "7".into(),
                output_per_million_usd: "31".into(),
                cache_read_per_million_usd: "0.7".into(),
                cache_write_per_million_usd: Some("8".into()),
            })
            .unwrap();
        store.seed_builtin_prices().unwrap();
        let overridden = store
            .model_prices(false)
            .unwrap()
            .into_iter()
            .find(|price| price.pricing_id == "gpt-5.6-sol")
            .unwrap();
        assert!(overridden.is_overridden);
        assert_eq!(overridden.input_per_million_usd, "7");

        store
            .restore_builtin_price("openai", "gpt-5.6-sol")
            .unwrap();
        let restored = store
            .model_prices(false)
            .unwrap()
            .into_iter()
            .find(|price| price.pricing_id == "gpt-5.6-sol")
            .unwrap();
        assert!(!restored.is_overridden);
        assert_eq!(restored.input_per_million_usd, "5");
    }

    #[test]
    fn custom_prices_can_be_deleted_while_builtins_can_be_restored() {
        let store = UsageStore::open_in_memory().unwrap();
        store.seed_builtin_prices().unwrap();
        store.delete_model_price("openai", "gpt-5.6-luna").unwrap();
        assert!(
            store
                .model_prices(false)
                .unwrap()
                .iter()
                .all(|price| price.pricing_id != "gpt-5.6-luna")
        );
        store
            .restore_builtin_price("openai", "gpt-5.6-luna")
            .unwrap();
        assert!(
            store
                .model_prices(false)
                .unwrap()
                .iter()
                .any(|price| price.pricing_id == "gpt-5.6-luna")
        );

        store
            .save_model_price(&ModelPriceInput {
                provider: "codex_local_access".into(),
                pricing_id: "gpt-custom".into(),
                display_name: "Custom".into(),
                input_per_million_usd: "1".into(),
                output_per_million_usd: "8".into(),
                cache_read_per_million_usd: "0.1".into(),
                cache_write_per_million_usd: None,
            })
            .unwrap();
        let custom = store
            .model_prices(false)
            .unwrap()
            .into_iter()
            .find(|price| price.pricing_id == "gpt-custom")
            .unwrap();
        assert_eq!(custom.provider, "custom");
    }

    #[test]
    fn user_price_change_reprices_events_sessions_and_daily_rollups_without_jsonl_reparse() {
        let codex = tempfile::tempdir().unwrap();
        let sessions = codex.path().join("sessions/2026/07/10");
        std::fs::create_dir_all(&sessions).unwrap();
        let content = [
            json!({"timestamp":"2026-07-10T01:00:00Z","type":"session_meta","payload":{"id":"reprice-session","cwd":"C:/workspace/demo","model_provider":"openai"}}),
            json!({"timestamp":"2026-07-10T01:00:01Z","type":"turn_context","payload":{"model":"gpt-5.6-sol"}}),
            json!({"timestamp":"2026-07-10T01:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1000000,"cached_input_tokens":0,"output_tokens":100000,"reasoning_output_tokens":0}}}}),
        ]
        .into_iter()
        .map(|value| serde_json::to_string(&value).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
            + "\n";
        std::fs::write(sessions.join("rollout-reprice.jsonl"), content).unwrap();

        let store = UsageStore::open_in_memory().unwrap();
        store.seed_builtin_prices().unwrap();
        let source = FsCodexSource::new(codex.path());
        let plan = source.plan(&[]).unwrap();
        store
            .apply_source_change(
                &source,
                &plan.changes[0],
                &store.pricing_catalog().unwrap(),
                chrono_tz::UTC,
                30 * 60 * 1000,
                &AtomicBool::new(false),
            )
            .unwrap();
        let before: i64 = store
            .with_reader(|connection| {
                connection
                    .query_row(
                        "SELECT estimated_cost_microusd FROM sessions WHERE id = 'reprice-session'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(AppError::from)
            })
            .unwrap();
        assert_eq!(before, 8_000_000);

        // 现实数据库可能同时保留同一模型的“未定价”和“已定价”日汇总行；二者
        // 重算后会收敛到同一个 pricing_model_id，必须合并而不是触发主键冲突。
        store
            .with_writer(|transaction| {
                transaction.execute(
                    "INSERT INTO session_daily_usage (
                        session_id, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
                        workspace_id, model_provider, model_raw, pricing_model_id, archived,
                        active_ms, input_tokens, fresh_input_tokens, cached_input_tokens,
                        output_tokens, reasoning_tokens, total_tokens, priced_cost_microusd,
                        priced_event_count, unpriced_event_count, last_activity_at_ms
                     ) SELECT session_id, local_date, timezone_id, day_start_utc_ms, day_end_utc_ms,
                              workspace_id, model_provider, model_raw, '', archived,
                              0, 0, 0, 0, 0, 0, 0, 0, 0, 0, last_activity_at_ms
                       FROM session_daily_usage LIMIT 1",
                    [],
                )?;
                Ok(())
            })
            .unwrap();

        store
            .save_model_price(&ModelPriceInput {
                provider: "openai".into(),
                pricing_id: "gpt-5.6-sol".into(),
                display_name: "GPT-5.6 Sol custom".into(),
                input_per_million_usd: "10".into(),
                output_per_million_usd: "40".into(),
                cache_read_per_million_usd: "1".into(),
                cache_write_per_million_usd: None,
            })
            .unwrap();
        let result = store.reprice_all(chrono_tz::UTC).unwrap();
        assert_eq!(result.events_repriced, 1);
        assert_eq!(result.model_segments_repriced, 1);
        assert_eq!(result.daily_rows_repriced, 2);
        store
            .with_reader(|connection| {
                let event_cost: i64 = connection.query_row(
                    "SELECT estimated_cost_microusd FROM usage_events",
                    [],
                    |row| row.get(0),
                )?;
                let session_cost: i64 = connection.query_row(
                    "SELECT estimated_cost_microusd FROM sessions WHERE id = 'reprice-session'",
                    [],
                    |row| row.get(0),
                )?;
                let daily_cost: i64 = connection.query_row(
                    "SELECT priced_cost_microusd FROM daily_usage_rollups",
                    [],
                    |row| row.get(0),
                )?;
                let daily_rows: i64 = connection.query_row(
                    "SELECT COUNT(*) FROM session_daily_usage",
                    [],
                    |row| row.get(0),
                )?;
                assert_eq!(event_cost, 14_000_000);
                assert_eq!(session_cost, 14_000_000);
                assert_eq!(daily_cost, 14_000_000);
                assert_eq!(daily_rows, 1);
                Ok(())
            })
            .unwrap();
    }
}
