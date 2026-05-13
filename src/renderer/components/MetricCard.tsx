import React from 'react';
import type { MetricCard as MetricCardType } from '../../shared/contracts';
import { useI18n } from '../i18n/I18nContext';

export function MetricCard({ card }: { card: MetricCardType }): React.ReactElement {
  const { t } = useI18n();
  const value = card.valueKey ? t(card.valueKey) : card.value;
  return (
    <section className={`metric-card metric-card--${card.tone || 'default'}`} aria-label={t(card.labelKey)}>
      <div className="metric-card__label">{t(card.labelKey)}</div>
      <div className="metric-card__value">{value}</div>
      {card.sublabel || card.sublabelKey ? (
        <div className="metric-card__sublabel">{card.sublabelKey ? t(card.sublabelKey, card.sublabelArgs) : card.sublabel}</div>
      ) : null}
      {card.detailItems?.length ? (
        <div className="metric-card__details">
          {card.detailItems.map((item) => (
            <span key={item.labelKey}>
              <span>{t(item.labelKey)}</span>
              <strong>{item.valueKey ? t(item.valueKey) : item.value}</strong>
            </span>
          ))}
        </div>
      ) : null}
    </section>
  );
}
