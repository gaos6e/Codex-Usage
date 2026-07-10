import {
  Activity,
  BrainCircuit,
  CalendarCheck2,
  Coins,
  DatabaseZap,
  Download,
  Gauge,
  Flame,
  MessageSquareText,
  Sparkles,
  Upload,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import type { HeroMetrics } from '../types';
import { formatCost, formatDuration, formatExact, formatTokens } from '../lib/format';

interface UsageHeroProps {
  hero: HeroMetrics;
  partial: boolean;
}

export function UsageHero({ hero, partial }: UsageHeroProps) {
  const { t } = useTranslation();
  const hitRate = Math.max(0, Math.min(1, hero.cacheHitRate ?? 0));
  return (
    <section className="usage-hero" aria-label={t('用量汇总')}>
      <div className="hero-primary">
        <div className="hero-title-row">
          <div className="hero-icon"><DatabaseZap /></div>
          <div className="hero-copy">
            <span>{t('真实总 Token')}</span>
            <div className="hero-total" title={formatExact(hero.realTotalTokens)}>
              {formatExact(hero.realTotalTokens)}
              <small>≈ {formatTokens(hero.realTotalTokens)}</small>
            </div>
          </div>
        </div>
        <div className="hero-side-metrics">
          <div><MessageSquareText /><span>{t('会话')}</span><strong>{formatExact(hero.sessionCount)}</strong><em aria-hidden="true">&nbsp;</em></div>
          <div><Activity /><span>{t('活跃时间')}</span><strong>{formatDuration(hero.activeMs)}</strong><em aria-hidden="true">&nbsp;</em></div>
          <div>
            <Coins />
            <span>{t('估算成本')}</span>
            <strong>{formatCost(hero.estimatedCostMicrousd)}</strong>
            {hero.unpricedEventCount > 0 && <em>{partial ? t('部分未定价') : t('含未定价事件')}</em>}
          </div>
        </div>
      </div>

      <div className="token-breakdown">
        <MiniMetric icon={<Download />} label={t('新增输入')} value={formatTokens(hero.freshInputTokens)} tone="blue" />
        <MiniMetric icon={<Upload />} label={t('输出')} value={formatTokens(hero.outputTokens)} tone="green" />
        <MiniMetric icon={<Sparkles />} label={t('缓存读取')} value={formatTokens(hero.cachedInputTokens)} tone="violet" />
        <MiniMetric icon={<BrainCircuit />} label={t('推理')} value={formatTokens(hero.reasoningTokens)} tone="orange" />
        <div className="hit-rate-card">
          <div><span>{t('缓存命中率')}</span><strong>{hero.cacheHitRate == null ? 'N/A' : `${(hitRate * 100).toFixed(1)}%`}</strong></div>
          <div className="hit-track"><span style={{ width: `${hitRate * 100}%` }} /></div>
        </div>
      </div>

      <div className="average-grid">
        <AverageMetric icon={<Gauge />} label={t('日均 Token')} value={formatTokens(hero.averageTokensPerDay)} />
        <AverageMetric icon={<Coins />} label={t('日均成本')} value={formatCost(hero.averageCostMicrousdPerDay)} />
        <AverageMetric icon={<MessageSquareText />} label={t('日均会话')} value={hero.averageSessionsPerDay.toFixed(1)} />
        <AverageMetric icon={<Activity />} label={t('日均活跃')} value={formatDuration(hero.averageActiveMsPerDay)} />
        <AverageMetric icon={<CalendarCheck2 />} label={t('活跃天数')} value={formatExact(hero.activeDays)} />
        <AverageMetric className="peak-day-metric" icon={<Sparkles />} label={t('峰值日')} value={hero.peakDay ? `${hero.peakDay} · ${formatTokens(hero.peakDayTokens)}` : '—'} />
        <AverageMetric icon={<Flame />} label={t('最长连续活跃')} value={`${formatExact(hero.longestActiveStreakDays)} ${t('天')}`} />
      </div>
    </section>
  );
}

function MiniMetric({
  icon,
  label,
  value,
  tone,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  tone: string;
}) {
  return (
    <div className={`mini-metric ${tone}`}>
      <span>{icon}{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function AverageMetric({
  icon,
  label,
  value,
  className,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
  className?: string;
}) {
  return <div className={`average-metric${className ? ` ${className}` : ''}`}><span>{icon}{label}</span><strong>{value}</strong></div>;
}
