import { invoke } from '@tauri-apps/api/core';
import type {
  BootstrapStatus,
  AppPreferences,
  DashboardSnapshot,
  DiagnosticsSnapshot,
  ExportRequest,
  ExportResult,
  HeatmapMetric,
  HeatmapSnapshot,
  HeatmapSpan,
  ListQuery,
  ModelPriceInput,
  ModelPriceRecord,
  ModelRow,
  Page,
  PriceUpdatePreview,
  PricingMutationResult,
  SessionDetail,
  SessionRow,
  SyncStatus,
  ToolsSnapshot,
  UsageFilters,
  UsageEventRow,
  WorkspaceRow,
  WorkspaceCatalogItem,
} from './types';

export function isTauriRuntime(): boolean {
  return '__TAURI_INTERNALS__' in window;
}

const emptySyncStatus: SyncStatus = {
  phase: 'idle',
  filesTotal: 0,
  filesCompleted: 0,
  bytesTotal: 0,
  bytesRead: 0,
  recordsWritten: 0,
  recordsSkipped: 0,
  parseFailures: 0,
  fileErrors: 0,
  speedBytesPerSecond: 0,
  updatedAtMs: Date.now(),
  cancelRequested: false,
};

export async function getBootstrapStatus(): Promise<BootstrapStatus> {
  if (!isTauriRuntime()) {
    return {
      appVersion: '2.1.0-dev',
      platform: 'browser',
      dataDirectory: '浏览器预览模式',
      databasePath: '浏览器预览模式',
      schemaVersion: 1,
      databaseSizeBytes: 0,
    };
  }
  return invoke<BootstrapStatus>('get_bootstrap_status');
}

export async function getSyncStatus(): Promise<SyncStatus> {
  return isTauriRuntime() ? invoke<SyncStatus>('get_sync_status') : emptySyncStatus;
}

export async function cancelSync(): Promise<SyncStatus> {
  return isTauriRuntime() ? invoke<SyncStatus>('cancel_sync') : emptySyncStatus;
}

export async function startSync(mode: 'incremental' | 'rebuild' | 'repair' = 'incremental'): Promise<SyncStatus> {
  return isTauriRuntime()
    ? invoke<SyncStatus>('sync_usage', { mode })
    : emptySyncStatus;
}

export async function getDashboard(filters: UsageFilters): Promise<DashboardSnapshot> {
  if (!isTauriRuntime()) {
    return {
      resolvedRange: {
        startMs: Date.now() - 30 * 86_400_000,
        endMs: Date.now(),
        startLocalDate: '',
        endLocalDate: '',
        calendarDays: 30,
        granularity: 'day',
      },
      hero: {
        realTotalTokens: 0,
        inputTokens: 0,
        freshInputTokens: 0,
        cachedInputTokens: 0,
        outputTokens: 0,
        reasoningTokens: 0,
        unpricedEventCount: 0,
        sessionCount: 0,
        activeMs: 0,
        activeDays: 0,
        averageTokensPerDay: 0,
        averageSessionsPerDay: 0,
        averageActiveMsPerDay: 0,
        peakDayTokens: 0,
        longestActiveStreakDays: 0,
      },
      trend: [],
      filterOptions: { workspaces: [], providers: [], models: [] },
      dataState: 'empty',
      generatedAtMs: Date.now(),
    };
  }
  return invoke<DashboardSnapshot>('get_dashboard', { filters });
}

const emptyPage = <T>(query: ListQuery): Page<T> => ({
  items: [], page: query.page, pageSize: query.pageSize, total: 0,
});

export async function getWorkspaces(query: ListQuery): Promise<Page<WorkspaceRow>> {
  return isTauriRuntime()
    ? invoke<Page<WorkspaceRow>>('get_workspaces', { query })
    : emptyPage(query);
}

export async function getWorkspaceCatalog(): Promise<WorkspaceCatalogItem[]> {
  return isTauriRuntime()
    ? invoke<WorkspaceCatalogItem[]>('get_workspace_catalog')
    : [];
}

export async function getSessions(query: ListQuery): Promise<Page<SessionRow>> {
  return isTauriRuntime()
    ? invoke<Page<SessionRow>>('get_sessions', { query })
    : emptyPage(query);
}

