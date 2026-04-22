import React from 'react';
import type { RunRecord } from '../../shared/contracts';
import { formatDateTime, formatDuration, formatTokens } from '../../shared/formatting';
import { useI18n } from '../i18n/I18nContext';

interface Props {
  runs: RunRecord[];
  compact?: boolean;
  scrollable?: boolean;
}

export function RunsTable({ runs, compact = false, scrollable = false }: Props): React.ReactElement {
  const { t } = useI18n();
  if (!runs.length) {
    return (
      <div className="empty-state empty-state--small">
        <h2>{t('runs.emptyTitle')}</h2>
        <p>{t('runs.emptyHint')}</p>
      </div>
    );
  }

  return (
    <div
      className={[
        'table-shell',
        compact ? 'table-shell--compact' : '',
        scrollable ? 'table-shell--scrollable' : '',
      ].filter(Boolean).join(' ')}
    >
      <table className={compact ? 'runs-table runs-table--compact' : 'runs-table'}>
        <thead>
          <tr>
            <th>{t('runs.column.run')}</th>
            <th>{t('runs.column.workspace')}</th>
            <th>{t('runs.column.started')}</th>
            <th>{t('runs.column.duration')}</th>
            <th>{t('runs.column.model')}</th>
            <th>{t('runs.column.tokens')}</th>
            <th>{t('runs.column.status')}</th>
          </tr>
        </thead>
        <tbody>
          {runs.map((run) => (
            <tr key={run.id}>
              <td>
                <div className="table-title">{run.title || t('runs.untitled')}</div>
                <div className="table-meta">{run.id}</div>
              </td>
              <td title={run.workspacePath}>{run.workspaceName}</td>
              <td>{formatDateTime(run.startTime)}</td>
              <td>
                {formatDuration(run.durationMs)}
                <div className="table-meta">{run.durationMethod === 'jsonl-events' ? t('runs.duration.jsonl') : t('runs.duration.thread')}</div>
              </td>
              <td>{run.model}</td>
              <td className="numeric">{formatTokens(run.totalTokens)}</td>
              <td>{run.archived ? <span className="badge">{t('runs.status.archived')}</span> : <span className="badge badge--ok">{t('runs.status.active')}</span>}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
