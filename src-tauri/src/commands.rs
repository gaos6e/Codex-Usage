use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::export::{ExportRequest, ExportResult, UsageExporter};
use crate::indexer::{SyncMode, SyncStatus, UsageIndexer};
use crate::pricing_update::{PriceUpdatePreview, PriceUpdateService};
use crate::query::{
    DashboardSnapshot, DiagnosticsSnapshot, HeatmapQuery, HeatmapSnapshot, ListQuery, ModelRow,
    Page, SessionDetail, SessionRow, ToolsSnapshot, UsageEventRow, UsageFilters, UsageQuery,
    WorkspaceCatalogItem, WorkspaceRow,
};
use crate::store::{
    ModelPriceInput, ModelPriceRecord, RepriceResult, UsageStore, WorkspaceSettingsRecord,
};

pub struct RuntimeState {
    pub store: Arc<UsageStore>,
    pub indexer: Arc<UsageIndexer>,
    pub query: Arc<UsageQuery>,
    pub price_updates: Arc<PriceUpdateService>,
    pub exporter: Arc<UsageExporter>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapStatus {
    app_version: &'static str,
    platform: &'static str,
    data_directory: String,
    database_path: String,
    schema_version: i64,
    database_size_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingMutationResult {
    prices: Vec<ModelPriceRecord>,
    reprice: RepriceResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPreferences {
    #[serde(default = "default_idle_gap_minutes")]
    idle_gap_minutes: u64,
    #[serde(default)]
    visible_workspace_ids: Vec<String>,
}

const fn default_idle_gap_minutes() -> u64 {
    30
}

#[tauri::command]
pub fn get_bootstrap_status(state: State<'_, RuntimeState>) -> AppResult<BootstrapStatus> {
    Ok(BootstrapStatus {
        app_version: env!("CARGO_PKG_VERSION"),
        platform: crate::platform::platform_id(),
        data_directory: state
            .store
            .path()
            .parent()
            .unwrap_or_else(|| state.store.path())
            .to_string_lossy()
            .into_owned(),
        database_path: state.store.path().to_string_lossy().into_owned(),
        schema_version: state.store.schema_version()?,
        database_size_bytes: state.store.database_size_bytes()?,
    })
}

#[tauri::command]
pub fn get_sync_status(state: State<'_, RuntimeState>) -> SyncStatus {
    state.indexer.status()
}

#[tauri::command]
pub fn get_dashboard(
    filters: UsageFilters,
    state: State<'_, RuntimeState>,
) -> AppResult<DashboardSnapshot> {
    state.query.dashboard(&filters)
}

#[tauri::command]
pub fn get_workspaces(
    query: ListQuery,
    state: State<'_, RuntimeState>,
) -> AppResult<Page<WorkspaceRow>> {
    state.query.workspaces(&query)
}

#[tauri::command]
pub fn get_workspace_catalog(
    state: State<'_, RuntimeState>,
) -> AppResult<Vec<WorkspaceCatalogItem>> {
    state.query.workspace_catalog()
}

#[tauri::command]
pub fn get_sessions(
    query: ListQuery,
    state: State<'_, RuntimeState>,
) -> AppResult<Page<SessionRow>> {
    state.query.sessions(&query)
}

#[tauri::command]
pub fn get_models(query: ListQuery, state: State<'_, RuntimeState>) -> AppResult<Page<ModelRow>> {
    state.query.models(&query)
}

#[tauri::command]
pub fn get_heatmap(
    query: HeatmapQuery,
    state: State<'_, RuntimeState>,
) -> AppResult<HeatmapSnapshot> {
    state.query.heatmap(&query)
}

#[tauri::command]
pub fn get_tools(
    filters: UsageFilters,
    state: State<'_, RuntimeState>,
) -> AppResult<ToolsSnapshot> {
    state.query.tools(&filters)
}

#[tauri::command]
pub fn get_session_detail(
    session_id: String,
    state: State<'_, RuntimeState>,
) -> AppResult<SessionDetail> {
    state.query.session_detail(&session_id)
}

#[tauri::command]
pub fn get_usage_events(
    session_id: String,
    page: u32,
    page_size: u32,
    state: State<'_, RuntimeState>,
) -> AppResult<Page<UsageEventRow>> {
    state.query.usage_events(&session_id, page, page_size)
}

#[tauri::command]
pub fn get_diagnostics(state: State<'_, RuntimeState>) -> AppResult<DiagnosticsSnapshot> {
    state.query.diagnostics()
}

#[tauri::command]
pub fn update_workspace_settings(
    workspace_id: String,
    alias: Option<String>,
    ignored: bool,
    state: State<'_, RuntimeState>,
) -> AppResult<WorkspaceSettingsRecord> {
    state
        .store
        .update_workspace_settings(&workspace_id, alias.as_deref(), ignored)
}

#[tauri::command]
pub fn clear_analysis(state: State<'_, RuntimeState>) -> AppResult<()> {
    if matches!(
        state.indexer.status().phase,
        crate::indexer::SyncPhase::Detecting
            | crate::indexer::SyncPhase::Planning
            | crate::indexer::SyncPhase::Importing
            | crate::indexer::SyncPhase::RollingUp
    ) {
        return Err(AppError::new(
            "sync_in_progress",
            "请先取消并等待当前同步停止",
        ));
    }
    state.store.clear_analysis()
}

#[tauri::command]
pub fn cancel_sync(state: State<'_, RuntimeState>) -> SyncStatus {
    state.indexer.cancel()
}

#[tauri::command]
pub async fn sync_usage(mode: SyncMode, state: State<'_, RuntimeState>) -> AppResult<SyncStatus> {
    let indexer = Arc::clone(&state.indexer);
    tauri::async_runtime::spawn_blocking(move || indexer.sync(mode))
        .await
        .map_err(|_| AppError::new("sync_worker_failed", "后台同步任务异常退出"))?
}

#[tauri::command]
pub fn list_model_prices(
    include_deleted: bool,
    state: State<'_, RuntimeState>,
) -> AppResult<Vec<ModelPriceRecord>> {
    state.store.model_prices(include_deleted)
}

#[tauri::command]
pub fn save_model_price(
    input: ModelPriceInput,
    state: State<'_, RuntimeState>,
) -> AppResult<PricingMutationResult> {
    state.store.save_model_price(&input)?;
    finish_pricing_mutation(&state)
}

#[tauri::command]
pub fn delete_model_price(
    provider: String,
    pricing_id: String,
    state: State<'_, RuntimeState>,
) -> AppResult<PricingMutationResult> {
    state.store.delete_model_price(&provider, &pricing_id)?;
    finish_pricing_mutation(&state)
}

#[tauri::command]
pub fn restore_builtin_price(
    provider: String,
    pricing_id: String,
    state: State<'_, RuntimeState>,
) -> AppResult<PricingMutationResult> {
    state.store.restore_builtin_price(&provider, &pricing_id)?;
    finish_pricing_mutation(&state)
}

fn finish_pricing_mutation(state: &RuntimeState) -> AppResult<PricingMutationResult> {
    let timezone = crate::indexer::system_timezone();
    let reprice = state.store.reprice_all(timezone)?;
    state
        .indexer
        .replace_pricing(state.store.pricing_catalog()?);
    Ok(PricingMutationResult {
        prices: state.store.model_prices(true)?,
        reprice,
    })
}

#[tauri::command]
pub async fn preview_price_update(state: State<'_, RuntimeState>) -> AppResult<PriceUpdatePreview> {
    let prices = state.store.model_prices(true)?;
    let service = Arc::clone(&state.price_updates);
    service.preview(&prices).await
}

#[tauri::command]
pub fn apply_price_update(
    preview_id: String,
    state: State<'_, RuntimeState>,
) -> AppResult<PricingMutationResult> {
    let (fetched_at_ms, rows) = state.price_updates.take(&preview_id)?;
    state.store.apply_trusted_prices(&rows, fetched_at_ms)?;
    finish_pricing_mutation(&state)
}

#[tauri::command]
pub fn export_data(
    request: ExportRequest,
    path: String,
    state: State<'_, RuntimeState>,
) -> AppResult<ExportResult> {
    state.exporter.export_to_path(&request, &path)
}

#[tauri::command]
pub fn write_chart_png(path: String, bytes: Vec<u8>) -> AppResult<ExportResult> {
    const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() > 20 * 1024 * 1024 || !bytes.starts_with(PNG_SIGNATURE) {
        return Err(AppError::new("invalid_png", "PNG 数据无效或超出 20 MB"));
    }
    std::fs::write(&path, &bytes)?;
    Ok(ExportResult {
        path,
        bytes_written: bytes.len() as u64,
        rows_written: 1,
    })
}

#[tauri::command]
pub fn get_app_preferences(state: State<'_, RuntimeState>) -> AppResult<AppPreferences> {
    Ok(state
        .store
        .app_setting("activity")?
        .and_then(|value| serde_json::from_str::<AppPreferences>(&value).ok())
        .unwrap_or(AppPreferences {
            idle_gap_minutes: 30,
            visible_workspace_ids: Vec::new(),
        }))
}

#[tauri::command]
pub fn save_app_preferences(
    mut preferences: AppPreferences,
    state: State<'_, RuntimeState>,
) -> AppResult<AppPreferences> {
    if !(1..=240).contains(&preferences.idle_gap_minutes) {
        return Err(AppError::new(
            "invalid_idle_gap",
            "空闲间隔必须在 1 到 240 分钟之间",
        ));
    }
    preferences
        .visible_workspace_ids
        .retain(|id| !id.trim().is_empty());
    preferences.visible_workspace_ids.sort();
    preferences.visible_workspace_ids.dedup();
    let serialized = serde_json::to_string(&preferences)
        .map_err(|_| AppError::new("setting_serialization_failed", "设置序列化失败"))?;
    state.store.save_app_setting("activity", &serialized)?;
    state
        .indexer
        .set_idle_gap_ms(preferences.idle_gap_minutes * 60 * 1000);
    Ok(preferences)
}
