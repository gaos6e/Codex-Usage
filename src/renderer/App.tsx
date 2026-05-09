import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Activity, BarChart3, ChevronDown, ChevronRight, Folder, Gauge, Settings as SettingsIcon, Sparkles, Stethoscope } from 'lucide-react';
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

function waitForRenderCycle(): Promise<void> {
  return new Promise((resolve) => {
    window.requestAnimationFrame(() => window.setTimeout(resolve, 0));
  });
}

function settingsAffectData(previous: AppSettings, next: AppSettings): boolean {
  return previous.codexDir !== next.codexDir
    || previous.includeArchivedSessions !== next.includeArchivedSessions
    || previous.includeDetailedLogs !== next.includeDetailedLogs
    || previous.idleGapMinutes !== next.idleGapMinutes
    || JSON.stringify(previous.aliases) !== JSON.stringify(next.aliases)
    || JSON.stringify(previous.ignoredWorkspaces) !== JSON.stringify(next.ignoredWorkspaces);
}

export function App(): React.ReactElement {
  const [page, setPage] = useState<PageId>('dashboard');
  const [filters, setFilters] = useState<UsageFilters>(initialFilters);
  const [snapshot, setSnapshot] = useState<UsageSnapshot | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [selectedWorkspaceId, setSelectedWorkspaceId] = useState<string | null>(null);
  const [projectListExpanded, setProjectListExpanded] = useState(true);
  const [toast, setToast] = useState<string | null>(null);
  const [bootstrapped, setBootstrapped] = useState(false);
  const snapshotRequestId = useRef(0);
  const settingsSaveRequestId = useRef(0);
  const settingsRefreshTimer = useRef<number | null>(null);
  const filtersRef = useRef(filters);
  const snapshotRef = useRef<UsageSnapshot | null>(null);
  const settingsRef = useRef<AppSettings | null>(null);

  const effectiveTheme = useMemo(() => {
    if (!settings || settings.theme === 'system') {
      return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
    }
    return settings.theme;
  }, [settings]);
  const language = settings?.language || 'zh-CN';
  const t = useMemo(() => buildTranslator(language), [language]);
  const isRefreshing = loading || refreshing;
  const fontScaleStyle = useMemo(() => ({
    '--ui-font-scale': String(settings?.fontScale.ui ?? 1),
    '--data-font-scale': String(settings?.fontScale.data ?? 1),
  }) as React.CSSProperties, [settings?.fontScale.data, settings?.fontScale.ui]);
  const pageTitle = useMemo(() => {
    if (page === 'dashboard') {
      return t('nav.dashboard');
    }
    if (page === 'project') {
      return t('nav.projectDetail');
    }
    if (page === 'settings') {
      return t('nav.settings');
    }
    return t('nav.diagnostics');
  }, [page, t]);
  const projectWorkspaceId = useMemo(() => {
    if (selectedWorkspaceId && selectedWorkspaceId !== ALL_WORKSPACES_ID) {
      return selectedWorkspaceId;
    }
    return null;
  }, [selectedWorkspaceId]);

  useEffect(() => {
    filtersRef.current = filters;
  }, [filters]);

  useEffect(() => {
    snapshotRef.current = snapshot;
  }, [snapshot]);

  useEffect(() => {
    settingsRef.current = settings;
  }, [settings]);

  const loadSnapshot = useCallback(async (nextFilters = filtersRef.current, background = Boolean(snapshotRef.current)) => {
    const requestId = snapshotRequestId.current + 1;
    snapshotRequestId.current = requestId;
    if (!background) {
      setLoading(true);
    } else {
      setRefreshing(true);
    }
    try {
      await waitForRenderCycle();
      const result = await window.codexUsage.getUsageSnapshot(nextFilters);
      if (requestId === snapshotRequestId.current) {
        snapshotRef.current = result;
        setSnapshot(result);
      }
    } catch (error) {
      if (requestId === snapshotRequestId.current) {
        setToast(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (requestId === snapshotRequestId.current) {
        if (!background) {
          setLoading(false);
        } else {
          setRefreshing(false);
        }
      }
    }
  }, []);

  const refreshUsageSnapshot = useCallback(async (showSuccessToast: boolean) => {
    const background = Boolean(snapshotRef.current);
    const requestId = snapshotRequestId.current + 1;
    snapshotRequestId.current = requestId;
    if (!background) {
      setLoading(true);
    } else {
      setRefreshing(true);
    }
    try {
      await waitForRenderCycle();
      await window.codexUsage.refreshUsage();
      if (requestId !== snapshotRequestId.current) {
        return;
      }
      await waitForRenderCycle();
      const result = await window.codexUsage.getUsageSnapshot(filtersRef.current);
      if (requestId === snapshotRequestId.current) {
        snapshotRef.current = result;
        setSnapshot(result);
        if (showSuccessToast) {
          setToast(t('toast.refreshSuccess'));
        }
      }
    } catch (error) {
      if (requestId === snapshotRequestId.current) {
        setToast(error instanceof Error ? error.message : String(error));
      }
    } finally {
      if (requestId === snapshotRequestId.current) {
        if (!background) {
          setLoading(false);
        } else {
          setRefreshing(false);
        }
      }
    }
  }, [t]);

  const scheduleSettingsDataRefresh = useCallback(() => {
    if (settingsRefreshTimer.current) {
      window.clearTimeout(settingsRefreshTimer.current);
    }
    settingsRefreshTimer.current = window.setTimeout(() => {
      settingsRefreshTimer.current = null;
      void refreshUsageSnapshot(false);
    }, 650);
  }, [refreshUsageSnapshot]);

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
        settingsRef.current = loadedSettings;
        setAppInfo(info);
        if (cachedSnapshot) {
          snapshotRef.current = cachedSnapshot;
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

  useEffect(() => () => {
    if (settingsRefreshTimer.current) {
      window.clearTimeout(settingsRefreshTimer.current);
    }
  }, []);

  useEffect(() => {
    if (!bootstrapped) {
      return;
    }
    loadSnapshot(filters, Boolean(snapshotRef.current));
  }, [bootstrapped, filters, loadSnapshot]);

  useEffect(() => {
    if (!settings?.autoRefreshSeconds) {
      return undefined;
    }
    const timer = window.setInterval(() => loadSnapshot(filters, Boolean(snapshotRef.current)), settings.autoRefreshSeconds * 1000);
    return () => window.clearInterval(timer);
  }, [settings?.autoRefreshSeconds, filters, loadSnapshot]);

  const applySettings = useCallback((nextSettings: AppSettings) => {
    const previousSettings = settingsRef.current;
    settingsRef.current = nextSettings;
    setSettings(nextSettings);

    const requestId = settingsSaveRequestId.current + 1;
    settingsSaveRequestId.current = requestId;
    void window.codexUsage.saveSettings(nextSettings)
      .then((saved) => {
        if (requestId !== settingsSaveRequestId.current) {
          return;
        }
        settingsRef.current = saved;
        setSettings(saved);
        if (previousSettings && settingsAffectData(previousSettings, saved)) {
          scheduleSettingsDataRefresh();
        }
      })
      .catch((error) => {
        setToast(error instanceof Error ? error.message : String(error));
      });
  }, [scheduleSettingsDataRefresh]);

  const refresh = () => {
    void refreshUsageSnapshot(true);
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
    setProjectListExpanded(true);
    setPage('project');
  };

  const openDashboard = () => {
    setFilters((current) => ({ ...current, workspaceId: ALL_WORKSPACES_ID }));
    setPage('dashboard');
  };

  return (
    <I18nContext.Provider value={{ language, t }}>
      <div className="app-shell" data-theme={effectiveTheme} style={fontScaleStyle}>
        <aside className="sidebar">
          <div className="brand">
            <div className="brand-mark" aria-hidden="true">
              <Gauge size={18} />
            </div>
            <div>
              <strong>{t('app.brand')}</strong>
              <span>{appInfo?.version || '1.0.0'}</span>
            </div>
          </div>
          <nav aria-label="Primary">
            <button
              className={page === 'dashboard' ? 'active' : ''}
              onClick={openDashboard}
              aria-label={t('nav.dashboard')}
              title={t('nav.dashboard')}
            >
              <BarChart3 size={16} />
              <span className="sidebar-nav-label">{t('nav.dashboard')}</span>
            </button>
            <div className="sidebar-nav-group">
              <div className="sidebar-nav-row">
                <button
                  className={page === 'project' ? 'active sidebar-page-button' : 'sidebar-page-button'}
                  onClick={() => setPage('project')}
                  aria-label={t('nav.projectDetail')}
                  title={t('nav.projectDetail')}
                >
                  <Folder size={16} />
                  <span className="sidebar-nav-label">{t('nav.projectDetail')}</span>
                </button>
                <button
                  className="sidebar-expander"
                  onClick={() => setProjectListExpanded((current) => !current)}
                  aria-expanded={projectListExpanded}
                  aria-label={projectListExpanded ? t('sidebar.collapseTopWorkspaces') : t('sidebar.expandTopWorkspaces')}
                  title={projectListExpanded ? t('sidebar.collapseTopWorkspaces') : t('sidebar.expandTopWorkspaces')}
                >
                  {projectListExpanded ? <ChevronDown size={15} /> : <ChevronRight size={15} />}
                </button>
              </div>
              {projectListExpanded ? (
                <div className="workspace-subnav">
                  <div className="sidebar-subheading">{t('sidebar.topWorkspaces')}</div>
                  {snapshot?.workspaces.slice(0, 8).map((workspace) => (
                    <button
                      key={workspace.id}
                      className={projectWorkspaceId === workspace.id ? 'workspace-nav active' : 'workspace-nav'}
                      onClick={() => openProject(workspace.id)}
                      title={workspace.normalizedPath}
                      aria-label={`${workspace.displayName}, ${t('dashboard.matchingRuns', { count: workspace.runs })}`}
                    >
                      <Activity size={14} />
                      <span>{workspace.displayName}</span>
                    </button>
                  ))}
                </div>
              ) : null}
            </div>
            <button
              className={page === 'settings' ? 'active' : ''}
              onClick={() => setPage('settings')}
              aria-label={t('nav.settings')}
              title={t('nav.settings')}
            >
              <SettingsIcon size={16} />
              <span className="sidebar-nav-label">{t('nav.settings')}</span>
            </button>
            <button
              className={page === 'diagnostics' ? 'active' : ''}
              onClick={() => setPage('diagnostics')}
              aria-label={t('nav.diagnostics')}
              title={t('nav.diagnostics')}
            >
              <Stethoscope size={16} />
              <span className="sidebar-nav-label">{t('nav.diagnostics')}</span>
            </button>
          </nav>
        </aside>

        <div className="main-area">
          <header className="top-toolbar">
            <div className="window-title">
              <Sparkles size={14} aria-hidden="true" />
              <span>{pageTitle}</span>
            </div>
            <div className="toolbar-meta">
              <span>{settings?.codexDir || t('toolbar.noCodexDirConfigured')}</span>
              {isRefreshing ? <span className="loading-row" role="status">{t('toolbar.refreshing')}</span> : null}
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
          {page === 'project' ? <ProjectDetail workspaceId={projectWorkspaceId} filters={filters} onFiltersChange={setFilters} /> : null}
          {page === 'settings' ? <Settings settings={settings} onChange={applySettings} /> : null}
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
