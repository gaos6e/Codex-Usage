use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Utc;
use chronolume_lib::indexer::{SyncMode, UsageIndexer, system_timezone};
use chronolume_lib::query::{ArchiveFilter, RangePreset, RangeSelection, UsageFilters, UsageQuery};
use chronolume_lib::source::FsCodexSource;
use chronolume_lib::store::UsageStore;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkReport {
    generated_at_ms: i64,
    codex_root: String,
    database_path: String,
    initial_sync_ms: u128,
    incremental_sync_ms: u128,
    dashboard_available_during_import_ms: Option<u128>,
    files_indexed: u64,
    bytes_read: u64,
    records_written: u64,
    parse_failures: u64,
    file_errors: u64,
    query_samples: usize,
    query_p50_ms: f64,
    query_p95_ms: f64,
    query_max_ms: f64,
    database_size_bytes: u64,
    integrity_ok: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments.len() != 3 {
        return Err("usage: usage-benchmark <codex-root> <benchmark-db>".into());
    }
    let codex_root = PathBuf::from(&arguments[1]);
    let database_path = PathBuf::from(&arguments[2]);
    if database_path.exists() {
        return Err("benchmark database already exists; choose a fresh path".into());
    }

    let store = Arc::new(UsageStore::open(&database_path)?);
    store.seed_builtin_prices()?;
    let timezone = system_timezone();
    let indexer = Arc::new(UsageIndexer::new(
        Arc::clone(&store),
        Arc::new(FsCodexSource::new(&codex_root)),
        store.pricing_catalog()?,
        timezone,
        30 * 60 * 1000,
    ));
    let query = UsageQuery::new(Arc::clone(&store), timezone);

    let started = Instant::now();
    let worker = {
        let indexer = Arc::clone(&indexer);
        std::thread::spawn(move || indexer.sync(SyncMode::Initial))
    };
    let mut dashboard_available_during_import_ms = None;
    let mut poll_count = 0_u64;
    while !worker.is_finished() {
        if dashboard_available_during_import_ms.is_none()
            && query.dashboard(&filters(RangePreset::Last30Days)).is_ok()
        {
            dashboard_available_during_import_ms = Some(started.elapsed().as_millis());
        }
        if poll_count % 10 == 0 {
            let status = indexer.status();
            eprintln!(
                "phase={:?} files={}/{} bytes={}/{} records={} failures={}",
                status.phase,
                status.files_completed,
                status.files_total,
                status.bytes_read,
                status.bytes_total,
                status.records_written,
                status.parse_failures
            );
        }
        poll_count = poll_count.saturating_add(1);
        std::thread::sleep(Duration::from_millis(500));
    }
    let initial = worker
        .join()
        .map_err(|_| "initial benchmark worker panicked")??;
    let initial_sync_ms = started.elapsed().as_millis();

    let incremental_started = Instant::now();
    let incremental = indexer.sync(SyncMode::Incremental)?;
    let incremental_sync_ms = incremental_started.elapsed().as_millis();

    let presets = [
        RangePreset::Today,
        RangePreset::Last24Hours,
        RangePreset::Last7Days,
        RangePreset::Last30Days,
        RangePreset::Last90Days,
        RangePreset::All,
    ];
    let mut timings = Vec::with_capacity(presets.len() * 20);
    for _ in 0..20 {
        for preset in presets {
            let query_started = Instant::now();
            std::hint::black_box(query.dashboard(&filters(preset))?);
            timings.push(query_started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    timings.sort_by(f64::total_cmp);
    let p50 = percentile(&timings, 0.50);
    let p95 = percentile(&timings, 0.95);
    let max = timings.last().copied().unwrap_or_default();
    let report = BenchmarkReport {
        generated_at_ms: Utc::now().timestamp_millis(),
        codex_root: codex_root.to_string_lossy().into_owned(),
        database_path: database_path.to_string_lossy().into_owned(),
        initial_sync_ms,
        incremental_sync_ms,
        dashboard_available_during_import_ms,
        files_indexed: initial.files_completed,
        bytes_read: initial.bytes_read,
        records_written: initial.records_written,
        parse_failures: initial.parse_failures,
        file_errors: initial.file_errors,
        query_samples: timings.len(),
        query_p50_ms: p50,
        query_p95_ms: p95,
        query_max_ms: max,
        database_size_bytes: store.database_size_bytes()?,
        integrity_ok: store.integrity_check()?,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    eprintln!(
        "incremental files={} bytes={} failures={}",
        incremental.files_completed, incremental.bytes_read, incremental.parse_failures
    );
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

fn percentile(sorted: &[f64], percentile: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
    sorted[index]
}
