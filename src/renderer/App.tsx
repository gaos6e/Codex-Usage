import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { Activity, BarChart3, Folder, Gauge, Settings as SettingsIcon, Stethoscope } from 'lucide-react';
import type { AppInfo, AppSettings, ExportKind, ExportPrivacyMode, PageId, UsageFilters, UsageSnapshot } from '../shared/contracts';
import { ALL_WORKSPACES_ID } from '../shared/pathUtils';
import { Dashboard } from './pages/Dashboard';
import { Settings } from './pages/Settings';
import { Diagnostics } from './pages/Diagnostics';
import { ProjectDetail } from './pages/ProjectDetail';
import { I18nContext, buildTranslator } from './i18n/I18nContext';

const initialFilters: UsageFilters = {
  workspaceId: ALL_WORKSPACES_ID,
  view: 'time',
  range: { preset: 'last7', aggregation: 'daily' },
};

export function App(): React.ReactElement {
  const [page, setPage] = useState<PageId>('dashboard');
  const [filters, setFilters] = useState<UsageFilters>(initialFilters);
  const [snapshot, setSnapshot] = useState<UsageSnapshot | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  const [bootstrapped, setBootstrapped] = useState(false);

  const effectiveTheme = useMemo(() => {
    if (!settings || settings.theme === 'system') {
      return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }
    return settings.theme;
  }, [settings]);
  const language = settings?.language || 'zh-CN';
  const t = useMemo(() => buildTranslator(language), [language]);
  const projectWorkspaceId = useMemo(() => {
    if (selectedWorkspaceId && selectedWorkspaceId !== ALL_WORKSPACES_ID) {
      return selectedWorkspaceId;
    }
    if (filters.workspaceId && filters.workspaceId !== ALL_WORKSPACES_ID) {
      return filters.workspaceId;
    }
    return null;
  }, [selectedWorkspaceId, filters.workspaceId]);

  const loadSnapshot = useCallback(async (nextFilters = filters, background = false) => {
    setLoading(true);
    try {
      const result = await window.codexUsage.getUsageSnapshot(nextFilters);
      setSnapshot(result);
    } catch (error) {
      setToast(error instanceof Error ? error.message : String(error));
    } finally {
      if (!background || !snapshot) {
        setLoading(false);
      } else {
        setLoading(false);
      }
    }
  }, [filters, snapshot]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const [loadedSettings, info, cachedSnapshot] = await Promise.all([
          window.codexUsage.getSettings(),
          window.codexUsage.getAppInfo(),
          window.codexUsage.getCachedUsageSnapshot(initialFilters),
        ]);
        if (cancelled) {
          return;
        }
        setSettings(loadedSettings);
        setAppInfo(info);
        if (cachedSnapshot) {
          setSnapshot(cachedSnapshot);
        }
        setBootstrapped(true);
      } catch (error) {
        if (!cancelled) {
          setToast(error instanceof Error ? error.message : String(error));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!bootstrapped) {
      return;
    }
    loadSnapshot(filters, Boolean(snapshot));
  }, [bootstrapped, filters]);

  useEffect(() => {
    if (!settings?.autoRefreshSeconds) {
      return undefined;
    }
    const timer = window.setInterval(() => loadSnapshot(filters), settings.autoRefreshSeconds * 1000);
    return () => window.clearInterval(timer);
  }, [settings?.autoRefreshSeconds, filters, loadSnapshot]);

  const saveSettings = async (nextSettings: AppSettings) => {
    const saved = await window.codexUsage.saveSettings(nextSettings);
    setSettings(saved);
    await loadSnapshot(filters);
  };

  const refresh = async () => {
    setLoading(true);
    try {
      await window.codexUsage.refreshUsage();
      await loadSnapshot(filters);
      setToast(t('toast.refreshSuccess'));
    } finally {
      setLoading(false);
    }
  };

  const exportUsage = async (kind: ExportKind, privacyMode: ExportPrivacyMode) => {
    const targetPath = await window.codexUsage.chooseExportPath(kind);
    if (!targetPath) {
      return;
    }
    const result = await window.codexUsage.exportUsage({ kind, privacyMode, targetPath, filters });
    setToast(t(result.messageKey, result.messageArgs));
  };

  const openProject = (workspaceId: string) => {
    setSelectedWorkspaceId(workspaceId);
    setFilters((current) => ({ ...current, workspaceId }));
    setPage('project');
  };

  return (
    <I18nContext.Provider value={{ language, t }}>
      <div className="app-shell" data-theme={effectiveTheme}>
        <aside className="sidebar">
          <div className="brand">
            <Gauge size={22} />
            <div>
              <strong>{t('app.brand')}</strong>
              <span>{appInfo?.version || '1.0.0'}</span>
            </div>
          </div>
          <nav aria-label="Primary">
            <button className={page === 'dashboard' ? 'active' : ''} onClick={() => setPage('dashboard')}>
              <BarChart3 size={16} />
              <span className="sidebar-nav-label">{t('nav.dashboard')}</span>
            </button>
            <button className={page === 'project' ? 'active' : ''} onClick={() => setPage('project')}>
              <Folder size={16} />
              <span className="sidebar-nav-label">{t('nav.projectDetail')}</span>
            </button>
            <button className={page === 'settings' ? 'active' : ''} onClick={() => setPage('settings')}>
              <SettingsIcon size={16} />
              <span className="sidebar-nav-label">{t('nav.settings')}</span>
            </button>
            <button className={page === 'diagnostics' ? 'active' : ''} onClick={() => setPage('diagnostics')}>
              <Stethoscope size={16} />
              <span className="sidebar-nav-label">{t('nav.diagnostics')}</span>
            </button>
          </nav>
          <div className="sidebar-section">
            <h2>{t('sidebar.topWorkspaces')}</h2>
            {snapshot?.workspaces.slice(0, 8).map((workspace) => (
              <button key={workspace.id} className="workspace-nav" onClick={() => openProject(workspace.id)} title={workspace.normalizedPath}>
                <Activity size={14} />
                <span>{workspace.displayName}</span>
              </button>
            ))}
          </div>
        </aside>

        <div className="main-area">
          <header className="top-toolbar">
            <div className="window-title">
              {page === 'dashboard' && t('nav.dashboard')}
              {page === 'project' && t('nav.projectDetail')}
              {page === 'settings' && t('nav.settings')}
              {page === 'diagnostics' && t('nav.diagnostics')}
            </div>
            <div className="toolbar-meta">
              <span>{settings?.codexDir || t('toolbar.noCodexDirConfigured')}</span>
              {loading ? <span className="loading-row">{t('toolbar.refreshing')}</span> : null}
            </div>
          </header>

          {page === 'dashboard' ? (
            <Dashboard
              snapshot={snapshot}
              filters={filters}
              settings={settings}
              loading={loading}
              onFiltersChange={setFilters}
              onRefresh={refresh}
              onExport={exportUsage}
              onOpenProject={openProject}
            />
          ) : null}
          {page === 'project' ? <ProjectDetail workspaceId={projectWorkspaceId} filters={filters} /> : null}
          {page === 'settings' ? <Settings settings={settings} onSave={saveSettings} /> : null}
          {page === 'diagnostics' ? <Diagnostics /> : null}
        </div>

        {toast ? (
          <div className="toast" role="status">
            <span>{toast}</span>
            <button onClick={() => setToast(null)} aria-label={t('toast.close')}>{t('toast.close')}</button>
          </div>
        ) : null}
      </div>
    </I18nContext.Provider>
  );
}
