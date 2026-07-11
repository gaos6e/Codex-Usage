import { useMemo } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { AlertTriangle, CheckCircle2, Database, FileStack, RefreshCw, ShieldCheck, Trash2, Wrench } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { clearAnalysis, getBootstrapStatus, getDiagnostics, startSync } from '../../api';
import { QueryStatus } from '../../components/QueryStatus';
import { formatBytes, formatDuration } from '../../lib/format';

export function DataPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const bootstrap = useQuery({ queryKey: ['bootstrap'], queryFn: getBootstrapStatus });
  const diagnostics = useQuery({ queryKey: ['diagnostics'], queryFn: getDiagnostics, refetchInterval: 10_000 });
  const refresh = useMutation({
    mutationFn: (mode: 'incremental' | 'rebuild' | 'repair') => startSync(mode),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['sync-status'] });
      void queryClient.invalidateQueries({ queryKey: ['diagnostics'] });
    },
  });
  const clear = useMutation({
    mutationFn: clearAnalysis,
    onSuccess: () => {
      void queryClient.invalidateQueries();
    },
  });
  const grouped = useMemo(() => {
    const values = new Map<string, { files: number; bytes: number; ready: number; errors: number }>();
    diagnostics.data?.sources.forEach((source) => {
      const current = values.get(source.kind) ?? { files: 0, bytes: 0, ready: 0, errors: 0 };
      current.files += 1; current.bytes += source.fileSize;
      if (source.status === 'ready') current.ready += 1;
      if (source.status === 'error') current.errors += 1;
      values.set(source.kind, current);
    });
    return ['state_db', 'session', 'archived_session', 'logs_db'].map((kind) => [
      kind,
      values.get(kind) ?? { files: 0, bytes: 0, ready: 0, errors: 0 },
    ] as const);
  }, [diagnostics.data?.sources]);

  const confirmClear = () => {
    if (window.confirm(t('仅清空 Chronolume v2 分析库。~/.codex 原始数据不会被修改。是否继续？'))) clear.mutate();
  };

  return <section className="feature-page">
    <QueryStatus
      loading={diagnostics.isLoading && !diagnostics.data}
      error={diagnostics.error}
      onRetry={() => void diagnostics.refetch()}
    />
    <div className="stat-card-grid diagnostics-grid">
      <div className="stat-card">{diagnostics.data?.databaseIntegrityOk ? <ShieldCheck /> : <AlertTriangle />}<span>{t('数据库完整性')}</span><strong>{diagnostics.data?.databaseIntegrityOk ? t('正常') : t('异常')}</strong></div>
      <div className="stat-card"><Database /><span>{t('分析库大小')}</span><strong>{formatBytes(diagnostics.data?.databaseSizeBytes ?? 0)}</strong></div>
      <div className="stat-card"><FileStack /><span>{t('已索引会话')}</span><strong>{diagnostics.data?.indexedSessions.toLocaleString() ?? '—'}</strong></div>
      <div className="stat-card"><CheckCircle2 /><span>{t('保留事件')}</span><strong>{((diagnostics.data?.retainedUsageEvents ?? 0) + (diagnostics.data?.retainedToolEvents ?? 0)).toLocaleString()}</strong></div>
    </div>
    <section className="feature-card">
      <div className="feature-card-heading"><div><span>SOURCES</span><h2>{t('数据源能力与索引')}</h2></div><small>Schema v{diagnostics.data?.schemaVersion ?? '…'} · Parser v{diagnostics.data?.parserVersion ?? '…'}</small></div>
      <div className="source-grid">{grouped.map(([kind, source]) => <div key={kind}><strong>{sourceLabel(kind)}</strong><span>{source.files.toLocaleString()} {t('文件')} · {formatBytes(source.bytes)}</span><small>{source.files === 0 ? t('不可用或尚未检测') : `${source.ready}/${source.files} ${t('就绪')}${source.errors > 0 ? ` · ${source.errors} ${t('错误')}` : ''}`}</small></div>)}</div>
    </section>
    <section className="feature-card">
      <div className="feature-card-heading"><div><span>MAINTENANCE</span><h2>{t('索引维护')}</h2></div></div>
      <div className="maintenance-actions">
        <button type="button" className="primary-button" disabled={refresh.isPending} onClick={() => refresh.mutate('incremental')}><RefreshCw />{t('增量同步')}</button>
        <button type="button" className="quiet-button" disabled={refresh.isPending} onClick={() => refresh.mutate('repair')}><ShieldCheck />{t('修复索引')}</button>
        <button type="button" className="quiet-button" disabled={refresh.isPending} onClick={() => refresh.mutate('rebuild')}><Wrench />{t('重新索引')}</button>
        <button type="button" className="danger-button" disabled={clear.isPending} onClick={confirmClear}><Trash2 />{t('清空分析库')}</button>
      </div>
      <p className="privacy-note">
        <span>{t('这些操作只写入 Chronolume 分析目录：')}</span>
        <code>{bootstrap.data?.dataDirectory ?? '—'}</code>
        <span>{t('；不会写入、移动或删除 ~/.codex。')}</span>
      </p>
    </section>
    <section className="feature-card">
      <div className="feature-card-heading"><div><span>RUNS</span><h2>{t('最近同步与性能')}</h2></div></div>
      <div className="data-table-wrap embedded"><table className="data-table"><thead><tr><th>{t('开始')}</th><th>{t('模式')}</th><th>{t('状态')}</th><th>{t('文件')}</th><th>{t('读取')}</th><th>{t('跳过')}</th><th>{t('耗时')}</th><th>{t('解析失败')}</th></tr></thead><tbody>
        {diagnostics.data?.recentRuns.map((run) => <tr key={run.id}><td>{new Date(run.startedAtMs).toLocaleString()}</td><td>{run.mode}</td><td>{run.status}</td><td>{run.filesCompleted}/{run.filesTotal}</td><td>{formatBytes(run.bytesRead)}</td><td>{run.recordsSkipped}</td><td>{run.elapsedMs == null ? '—' : formatDuration(run.elapsedMs)}</td><td>{run.parseFailures}</td></tr>)}
      </tbody></table></div>
    </section>
  </section>;
}

function sourceLabel(kind: string): string {
  return ({ session: 'sessions', archived_session: 'archived_sessions', state_db: 'state_5.sqlite', logs_db: 'logs_2.sqlite' } as Record<string, string>)[kind] ?? kind;
}