export async function getModels(query: ListQuery): Promise<Page<ModelRow>> {
  return isTauriRuntime()
    ? invoke<Page<ModelRow>>('get_models', { query })
    : emptyPage(query);
}

export async function getHeatmap(
  filters: UsageFilters,
  metric: HeatmapMetric,
  span: HeatmapSpan,
): Promise<HeatmapSnapshot> {
  if (!isTauriRuntime()) {
    return { metric, span, startDate: '', endDate: '', points: [], maxValue: 0 };
  }
  return invoke<HeatmapSnapshot>('get_heatmap', { query: { filters, metric, span } });
}

export async function getTools(filters: UsageFilters): Promise<ToolsSnapshot> {
  if (!isTauriRuntime()) {
    return { totalCalls: 0, uniqueTools: 0, topTools: [], categories: [], trend: [] };
  }
  return invoke<ToolsSnapshot>('get_tools', { filters });
}

export async function getSessionDetail(sessionId: string): Promise<SessionDetail> {
  return invoke<SessionDetail>('get_session_detail', { sessionId });
}

export async function getUsageEvents(
  sessionId: string,
  page: number,
  pageSize = 100,
): Promise<Page<UsageEventRow>> {
  return invoke<Page<UsageEventRow>>('get_usage_events', { sessionId, page, pageSize });
}

export async function getDiagnostics(): Promise<DiagnosticsSnapshot> {
  if (!isTauriRuntime()) {
    return {
      databaseSizeBytes: 0,
      databaseIntegrityOk: true,
      schemaVersion: 1,
      parserVersion: 1,
      indexedSessions: 0,
      retainedUsageEvents: 0,
      retainedToolEvents: 0,
      sources: [],
      recentRuns: [],
      generatedAtMs: Date.now(),
    };
  }
  return invoke<DiagnosticsSnapshot>('get_diagnostics');
}

export async function listModelPrices(includeDeleted = true): Promise<ModelPriceRecord[]> {
  return isTauriRuntime()
    ? invoke<ModelPriceRecord[]>('list_model_prices', { includeDeleted })
    : [];
}

export async function saveModelPrice(input: ModelPriceInput): Promise<PricingMutationResult> {
  return invoke<PricingMutationResult>('save_model_price', { input });
}

export async function deleteModelPrice(
  provider: string,
  pricingId: string,
): Promise<PricingMutationResult> {
  return invoke<PricingMutationResult>('delete_model_price', { provider, pricingId });
}

export async function restoreBuiltinPrice(
  provider: string,
  pricingId: string,
): Promise<PricingMutationResult> {
  return invoke<PricingMutationResult>('restore_builtin_price', { provider, pricingId });
}

export async function updateWorkspaceSettings(
  workspaceId: string,
  alias: string | undefined,
  ignored: boolean,
): Promise<void> {
  if (isTauriRuntime()) {
    await invoke('update_workspace_settings', { workspaceId, alias, ignored });
  }
}

export async function clearAnalysis(): Promise<void> {
  if (isTauriRuntime()) await invoke('clear_analysis');
}

export async function previewPriceUpdate(): Promise<PriceUpdatePreview> {
  return invoke<PriceUpdatePreview>('preview_price_update');
}

export async function applyPriceUpdate(previewId: string): Promise<PricingMutationResult> {
  return invoke<PricingMutationResult>('apply_price_update', { previewId });
}

export async function exportData(request: ExportRequest, path: string): Promise<ExportResult> {
  return invoke<ExportResult>('export_data', { request, path });
}

export async function writeChartPng(path: string, bytes: number[]): Promise<ExportResult> {
  return invoke<ExportResult>('write_chart_png', { path, bytes });
}

export async function getAppPreferences(): Promise<AppPreferences> {
  return isTauriRuntime()
    ? invoke<AppPreferences>('get_app_preferences')
    : { idleGapMinutes: 30, visibleWorkspaceIds: [] };
}

export async function saveAppPreferences(preferences: AppPreferences): Promise<AppPreferences> {
  return isTauriRuntime()
    ? invoke<AppPreferences>('save_app_preferences', { preferences })
    : preferences;
}
