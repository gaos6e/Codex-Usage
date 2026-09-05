use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use chrono::Utc;
use parking_lot::Mutex;
use regex::Regex;
use serde::Serialize;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::pricing::OFFICIAL_PRICING_SOURCE;
use crate::store::ModelPriceRecord;

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedPriceRow {
    pub provider: String,
    pub pricing_id: String,
    pub display_name: String,
    pub input_per_million_usd: String,
    pub output_per_million_usd: String,
    pub cache_read_per_million_usd: String,
    pub cache_write_per_million_usd: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceChangeKind {
    Added,
    Updated,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceChange {
    pub kind: PriceChangeKind,
    pub pricing_id: String,
    pub before: Option<TrustedPriceRow>,
    pub after: TrustedPriceRow,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceUpdatePreview {
    pub preview_id: String,
    pub source_url: &'static str,
    pub fetched_at_ms: i64,
    pub changes: Vec<PriceChange>,
    pub unchanged_count: usize,
}

#[derive(Debug, Clone)]
struct PendingUpdate {
    id: String,
    fetched_at_ms: i64,
    rows: Vec<TrustedPriceRow>,
}

#[derive(Default)]
pub struct PriceUpdateService {
    pending: Mutex<Option<PendingUpdate>>,
}

impl PriceUpdateService {
    pub async fn preview(&self, current: &[ModelPriceRecord]) -> AppResult<PriceUpdatePreview> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| network_error())?;
        let response = client
            .get(OFFICIAL_PRICING_SOURCE)
            .send()
            .await
            .map_err(|_| network_error())?;
        if !response.status().is_success() {
            return Err(AppError::new(
                "pricing_source_unavailable",
                "OpenAI 官方价格页暂时不可用",
            ));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(AppError::new(
                "pricing_response_too_large",
                "官方价格页响应超出安全上限",
            ));
        }
        let bytes = response.bytes().await.map_err(|_| network_error())?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Err(AppError::new(
                "pricing_response_too_large",
                "官方价格页响应超出安全上限",
            ));
        }
        let html = std::str::from_utf8(&bytes)
            .map_err(|_| AppError::new("pricing_parse_failed", "官方价格页编码无效"))?;
        let rows = parse_official_pricing(html)?;
        let current_by_id: HashMap<_, _> = current
            .iter()
            .filter(|price| price.provider == "openai")
            .map(|price| (price.pricing_id.as_str(), price))
            .collect();
        let mut changes = Vec::new();
        let mut unchanged_count = 0;
        for row in &rows {
            let before = current_by_id
                .get(row.pricing_id.as_str())
                .map(|price| TrustedPriceRow {
                    provider: price.provider.clone(),
                    pricing_id: price.pricing_id.clone(),
                    display_name: price.display_name.clone(),
                    input_per_million_usd: price.input_per_million_usd.clone(),
                    output_per_million_usd: price.output_per_million_usd.clone(),
                    cache_read_per_million_usd: price.cache_read_per_million_usd.clone(),
                    cache_write_per_million_usd: price.cache_write_per_million_usd.clone(),
                });
            if before.as_ref().is_some_and(|value| same_prices(value, row)) {
                unchanged_count += 1;
            } else {
                changes.push(PriceChange {
                    kind: if before.is_some() {
                        PriceChangeKind::Updated
                    } else {
                        PriceChangeKind::Added
                    },
                    pricing_id: row.pricing_id.clone(),
                    before,
                    after: row.clone(),
                });
            }
        }
        let preview_id = Uuid::new_v4().to_string();
        let fetched_at_ms = Utc::now().timestamp_millis();
        *self.pending.lock() = Some(PendingUpdate {
            id: preview_id.clone(),
            fetched_at_ms,
            rows,
        });
        Ok(PriceUpdatePreview {
            preview_id,
            source_url: OFFICIAL_PRICING_SOURCE,
            fetched_at_ms,
            changes,
            unchanged_count,
        })
    }

    pub fn take(&self, preview_id: &str) -> AppResult<(i64, Vec<TrustedPriceRow>)> {
        let pending = self.pending.lock().take().ok_or_else(|| {
            AppError::new("pricing_preview_expired", "价格预览不存在，请重新检查")
        })?;
        if pending.id != preview_id
            || Utc::now()
                .timestamp_millis()
                .saturating_sub(pending.fetched_at_ms)
                > 10 * 60 * 1000
        {
            return Err(AppError::new(
                "pricing_preview_expired",
                "价格预览已过期，请重新检查",
            ));
        }
        Ok((pending.fetched_at_ms, pending.rows))
    }
}

