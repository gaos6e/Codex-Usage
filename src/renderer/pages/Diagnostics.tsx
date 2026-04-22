import React, { useEffect, useState } from 'react';
import type { DiagnosticsSnapshot } from '../../shared/contracts';
import { formatDateTime, formatDuration, formatInteger } from '../../shared/formatting';
import { useI18n } from '../i18n/I18nContext';

export function Diagnostics(): React.ReactElement {
  const [diagnostics, setDiagnostics] = useState<DiagnosticsSnapshot | null>(null);
  const { t } = useI18n();

  useEffect(() => {
    window.codexUsage.getDiagnostics().then(setDiagnostics).catch(() => setDiagnostics(null));
  }, []);

  if (!diagnostics) {
    return <main className="content-pane"><div className="loading-row">{t('diagnostics.loading')}</div></main>;
  }

  return (
    <main className="content-pane">
      <div className="content-header">
        <div>
          <h1>{t('diagnostics.title')}</h1>
          <p>{t('diagnostics.subtitle')}</p>
        </div>
        <div className="project-stats">
          <span>{formatDuration(diagnostics.parseDurationMs)}</span>
          <span>{diagnostics.cacheStatus}</span>
        </div>
      </div>

      <section className="settings-group">
        <h2>{t('diagnostics.runtimePaths')}</h2>
        <div className="diagnostic-grid">
          <span>{t('diagnostics.codexDir')}</span><strong>{diagnostics.codexDir}</strong>
          <span>{t('diagnostics.appData')}</span><strong>{diagnostics.appDataDir}</strong>
          <span>{t('diagnostics.logFile')}</span><strong>{diagnostics.logFilePath}</strong>
          <span>{t('diagnostics.generated')}</span><strong>{formatDateTime(diagnostics.generatedAt)}</strong>
        </div>
      </section>

      <section className="settings-group">
        <h2>{t('diagnostics.sources')}</h2>
        <div className="source-list">
          {diagnostics.sources.map((source) => (
            <article key={source.key} className="source-row">
              <div>
                <strong>{source.label}</strong>
                <span>{source.path}</span>
              </div>
              <div className="source-meta">
                <span className={source.exists ? 'badge badge--ok' : 'badge badge--warning'}>{source.exists ? t('diagnostics.detected') : t('diagnostics.missing')}</span>
                {typeof source.rows === 'number' ? <span>{t('diagnostics.rows', { count: formatInteger(source.rows) })}</span> : null}
                {typeof source.files === 'number' ? <span>{t('diagnostics.files', { count: formatInteger(source.files) })}</span> : null}
              </div>
              {source.columns?.length ? <p className="help-text">{t('diagnostics.columns', { columns: source.columns.join(', ') })}</p> : null}
              {source.warning ? <p className="warning-text">{source.warning}</p> : null}
            </article>
          ))}
        </div>
      </section>

      <section className="settings-group">
        <h2>{t('diagnostics.warnings')}</h2>
        {diagnostics.warnings.length ? (
          <div className="warning-list">
            {diagnostics.warnings.map((warning, index) => (
              <div key={`${warning.code}-${index}`} className={`warning-item warning-item--${warning.severity}`}>
                <strong>{warning.code}</strong>
                <span>{t(warning.messageKey, warning.messageArgs)}</span>
                {warning.source ? <small>{warning.source}</small> : null}
              </div>
            ))}
          </div>
        ) : (
          <p className="help-text">{t('diagnostics.noWarnings')}</p>
        )}
      </section>
    </main>
  );
}
