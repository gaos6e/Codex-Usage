import React from 'react';
import {
  Bar,
  BarChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import type { DailyUsageBucket, MetricView } from '../../shared/contracts';
import { formatDuration, formatTokens } from '../../shared/formatting';
import { useI18n } from '../i18n/I18nContext';

interface Props {
  data: DailyUsageBucket[];
  view: MetricView;
}

export function DailyChart({ data, view }: Props): React.ReactElement {
  const { t } = useI18n();
  const dataKey = view === 'time' ? 'agentTimeHours' : 'tokens';
  const chartData = data.map((bucket) => ({
    ...bucket,
    agentTimeHours: Number((bucket.agentTimeMs / 3600000).toFixed(2)),
  }));

  if (!chartData.length) {
    return (
      <div className="empty-state">
        <h2>{t('dashboard.noUsage')}</h2>
        <p>{t('dashboard.noUsageHint')}</p>
      </div>
    );
  }

  return (
    <div className="chart-shell" role="img" aria-label={view === 'time' ? t('chart.aria.time') : t('chart.aria.tokens')}>
      <ResponsiveContainer width="100%" height={310}>
        <BarChart data={chartData} margin={{ top: 12, right: 18, left: 0, bottom: 6 }}>
          <CartesianGrid stroke="var(--divider)" vertical={false} />
          <XAxis dataKey="label" tickLine={false} axisLine={false} tick={{ fill: 'var(--text-tertiary)', fontSize: 12 }} />
          <YAxis
            tickLine={false}
            axisLine={false}
            tick={{ fill: 'var(--text-tertiary)', fontSize: 12 }}
            tickFormatter={(value) => view === 'time' ? `${value}h` : formatTokens(Number(value))}
          />
          <Tooltip
            cursor={{ fill: 'var(--selection-wash)' }}
            content={({ active, payload }) => {
              if (!active || !payload?.length) {
                return null;
              }
              const bucket = payload[0].payload as DailyUsageBucket & { agentTimeHours: number };
              return (
                <div className="chart-tooltip">
                  <strong>{bucket.key}</strong>
                  <span>{formatDuration(bucket.agentTimeMs)} {t('metric.agentTime')}</span>
                  <span>{formatTokens(bucket.tokens)} {t('metric.tokens')}</span>
                  <span>{t('chart.activeProjects', { runs: bucket.runs, projects: bucket.activeProjects })}</span>
                  {bucket.topWorkspace ? <span>{t('chart.topWorkspace', { name: bucket.topWorkspace })}</span> : null}
                </div>
              );
            }}
          />
          <Bar dataKey={dataKey} fill="var(--blue)" radius={[5, 5, 0, 0]} maxBarSize={42} />
        </BarChart>
      </ResponsiveContainer>
    </div>
  );
}
