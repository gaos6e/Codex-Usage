use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use chronolume_lib::indexer::system_timezone;
use chronolume_lib::query::{ArchiveFilter, RangePreset, RangeSelection, UsageFilters, UsageQuery};
use chronolume_lib::store::UsageStore;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct QueryReport {
    open_and_first_dashboard_ms: f64,
    samples: usize,
    p50_ms: f64,
    p95_ms: f64,
    max_ms: f64,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: query-benchmark <existing-v2-database>")?;
    if !path.is_file() {
        return Err("benchmark database does not exist".into());
    }
    let started = Instant::now();
    let query = UsageQuery::new(Arc::new(UsageStore::open(&path)?), system_timezone());
    std::hint::black_box(query.dashboard(&filters(RangePreset::Last30Days))?);
    let open_and_first_dashboard_ms = started.elapsed().as_secs_f64() * 1000.0;

    let presets = [
        RangePreset::Today,
        RangePreset::Last24Hours,
        RangePreset::Last7Days,
        RangePreset::Last30Days,
        RangePreset::Last90Days,
        RangePreset::All,
    ];
    let mut timings = Vec::with_capacity(presets.len() * 30);
    for _ in 0..30 {
        for preset in presets {
            let started = Instant::now();
            std::hint::black_box(query.dashboard(&filters(preset))?);
            timings.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    timings.sort_by(f64::total_cmp);
    let report = QueryReport {
        open_and_first_dashboard_ms,
        samples: timings.len(),
        p50_ms: percentile(&timings, 0.50),
        p95_ms: percentile(&timings, 0.95),
        max_ms: timings.last().copied().unwrap_or_default(),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn filters(preset: RangePreset) -> UsageFilters {
    UsageFilters {
        range: RangeSelection {
            preset,
            start_ms: None,
            end_ms: None,
            live_end: false,
        },
        workspace_id: None,
        model_provider: None,
        model: None,
        archived: ArchiveFilter::All,
    }
}

fn percentile(sorted: &[f64], value: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    sorted[((sorted.len() - 1) as f64 * value).round() as usize]
}