fn parse_official_pricing(html: &str) -> AppResult<Vec<TrustedPriceRow>> {
    let row_pattern = Regex::new(
        r#"\[1,\[\[0,&quot;([^\"]+?)&quot;\],\[0,([^\]]+)\],\[0,([^\]]+)\],\[0,([^\]]+)\],\[0,([^\]]+)\]\]\]"#,
    )
    .map_err(|_| AppError::new("pricing_parse_failed", "价格解析器初始化失败"))?;
    let mut rows = BTreeMap::new();
    for captures in row_pattern.captures_iter(html) {
        let raw_model = captures.get(1).map_or("", |value| value.as_str());
        let pricing_id = decode_model_id(raw_model);
        if !is_openai_model_id(&pricing_id) {
            continue;
        }
        let Some(input) = parse_price(captures.get(2).map_or("", |value| value.as_str())) else {
            continue;
        };
        let Some(output) = parse_price(captures.get(5).map_or("", |value| value.as_str())) else {
            continue;
        };
        let Some(cache_read) = parse_price(captures.get(3).map_or("", |value| value.as_str()))
        else {
            // 缓存读取价格缺失时不能用虚假的 $0；该模型保持现有价格或未定价。
            continue;
        };
        let cache_write = parse_price(captures.get(4).map_or("", |value| value.as_str()));
        rows.entry(pricing_id.clone()).or_insert(TrustedPriceRow {
            provider: "openai".to_string(),
            display_name: official_display_name(&pricing_id),
            pricing_id,
            input_per_million_usd: input,
            output_per_million_usd: output,
            cache_read_per_million_usd: cache_read,
            cache_write_per_million_usd: cache_write,
        });
    }
    if rows.len() < 8 {
        return Err(AppError::new(
            "pricing_parse_failed",
            "官方价格页格式已变化，未应用任何更新",
        ));
    }
    Ok(rows.into_values().collect())
}

fn official_display_name(pricing_id: &str) -> String {
    match pricing_id {
        "gpt-6-astra" => "GPT-6 Astra".to_string(),
        _ => pricing_id.to_string(),
    }
}

fn decode_model_id(raw: &str) -> String {
    raw.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .split(" (")
        .next()
        .unwrap_or(raw)
        .trim()
        .to_ascii_lowercase()
}

fn is_openai_model_id(value: &str) -> bool {
    value.starts_with("gpt-")
        || value
            .strip_prefix('o')
            .and_then(|rest| rest.chars().next())
            .is_some_and(|character| character.is_ascii_digit())
}

fn parse_price(raw: &str) -> Option<String> {
    let value = raw.trim().trim_matches('&').trim_matches('"');
    if value.contains("quot") || value == "-" {
        return None;
    }
    value
        .parse::<rust_decimal::Decimal>()
        .ok()
        .map(|decimal| decimal.normalize().to_string())
}

fn same_prices(left: &TrustedPriceRow, right: &TrustedPriceRow) -> bool {
    left.input_per_million_usd == right.input_per_million_usd
        && left.output_per_million_usd == right.output_per_million_usd
        && left.cache_read_per_million_usd == right.cache_read_per_million_usd
        && left.cache_write_per_million_usd == right.cache_write_per_million_usd
}

fn network_error() -> AppError {
    AppError::new(
        "pricing_network_failed",
        "无法连接 OpenAI 官方价格页；没有应用任何更改",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_safe_openai_rows_from_official_shape() {
        let html = r#"rows&quot;:[1,[[1,[[0,&quot;gpt-5.6-sol&quot;],[0,5],[0,0.5],[0,6.25],[0,30]]],[1,[[0,&quot;gpt-5.6-terra&quot;],[0,2.5],[0,0.25],[0,3.125],[0,15]]],[1,[[0,&quot;gpt-5.6-luna&quot;],[0,1],[0,0.1],[0,1.25],[0,6]]],[1,[[0,&quot;gpt-5.5 (&lt;272K context length)&quot;],[0,5],[0,0.5],[0,&quot;-&quot;],[0,30]]],[1,[[0,&quot;gpt-5.4&quot;],[0,2.5],[0,0.25],[0,&quot;-&quot;],[0,15]]],[1,[[0,&quot;gpt-5.2&quot;],[0,1.75],[0,0.175],[0,&quot;-&quot;],[0,14]]],[1,[[0,&quot;gpt-5&quot;],[0,1.25],[0,0.125],[0,&quot;-&quot;],[0,10]]],[1,[[0,&quot;o3&quot;],[0,2],[0,0.5],[0,&quot;-&quot;],[0,8]]],[1,[[0,&quot;external-model&quot;],[0,1],[0,1],[0,1],[0,1]]]]"#;
        let rows = parse_official_pricing(html).unwrap();
        assert_eq!(rows.len(), 8);
        let sol = rows
            .iter()
            .find(|row| row.pricing_id == "gpt-5.6-sol")
            .unwrap();
        assert_eq!(sol.cache_write_per_million_usd.as_deref(), Some("6.25"));
        let gpt_55 = rows.iter().find(|row| row.pricing_id == "gpt-5.5").unwrap();
        assert_eq!(gpt_55.cache_write_per_million_usd, None);
    }
}
