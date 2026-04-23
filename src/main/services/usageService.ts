import fs from 'fs';
import path from 'path';
import type {
  AppSettings,
  DailyUsageBucket,
  DiagnosticWarning,
  DiagnosticsSnapshot,
  MetricCard,
  ProjectDetail,
  RunRecord,
  TokenBreakdown,
  UsageFilters,
  UsageSnapshot,
  WorkspaceSummary,
} from '../../shared/contracts';
import { formatDuration, formatInteger, formatPercent, formatTokens } from '../../shared/formatting';
import { ALL_WORKSPACES_ID, anonymizePath } from '../../shared/pathUtils';
import {
  bucketKey,
  bucketLabel,
  isInsideRange,
  localDateKey,
  resolveTimeRange,
  ResolvedRange,
} from '../../shared/timeRange';
import { averagePerDay, cacheHitRateForBreakdown, calendarDaysInRange } from '../../shared/usageMath';
import type { AppPaths } from './appPaths';
import { detectSources, SourceDetection } from './sourceDetector';
import { readLogsDb } from './logsReader';
import { readSessions, SessionReadResult } from './sessionReader';
import { readStateDb, ThreadRecord } from './stateDbReader';

interface UsageData {
  generatedAt: string;
  threads: ThreadRecord[];
  runs: RunRecord[];
  workspaces: WorkspaceSummary[];
  diagnostics: DiagnosticsSnapshot;
  allTimeStart?: Date;
  allTimeEnd?: Date;
}

interface CacheEnvelope {
  version?: number;
  fingerprint: string;
  data: UsageData;
}

const CACHE_SCHEMA_VERSION = 3;

function reviveDate(value: unknown): Date | undefined {
  if (!value) {
    return undefined;
  }
  if (value instanceof Date) {
    return value;
  }
  if (typeof value === 'string' || typeof value === 'number') {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? undefined : date;
  }
  return undefined;
}

function hasBreakdownValues(breakdown: TokenBreakdown | undefined): boolean {
  return Boolean(
    breakdown
    && (
      breakdown.input !== undefined
      || breakdown.output !== undefined
      || breakdown.cached !== undefined
      || breakdown.reasoning !== undefined
    ),
  );
}

export class UsageService {
  private currentData: UsageData | null = null;
  private currentSettings: AppSettings | null = null;

  constructor(
    private readonly paths: AppPaths,
    private readonly getSettings: () => AppSettings,
  ) {}

  async refresh(force = false): Promise<UsageData> {
    const settings = this.getSettings();
    const started = Date.now();
    const detection = detectSources(settings);
    const fingerprint = this.createFingerprint(settings, detection);

    if (!force) {
      const cached = this.loadCache(fingerprint);
      if (cached) {
        this.currentData = cached;
        this.currentSettings = settings;
        return cached;
      }
    }

    const state = readStateDb(detection.stateDbPath, settings);
    const sessions = await readSessions(
      detection.sessionsDir,
      detection.archivedSessionsDir,
      settings.includeArchivedSessions,
      settings.idleGapMinutes,
    );
    const logs = readLogsDb(detection.logsDbPath, settings.includeDetailedLogs);

    const warnings: DiagnosticWarning[] = [
      ...detection.warnings,
      ...state.warnings,
      ...sessions.warnings.slice(0, 20),
      ...logs.warnings,
      {
        code: 'privacy_skipped',
        severity: 'info',
        messageKey: 'warning.privacySkipped',
      },
    ];
    if (sessions.warnings.length > 20) {
      warnings.push({
        code: 'parse_warning',
        severity: 'warning',
        messageKey: 'warning.additionalJsonlWarningsSuppressed',
        messageArgs: { count: sessions.warnings.length - 20 },
      });
    }

    const runs = this.createRuns(state.threads, sessions, logs.byThreadId, settings);
    const workspaces = this.createWorkspaces(runs, state.threads);
    const bounds = this.findBounds(runs);
    const diagnostics: DiagnosticsSnapshot = {
      codexDir: settings.codexDir,
      generatedAt: new Date().toISOString(),
      parseDurationMs: Date.now() - started,
      sources: detection.statuses.map((status) => {
        if (status.key === 'state') {
          return { ...status, rows: state.rows, columns: state.columns };
        }
        if (status.key === 'logs') {
          return { ...status, rows: logs.rows, columns: logs.columns };
        }
        return status;
      }),
      warnings,
      cacheStatus: 'rebuilt',
      appDataDir: this.paths.baseDir,
      logFilePath: this.paths.logFilePath,
    };

    const data: UsageData = {
      generatedAt: diagnostics.generatedAt,
      threads: state.threads,
      runs,
      workspaces,
      diagnostics,
      allTimeStart: bounds.start,
      allTimeEnd: bounds.end,
    };
    this.currentData = data;
    this.currentSettings = settings;
    this.saveCache({ version: CACHE_SCHEMA_VERSION, fingerprint, data });
    return data;
  }

