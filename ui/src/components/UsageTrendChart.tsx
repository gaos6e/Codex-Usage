import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import type { ResolvedRange, TrendPoint } from '../types';
import { formatCost, formatExact } from '../lib/format';

export type TrendSeriesKey =
  | 'freshInputTokens'
  | 'outputTokens'
  | 'cachedInputTokens'
  | 'reasoningTokens'
  | 'estimatedCostMicrousd';

interface ChartDatum extends TrendPoint {
  label: string;
}

interface SeriesDefinition {
  key: TrendSeriesKey;
  label: string;
  color: string;
  gradient?: string;
  cost?: boolean;
}

const SERIES: SeriesDefinition[] = [
  { key: 'freshInputTokens', label: '新增输入', color: '#6c8cff', gradient: 'inputGradient' },
  { key: 'outputTokens', label: '输出', color: '#45d6a4', gradient: 'outputGradient' },
  { key: 'cachedInputTokens', label: '缓存读取', color: '#a57cff', gradient: 'cachedGradient' },
  { key: 'reasoningTokens', label: '推理', color: '#f1a35b', gradient: 'reasoningGradient' },
  { key: 'estimatedCostMicrousd', label: '估算成本', color: '#f06b78', cost: true },
];

const STORAGE_KEY = 'codex-usage.trend-series.v1';

export function buildChartData(
  trend: TrendPoint[],
  granularity: ResolvedRange['granularity'],
): ChartDatum[] {
  const dateTime = new Intl.DateTimeFormat(undefined, {
    month: granularity === 'hour' ? undefined : 'short',
    day: granularity === 'hour' ? undefined : 'numeric',
    hour: granularity === 'hour' ? '2-digit' : undefined,
    minute: granularity === 'hour' ? '2-digit' : undefined,
  });
  return trend.map((point) => ({
    ...point,
    label: granularity === 'week'
      ? `周 · ${dateTime.format(point.timestampMs)}`
      : dateTime.format(point.timestampMs),
  }));
}

export function deserializeSeriesVisibility(serialized: string | null): Set<TrendSeriesKey> {
  if (!serialized) return new Set(SERIES.map((series) => series.key));
  try {
    const values = JSON.parse(serialized) as unknown;
    if (!Array.isArray(values)) throw new Error('Expected an array');
    const allowed = new Set(SERIES.map((series) => series.key));
    const selected = values.filter(
      (value): value is TrendSeriesKey => typeof value === 'string' && allowed.has(value as TrendSeriesKey),
    );
    return selected.length > 0 ? new Set(selected) : new Set(allowed);
  } catch {
    return new Set(SERIES.map((series) => series.key));
  }
}

export function toggleSeries(
  current: ReadonlySet<TrendSeriesKey>,
  key: TrendSeriesKey,
): Set<TrendSeriesKey> {
  const next = new Set(current);
  if (next.has(key) && next.size > 1) next.delete(key);
  else next.add(key);
  return next;
}

export function trendAnimationEnabled(
  matchMedia: (query: string) => Pick<MediaQueryList, 'matches'> = window.matchMedia.bind(window),
): boolean {
  return !matchMedia('(prefers-reduced-motion: reduce)').matches;
}

interface UsageTrendChartProps {
  trend: TrendPoint[];
  granularity: ResolvedRange['granularity'];
}

export function UsageTrendChart({ trend, granularity }: UsageTrendChartProps) {
  const { t } = useTranslation();
  const data = useMemo(() => buildChartData(trend, granularity), [trend, granularity]);
  const [visible, setVisible] = useState<Set<TrendSeriesKey>>(() =>
    deserializeSeriesVisibility(window.localStorage.getItem(STORAGE_KEY)),
  );

  const handleToggle = (key: TrendSeriesKey) => {
    setVisible((current) => {
      const next = toggleSeries(current, key);
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify([...next]));
      return next;
    });
  };

  return (
    <section className="chart-card" aria-label={t('用量趋势')}>
      <div className="section-heading">
        <div>
          <span>{t('趋势')}</span>
          <h2>{t('Token 与成本')}</h2>
        </div>
        <div className="chart-legend" role="group" aria-label={t('趋势序列')}>
          {SERIES.map((series) => (
            <button
              key={series.key}
              type="button"
              aria-pressed={visible.has(series.key)}
              className={visible.has(series.key) ? 'legend-item active' : 'legend-item'}
              onClick={() => handleToggle(series.key)}
            >
              <i style={{ '--series-color': series.color } as React.CSSProperties} />
              {t(series.label)}
            </button>
          ))}
        </div>
      </div>

      {data.length === 0 ? (
        <div className="chart-empty">{t('当前筛选范围内还没有可绘制的数据。')}</div>
      ) : (
        <div className="chart-wrap">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={data} margin={{ top: 14, right: 8, bottom: 0, left: 8 }}>
              <defs>
                {SERIES.filter((series) => series.gradient).map((series) => (
                  <linearGradient key={series.gradient} id={series.gradient} x1="0" y1="0" x2="0" y2="1">
                    <stop offset="0%" stopColor={series.color} stopOpacity={0.2} />
                    <stop offset="100%" stopColor={series.color} stopOpacity={0} />
                  </linearGradient>
                ))}
              </defs>
              <CartesianGrid vertical={false} stroke="var(--chart-grid)" />
              <XAxis
                dataKey="label"
                axisLine={false}
                tickLine={false}
                minTickGap={30}
                tick={{ fill: 'var(--chart-axis)', fontSize: '0.75rem' }}
              />
              <YAxis yAxisId="tokens" hide domain={[0, 'auto']} />
              <YAxis yAxisId="cost" hide orientation="right" domain={[0, 'auto']} />
              <Tooltip
                cursor={{ stroke: 'rgba(159,176,255,.32)', strokeDasharray: '4 4' }}
                contentStyle={{
                  background: 'var(--chart-tooltip)',
                  border: '1px solid var(--chart-tooltip-border)',
                  borderRadius: 12,
                  boxShadow: '0 18px 50px rgba(0,0,0,.35)',
                }}
                labelStyle={{ color: 'var(--chart-tooltip-label)', marginBottom: 7 }}
                formatter={(value, name) => [
                  name === t('估算成本') ? formatCost(Number(value)) : formatExact(Number(value)),
                  name,
                ]}
              />
              {SERIES.map((series) => visible.has(series.key) && (
                <Area
                  key={series.key}
                  yAxisId={series.cost ? 'cost' : 'tokens'}
                  type="monotone"
                  dataKey={series.key}
                  name={t(series.label)}
                  stroke={series.color}
                  strokeWidth={2}
                  strokeDasharray={series.cost ? '6 5' : undefined}
                  fill={series.gradient ? `url(#${series.gradient})` : 'transparent'}
                  isAnimationActive={trendAnimationEnabled()}
                  connectNulls={false}
                />
              ))}
            </AreaChart>
          </ResponsiveContainer>
        </div>
      )}
    </section>
  );
}
