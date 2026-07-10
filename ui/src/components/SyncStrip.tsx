import { AlertTriangle, CheckCircle2, Database, LoaderCircle, PauseCircle, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { SyncStatus } from '../types';
import { formatBytes } from '../lib/format';

interface SyncStripProps {
  status?: SyncStatus;
  onCancel: () => void;
}

const activePhases = new Set(['detecting', 'planning', 'importing', 'rolling_up']);

export function SyncStrip({ status, onCancel }: SyncStripProps) {
  const { t } = useTranslation();
  if (!status) return null;
  const active = activePhases.has(status.phase);
  const progress = status.bytesTotal > 0
    ? Math.min(100, (status.bytesRead / status.bytesTotal) * 100)
    : status.filesTotal > 0
      ? Math.min(100, (status.filesCompleted / status.filesTotal) * 100)
      : 0;
  const Icon = status.phase === 'failed'
    ? AlertTriangle
    : status.phase === 'completed'
      ? CheckCircle2
      : status.phase === 'cancelled'
        ? PauseCircle
        : active
          ? LoaderCircle
          : Database;

  return (
    <section className={`sync-strip phase-${status.phase}`} aria-live="polite">
      <Icon className={active ? 'spin' : ''} />
      <div className="sync-copy">
        <div>
          <strong>{t(phaseLabel(status.phase))}</strong>
          {active && (
            <span>
              {status.filesCompleted}/{status.filesTotal} {t('文件')} · {formatBytes(status.bytesRead)} / {formatBytes(status.bytesTotal)} · {formatBytes(status.speedBytesPerSecond)}/s
            </span>
          )}
          {!active && status.lastCompletedAtMs && (
            <span>{t('最后同步')} {new Date(status.lastCompletedAtMs).toLocaleString()}</span>
          )}
        </div>
        {active && <div className="sync-track"><span style={{ width: `${progress}%` }} /></div>}
      </div>
      {active && (
        <button type="button" onClick={onCancel} disabled={status.cancelRequested}>
          <X />{status.cancelRequested ? t('正在取消') : t('取消')}
        </button>
      )}
    </section>
  );
}

function phaseLabel(phase: SyncStatus['phase']): string {
  const labels: Record<SyncStatus['phase'], string> = {
    idle: '等待本地同步',
    detecting: '正在检测 Codex 数据源',
    planning: '正在生成增量计划',
    importing: '正在后台导入',
    rolling_up: '正在更新永久汇总',
    completed: '本地数据已同步',
    cancelled: '导入已暂停，可继续',
    failed: '同步失败，请打开诊断',
  };
  return labels[phase];
}