  async getSnapshot(filters: UsageFilters): Promise<UsageSnapshot> {
    const data = this.currentData || await this.refresh(false);
    return this.buildSnapshot(data, filters);
  }

  getCachedSnapshot(filters: UsageFilters): UsageSnapshot | null {
    if (this.currentData) {
      return this.buildSnapshot(this.currentData, filters);
    }
    const cached = this.loadAnyCache();
    if (!cached) {
      return null;
    }
    this.currentData = cached;
    return this.buildSnapshot(cached, filters);
  }

  async getProjectDetail(workspaceId: string, filters: UsageFilters): Promise<ProjectDetail> {
    const data = this.currentData || await this.refresh(false);
    const snapshot = this.buildSnapshot(data, { ...filters, workspaceId });
    const workspace = data.workspaces.find((item) => item.id === workspaceId) || snapshot.selectedWorkspace;
    if (!workspace) {
      throw new Error('error.workspaceNotFound');
    }
    const runs = snapshot.runs;
    const modelMap = new Map<string, { model: string; tokens: number; runs: number }>();
    for (const run of runs) {
      const key = run.model || 'unknown';
      const current = modelMap.get(key) || { model: key, tokens: 0, runs: 0 };
      current.tokens += run.totalTokens;
      current.runs += 1;
      modelMap.set(key, current);
    }
    return {
      workspace,
      daily: snapshot.daily,
      runs,
      tokensByModel: [...modelMap.values()].sort((a, b) => b.tokens - a.tokens),
      dailyAverage: {
        calendarDays: snapshot.timeSummary.calendarDays,
        tokensPerDay: snapshot.tokenSummary.averageTokensPerDay,
        agentTimePerDayMs: snapshot.timeSummary.averageTimePerDayMs,
        runsPerDay: snapshot.timeSummary.averageRunsPerDay,
      },
      recentSessions: runs.slice(0, 10),
    };
  }

  async getDiagnostics(): Promise<DiagnosticsSnapshot> {
    const data = this.currentData || await this.refresh(false);
    return data.diagnostics;
  }

