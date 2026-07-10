export type RangePreset =
  | 'today'
  | 'last24_hours'
  | 'last7_days'
  | 'last14_days'
  | 'last30_days'
  | 'last90_days'
  | 'all'
  | 'custom';

export type ArchiveFilter = 'all' | 'active' | 'archived';

export interface RangeSelection {
  preset: RangePreset;
  startMs?: number;
  endMs?: number;
  liveEnd: boolean;
}

export interface UsageFilters {
  range: RangeSelection;
  workspaceId?: string;
  modelProvider?: string;
  model?: string;
  archived: ArchiveFilter;
}

export interface ResolvedRange {
  startMs: number;
  endMs: number;
  startLocalDate: string;
  endLocalDate: string;
  calendarDays: number;
  granularity: 'hour' | 'day' | 'week';
}

export interface HeroMetrics {
  realTotalTokens: number;
  inputTokens: number;
  freshInputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  cacheHitRate?: number;
  estimatedCostMicrousd?: number;
  unpricedEventCount: number;
  sessionCount: number;
  activeMs: number;
  activeDays: number;
  averageTokensPerDay: number;
  averageCostMicrousdPerDay?: number;
  averageSessionsPerDay: number;
  averageActiveMsPerDay: number;
  peakDay?: string;
  peakDayTokens: number;
  longestActiveStreakDays: number;
}

export interface TrendPoint {
  key: string;
  timestampMs: number;
  inputTokens: number;
  freshInputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  totalTokens: number;
  estimatedCostMicrousd?: number;
  unpricedEventCount: number;
  sessionCount: number;
  activeMs: number;
}

export interface FilterOption {
  value: string;
  label: string;
}

export interface WorkspaceCatalogItem extends FilterOption {
  normalizedPath: string;
}

export interface DashboardSnapshot {
  resolvedRange: ResolvedRange;
  hero: HeroMetrics;
  trend: TrendPoint[];
  filterOptions: {
    workspaces: FilterOption[];
    providers: FilterOption[];
    models: FilterOption[];
  };
  dataState: 'complete' | 'partial' | 'empty';
  generatedAtMs: number;
}

export interface SourceStatus {
  kind: 'session' | 'archived_session' | 'state_db' | 'logs_db';
  exists: boolean;
  fileCount: number;
  totalBytes: number;
}

export interface SyncStatus {
  runId?: string;
  mode?: 'initial' | 'incremental' | 'rebuild' | 'repair';
  phase:
    | 'idle'
    | 'detecting'
    | 'planning'
    | 'importing'
    | 'rolling_up'
    | 'completed'
    | 'cancelled'
    | 'failed';
  filesTotal: number;
  filesCompleted: number;
  bytesTotal: number;
  bytesRead: number;
  recordsWritten: number;
  recordsSkipped: number;
  parseFailures: number;
  fileErrors: number;
  errorCounts?: Record<string, number>;
  speedBytesPerSecond: number;
  startedAtMs?: number;
  updatedAtMs: number;
  lastCompletedAtMs?: number;
  cancelRequested: boolean;
  capabilities?: {
    statuses: SourceStatus[];
    canReadSessionTokens: boolean;
    canReadSessionMetadata: boolean;
    canReadLogs: boolean;
  };
  errorCode?: string;
}

export interface BootstrapStatus {
  appVersion: string;
  databasePath: string;
  schemaVersion: number;
  databaseSizeBytes: number;
}

export interface Page<T> {
  items: T[];
  page: number;
  pageSize: number;
  total: number;
}

export interface ListQuery {
  filters: UsageFilters;
  /** 仅用于工作区列表；未提供表示不限，空数组表示不显示任何工作区。 */
  workspaceIds?: string[];
  search: string;
  sort: string;
  descending: boolean;
  page: number;
  pageSize: number;
}

export interface WorkspaceRow {
  id: string;
  label: string;
  normalizedPath: string;
  ignored: boolean;
  sessionCount: number;
  totalTokens: number;
  estimatedCostMicrousd?: number;
  unpricedEventCount: number;
  activeMs: number;
  activeDays: number;
  lastActivityAtMs?: number;
}

export interface SessionRow {
  id: string;
  title: string;
  workspaceId: string;
  workspaceLabel: string;
  startedAtMs?: number;
  endedAtMs?: number;
  activeMs: number;
  activeMethod: string;
  activeIsEstimate: boolean;
  modelProvider: string;
  latestModel: string;
  totalTokens: number;
  inputTokens: number;
  freshInputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  estimatedCostMicrousd?: number;
  unpricedEventCount: number;
  archived: boolean;
  integrityStatus: string;
}

export interface ModelRow {
  provider: string;
  model: string;
  pricingModelId?: string;
  sessionCount: number;
  inputTokens: number;
  freshInputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  totalTokens: number;
  cacheHitRate?: number;
  estimatedCostMicrousd?: number;
  unpricedEventCount: number;
  averageCostMicrousdPerMillionTokens?: number;
  lastUsedAtMs?: number;
}

export type HeatmapMetric = 'sessions' | 'tokens' | 'active_time';
export type HeatmapSpan = 'week' | 'month' | 'year';

