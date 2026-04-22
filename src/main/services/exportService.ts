import fs from 'fs';
import { BrowserWindow } from 'electron';
import type { ExportRequest, ExportResult, UsageSnapshot } from '../../shared/contracts';
import { UsageService } from './usageService';

function escapeCsv(value: unknown): string {
  const text = String(value ?? '');
  if (/[",\n]/.test(text)) {
    return `"${text.replace(/"/g, '""')}"`;
  }
  return text;
}

function rowsToCsv(headers: string[], rows: Array<Array<unknown>>): string {
  return [
    headers.map(escapeCsv).join(','),
    ...rows.map((row) => row.map(escapeCsv).join(',')),
  ].join('\n');
}

export class ExportService {
  constructor(private readonly usageService: UsageService) {}

  async exportUsage(request: ExportRequest): Promise<ExportResult> {
    try {
      if (request.kind === 'dashboard-png') {
        return await this.exportScreenshot(request.targetPath);
      }
      const snapshot = await this.usageService.getSnapshot(request.filters);
      if (request.kind === 'daily-csv') {
        return this.writeDailyCsv(snapshot, request);
      }
      if (request.kind === 'runs-csv') {
        return this.writeRunsCsv(snapshot, request);
      }
      return this.writeSummaryJson(snapshot, request);
    } catch (error) {
      return {
        ok: false,
        messageKey: 'export.failed',
        messageArgs: { detail: error instanceof Error ? error.message : String(error) },
      };
    }
  }

  private writeDailyCsv(snapshot: UsageSnapshot, request: ExportRequest): ExportResult {
    const rows = snapshot.daily.map((bucket) => [
      bucket.key,
      bucket.runs,
      bucket.tokens,
      Math.round(bucket.agentTimeMs / 60000),
      bucket.activeProjects,
      request.privacyMode === 'full-paths' ? bucket.topWorkspace || '' : bucket.topWorkspace ? 'Workspace' : '',
    ]);
    fs.writeFileSync(
      request.targetPath,
      rowsToCsv(['date', 'runs', 'tokens', 'agent_time_minutes', 'active_projects', 'top_workspace'], rows),
      'utf8',
    );
    return { ok: true, path: request.targetPath, messageKey: 'export.dailyCsvSuccess' };
  }

  private writeRunsCsv(snapshot: UsageSnapshot, request: ExportRequest): ExportResult {
    const runs = request.privacyMode === 'full-paths'
      ? snapshot.runs
      : this.usageService.anonymizeRuns(snapshot.runs);
    const rows = runs.map((run) => [
      run.id,
      run.title,
      run.workspaceName,
      run.workspacePath,
      run.startTime,
      run.endTime,
      Math.round(run.durationMs / 60000),
      run.model,
      run.totalTokens,
      run.archived,
      request.privacyMode === 'full-paths' ? run.rolloutPath || '' : '',
    ]);
    fs.writeFileSync(
      request.targetPath,
      rowsToCsv(
        ['id', 'title', 'workspace', 'workspace_path', 'start_time', 'end_time', 'duration_minutes', 'model', 'tokens', 'archived', 'rollout_path'],
        rows,
      ),
      'utf8',
    );
    return { ok: true, path: request.targetPath, messageKey: 'export.runsCsvSuccess' };
  }

  private writeSummaryJson(snapshot: UsageSnapshot, request: ExportRequest): ExportResult {
    const safeSnapshot = request.privacyMode === 'full-paths'
      ? snapshot
      : {
          ...snapshot,
          runs: this.usageService.anonymizeRuns(snapshot.runs),
          workspaces: snapshot.workspaces.map((workspace, index) => ({
            ...workspace,
            displayName: `Workspace ${index + 1}`,
            normalizedPath: `Workspace ${index + 1}`,
            rawPath: `Workspace ${index + 1}`,
          })),
        };
    fs.writeFileSync(request.targetPath, JSON.stringify(safeSnapshot, null, 2), 'utf8');
    return { ok: true, path: request.targetPath, messageKey: 'export.summaryJsonSuccess' };
  }

  private async exportScreenshot(targetPath: string): Promise<ExportResult> {
    const window = BrowserWindow.getFocusedWindow() || BrowserWindow.getAllWindows()[0];
    if (!window) {
      return { ok: false, messageKey: 'export.noWindowAvailable' };
    }
    const image = await window.webContents.capturePage();
    fs.writeFileSync(targetPath, image.toPNG());
    return { ok: true, path: targetPath, messageKey: 'export.dashboardImageSuccess' };
  }
}