  private buildSnapshot(data: UsageData, filters: UsageFilters): UsageSnapshot {
    const range = resolveTimeRange(filters.range, data.allTimeStart, data.allTimeEnd);
    const scopedRuns = this.filterRuns(data.runs, filters.workspaceId, range);
    const daily = this.createDailyBuckets(scopedRuns, range);
    const selectedWorkspace = data.workspaces.find((workspace) => workspace.id === filters.workspaceId);
    const fixedRanges = {
      today: resolveTimeRange({ preset: 'today' }, data.allTimeStart, data.allTimeEnd),
      last7: resolveTimeRange({ preset: 'last7' }, data.allTimeStart, data.allTimeEnd),
      last30: resolveTimeRange({ preset: 'last30' }, data.allTimeStart, data.allTimeEnd),
      last90: resolveTimeRange({ preset: 'last90' }, data.allTimeStart, data.allTimeEnd),
      all: resolveTimeRange({ preset: 'all' }, data.allTimeStart, data.allTimeEnd),
    };

    const sumFor = (targetRange: ResolvedRange) =>
      this.filterRuns(data.runs, filters.workspaceId, targetRange).reduce((acc, run) => {
        acc.tokens += run.totalTokens;
        acc.agentTimeMs += run.durationMs;
        acc.runs += 1;
        return acc;
      }, { tokens: 0, agentTimeMs: 0, runs: 0 });

    const selected = scopedRuns.reduce((acc, run) => {
      acc.tokens += run.totalTokens;
      acc.agentTimeMs += run.durationMs;
      acc.runs += 1;
      return acc;
    }, { tokens: 0, agentTimeMs: 0, runs: 0 });
    const today = sumFor(fixedRanges.today);
    const last7 = sumFor(fixedRanges.last7);
    const last30 = sumFor(fixedRanges.last30);
    const last90 = sumFor(fixedRanges.last90);
    const all = sumFor(fixedRanges.all);
    const calendarDays = calendarDaysInRange(range);
    const activeDays = new Set(scopedRuns.map((run) => localDateKey(new Date(run.startTime)))).size;
    const peakByTime = [...daily].sort((a, b) => b.agentTimeMs - a.agentTimeMs)[0];
    const peakByTokens = [...daily].sort((a, b) => b.tokens - a.tokens)[0];
    const longestStreakDays = this.longestStreak(scopedRuns);
    const averageTimePerRunMs = scopedRuns.length ? selected.agentTimeMs / scopedRuns.length : 0;
    const averageTimePerActiveDayMs = activeDays ? selected.agentTimeMs / activeDays : 0;
    const averageTimePerDayMs = averagePerDay(selected.agentTimeMs, calendarDays);
    const averageRunsPerDay = averagePerDay(selected.runs, calendarDays);
    const averageTokensPerRun = scopedRuns.length ? selected.tokens / scopedRuns.length : 0;
    const averageTokensPerDay = averagePerDay(selected.tokens, calendarDays);
    const breakdown = this.sumBreakdown(scopedRuns);
    const cacheHitRate = cacheHitRateForBreakdown(breakdown);

    const timeCards: MetricCard[] = [
      { id: 'selected-time', labelKey: 'metric.agentTime', value: formatDuration(selected.agentTimeMs), sublabel: range.label, tone: 'blue' },
      { id: 'runs', labelKey: 'metric.runs', value: formatInteger(selected.runs), sublabelKey: 'metric.selectedRange' },
      { id: 'avg-day-time', labelKey: 'metric.avgTimePerDay', value: formatDuration(averageTimePerDayMs), sublabelKey: 'metric.calendarDays', sublabelArgs: { count: calendarDays } },
      { id: 'active-days', labelKey: 'metric.activeDays', value: formatInteger(activeDays), sublabelKey: 'metric.longestStreakDays', sublabelArgs: { count: longestStreakDays } },
      { id: 'last7-time', labelKey: 'metric.last7Days', value: formatDuration(last7.agentTimeMs) },
      { id: 'last30-time', labelKey: 'metric.last30Days', value: formatDuration(last30.agentTimeMs) },
      { id: 'last90-time', labelKey: 'metric.last90Days', value: formatDuration(last90.agentTimeMs) },
      { id: 'all-time', labelKey: 'metric.allTime', value: formatDuration(all.agentTimeMs) },
    ];
    const tokenCards: MetricCard[] = [
      { id: 'selected-tokens', labelKey: 'metric.tokens', value: formatTokens(selected.tokens), sublabel: range.label, tone: 'blue' },
      { id: 'today-tokens', labelKey: 'metric.today', value: formatTokens(today.tokens) },
      { id: 'avg-tokens', labelKey: 'metric.avgTokensPerRun', value: formatTokens(averageTokensPerRun) },
      { id: 'avg-day-tokens', labelKey: 'metric.avgTokensPerDay', value: formatTokens(averageTokensPerDay), sublabelKey: 'metric.calendarDays', sublabelArgs: { count: calendarDays } },
      cacheHitRate !== undefined
        ? {
            id: 'token-cache-hit-rate',
            labelKey: 'metric.tokenCacheHitRate',
            value: formatPercent(cacheHitRate),
            sublabelKey: 'metric.currentRangeCachedInputTokens',
            sublabelArgs: {
              cached: formatTokens(breakdown.cached || 0),
              input: formatTokens(breakdown.input || 0),
            },
            tone: 'success',
          }
        : { id: 'token-cache-hit-rate', labelKey: 'metric.tokenCacheHitRate', valueKey: 'metric.unavailable', tone: 'warning' },
      { id: 'last7-tokens', labelKey: 'metric.last7Days', value: formatTokens(last7.tokens) },
      { id: 'last30-tokens', labelKey: 'metric.last30Days', value: formatTokens(last30.tokens) },
      { id: 'last90-tokens', labelKey: 'metric.last90Days', value: formatTokens(last90.tokens) },
      { id: 'all-tokens', labelKey: 'metric.allTime', value: formatTokens(all.tokens) },
    ];

    return {
      generatedAt: new Date().toISOString(),
      filters,
      workspaces: data.workspaces,
      selectedWorkspace,
      cards: filters.view === 'time' ? timeCards : tokenCards,
      daily,
      runs: scopedRuns.sort((a, b) => new Date(b.startTime).getTime() - new Date(a.startTime).getTime()).slice(0, 250),
      tokenSummary: {
        selectedTokens: selected.tokens,
        todayTokens: today.tokens,
        last7Tokens: last7.tokens,
        last30Tokens: last30.tokens,
        last90Tokens: last90.tokens,
        allTimeTokens: all.tokens,
        averageTokensPerRun,
        averageTokensPerDay,
        cacheHitRate,
        peakDay: peakByTokens,
        breakdown,
      },
      timeSummary: {
        selectedAgentTimeMs: selected.agentTimeMs,
        last7AgentTimeMs: last7.agentTimeMs,
        last30AgentTimeMs: last30.agentTimeMs,
        last90AgentTimeMs: last90.agentTimeMs,
        allTimeAgentTimeMs: all.agentTimeMs,
        runs: selected.runs,
        averageTimePerRunMs,
        averageTimePerActiveDayMs,
        averageTimePerDayMs,
        averageRunsPerDay,
        calendarDays,
        peakDay: peakByTime,
        longestStreakDays,
        activeDays,
        methodology: 'methodology.estimatedFromLocalSessions',
      },
      diagnostics: data.diagnostics,
    };
  }