export interface HeatmapPoint {
  date: string;
  value: number;
  sessionCount: number;
  totalTokens: number;
  activeMs: number;
}

export interface HeatmapSnapshot {
  metric: HeatmapMetric;
  span: HeatmapSpan;
  startDate: string;
  endDate: string;
  points: HeatmapPoint[];
  maxValue: number;
}

export interface ToolStat {
  toolName: string;
  category: string;
  operationKind: string;
  callCount: number;
  sessionCount: number;
}

export interface ToolsSnapshot {
  totalCalls: number;
  uniqueTools: number;
  topTools: ToolStat[];
  categories: Array<{ category: string; callCount: number }>;
  trend: Array<{ date: string; callCount: number }>;
}

export interface ModelSegmentRow {
  segmentIndex: number;
  startedAtMs: number;
  endedAtMs?: number;
  provider: string;
  model: string;
  inputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  estimatedCostMicrousd?: number;
  unpricedEventCount: number;
}

export interface ActivitySegmentRow {
  segmentIndex: number;
  startedAtMs: number;
  endedAtMs: number;
  activeMs: number;
  method: string;
  isEstimate: boolean;
}

export interface UsageEventRow {
  id: number;
  occurredAtMs: number;
  provider: string;
  model: string;
  inputTokens: number;
  cachedInputTokens: number;
  outputTokens: number;
  reasoningTokens: number;
  totalTokens: number;
  estimatedCostMicrousd?: number;
  integrityStatus: string;
}

export interface SessionDetail {
  session: SessionRow;
  parsing: {
    sourceKind: string;
    sourceStatus: string;
    parserVersion: number;
    warningCount: number;
    lastErrorCode?: string;
  };
  modelSegments: ModelSegmentRow[];
  activitySegments: ActivitySegmentRow[];
  tools: ToolStat[];
  recentUsageEvents: UsageEventRow[];
  retainedEventCount: number;
}

export interface DiagnosticSource {
  kind: string;
  relativePath: string;
  status: string;
  fileSize: number;
  safeOffset: number;
  logsRowidWatermark: number;
  parserVersion: number;
  lastErrorCode?: string;
  lastSeenAtMs: number;
}

export interface DiagnosticsSnapshot {
  databaseSizeBytes: number;
  databaseIntegrityOk: boolean;
  schemaVersion: number;
  parserVersion: number;
  indexedSessions: number;
  retainedUsageEvents: number;
  retainedToolEvents: number;
  sources: DiagnosticSource[];
  recentRuns: Array<{
    id: string;
    mode: string;
    status: string;
    stage: string;
    startedAtMs: number;
    finishedAtMs?: number;
    filesCompleted: number;
    filesTotal: number;
    bytesRead: number;
    recordsSkipped: number;
    elapsedMs?: number;
    parseFailures: number;
    errorCode?: string;
  }>;
  generatedAtMs: number;
}

export interface ModelPriceRecord {
  provider: string;
  pricingId: string;
  displayName: string;
  inputPerMillionUsd: string;
  outputPerMillionUsd: string;
  cacheReadPerMillionUsd: string;
  cacheWritePerMillionUsd?: string;
  isBuiltin: boolean;
  isOverridden: boolean;
  isDeleted: boolean;
  revision: number;
  sourceUrl?: string;
  sourceUpdatedAtMs?: number;
}

export type ModelPriceInput = Pick<
  ModelPriceRecord,
  | 'provider'
  | 'pricingId'
  | 'displayName'
  | 'inputPerMillionUsd'
  | 'outputPerMillionUsd'
  | 'cacheReadPerMillionUsd'
  | 'cacheWritePerMillionUsd'
>;

export interface PricingMutationResult {
  prices: ModelPriceRecord[];
  reprice: {
    eventsRepriced: number;
    modelSegmentsRepriced: number;
    dailyRowsRepriced: number;
  };
}

export interface PriceUpdatePreview {
  previewId: string;
  sourceUrl: string;
  fetchedAtMs: number;
  unchangedCount: number;
  changes: Array<{
    kind: 'added' | 'updated';
    pricingId: string;
    before?: {
      inputPerMillionUsd: string;
      outputPerMillionUsd: string;
      cacheReadPerMillionUsd: string;
      cacheWritePerMillionUsd?: string;
    };
    after: {
      inputPerMillionUsd: string;
      outputPerMillionUsd: string;
      cacheReadPerMillionUsd: string;
      cacheWritePerMillionUsd?: string;
    };
  }>;
}

export type ExportScope = 'dashboard' | 'workspaces' | 'sessions' | 'models' | 'tools';
export type ExportFormat = 'csv' | 'json';
export type ExportPrivacy = 'anonymous' | 'full_path';

export interface ExportRequest {
  format: ExportFormat;
  scope: ExportScope;
  privacy: ExportPrivacy;
  filters: UsageFilters;
}

export interface ExportResult {
  path: string;
  bytesWritten: number;
  rowsWritten: number;
}

export interface AppPreferences {
  idleGapMinutes: number;
  visibleWorkspaceIds: string[];
}
