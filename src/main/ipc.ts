import path from 'path';
import { app, BrowserWindow, dialog, ipcMain } from 'electron';
import type { ExportKind, ExportRequest, UsageFilters } from '../shared/contracts';
import { ALL_WORKSPACES_ID } from '../shared/pathUtils';
import { SettingsStore } from './services/settings';
import { UsageService } from './services/usageService';
import { ExportService } from './services/exportService';
import type { AppPaths } from './services/appPaths';

const defaultFilters: UsageFilters = {
  workspaceId: ALL_WORKSPACES_ID,
  view: 'time',
  range: { preset: 'last7' },
};

export function registerIpcHandlers(paths: AppPaths, settingsStore: SettingsStore, usageService: UsageService): void {
  const exportService = new ExportService(usageService);

  ipcMain.handle('settings:get', () => settingsStore.load());
  ipcMain.handle('settings:save', async (_event, settings) => {
    const saved = settingsStore.save(settings);
    await usageService.refresh(true);
    return saved;
  });

  ipcMain.handle('dialog:codex-directory', async () => {
    const result = await dialog.showOpenDialog(BrowserWindow.getFocusedWindow() || undefined, {
      title: 'Select Codex data directory',
      properties: ['openDirectory'],
    });
    return result.canceled ? null : result.filePaths[0];
  });

  ipcMain.handle('dialog:export-path', async (_event, kind: ExportKind) => {
    const extension = kind === 'summary-json' ? 'json' : kind === 'dashboard-png' ? 'png' : 'csv';
    const result = await dialog.showSaveDialog(BrowserWindow.getFocusedWindow() || undefined, {
      title: 'Export Codex usage data',
      defaultPath: path.join(paths.exportsDir, `codex-usage-${Date.now()}.${extension}`),
      filters: [{ name: extension.toUpperCase(), extensions: [extension] }],
    });
    return result.canceled ? null : result.filePath || null;
  });

  ipcMain.handle('usage:cached-snapshot', async (_event, filters: UsageFilters) => {
    return usageService.getCachedSnapshot(filters || defaultFilters);
  });
  ipcMain.handle('usage:snapshot', async (_event, filters: UsageFilters) => {
    return usageService.getSnapshot(filters || defaultFilters);
  });
  ipcMain.handle('usage:project-detail', async (_event, workspaceId: string, filters: UsageFilters) => {
    return usageService.getProjectDetail(workspaceId, filters || defaultFilters);
  });
  ipcMain.handle('usage:diagnostics', async () => usageService.getDiagnostics());
  ipcMain.handle('usage:refresh', async () => {
    await usageService.refresh(true);
    return usageService.getSnapshot(defaultFilters);
  });
  ipcMain.handle('usage:export', async (_event, request: ExportRequest) => exportService.exportUsage(request));
  ipcMain.handle('app:info', () => ({
    appName: app.getName(),
    version: app.getVersion(),
    appDataDir: paths.baseDir,
  }));
}