  private createRuns(
    threads: ThreadRecord[],
    sessions: SessionReadResult,
    logBreakdowns: Map<string, TokenBreakdown>,
    settings: AppSettings,
  ): RunRecord[] {
    return threads
      .filter((thread) => settings.includeArchivedSessions || !thread.archived)
      .map((thread) => {
        const fileStem = thread.rolloutPath ? path.basename(thread.rolloutPath, '.jsonl') : undefined;
        const timing = sessions.sessionsById.get(thread.id) || (fileStem ? sessions.sessionsByFileStem.get(fileStem) : undefined);
        const jsonlStart = timing?.start;
        const jsonlEnd = timing?.end;
        const durationMethod = timing?.activeMs && timing.activeMs > 0 ? 'jsonl-events' : 'thread-span';
        const start = jsonlStart || thread.createdAt;
        const end = jsonlEnd || thread.updatedAt;
        const fallbackDuration = Math.max(0, end.getTime() - start.getTime());
        const maxSingleRunMs = settings.idleGapMinutes * 60 * 1000 * 24;
        const durationMs = durationMethod === 'jsonl-events'
          ? Number(timing?.activeMs || 0)
          : Math.min(fallbackDuration, maxSingleRunMs);
        const jsonlBreakdown = timing?.tokenBreakdown;
        const logBreakdown = logBreakdowns.get(thread.id);
        const breakdown = this.resolveRunBreakdown(jsonlBreakdown, logBreakdown);
        return {
          id: thread.id,
          title: thread.title,
          workspaceId: thread.workspaceId,
          workspaceName: thread.workspaceName,
          workspacePath: thread.normalizedPath,
          startTime: start.toISOString(),
          endTime: end.toISOString(),
          durationMs,
          durationMethod,
          model: thread.model,
          modelProvider: thread.modelProvider,
          totalTokens: thread.tokensUsed,
          tokenBreakdown: breakdown || { unavailableReason: 'Breakdown unavailable for this run.' },
          rolloutPath: thread.rolloutPath,
          archived: thread.archived,
        };
      });
  }

  private createWorkspaces(runs: RunRecord[], threads: ThreadRecord[]): WorkspaceSummary[] {
    const map = new Map<string, WorkspaceSummary>();
    for (const thread of threads) {
      if (!map.has(thread.workspaceId)) {
        map.set(thread.workspaceId, {
          id: thread.workspaceId,
          displayName: thread.workspaceName,
          normalizedPath: thread.normalizedPath,
          rawPath: thread.cwd,
          runs: 0,
          tokens: 0,
          agentTimeMs: 0,
          activeDays: 0,
          hidden: thread.hidden,
        });
      }
    }
    for (const run of runs) {
      const current = map.get(run.workspaceId);
      if (!current) {
        continue;
      }
      current.runs += 1;
      current.tokens += run.totalTokens;
      current.agentTimeMs += run.durationMs;
      current.lastActivity = !current.lastActivity || run.startTime > current.lastActivity ? run.startTime : current.lastActivity;
    }
    const activeDaysByWorkspace = new Map<string, Set<string>>();
    for (const run of runs) {
      const set = activeDaysByWorkspace.get(run.workspaceId) || new Set<string>();
      set.add(localDateKey(new Date(run.startTime)));
      activeDaysByWorkspace.set(run.workspaceId, set);
    }
    for (const workspace of map.values()) {
      workspace.activeDays = activeDaysByWorkspace.get(workspace.id)?.size || 0;
    }
    return [...map.values()]
      .filter((workspace) => !workspace.hidden)
      .sort((a, b) => b.tokens + b.agentTimeMs - (a.tokens + a.agentTimeMs));
  }

