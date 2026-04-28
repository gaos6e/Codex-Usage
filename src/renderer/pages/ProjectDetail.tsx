import React, { useEffect, useState } from 'react';
import type { ProjectDetail as ProjectDetailType, UsageFilters } from '../../shared/contracts';
import { formatDecimal, formatDuration, formatTokens } from '../../shared/formatting';
import { DailyChart } from '../components/DailyChart';
import { RunsTable } from '../components/RunsTable';
import { TimeRangeControls } from '../components/TimeRangeControls';
import { useI18n } from '../i18n/I18nContext';

interface Props {
  workspaceId: string | null;
  filters: UsageFilters;
  onFiltersChange: (filters: UsageFilters) => void;
}

export function ProjectDetail({ workspaceId, filters, onFiltersChange }: Props): React.ReactElement {
  const [detail, setDetail] = useState<ProjectDetailType | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { t } = useI18n();

  useEffect(() => {
    if (!workspaceId) {
      setDetail(null);
      setError(null);
      return;
    }
    let cancelled = false;
    setDetail(null);
    setError(null);
    window.codexUsage.getProjectDetail(workspaceId, filters)
      .then((result) => {
        if (cancelled) {
          return;
        }
        setDetail(result);
        setError(null);
      })
      .catch((err) => {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
        }
      });
    return () => {
      cancelled = true;
    };
  }, [workspaceId, filters]);

  if (!workspaceId) {
    return (
      <main className="content-pane">
        <div className="empty-state">
          <h1>{t('project.noSelectionTitle')}</h1>
          <p>{t('project.noSelectionHint')}</p>
        </div>
      </main>
    );
  }

  if (error) {
    return (
      <main className="content-pane">
        <div className="empty-state">
          <h1>{t('project.unavailableTitle')}</h1>
          <p>{error}</p>
        </div>
      </main>
    );
  }

  if (!detail) {
    return <main className="content-pane"><div className="loading-row">{t('project.loading')}</div></main>;
  }

  return (
    <main className="content-pane">
      <div className="content-header">
        <div>
          <h1>{detail.workspace.displayName}</h1>
          <p title={detail.workspace.normalizedPath}>{detail.workspace.normalizedPath}</p>
        </div>
        <div className="project-stats">
          <span>{formatTokens(detail.workspace.tokens)} {t('metric.tokens')}</span>
          <span>{formatDuration(detail.workspace.agentTimeMs)} {t('metric.agentTime')}</span>
          <span>{detail.workspace.runs} {t('metric.runs')}</span>
        </div>
      </div>
      <section className="control-strip project-control-strip" aria-label={t('project.filters')}>
        <TimeRangeControls filters={filters} onFiltersChange={onFiltersChange} />
      </section>
      <section className="workspace-layout">
        <div className="primary-workspace">
          <div className="section-heading"><h2>{t('project.usageOverTime')}</h2></div>
          <DailyChart data={detail.daily} view={filters.view} />
          <div className="section-heading"><h2>{t('dashboard.runs')}</h2><span>{t('project.recordsCount', { count: detail.runs.length })}</span></div>
          <RunsTable runs={detail.runs} scrollable />
        </div>
        <aside className="inspector-pane">
          <h2>{t('project.modelTokens')}</h2>
          <div className="model-list">
            {detail.tokensByModel.map((item) => (
              <div key={item.model}>
                <span>{item.model}</span>
                <strong>{formatTokens(item.tokens)}</strong>
                <small>{item.runs} runs</small>
              </div>
            ))}
          </div>
          <div className="inspector-section">
            <h3>{t('project.dailyAverage')}</h3>
            <p><strong>{formatTokens(detail.dailyAverage.tokensPerDay)}</strong> {t('metric.avgTokensPerDay')}</p>
            <p><strong>{formatDuration(detail.dailyAverage.agentTimePerDayMs)}</strong> {t('metric.avgTimePerDay')}</p>
            <p><strong>{formatDecimal(detail.dailyAverage.runsPerDay)}</strong> {t('metric.avgRunsPerDay')}</p>
            <p className="help-text">{t('metric.calendarDays', { count: detail.dailyAverage.calendarDays })}</p>
          </div>
          <div className="inspector-section">
            <h3>{t('project.recentSessions')}</h3>
            <RunsTable runs={detail.recentSessions.slice(0, 5)} compact />
          </div>
        </aside>
      </section>
    </main>
  );
}
