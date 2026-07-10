import type { HeatmapMetric, HeatmapSnapshot, HeatmapSpan } from '../types';
import { useLayoutEffect, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { formatDuration, formatExact, formatTokens } from '../lib/format';

interface ActivityHeatmapProps {
  snapshot?: HeatmapSnapshot;
  metric: HeatmapMetric;
  span: HeatmapSpan;
  loading: boolean;
  onMetric: (metric: HeatmapMetric) => void;
  onSpan: (span: HeatmapSpan) => void;
}

export function ActivityHeatmap({ snapshot, metric, span, loading, onMetric, onSpan }: ActivityHeatmapProps) {
  const { t } = useTranslation();
  const scrollRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<{ pointerId: number; startX: number; startScrollLeft: number } | null>(null);
  const positiveValues = snapshot?.points
    .map((point) => point.value)
    .filter((value) => value > 0)
    .sort((left, right) => left - right) ?? [];

  useLayoutEffect(() => {
    const scroll = scrollRef.current;
    if (!scroll || loading) return;
    scroll.scrollLeft = Math.max(0, scroll.scrollWidth - scroll.clientWidth);
  }, [loading, metric, snapshot?.endDate, snapshot?.points.length, span]);

  const beginDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    const scroll = event.currentTarget;
    if (event.pointerType === 'touch' || event.button !== 0 || scroll.scrollWidth <= scroll.clientWidth) return;
    dragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startScrollLeft: scroll.scrollLeft,
    };
    scroll.dataset.dragging = 'true';
    scroll.setPointerCapture?.(event.pointerId);
  };

  const moveDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.currentTarget.scrollLeft = drag.startScrollLeft - (event.clientX - drag.startX);
    event.preventDefault();
  };

  const endDrag = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    dragRef.current = null;
    delete event.currentTarget.dataset.dragging;
    if (event.currentTarget.hasPointerCapture?.(event.pointerId)) {
      event.currentTarget.releasePointerCapture?.(event.pointerId);
    }
  };

  const navigateWithKeyboard = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const scroll = event.currentTarget;
    const step = Math.max(80, scroll.clientWidth * 0.7);
    if (event.key === 'ArrowLeft') scroll.scrollLeft -= step;
    else if (event.key === 'ArrowRight') scroll.scrollLeft += step;
    else if (event.key === 'Home') scroll.scrollLeft = 0;
    else if (event.key === 'End') scroll.scrollLeft = Math.max(0, scroll.scrollWidth - scroll.clientWidth);
    else return;
    event.preventDefault();
  };
  return (
    <section className={`feature-card heatmap-card metric-${metric.replace('_', '-')}`}>
      <div className="feature-card-heading">
        <div><span>ACTIVITY</span><h2>{t('活跃热力图')}</h2></div>
        <div className="segmented-controls">
          <select value={metric} aria-label={t('热力图指标')} onChange={(event) => onMetric(event.target.value as HeatmapMetric)}>
            <option value="sessions">{t('会话数')}</option>
            <option value="tokens">Token</option>
            <option value="active_time">{t('活跃时间')}</option>
          </select>
          <select value={span} aria-label={t('热力图范围')} onChange={(event) => onSpan(event.target.value as HeatmapSpan)}>
            <option value="week">{t('周')}</option>
            <option value="month">{t('月')}</option>
            <option value="year">{t('年')}</option>
          </select>
        </div>
      </div>
      {loading ? <div className="inline-loading">{t('正在聚合每日活动…')}</div> : (
        <div
          ref={scrollRef}
          className="heatmap-scroll"
          role="region"
          tabIndex={0}
          aria-label={`${t('活跃热力图')} · ${t('拖动查看更早日期')}`}
          title={t('拖动查看更早日期')}
          onPointerDown={beginDrag}
          onPointerMove={moveDrag}
          onPointerUp={endDrag}
          onPointerCancel={endDrag}
          onLostPointerCapture={endDrag}
          onKeyDown={navigateWithKeyboard}
        >
          <div className={`heatmap-grid span-${span}`}>
            {snapshot?.points.map((point) => {
              const level = heatLevel(metric, point.value, snapshot.maxValue, positiveValues);
              return (
                <div
                  key={point.date}
                  className={`heat-cell metric-${metric.replace('_', '-')} level-${level}`}
                  title={`${point.date} · ${metricValue(metric, point.value)}`}
                  aria-label={`${point.date}，${metricValue(metric, point.value)}`}
                  role="img"
                />
              );
            })}
          </div>
        </div>
      )}
      <div className="heatmap-legend"><span>{t('少')}</span>{[0, 1, 2, 3, 4].map((level) => <i key={level} className={`level-${level}`} />)}<span>{t('多')}</span></div>
    </section>
  );
}

function heatLevel(metric: HeatmapMetric, value: number, maxValue: number, positiveValues: number[]): number {
  if (value <= 0 || maxValue <= 0) return 0;
  if (metric === 'sessions') return Math.min(4, Math.max(1, Math.ceil((value / maxValue) * 4)));

  // Token 和活跃时间通常是长尾分布。按正值的百分位着色，避免一个峰值让全年
  // 其余方块都落在同一档；会话数沿用原来的线性刻度。
  let upperBound = 0;
  while (upperBound < positiveValues.length && positiveValues[upperBound] <= value) upperBound += 1;
  return Math.min(4, Math.max(1, Math.ceil((upperBound / positiveValues.length) * 4)));
}

function metricValue(metric: HeatmapMetric, value: number): string {
  if (metric === 'tokens') return `${formatTokens(value)} Token`;
  if (metric === 'active_time') return formatDuration(value);
  return `${formatExact(value)} 个会话`;
}