  private filterRuns(runs: RunRecord[], workspaceId: string, range: ResolvedRange): RunRecord[] {
    return runs.filter((run) => {
      const workspaceMatches = workspaceId === ALL_WORKSPACES_ID || run.workspaceId === workspaceId;
      return workspaceMatches && isInsideRange(new Date(run.startTime), range);
    });
  }

  private createDailyBuckets(runs: RunRecord[], range: ResolvedRange): DailyUsageBucket[] {
    const map = new Map<string, DailyUsageBucket>();
    for (const run of runs) {
      const date = new Date(run.startTime);
      const key = bucketKey(date, range.aggregation);
      const current = map.get(key) || {
        key,
        label: bucketLabel(key, range.aggregation),
        start: date.toISOString(),
        end: date.toISOString(),
        runs: 0,
        tokens: 0,
        agentTimeMs: 0,
        activeProjects: 0,
        tokenBreakdown: {},
      };
      current.runs += 1;
      current.tokens += run.totalTokens;
      current.agentTimeMs += run.durationMs;
      if (run.tokenBreakdown) {
        current.tokenBreakdown = this.mergeBreakdown(current.tokenBreakdown || {}, run.tokenBreakdown);
      }
      map.set(key, current);
    }

    const projectsByBucket = new Map<string, Set<string>>();
    for (const run of runs) {
      const key = bucketKey(new Date(run.startTime), range.aggregation);
      const set = projectsByBucket.get(key) || new Set<string>();
      set.add(run.workspaceId);
      projectsByBucket.set(key, set);
    }

    for (const bucket of map.values()) {
      bucket.activeProjects = projectsByBucket.get(bucket.key)?.size || 0;
      const topWorkspace = this.topWorkspaceForBucket(runs, bucket.key, range.aggregation);
      bucket.topWorkspace = topWorkspace;
    }

    return [...map.values()].sort((a, b) => a.key.localeCompare(b.key));
  }

  private topWorkspaceForBucket(runs: RunRecord[], key: string, aggregation: 'daily' | 'weekly'): string | undefined {
    const map = new Map<string, { name: string; tokens: number; time: number }>();
    for (const run of runs) {
      if (bucketKey(new Date(run.startTime), aggregation) !== key) {
        continue;
      }
      const current = map.get(run.workspaceId) || { name: run.workspaceName, tokens: 0, time: 0 };
      current.tokens += run.totalTokens;
      current.time += run.durationMs;
      map.set(run.workspaceId, current);
    }
    return [...map.values()].sort((a, b) => b.tokens + b.time - (a.tokens + a.time))[0]?.name;
  }

  private longestStreak(runs: RunRecord[]): number {
    const days = [...new Set(runs.map((run) => localDateKey(new Date(run.startTime))))].sort();
    let longest = 0;
    let current = 0;
    let previous: Date | null = null;
    for (const key of days) {
      const date = new Date(`${key}T00:00:00`);
      if (previous && date.getTime() - previous.getTime() === 24 * 60 * 60 * 1000) {
        current += 1;
      } else {
        current = 1;
      }
      longest = Math.max(longest, current);
      previous = date;
    }
    return longest;
  }

  private sumBreakdown(runs: RunRecord[]): TokenBreakdown {
    return runs.reduce((acc, run) => this.mergeBreakdown(acc, run.tokenBreakdown || {}), {} as TokenBreakdown);
  }

  private mergeBreakdown(a: TokenBreakdown, b: TokenBreakdown): TokenBreakdown {
    const merged: TokenBreakdown = {
      input: (a.input || 0) + (b.input || 0),
      output: (a.output || 0) + (b.output || 0),
      cached: (a.cached || 0) + (b.cached || 0),
      reasoning: (a.reasoning || 0) + (b.reasoning || 0),
      unavailableReason: b.unavailableReason || a.unavailableReason,
    };
    merged.cacheHitRate = cacheHitRateForBreakdown(merged);
    return merged;
  }

