import { contextBridge, ipcRenderer } from 'electron';
import type { AppSettings, CodexUsageApi, ExportKind, ExportRequest, UsageFilters } from '../shared/contracts';

const api: CodexUsageApi = {
  getSettings: () => ipcRenderer.invoke('settings:get'),
  saveSettings: (settings: AppSettings) => ipcRenderer.invoke('settings:save', settings),
  chooseCodexDirectory: () => ipcRenderer.invoke('dialog:codex-directory'),
  chooseExportPath: (kind: ExportKind) => ipcRenderer.invoke('dialog:export-path', kind),
  getCachedUsageSnapshot: (filters: UsageFilters) => ipcRenderer.invoke('usage:cached-snapshot', filters),
  getUsageSnapshot: (filters: UsageFilters) => ipcRenderer.invoke('usage:snapshot', filters),
  getProjectDetail: (workspaceId: string, filters: UsageFilters) =>
    ipcRenderer.invoke('usage:project-detail', workspaceId, filters),
  getDiagnostics: () => ipcRenderer.invoke('usage:diagnostics'),
  refreshUsage: () => ipcRenderer.invoke('usage:refresh'),
  exportUsage: (request: ExportRequest) => ipcRenderer.invoke('usage:export', request),
  getAppInfo: () => ipcRenderer.invoke('app:info'),
};

contextBridge.exposeInMainWorld('codexUsage', api);
