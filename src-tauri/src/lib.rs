#![allow(linker_messages)]

pub mod activity;
mod commands;
mod error;
pub mod export;
pub mod indexer;
pub mod pricing;
pub mod pricing_update;
pub mod query;
pub mod source;
pub mod store;

use std::sync::Arc;
use std::{
    fs,
    path::{Path, PathBuf},
};

use commands::RuntimeState;
use export::UsageExporter;
use indexer::{SyncMode, UsageIndexer, default_codex_root, system_timezone};
use pricing::BUILTIN_PRICING_REVISION;
use pricing_update::PriceUpdateService;
use source::FsCodexSource;
use store::UsageStore;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let database_path = resolve_analysis_database(&app.path().local_data_dir()?)
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            let store = UsageStore::open(database_path)
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            let store = Arc::new(store);
            let timezone = system_timezone();
            store
                .seed_builtin_prices()
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            let pricing_marker = store
                .app_setting("builtin-pricing-revision")
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            let expected_pricing_marker = BUILTIN_PRICING_REVISION.to_string();
            if pricing_marker.as_deref() != Some(expected_pricing_marker.as_str()) {
                store
                    .reprice_all(timezone)
                    .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
                store
                    .save_app_setting("builtin-pricing-revision", &expected_pricing_marker)
                    .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            }
            let pricing = store
                .pricing_catalog()
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            let initial_mode = if store
                .source_checkpoints()
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?
                .is_empty()
            {
                SyncMode::Initial
            } else {
                SyncMode::Incremental
            };
            let codex_root = default_codex_root()
                .map_err(|error| Box::<dyn std::error::Error>::from(error.to_string()))?;
            let idle_gap_ms = store
                .app_setting("activity")
                .ok()
                .flatten()
                .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok())
                .and_then(|value| {
                    value
                        .get("idleGapMinutes")
                        .and_then(serde_json::Value::as_u64)
                })
                .unwrap_or(30)
                .clamp(1, 240)
                * 60
                * 1000;
            let indexer = Arc::new(UsageIndexer::new(
                Arc::clone(&store),
                Arc::new(FsCodexSource::new(codex_root)),
                pricing,
                timezone,
                idle_gap_ms,
            ));
            let query = Arc::new(query::UsageQuery::new(Arc::clone(&store), timezone));
            let exporter = Arc::new(UsageExporter::new(Arc::clone(&query)));
            app.manage(RuntimeState {
                store: Arc::clone(&store),
                indexer: Arc::clone(&indexer),
                query,
                price_updates: Arc::new(PriceUpdateService::default()),
                exporter,
            });
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let result = indexer.sync(initial_mode);
                match result {
                    Ok(status) => {
                        let _ = app_handle.emit("usage-sync-completed", status);
                    }
                    Err(error) => {
                        let _ = app_handle.emit("usage-sync-failed", error);
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap_status,
            commands::get_sync_status,
            commands::get_dashboard,
            commands::get_workspaces,
            commands::get_workspace_catalog,
            commands::get_sessions,
            commands::get_models,
            commands::get_heatmap,
            commands::get_tools,
            commands::get_session_detail,
            commands::get_usage_events,
            commands::get_diagnostics,
            commands::cancel_sync,
            commands::sync_usage,
            commands::update_workspace_settings,
            commands::clear_analysis,
            commands::list_model_prices,
            commands::save_model_price,
            commands::delete_model_price,
            commands::restore_builtin_price,
            commands::preview_price_update,
            commands::apply_price_update,
            commands::export_data,
            commands::write_chart_png,
            commands::get_app_preferences,
            commands::save_app_preferences
        ])
        .run(tauri::generate_context!())
        .expect("Chronolume runtime failed");
}

const DATA_DIRECTORY: &str = "Chronolume";
const DATABASE_NAME: &str = "chronolume-v2.sqlite3";
const LEGACY_DATA_DIRECTORY: &str = "CodexUsage";
const LEGACY_DATABASE_NAME: &str = "codex-usage-v2.sqlite3";

