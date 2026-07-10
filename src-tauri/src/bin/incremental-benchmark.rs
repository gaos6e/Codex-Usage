use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use codex_usage_lib::indexer::{SyncMode, UsageIndexer, system_timezone};
use codex_usage_lib::query::UsageQuery;
use codex_usage_lib::source::FsCodexSource;
use codex_usage_lib::store::UsageStore;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IncrementalReport {
    elapsed_ms: f64,
    planned_files: u64,
    completed_files: u64,
    bytes_read: u64,
    records_written: u64,
    parse_failures: u64,
    file_errors: u64,
    run_errors: BTreeMap<String, u64>,
    source_errors: BTreeMap<String, u64>,
    database_size_bytes: u64,
    integrity_ok: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<_> = std::env::args_os().collect();
    if arguments.len() != 3 {
        return Err("usage: incremental-benchmark <codex-root> <existing-benchmark-db>".into());
    }
    let codex_root = PathBuf::from(&arguments[1]);
    let database_path = PathBuf::from(&arguments[2]);
    if !database_path.is_file() {
        return Err("benchmark database does not exist".into());
    }

    let store = Arc::new(UsageStore::open(&database_path)?);
    let timezone = system_timezone();
    let indexer = UsageIndexer::new(
        Arc::clone(&store),
        Arc::new(FsCodexSource::new(&codex_root)),
        store.pricing_catalog()?,
        timezone,
        30 * 60 * 1000,
    );
    let started = Instant::now();
    let status = indexer.sync(SyncMode::Incremental)?;
    let mut source_errors = BTreeMap::new();
    for source in UsageQuery::new(Arc::clone(&store), timezone)
        .diagnostics()?
        .sources
        .into_iter()
        .filter(|source| source.status == "error")
    {
        let key = format!(
            "{}:{}",
            source.kind,
            source.last_error_code.as_deref().unwrap_or("unknown")
        );
        *source_errors.entry(key).or_default() += 1;
    }
    let report = IncrementalReport {
        elapsed_ms: started.elapsed().as_secs_f64() * 1000.0,
        planned_files: status.files_total,
        completed_files: status.files_completed,
        bytes_read: status.bytes_read,
        records_written: status.records_written,
        parse_failures: status.parse_failures,
        file_errors: status.file_errors,
        run_errors: status.error_counts,
        source_errors,
        database_size_bytes: store.database_size_bytes()?,
        integrity_ok: store.integrity_check()?,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
