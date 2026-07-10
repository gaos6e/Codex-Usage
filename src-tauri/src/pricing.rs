use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

pub const OFFICIAL_PRICING_SOURCE: &str = "https://developers.openai.com/api/docs/pricing";
pub const BUILTIN_PRICING_REVISION: i64 = 2_026_071_002;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedModelId {
    pub exact: String,
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPrice {
    pub provider: String,
    pub pricing_id: String,
    pub input_per_million_usd: Decimal,
    pub output_per_million_usd: Decimal,
    pub cache_read_per_million_usd: Decimal,
    pub cache_write_per_million_usd: Option<Decimal>,
    pub revision: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceableTokens {
    pub fresh_input: u64,
    pub cached_input: u64,
    pub output: u64,
    pub cache_write: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PriceQuote {
    Priced {
        pricing_id: String,
        revision: i64,
        total_microusd: i64,
    },
    /// 内部功能产生的 Token 保留统计，但既不猜价，也不计入缺价告警。
    Excluded,
    Unpriced,
}

#[derive(Debug, Clone, Default)]
pub struct PricingCatalog {
    prices: Vec<ModelPrice>,
}

pub fn builtin_model_prices() -> Vec<ModelPrice> {
    [
        ("gpt-5.6-sol", "5", "30", "0.5", Some("6.25")),
        ("gpt-5.6-terra", "2.5", "15", "0.25", Some("3.125")),
        ("gpt-5.6-luna", "1", "6", "0.1", Some("1.25")),
        ("gpt-5.5", "5", "30", "0.5", None),
        ("gpt-5.4", "2.5", "15", "0.25", None),
        ("gpt-5.4-mini", "0.75", "4.5", "0.075", None),
        ("gpt-5.4-nano", "0.2", "1.25", "0.02", None),
        ("gpt-5.3-codex", "1.75", "14", "0.175", None),
        ("gpt-5.2", "1.75", "14", "0.175", None),
        ("gpt-5.1", "1.25", "10", "0.125", None),
        ("gpt-5", "1.25", "10", "0.125", None),
        ("gpt-5-mini", "0.25", "2", "0.025", None),
        ("gpt-5-nano", "0.05", "0.4", "0.005", None),
        ("gpt-4.1", "2", "8", "0.5", None),
        ("gpt-4.1-mini", "0.4", "1.6", "0.1", None),
        ("gpt-4.1-nano", "0.1", "0.4", "0.025", None),
        ("gpt-4o", "2.5", "10", "1.25", None),
        ("gpt-4o-mini", "0.15", "0.6", "0.075", None),
        ("o3", "2", "8", "0.5", None),
        ("o4-mini", "1.1", "4.4", "0.275", None),
        ("o3-mini", "1.1", "4.4", "0.55", None),
        ("o1", "15", "60", "7.5", None),
        ("o1-mini", "1.1", "4.4", "0.55", None),
    ]
    .into_iter()
    .map(|(id, input, output, cache_read, cache_write)| ModelPrice {
        provider: "openai".to_string(),
        pricing_id: id.to_string(),
        input_per_million_usd: Decimal::from_str(input).expect("built-in input price"),
        output_per_million_usd: Decimal::from_str(output).expect("built-in output price"),
        cache_read_per_million_usd: Decimal::from_str(cache_read)
            .expect("built-in cache-read price"),
        cache_write_per_million_usd: cache_write
            .map(|value| Decimal::from_str(value).expect("built-in cache-write price")),
        revision: BUILTIN_PRICING_REVISION,
    })
    .collect()
}

impl PricingCatalog {
    pub fn new(mut prices: Vec<ModelPrice>) -> Self {
        prices.sort_by(|left, right| {
            right
                .pricing_id
                .len()
                .cmp(&left.pricing_id.len())
                .then_with(|| left.pricing_id.cmp(&right.pricing_id))
        });
        Self { prices }
    }

    pub fn quote(&self, provider: &str, raw_model: &str, tokens: PriceableTokens) -> PriceQuote {
        let normalized = normalize_model_id(raw_model);
        let provider = provider.trim().to_ascii_lowercase();
        if is_cost_excluded(&provider, &normalized.exact) {
            return PriceQuote::Excluded;
        }
        let price = normalized
            .candidates
            .iter()
            .find_map(|candidate| self.find_exact(&provider, candidate))
            .or_else(|| {
                // Codex 的 `custom` provider 常用于兼容 OpenAI API 的本地网关。用户为
                // custom 配置的价格始终优先；没有时仅对“精确官方模型 ID”采用 OpenAI
                // 参考价，不把 Azure/其他 provider 或未知后缀强行映射过来。
                (provider == "custom").then(|| {
                    normalized
                        .candidates
                        .iter()
                        .find_map(|candidate| self.find_exact("openai", candidate))
                })?
            });
        let Some(price) = price else {
            return PriceQuote::Unpriced;
        };
        if tokens.cache_write > 0 && price.cache_write_per_million_usd.is_none() {
            return PriceQuote::Unpriced;
        }

        let total = token_cost(tokens.fresh_input, price.input_per_million_usd)
            + token_cost(tokens.cached_input, price.cache_read_per_million_usd)
            + token_cost(tokens.output, price.output_per_million_usd)
            + token_cost(
                tokens.cache_write,
                price.cache_write_per_million_usd.unwrap_or(Decimal::ZERO),
            );
        let total_microusd = total.round().to_i64().unwrap_or(i64::MAX);

        PriceQuote::Priced {
            pricing_id: price.pricing_id.clone(),
            revision: price.revision,
            total_microusd,
        }
    }

    fn find_exact(&self, provider: &str, pricing_id: &str) -> Option<&ModelPrice> {
        self.prices.iter().find(|price| {
            price.provider.eq_ignore_ascii_case(provider)
                && price.pricing_id.eq_ignore_ascii_case(pricing_id)
        })
    }
}

pub fn is_cost_excluded(provider: &str, model: &str) -> bool {
    provider.eq_ignore_ascii_case("openai")
        && model.trim().eq_ignore_ascii_case("codex-auto-review")
}

/// 生成从最具体到最宽泛的计价候选，不改变或限制原始模型字符串。
pub fn normalize_model_id(raw: &str) -> NormalizedModelId {
    let mut exact = raw.trim().to_ascii_lowercase();
    if let Some(last) = exact.rsplit('/').next()
        && last != exact
    {
        exact = last.to_string();
    }
    if let Some((prefix, suffix)) = exact.split_once(':')
        && matches!(suffix, "minimal" | "low" | "medium" | "high" | "xhigh")
    {
        exact = prefix.to_string();
    }

    let mut candidates = vec![exact.clone()];
    let mut index = 0;
    while index < candidates.len() {
        let candidate = candidates[index].clone();
        push_candidate(&mut candidates, strip_iso_date_suffix(&candidate));
        push_candidate(&mut candidates, strip_compact_date_suffix(&candidate));
        push_candidate(&mut candidates, strip_reasoning_suffix(&candidate));
        push_candidate(&mut candidates, historical_tier_alias(&candidate));
        index += 1;
    }

    NormalizedModelId { exact, candidates }
}

/// 为已经下架且官方价格表不再单列的 Codex 型号提供可审计的同层级回退。
/// `gpt-5.2-codex` 的原始 ID 仍保留在统计中，计价 ID 明确显示为 `gpt-5.2`。
fn historical_tier_alias(value: &str) -> Option<String> {
    match value {
        "gpt-5.2-codex" => Some("gpt-5.2".to_string()),
        _ => None,
    }
}

/// 生成用于数据库名称排序的模型强度键。版本优先于同版本的强度层级，
/// 因此降序稳定得到 5.6 Sol、5.6 Terra、5.6 Luna、5.5…。
pub fn model_strength_sort_key(raw: &str) -> i64 {
    let normalized = normalize_model_id(raw).exact;
    let Some(rest) = normalized.strip_prefix("gpt-") else {
        return 0;
    };
    let version = rest.split('-').next().unwrap_or_default();
    let mut parts = version.split('.');
    let Some(major) = parts.next().and_then(|part| part.parse::<i64>().ok()) else {
        return 0;
    };
    let minor = parts
        .next()
        .and_then(|part| part.parse::<i64>().ok())
        .unwrap_or(0);
    let tier = if rest.contains("-pro") {
        900
    } else if rest.contains("-sol") {
        800
    } else if rest.contains("-terra") {
        700
    } else if rest.contains("-luna") {
        500
    } else if rest.contains("-mini") {
        300
    } else if rest.contains("-nano") {
        200
    } else if rest.contains("-spark") {
        100
    } else {
        600
    };
    major
        .saturating_mul(1_000_000)
        .saturating_add(minor.saturating_mul(10_000))
        .saturating_add(tier)
}

fn token_cost(tokens: u64, price_per_million_usd: Decimal) -> Decimal {
    // 每 Token 的 microUSD 数值恰好等于“每百万 Token 的 USD 价格”。
    Decimal::from(tokens) * price_per_million_usd
}

fn push_candidate(candidates: &mut Vec<String>, candidate: Option<String>) {
    if let Some(candidate) = candidate
        && !candidate.is_empty()
        && !candidates.contains(&candidate)
    {
        candidates.push(candidate);
    }
}

fn strip_iso_date_suffix(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    if bytes.len() < 11 {
        return None;
    }
    let suffix = &bytes[bytes.len() - 11..];
    let valid = suffix[0] == b'-'
        && suffix[1..5].iter().all(u8::is_ascii_digit)
        && suffix[5] == b'-'
        && suffix[6..8].iter().all(u8::is_ascii_digit)
        && suffix[8] == b'-'
        && suffix[9..11].iter().all(u8::is_ascii_digit);
    valid.then(|| value[..value.len() - 11].to_string())
}

fn strip_compact_date_suffix(value: &str) -> Option<String> {
    let (prefix, suffix) = value.rsplit_once('-')?;
    (suffix.len() == 8 && suffix.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| prefix.to_string())
}

fn strip_reasoning_suffix(value: &str) -> Option<String> {
    let (prefix, suffix) = value.rsplit_once('-')?;
    matches!(suffix, "minimal" | "low" | "medium" | "high" | "xhigh").then(|| prefix.to_string())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn price(id: &str) -> ModelPrice {
        ModelPrice {
            provider: "openai".to_string(),
            pricing_id: id.to_string(),
            input_per_million_usd: Decimal::from_str("2.50").unwrap(),
            output_per_million_usd: Decimal::from_str("10.00").unwrap(),
            cache_read_per_million_usd: Decimal::from_str("0.25").unwrap(),
            cache_write_per_million_usd: None,
            revision: 3,
        }
    }

    #[test]
    fn preserves_dynamic_gpt_5_6_variants() {
        for model in ["gpt-5.6", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna"] {
            assert_eq!(normalize_model_id(model).exact, model);
        }
    }

    #[test]
    fn sorts_models_by_version_then_strength_tier() {
        let ordered = ["gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna", "gpt-5.5"];
        assert!(
            ordered.windows(2).all(|pair| {
                model_strength_sort_key(pair[0]) > model_strength_sort_key(pair[1])
            })
        );
    }

    #[test]
    fn prices_current_codex_exactly_and_retired_codex_by_visible_tier_alias() {
        let catalog = PricingCatalog::new(builtin_model_prices());
        let tokens = PriceableTokens {
            fresh_input: 1_000_000,
            cached_input: 0,
            output: 0,
            cache_write: 0,
        };
        assert!(matches!(
            catalog.quote("openai", "gpt-5.3-codex", tokens),
            PriceQuote::Priced { pricing_id, total_microusd: 1_750_000, .. }
                if pricing_id == "gpt-5.3-codex"
        ));
        assert!(matches!(
            catalog.quote("openai", "gpt-5.2-codex", tokens),
            PriceQuote::Priced { pricing_id, total_microusd: 1_750_000, .. }
                if pricing_id == "gpt-5.2"
        ));
    }

    #[test]
    fn adds_provider_date_and_reasoning_candidates() {
        let normalized = normalize_model_id("OpenAI/GPT-5.6-2026-07-10-high");
        assert_eq!(normalized.exact, "gpt-5.6-2026-07-10-high");
        assert!(
            normalized
                .candidates
                .contains(&"gpt-5.6-2026-07-10".to_string())
        );

        let dated = normalize_model_id("openai/gpt-5.6-2026-07-10");
        assert!(dated.candidates.contains(&"gpt-5.6".to_string()));

        let effort = normalize_model_id("openai/gpt-5.6:high");
        assert_eq!(effort.exact, "gpt-5.6");
    }

    #[test]
    fn matches_exact_before_family_prefix_and_computes_micro_usd() {
        let catalog = PricingCatalog::new(vec![price("gpt-5.6"), price("gpt-5.6-sol")]);
        let quote = catalog.quote(
            "OpenAI",
            "openai/GPT-5.6-SOL",
            PriceableTokens {
                fresh_input: 1000,
                cached_input: 4000,
                output: 500,
                cache_write: 0,
            },
        );
        assert_eq!(
            quote,
            PriceQuote::Priced {
                pricing_id: "gpt-5.6-sol".to_string(),
                revision: 3,
                total_microusd: 8_500,
            }
        );
    }

    #[test]
    fn reports_unpriced_instead_of_zero() {
        let catalog = PricingCatalog::new(vec![price("gpt-5.6")]);
        assert_eq!(
            catalog.quote(
                "openai",
                "unknown-future-model",
                PriceableTokens {
                    fresh_input: 1,
                    cached_input: 0,
                    output: 0,
                    cache_write: 0,
                }
            ),
            PriceQuote::Unpriced
        );
    }

    #[test]
    fn excludes_internal_auto_review_without_creating_a_zero_price() {
        let catalog = PricingCatalog::new(vec![price("gpt-5")]);
        assert_eq!(
            catalog.quote(
                "openai",
                "codex-auto-review",
                PriceableTokens {
                    fresh_input: 10,
                    cached_input: 20,
                    output: 3,
                    cache_write: 0
                },
            ),
            PriceQuote::Excluded,
        );
    }

    #[test]
    fn does_not_guess_prices_for_unknown_family_suffixes() {
        let catalog = PricingCatalog::new(vec![price("gpt-5")]);
        assert_eq!(
            catalog.quote(
                "openai",
                "gpt-5.3-codex",
                PriceableTokens {
                    fresh_input: 10,
                    cached_input: 0,
                    output: 1,
                    cache_write: 0
                },
            ),
            PriceQuote::Unpriced,
        );
    }

    #[test]
    fn custom_provider_falls_back_only_to_an_exact_openai_model_price() {
        let catalog = PricingCatalog::new(vec![price("gpt-5")]);
        assert!(matches!(
            catalog.quote(
                "custom",
                "gpt-5",
                PriceableTokens { fresh_input: 10, cached_input: 0, output: 1, cache_write: 0 },
            ),
            PriceQuote::Priced { pricing_id, .. } if pricing_id == "gpt-5"
        ));
        assert_eq!(
            catalog.quote(
                "custom",
                "gpt-5.3-codex",
                PriceableTokens {
                    fresh_input: 10,
                    cached_input: 0,
                    output: 1,
                    cache_write: 0
                },
            ),
            PriceQuote::Unpriced,
        );
    }
}