/// Moves the 2.0 analytics database to the Chronolume namespace before it is opened.
/// The stable Tauri bundle identifier is intentionally retained so installed upgrades and
/// WebView preferences continue to belong to the same application.
fn resolve_analysis_database(local_data_root: &Path) -> std::io::Result<PathBuf> {
    let data_dir = local_data_root.join(DATA_DIRECTORY).join("v2");
    let legacy_data_dir = local_data_root.join(LEGACY_DATA_DIRECTORY).join("v2");

    if !data_dir.exists() && legacy_data_dir.exists() {
        if let Some(parent) = data_dir.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&legacy_data_dir, &data_dir)?;
    }
    fs::create_dir_all(&data_dir)?;

    let database_path = data_dir.join(DATABASE_NAME);
    let legacy_database_path = data_dir.join(LEGACY_DATABASE_NAME);
    let migration_started = legacy_database_path.exists()
        || (database_path.exists()
            && ["-wal", "-shm"].iter().any(|suffix| {
                data_dir
                    .join(format!("{LEGACY_DATABASE_NAME}{suffix}"))
                    .exists()
            }));
    if migration_started {
        for suffix in ["", "-wal", "-shm"] {
            let legacy = data_dir.join(format!("{LEGACY_DATABASE_NAME}{suffix}"));
            let current = data_dir.join(format!("{DATABASE_NAME}{suffix}"));
            if legacy.exists() && !current.exists() {
                fs::rename(legacy, current)?;
            }
        }
    }
    Ok(database_path)
}

#[cfg(test)]
mod brand_migration_tests {
    use super::*;

    #[test]
    fn migrates_legacy_database_directory_and_wal_sidecars() {
        let root = tempfile::tempdir().expect("temporary root");
        let legacy = root.path().join(LEGACY_DATA_DIRECTORY).join("v2");
        fs::create_dir_all(&legacy).expect("legacy data directory");
        fs::write(legacy.join(LEGACY_DATABASE_NAME), b"database").expect("database");
        fs::write(legacy.join(format!("{LEGACY_DATABASE_NAME}-wal")), b"wal").expect("wal");
        fs::write(legacy.join(format!("{LEGACY_DATABASE_NAME}-shm")), b"shm").expect("shm");

        let database = resolve_analysis_database(root.path()).expect("migrate database");

        assert_eq!(
            database,
            root.path()
                .join(DATA_DIRECTORY)
                .join("v2")
                .join(DATABASE_NAME)
        );
        assert_eq!(fs::read(&database).expect("migrated database"), b"database");
        assert_eq!(
            fs::read(database.with_file_name(format!("{DATABASE_NAME}-wal")))
                .expect("migrated wal"),
            b"wal"
        );
        assert_eq!(
            fs::read(database.with_file_name(format!("{DATABASE_NAME}-shm")))
                .expect("migrated shm"),
            b"shm"
        );
        assert!(!legacy.exists());
    }

    #[test]
    fn keeps_existing_chronolume_database_authoritative() {
        let root = tempfile::tempdir().expect("temporary root");
        let current = root.path().join(DATA_DIRECTORY).join("v2");
        let legacy = root.path().join(LEGACY_DATA_DIRECTORY).join("v2");
        fs::create_dir_all(&current).expect("current data directory");
        fs::create_dir_all(&legacy).expect("legacy data directory");
        fs::write(current.join(DATABASE_NAME), b"current").expect("current database");
        fs::write(legacy.join(LEGACY_DATABASE_NAME), b"legacy").expect("legacy database");

        let database = resolve_analysis_database(root.path()).expect("resolve database");

        assert_eq!(fs::read(database).expect("current database"), b"current");
        assert!(legacy.join(LEGACY_DATABASE_NAME).exists());
    }

    #[test]
    fn resumes_after_the_main_database_was_already_renamed() {
        let root = tempfile::tempdir().expect("temporary root");
        let current = root.path().join(DATA_DIRECTORY).join("v2");
        fs::create_dir_all(&current).expect("current data directory");
        fs::write(current.join(DATABASE_NAME), b"database").expect("current database");
        fs::write(current.join(format!("{LEGACY_DATABASE_NAME}-wal")), b"wal").expect("legacy wal");

        let database = resolve_analysis_database(root.path()).expect("resume migration");

        assert_eq!(fs::read(&database).expect("database"), b"database");
        assert_eq!(
            fs::read(database.with_file_name(format!("{DATABASE_NAME}-wal")))
                .expect("migrated wal"),
            b"wal"
        );
    }
}
