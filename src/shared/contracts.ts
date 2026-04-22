export type PageId = 'dashboard' | 'project' | 'settings' | 'diagnostics';

export type MetricView = 'time' | 'tokens';

export type TimeRangePreset =
  | 'today'
  | 'last7'
  | 'last30'
  | 'last90'
  | 'all'
  | 'custom';

export type AggregationMode = 'daily' | 'weekly';

export type ThemeMode = 'system' | 'light' | 'dark';
export type LanguageCode = 'zh-CN' | 'en';

export type ExportKind = 'daily-csv' | 'runs-csv' | 'summary-json' | 'dashboard-png';

export type ExportPrivacyMode = 'full-paths' | 'anonymized-paths';

export interface PathAliasRule {
  id: string;
  from: string;
  to: string;
  enabled: boolean;
}

export interface AppSettings {
  codexDir: string;
  includeArchivedSessions: boolean;
  includeDetailedLogs: boolean;
  autoRefreshSeconds: number;
  idleGapMinutes: number;
  theme: ThemeMode;
  language: LanguageCode;
  aliases: PathAliasRule[];
  ignoredWorkspaces: string[];
}

export interface TimeRangeFilter {
  preset: TimeRangePreset;
  startDate?: string;
  endDate?: string;
  aggregation?: AggregationMode;
}

export interface UsageFilters {
  workspaceId: string;
  view: MetricView;
  range: TimeRangeFilter;
}

export interface SourceStatus {
  key: 'state' | 'sessions' | 'archivedSessions' | 'logs';
  label: string;
  path: string;
  exists: boolean;
  rows?: number;
  files?: number;
  earliest?: string;
  latest?: string;
  columns?: string[];
  warning?: string;
}

export interface DiagnosticWarning {
  code:
    | 'missing_source'
    | 'schema_mismatch'
    | 'parse_warning'
    | 'partial_data'
    | 'privacy_skipped'
    | 'cache_stale'
    | 'export_warning';
  severity: 'info' | 'warning' | 'error';
  messageKey: string;
  messageArgs?: Record<string, string | number | boolean | null | undefined>;
  source?: string;
}

export interface DiagnosticsSnapshot {
  codexDir: string;
  generatedAt: string;
  parseDurationMs: number;
  sources: SourceStatus[];
  warnings: DiagnosticWarning[];
  cacheStatus: 'fresh' | 'rebuilt' | 'unavailable';
  appDataDir: string;
  logFilePath: string;
}

export interface WorkspaceSummary {
  id: string;
  displayName: string;
  normalizedPath: string;
  rawPath: string;
  runs: number;
  tokens: number;
  agentTimeMs: number;
  activeDays: number;
  lastActivity?: string;
  hidden: boolean;
}

export interface TokenBreakdown {
  input?: number;
  output?: number;
  cached?: number;
  reasoning?: number;
  cacheHitRate?: number;
  unavailableReason?: string;
}

export interface RunRecord {
  id: string;
  title: string;
  workspaceId: string;
  workspaceName: string;
  workspacePath: string;
  startTime: string;
  endTime: string;
  durationMs: number;
  durationMethod: 'jsonl-events' | 'thread-span' | 'unknown';
  model: string;
  modelProvider: string;
  totalTokens: number;
  tokenBreakdown?: TokenBreakdown;
  rolloutPath?: string;
  archived: boolean;
}

export interface DailyUsageBucket {
  key: string;
  label: string;
  start: string;
  end: string;
  runs: number;
  tokens: number;
  agentTimeMs: number;
  activeProjects: number;
  topWorkspace?: string;
  tokenBreakdown?: TokenBreakdown;
}

export interface MetricCard {
  id: string;
  labelKey: string;
  value: string;
  sublabelKey?: string;
  sublabel?: string;
  sublabelArgs?: Record<string, string | number | boolean | null | undefined>;
  tone?: 'default' | 'blue' | 'warning' | 'success';
}

export interface UsageSnapshot {
  generatedAt: string;
  filters: UsageFilters;
  workspaces: WorkspaceSummary[];
  selectedWorkspace?: WorkspaceSummary;
  cards: MetricCard[];
  daily: DailyUsageBucket[];
  runs: RunRecord[];
  tokenSummary: {
    selectedTokens: number;
    todayTokens: number;
    last7Tokens: number;
    last30Tokens: number;
    last90Tokens: number;
    allTimeTokens: number;
    averageTokensPerRun: number;
    peakDay?: DailyUsageBucket;
    breakdown?: TokenBreakdown;
  };
  timeSummary: {
    selectedAgentTimeMs: number;
    last7AgentTimeMs: number;
    last30AgentTimeMs: number;
    last90AgentTimeMs: number;
    allTimeAgentTimeMs: number;
    runs: number;
    averageTimePerRunMs: number;
    averageTimePerActiveDayMs: number;
    peakDay?: DailyUsageBucket;
    longestStreakDays: number;
    activeDays: number;
    methodology: string;
  };
  diagnostics: DiagnosticsSnapshot;
}

export interface ProjectDetail {
  workspace: WorkspaceSummary;
  daily: DailyUsageBucket[];
  runs: RunRecord[];
  tokensByModel: Array<{ model: string; tokens: number; runs: number }>;
  peakDays: DailyUsageBucket[];
  recentSessions: RunRecord[];
}

export interface ExportRequest {
  kind: ExportKind;
  privacyMode: ExportPrivacyMode;
  targetPath: string;
  filters: UsageFilters;
}

export interface ExportResult {
  ok: boolean;
  path?: string;
  messageKey: string;
  messageArgs?: Record<string, string | number | boolean | null | undefined>;
}

export interface AppInfo {
  appName: string;
  version: string;
  appDataDir: string;
}

export interface CodexUsageApi {
  getSettings: () => Promise<AppSettings>;
  saveSettings: (settings: AppSettings) => Promise<AppSettings>;
  chooseCodexDirectory: () => Promise<string | null>;
  chooseExportPath: (kind: ExportKind) => Promise<string | null>;
  getCachedUsageSnapshot: (filters: UsageFilters) => Promise<UsageSnapshot | null>;
  getUsageSnapshot: (filters: UsageFilters) => Promise<UsageSnapshot>;
  getProjectDetail: (workspaceId: string, filters: UsageFilters) => Promise<ProjectDetail>;
  getDiagnostics: () => Promise<DiagnosticsSnapshot>;
  refreshUsage: () => Promise<UsageSnapshot>;
  exportUsage: (request: ExportRequest) => Promise<ExportResult>;
  getAppInfo: () => Promise<AppInfo>;
}