  private resolveRunBreakdown(primary?: TokenBreakdown, fallback?: TokenBreakdown): TokenBreakdown | undefined {
    if (!hasBreakdownValues(primary) && !hasBreakdownValues(fallback)) {
      return undefined;
    }
    const resolved: TokenBreakdown = {
      input: primary?.input ?? fallback?.input,
      output: primary?.output ?? fallback?.output,
      cached: primary?.cached ?? fallback?.cached,
      reasoning: primary?.reasoning ?? fallback?.reasoning,
      unavailableReason: primary?.unavailableReason || fallback?.unavailableReason,
    };
    resolved.cacheHitRate = cacheHitRateForBreakdown(resolved);
    return resolved;
  }

  private findBounds(runs: RunRecord[]): { start?: Date; end?: Date } {
    if (!runs.length) {
      return {};
    }
    const dates = runs.flatMap((run) => [new Date(run.startTime), new Date(run.endTime)]);
    return {
      start: new Date(Math.min(...dates.map((date) => date.getTime()))),
      end: new Date(Math.max(...dates.map((date) => date.getTime()))),
    };
  }

  private createFingerprint(settings: AppSettings, detection: SourceDetection): string {
    const statPart = (target: string) => {
      try {
        const stat = fs.statSync(target);
        return `${target}:${stat.size}:${stat.mtimeMs}`;
      } catch {
        return `${target}:missing`;
      }
    };
    const sourceParts = [
      statPart(detection.stateDbPath),
      statPart(detection.logsDbPath),
      statPart(detection.sessionsDir),
      statPart(detection.archivedSessionsDir),
      JSON.stringify({
        tokenBreakdownParserVersion: 3,
        codexDir: settings.codexDir,
        includeArchivedSessions: settings.includeArchivedSessions,
        includeDetailedLogs: settings.includeDetailedLogs,
        idleGapMinutes: settings.idleGapMinutes,
        aliases: settings.aliases,
        ignoredWorkspaces: settings.ignoredWorkspaces,
      }),
    ];
    return Buffer.from(sourceParts.join('|')).toString('base64');
  }

  private loadCache(fingerprint: string): UsageData | null {
    try {
      if (!fs.existsSync(this.paths.cachePath)) {
        return null;
      }
      const envelope = JSON.parse(fs.readFileSync(this.paths.cachePath, 'utf8')) as CacheEnvelope;
      if (envelope.version !== CACHE_SCHEMA_VERSION) {
        return null;
      }
      if (envelope.fingerprint !== fingerprint) {
        return null;
      }
      return this.hydrateCachedData(envelope.data, 'fresh');
    } catch {
      return null;
    }
  }

  private loadAnyCache(): UsageData | null {
    try {
      if (!fs.existsSync(this.paths.cachePath)) {
        return null;
      }
      const envelope = JSON.parse(fs.readFileSync(this.paths.cachePath, 'utf8')) as CacheEnvelope;
      if (envelope.version !== CACHE_SCHEMA_VERSION) {
        return null;
      }
      return this.hydrateCachedData(envelope.data, 'fresh');
    } catch {
      return null;
    }
  }

  private hydrateCachedData(data: UsageData, cacheStatus: DiagnosticsSnapshot['cacheStatus']): UsageData {
    return {
      ...data,
      threads: data.threads.map((thread) => ({
        ...thread,
        createdAt: reviveDate(thread.createdAt) || new Date(0),
        updatedAt: reviveDate(thread.updatedAt) || new Date(0),
      })),
      allTimeStart: reviveDate(data.allTimeStart),
      allTimeEnd: reviveDate(data.allTimeEnd),
      diagnostics: {
        ...data.diagnostics,
        cacheStatus,
      },
    };
  }

  private saveCache(envelope: CacheEnvelope): void {
    try {
      fs.writeFileSync(this.paths.cachePath, JSON.stringify(envelope, null, 2), 'utf8');
    } catch {
      // Cache failure is non-fatal.
    }
  }

  anonymizeRuns(runs: RunRecord[]): RunRecord[] {
    const workspaceIndex = new Map<string, number>();
    return runs.map((run) => {
      if (!workspaceIndex.has(run.workspaceId)) {
        workspaceIndex.set(run.workspaceId, workspaceIndex.size);
      }
      const alias = anonymizePath(run.workspacePath, workspaceIndex.get(run.workspaceId) || 0);
      return {
        ...run,
        workspaceName: alias,
        workspacePath: alias,
        rolloutPath: undefined as string | undefined,
      };
    });
  }
}
