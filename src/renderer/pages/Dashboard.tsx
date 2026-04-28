import React from 'react';
import { Download, RefreshCw } from 'lucide-react';
import type { AppSettings, ExportKind, ExportPrivacyMode, UsageFilters, UsageSnapshot } from '../../shared/contracts';
import { ALL_WORKSPACES_ID } from '../../shared/pathUtils';
import { formatDateTime } from '../../shared/formatting';
import { MetricCard } from '../components/MetricCard';
import { DailyChart } from '../components/DailyChart';
import { RunsTable } from '../components/RunsTable';
import { TimeRangeControls } from '../components/TimeRangeControls';
import { useI18n } from '../i18n/I18nContext';

interface Props {
  snapshot: UsageSnapshot | null;
  filters: UsageFilters;
  settings: AppSettings | null;
  loading: boolean;
  onFiltersChange: (filters: UsageFilters) => void;
  onRefresh: () => void;
  onExport: (kind: ExportKind, privacyMode: ExportPrivacyMode) => void;
  onOpenProject: (workspaceId: string) => void;
}

export function Dashboard({ snapshot, filters, settings, loading, onFiltersChange, onRefresh, onExport, onOpenProject }: Props): React.ReactElement {
  const warnings = snapshot?.diagnostics.warnings.filter((warning) => warning.severity !== 'info') || [];
  const [exportPrivacyMode, setExportPrivacyMode] = React.useState<ExportPrivacyMode>('anonymized-paths');
  const { t } = useI18n();
  return (
    <main className="content-pane" aria-busy={loading}>
      <div className="content-header">
        <div>
          <h1>{t('dashboard.title')}</h1>
          <p>{t('dashboard.subtitle')}</p>
        </div>
        <div className="toolbar-actions">
          <span className="status-text">
            {snapshot ? t('dashboard.updated', { time: formatDateTime(snapshot.generatedAt) }) : t('dashboard.updatedNever')}
          </span>
          <button className="icon-button" onClick={onRefresh} title={t('toolbar.refreshing')} aria-label={t('toolbar.refreshing')}>
            <RefreshCw size={17} />
          </button>
        </div>
      </div>

      <section className="control-strip" aria-label={t('dashboard.filters')}>
        <label>
          {t('filter.workspace')}
          <select
            value={filters.workspaceId}
            onChange={(event) => onFiltersChange({ ...filters, workspaceId: event.target.value })}
          >
            <option value={ALL_WORKSPACES_ID}>{t('filter.allWorkspaces')}</option>
            {snapshot?.workspaces.map((workspace) => (
              <option key={workspace.id} value={workspace.id}>{workspace.displayName}</option>
            ))}
          </select>
        </label>

        <TimeRangeControls filters={filters} onFiltersChange={onFiltersChange} />
      </section>

      {warnings.length ? (
        <section className="warning-banner" role="status">
          <strong>{t('dashboard.partialData')}</strong>
          <span>{t(warnings[0].messageKey, warnings[0].messageArgs)}</span>
        </section>
      ) : null}

      <section className="metric-grid" aria-label={t('dashboard.usageMetrics')}>
        {snapshot?.cards.map((card) => <MetricCard key={card.id} card={card} />)}
      </section>

      <section className="workspace-layout">
        <div className="primary-workspace">
          <div className="section-heading">
            <h2>{filters.view === 'time' ? t('dashboard.agentTimeOverTime') : t('dashboard.tokenUsageOverTime')}</h2>
            <div className="export-group">
              <label className="inline-label">
                {t('filter.exportPaths')}
                <select value={exportPrivacyMode} onChange={(event) => setExportPrivacyMode(event.target.value as ExportPrivacyMode)}>
                  <option value="anonymized-paths">{t('filter.exportPathsAnonymized')}</option>
                  <option value="full-paths">{t('filter.exportPathsFull')}</option>
                </select>
              </label>
              <button className="secondary-button" onClick={() => onExport('daily-csv', exportPrivacyMode)} title={t('dashboard.export.dailyCsv')}>
                <Download size={15} /> {t('dashboard.export.dailyCsvShort')}
              </button>
              <button className="secondary-button" onClick={() => onExport('runs-csv', exportPrivacyMode)} title={t('dashboard.export.runsCsv')}>
                <Download size={15} /> {t('dashboard.export.runsCsvShort')}
              </button>
              <button className="secondary-button" onClick={() => onExport('summary-json', exportPrivacyMode)} title={t('dashboard.export.summaryJson')}>
                <Download size={15} /> {t('dashboard.export.summaryJsonShort')}
              </button>
              <button className="secondary-button" onClick={() => onExport('dashboard-png', 'full-paths')} title={t('dashboard.export.image')}>
                <Download size={15} /> {t('dashboard.export.imageShort')}
              </button>
            </div>
          </div>
          <DailyChart data={snapshot?.daily || []} view={filters.view} />
          <div className="section-heading">
            <h2>{t('dashboard.runs')}</h2>
            <span>{t('dashboard.matchingRuns', { count: snapshot?.runs.length || 0 })}</span>
          </div>
          <RunsTable runs={snapshot?.runs || []} scrollable />
        </div>

        <aside className="inspector-pane" aria-label={t('filter.workspace')}>
          <h2>{t('filter.workspace')}</h2>
          <div className="workspace-list">
            {snapshot?.workspaces.slice(0, 18).map((workspace) => (
              <button key={workspace.id} onClick={() => onOpenProject(workspace.id)} title={workspace.normalizedPath}>
                <span>{workspace.displayName}</span>
                <span>{t('dashboard.matchingRuns', { count: workspace.runs })}</span>
              </button>
            ))}
          </div>
          <div className="inspector-section">
            <h3>{t('dashboard.methodology')}</h3>
            <p>{snapshot ? t(snapshot.timeSummary.methodology) : ''}</p>
            <p>{t('dashboard.idleGapCap', { minutes: settings?.idleGapMinutes || 30 })}</p>
          </div>
        </aside>
      </section>
    </main>
  );
}
