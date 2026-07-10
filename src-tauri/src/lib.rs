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
            let data_dir = app.path().local_data_dir()?.join("CodexUsage").join("v2");
            let store = UsageStore::open(data_dir.join("codex-usage-v2.sqlite3"))
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
        .expect("Codex Usage runtime failed");
}
